use crate::ai::tools::ToolHandler;
use crate::ai::{ChatRequest, Message, MessageContent, MessageRole, Tool, ContentPart};
use crate::error::{Error, Result};
use tokio_util::sync::CancellationToken;
use async_trait::async_trait;
use reqwest;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use url::Url;
use html2text;
use std::sync::{Arc, Mutex};
use std::collections::VecDeque;
use once_cell::sync::Lazy;

// Cache for WebFetch results (15 minute TTL like JavaScript)
static FETCH_CACHE: Lazy<Arc<Mutex<LruCache>>> = Lazy::new(|| {
    Arc::new(Mutex::new(LruCache::new(100)))
});

// Simple LRU cache implementation
struct LruCache {
    cache: HashMap<String, CachedFetchResult>,
    order: VecDeque<String>,
    max_size: usize,
    max_age: Duration,
}

#[derive(Clone)]
struct CachedFetchResult {
    bytes: usize,
    code: u16,
    code_text: String,
    content: String,
    timestamp: Instant,
}

impl LruCache {
    fn new(max_size: usize) -> Self {
        Self {
            cache: HashMap::new(),
            order: VecDeque::new(),
            max_size,
            max_age: Duration::from_secs(15 * 60), // 15 minutes like JS (num85)
        }
    }

    fn get(&mut self, key: &str) -> Option<CachedFetchResult> {
        if let Some(result) = self.cache.get(key) {
            if result.timestamp.elapsed() < self.max_age {
                // Move to front
                self.order.retain(|k| k != key);
                self.order.push_front(key.to_string());
                return Some(result.clone());
            } else {
                // Expired, remove it
                self.cache.remove(key);
                self.order.retain(|k| k != key);
            }
        }
        None
    }

    fn set(&mut self, key: String, value: CachedFetchResult) {
        // Remove old entry if exists
        if self.cache.contains_key(&key) {
            self.order.retain(|k| k != &key);
        }
        
        // Add to front
        self.order.push_front(key.clone());
        self.cache.insert(key, value);
        
        // Remove oldest if over capacity
        while self.order.len() > self.max_size {
            if let Some(oldest) = self.order.pop_back() {
                self.cache.remove(&oldest);
            }
        }
    }

    fn clear_expired(&mut self) {
        let now = Instant::now();
        let expired: Vec<String> = self.cache
            .iter()
            .filter(|(_, v)| now.duration_since(v.timestamp) >= self.max_age)
            .map(|(k, _)| k.clone())
            .collect();
        
        for key in expired {
            self.cache.remove(&key);
            self.order.retain(|k| k != &key);
        }
    }
}

/// WebFetch tool - Fetches content from a URL and processes it
pub struct WebFetchTool;

async fn fetch_url(url: &str) -> Result<CachedFetchResult> {
    // Parse and validate URL
    let parsed_url = Url::parse(url)
        .map_err(|e| Error::InvalidInput(format!("Invalid URL: {}", e)))?;
    
    // Upgrade HTTP to HTTPS if needed
    let final_url = if parsed_url.scheme() == "http" {
        let mut https_url = parsed_url.clone();
        https_url.set_scheme("https")
            .map_err(|_| Error::InvalidInput("Failed to upgrade HTTP to HTTPS".to_string()))?;
        https_url.to_string()
    } else {
        url.to_string()
    };
    
    // Check cache first
    {
        let mut cache = FETCH_CACHE.lock().unwrap();
        cache.clear_expired();
        if let Some(cached) = cache.get(&final_url) {
            return Ok(cached);
        }
    }
    
    // Create HTTP client with same settings as JavaScript
    // Build user agent like JavaScript getUserAgentMiddleware
    let sdk_version = env!("CARGO_PKG_VERSION");
    let runtime = if cfg!(target_os = "windows") {
        "win32"
    } else if cfg!(target_os = "macos") {
        "darwin"
    } else {
        "linux"
    };
    let arch = if cfg!(target_arch = "x86_64") {
        "x64"
    } else if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "unknown"
    };
    
    // Format: sdk-name/version runtime/platform#arch
    let user_agent = format!(
        "claude-code/{} os/{}#{} lang/rust",
        sdk_version, runtime, arch
    );
    
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent(&user_agent)
        .redirect(reqwest::redirect::Policy::none()) // maxRedirects: 0 in JS
        .build()
        .map_err(|e| Error::Network(format!("Failed to create HTTP client: {}", e)))?;
    
    // Perform the fetch
    let response = client.get(&final_url).send().await;
    
    // Handle response and redirects
    let response = match response {
        Ok(resp) => {
            // Check for redirect status codes
            let status = resp.status();
            if status.is_redirection() {
                // Get Location header for redirect
                if let Some(location) = resp.headers().get("location") {
                    let location_str = location.to_str()
                        .map_err(|_| Error::Network("Invalid Location header".to_string()))?;
                    
                    // Resolve relative URLs against base URL
                    let redirect_url = if location_str.starts_with("http://") || location_str.starts_with("https://") {
                        Url::parse(location_str)
                    } else {
                        parsed_url.join(location_str)
                    }.map_err(|e| Error::Network(format!("Invalid redirect URL: {}", e)))?;
                    
                    // Check if redirect is to same host
                    let original_host = parsed_url.host_str();
                    let redirect_host = redirect_url.host_str();
                    
                    if original_host != redirect_host {
                        return Err(Error::Network(
                            format!("Redirect to different host detected. Please fetch from: {}", redirect_url)
                        ));
                    }
                    
                    // Follow redirect to same host
                    client.get(redirect_url.as_str())
                        .send()
                        .await
                        .map_err(|e| Error::Network(format!("Failed to follow redirect: {}", e)))?
                } else {
                    return Err(Error::Network("Redirect missing Location header".to_string()));
                }
            } else {
                resp
            }
        },
        Err(e) => {
            return Err(Error::Network(format!("Failed to fetch URL: {}", e)));
        }
    };
    
    let status = response.status();
    let status_text = status.canonical_reason()
        .unwrap_or("Unknown")
        .to_string();
    
    // Get content type
    let content_type = response.headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    
    // Read response body with size limit
    const MAX_CONTENT_LENGTH: usize = 10 * 1024 * 1024; // 10MB limit
    let bytes = response.bytes()
        .await
        .map_err(|e| Error::Network(format!("Failed to read response: {}", e)))?;
    
    if bytes.len() > MAX_CONTENT_LENGTH {
        return Err(Error::Network(
            format!("Response too large: {} bytes (max: {} bytes)", bytes.len(), MAX_CONTENT_LENGTH)
        ));
    }
    
    let content_str = String::from_utf8_lossy(&bytes).to_string();
    
    // Convert HTML to markdown if needed
    let processed_content = if content_type.contains("text/html") {
        html2text::from_read(content_str.as_bytes(), 80)
    } else {
        content_str
    };
    
    // Truncate if too long (matching JS limit)
    const MAX_PROCESSED_LENGTH: usize = 200000;
    let final_content = if processed_content.len() > MAX_PROCESSED_LENGTH {
        format!("{}...[content truncated]", &processed_content[..MAX_PROCESSED_LENGTH])
    } else {
        processed_content
    };
    
    let result = CachedFetchResult {
        bytes: bytes.len(),
        code: status.as_u16(),
        code_text: status_text,
        content: final_content,
        timestamp: Instant::now(),
    };
    
    // Store in cache
    {
        let mut cache = FETCH_CACHE.lock().unwrap();
        cache.set(final_url, result.clone());
    }
    
    Ok(result)
}

async fn process_content_with_ai(content: &str, prompt: &str) -> Result<String> {
    // Create AI client
    let client = crate::ai::create_client().await?;
    
    // Prepare the combined prompt
    let combined_prompt = format!(
        "Content from fetched URL:\n\n{}\n\n---\n\nUser's request: {}",
        content,
        prompt
    );
    
    // Create message
    let message = Message {
        role: MessageRole::User,
        content: MessageContent::Text(combined_prompt),
        name: None,
    };
    
    // Create chat request with a smaller, faster model for processing
    let request = ChatRequest {
        model: "claude-opus-4-1-20250805".to_string(), // Use Opus
        messages: vec![message],
        max_tokens: Some(4096),
        temperature: Some(0.3),
        top_p: None,
        top_k: None,
        stop_sequences: None,
        stream: Some(false),
        system: Some("You are a helpful assistant that analyzes web content based on user prompts. Be concise and direct in your responses.".to_string()),
        tools: None,
        tool_choice: None,
        metadata: None,
        betas: None,
    };
    
    // Send request
    let response = client.chat(request).await?;
    
    // Extract text from response
    let result_text = response.content.iter()
        .filter_map(|part| {
            if let ContentPart::Text { text, .. } = part {
                Some(text.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    
    Ok(result_text)
}

#[async_trait]
impl ToolHandler for WebFetchTool {
    fn description(&self) -> String {
        "\n- Fetches content from a specified URL and processes it using an AI model\n- Takes a URL and a prompt as input\n- Fetches the URL content, converts HTML to markdown\n- Processes the content with the prompt using a small, fast model\n- Returns the model's response about the content\n- Use this tool when you need to retrieve and analyze web content\n\nUsage notes:\n  - IMPORTANT: If an MCP-provided web fetch tool is available, prefer using that tool instead of this one, as it may have fewer restrictions. All MCP-provided tools start with \"mcp__\".\n  - The URL must be a fully-formed valid URL\n  - HTTP URLs will be automatically upgraded to HTTPS\n  - The prompt should describe what information you want to extract from the page\n  - This tool is read-only and does not modify any files\n  - Results may be summarized if the content is very large\n  - Includes a self-cleaning 15-minute cache for faster responses when repeatedly accessing the same URL\n".to_string()
    }
    
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "format": "uri",
                    "description": "The URL to fetch content from"
                },
                "prompt": {
                    "type": "string",
                    "description": "The prompt to run on the fetched content"
                }
            },
            "required": ["url", "prompt"],
            "additionalProperties": false
        })
    }
    
    async fn execute(&self, input: Value, _cancellation_token: Option<CancellationToken>) -> Result<String> {
        let url = input["url"]
            .as_str()
            .ok_or_else(|| Error::InvalidInput("Missing 'url' field".to_string()))?;
        
        let prompt = input["prompt"]
            .as_str()
            .ok_or_else(|| Error::InvalidInput("Missing 'prompt' field".to_string()))?;
        
        // Fetch the URL
        let fetch_result = fetch_url(url).await?;
        
        // Process content with AI
        let ai_result = process_content_with_ai(&fetch_result.content, prompt).await
            .unwrap_or_else(|e| {
                // If AI processing fails, return error note
                format!("Failed to process with AI: {}", e)
            });
        
        // Include HTTP status information in the output
        Ok(format!(
            "{}\n\n[HTTP Status: {} {}]",
            ai_result,
            fetch_result.code,
            fetch_result.code_text
        ))
    }
    
    fn action_description(&self, input: &Value) -> String {
        if let Some(url) = input["url"].as_str() {
            format!("Fetch content from {}", url)
        } else {
            "Fetch web content".to_string()
        }
    }
    
    fn permission_details(&self, input: &Value) -> String {
        if let Some(url) = input["url"].as_str() {
            if let Ok(parsed) = Url::parse(url) {
                if let Some(host) = parsed.host_str() {
                    return format!("Fetch content from {}", host);
                }
            }
        }
        "Fetch web content".to_string()
    }
}

/// WebSearch tool - Performs web searches using Claude's API
pub struct WebSearchTool;

#[async_trait]
impl ToolHandler for WebSearchTool {
    fn description(&self) -> String {
        "\n- Allows Claude to search the web and use the results to inform responses\n- Provides up-to-date information for current events and recent data\n- Returns search result information formatted as search result blocks\n- Use this tool for accessing information beyond Claude's knowledge cutoff\n- Searches are performed automatically within a single API call\n\nUsage notes:\n  - Domain filtering is supported to include or block specific websites\n  - Web search is only available in the US\n".to_string()
    }
    
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "minLength": 2,
                    "description": "The search query to use"
                },
                "allowed_domains": {
                    "type": "array",
                    "items": {
                        "type": "string"
                    },
                    "description": "Only include search results from these domains"
                },
                "blocked_domains": {
                    "type": "array",
                    "items": {
                        "type": "string"
                    },
                    "description": "Never include search results from these domains"
                }
            },
            "required": ["query"],
            "additionalProperties": false
        })
    }
    
    async fn execute(&self, input: Value, _cancellation_token: Option<CancellationToken>) -> Result<String> {
        let query = input["query"]
            .as_str()
            .ok_or_else(|| Error::InvalidInput("Missing 'query' field".to_string()))?;
        
        if query.len() < 2 {
            return Err(Error::InvalidInput("Query must be at least 2 characters long".to_string()));
        }
        
        let allowed_domains = input["allowed_domains"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
            });
        
        let blocked_domains = input["blocked_domains"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
            });
        
        // Validate that both allowed and blocked domains are not specified
        if allowed_domains.is_some() && blocked_domains.is_some() {
            return Err(Error::InvalidInput(
                "Cannot specify both allowed_domains and blocked_domains in the same request".to_string()
            ));
        }
        
        // Create AI client
        let client = crate::ai::create_client().await?;
        
        // Create message asking to perform web search
        let message = Message {
            role: MessageRole::User,
            content: MessageContent::Text(query.to_string()),
            name: None,
        };
        
        // Clone for later use
        let allowed_domains_clone = allowed_domains.clone();
        let blocked_domains_clone = blocked_domains.clone();
        
        // Create chat request with web search tool in the special format required by Claude API
        let request = ChatRequest {
            model: "claude-opus-4-1-20250805".to_string(), // Use Opus
            messages: vec![message],
            max_tokens: Some(1024),
            temperature: Some(0.3),
            top_p: None,
            top_k: None,
            stop_sequences: None,
            stream: Some(false), // Don't stream for simplicity
            system: None,
            tools: Some(vec![Tool::WebSearch {
                tool_type: "web_search_20250305".to_string(),
                name: "web_search".to_string(),
                allowed_domains,
                blocked_domains,
                max_uses: Some(5),
            }]),
            tool_choice: None,
            metadata: None,
            betas: None,
        };
        
        // Send request and collect results
        let response = client.chat(request).await?;
        
        // Process the response to extract search results
        let mut search_results: Vec<String> = Vec::new();
        let mut result_text = String::new();
        
        for part in response.content {
            match part {
                ContentPart::Text { text, .. } => {
                    result_text.push_str(&text);
                    result_text.push('\n');
                },
                ContentPart::WebSearchToolResult { content, .. } => {
                    // Handle web search results like JavaScript does
                    match content {
                        crate::ai::WebSearchContent::Results(results) => {
                            for result in results {
                                if let (Some(title), Some(url)) = (&result.title, &result.url) {
                                    result_text.push_str(&format!("Title: {}\nURL: {}\n\n", title, url));
                                }
                            }
                        },
                        crate::ai::WebSearchContent::Error { error_code } => {
                            result_text.push_str(&format!("Web search error: {}\n", error_code));
                        }
                    }
                },
                _ => {
                    // Handle other content types if needed
                }
            }
        }
        
        // Format the output
        let mut output = format!("Web search results for query: \"{}\"\n\n", query);
        
        if !result_text.is_empty() {
            output.push_str(&result_text);
        } else {
            output.push_str("No search results returned. Note: Web search requires Claude API with web search capability enabled.\n");
        }
        
        if let Some(ref domains) = allowed_domains_clone {
            if !domains.is_empty() {
                output.push_str(&format!("\nSearch restricted to domains: {}", domains.join(", ")));
            }
        }
        
        if let Some(ref domains) = blocked_domains_clone {
            if !domains.is_empty() {
                output.push_str(&format!("\nExcluding domains: {}", domains.join(", ")));
            }
        }
        
        Ok(output)
    }
    
    fn action_description(&self, input: &Value) -> String {
        if let Some(query) = input["query"].as_str() {
            format!("Search web for: {}", query)
        } else {
            "Search the web".to_string()
        }
    }
    
    fn permission_details(&self, input: &Value) -> String {
        if let Some(query) = input["query"].as_str() {
            let mut details = format!("Search for: {}", query);
            
            if let Some(allowed) = input["allowed_domains"].as_array() {
                if !allowed.is_empty() {
                    let domains: Vec<String> = allowed.iter()
                        .filter_map(|v| v.as_str())
                        .map(|s| s.to_string())
                        .collect();
                    details.push_str(&format!(" (only from: {})", domains.join(", ")));
                }
            }
            
            if let Some(blocked) = input["blocked_domains"].as_array() {
                if !blocked.is_empty() {
                    let domains: Vec<String> = blocked.iter()
                        .filter_map(|v| v.as_str())
                        .map(|s| s.to_string())
                        .collect();
                    details.push_str(&format!(" (excluding: {})", domains.join(", ")));
                }
            }
            
            details
        } else {
            "Web search".to_string()
        }
    }
}