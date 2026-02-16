use crate::ai::{
    AIConfig, ChatRequest, ChatResponse, ContentPart, ErrorResponse, Message, MessageContent,
    MessageRole, RetryConfig, StopReason, Tool, ToolChoice, Usage,
};
use crate::error::{Error, Result};
use anyhow::Context;
use reqwest::{Client, Response, StatusCode};
use serde::de::DeserializeOwned;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, info, error};

/// AI client for making API requests - matches JavaScript AnthropicClient
pub struct AIClient {
    config: AIConfig,
    http_client: Client,
    max_retries: u32,
    dangerously_allow_browser: bool,
    log_level: String,
}

impl AIClient {
    /// Create a new AI client matching JavaScript AnthropicClient constructor
    pub fn new(config: AIConfig) -> Result<Self> {
        // Check for browser environment like JavaScript
        #[cfg(target_arch = "wasm32")]
        {
            if !config.dangerously_allow_browser.unwrap_or(false) {
                return Err(Error::Config(
                    "It looks like you're running in a browser-like environment.\n\n\
                    This is disabled by default, as it risks exposing your secret API credentials to attackers.\n\
                    If you understand the risks and have appropriate mitigations in place,\n\
                    you can set the `dangerously_allow_browser` option to `true`.".to_string()
                ));
            }
        }
        
        // Set defaults matching JavaScript (cli-jsdef-fixed.js:66745, 191559)
        let base_url = if config.base_url.is_empty() {
            std::env::var("ANTHROPIC_BASE_URL")
                .unwrap_or_else(|_| "https://api.anthropic.com/v1".to_string())
        } else {
            config.base_url.clone()
        };
        
        let timeout_secs = if config.timeout_secs == 0 { 60 } else { config.timeout_secs };
        let max_retries = config.max_retries.unwrap_or(2);
        let log_level = config.log_level.clone().unwrap_or_else(|| "warn".to_string());
        
        let http_client = Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .map_err(|e| Error::Network(format!("Failed to build HTTP client: {}", e)))?;
        
        let mut updated_config = config;
        updated_config.base_url = base_url;
        updated_config.timeout_secs = timeout_secs;
        
        Ok(Self {
            config: updated_config,
            http_client,
            max_retries,
            dangerously_allow_browser: false,
            log_level,
        })
    }
    
    /// Send a chat completion request
    pub async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        // OAUTH DISABLED: Anthropic has disabled 3rd party OAuth support for Claude Code CLI
        // let url = if self.config.auth_token.is_some() {
        //     format!("{}/messages?beta=true", self.config.base_url)
        // } else {
        //     format!("{}/messages", self.config.base_url)
        // };
        let url = format!("{}/messages", self.config.base_url);

        let response = self.send_request(&url, &request).await?;

        Ok(response)
    }
    
    /// Send a streaming chat completion request
    pub async fn chat_stream(
        &self,
        request: ChatRequest,
    ) -> Result<impl futures::Stream<Item = Result<StreamEvent>>> {
        let mut request = request;
        request.stream = Some(true);

        // OAUTH DISABLED: Anthropic has disabled 3rd party OAuth support for Claude Code CLI
        // let url = if self.config.auth_token.is_some() {
        //     format!("{}/messages?beta=true", self.config.base_url)
        // } else {
        //     format!("{}/messages", self.config.base_url)
        // };
        let url = format!("{}/messages", self.config.base_url);

        // DEBUG: Write request details to file for debugging
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("/tmp/llminate-debug.log")
        {
            use std::io::Write;
            let _ = writeln!(file, "=== DEBUG: SENDING MESSAGE REQUEST ===");
            let _ = writeln!(file, "URL: {}", url);
            let _ = writeln!(file, "Request body: {}", serde_json::to_string_pretty(&request).unwrap_or_else(|_| "Failed to serialize".to_string()));
            let _ = writeln!(file, "=== END DEBUG ===\n");
        }
        
        // Default headers matching JavaScript SDK initialization (cli-jsdef-fixed.js:272469-272484)
        // x-app: cli - Required identifier for Claude Code CLI
        // User-Agent: claude-cli/2.0.72 (external, cli) - Exact format from variable22811()
        // X-Stainless headers: SDK telemetry headers (cli-jsdef-fixed.js:189829-189835)
        let mut request_builder = self
            .http_client
            .post(&url)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .header("Accept", "application/json")
            .header("x-app", "cli")
            .header("User-Agent", "claude-cli/2.0.72 (external, cli)")
            .header("X-Stainless-Helper-Method", "stream")
            .header("X-Stainless-Lang", "js")
            .header("X-Stainless-Package-Version", "0.70.0")
            .header("X-Stainless-OS", "MacOS")
            .header("X-Stainless-Arch", "arm64")
            .header("X-Stainless-Runtime", "node")
            .header("X-Stainless-Runtime-Version", "v22.0.0")
            .header("X-Stainless-Retry-Count", "0");

        // OAUTH DISABLED: Anthropic has disabled 3rd party OAuth support for Claude Code CLI
        // Use API key authentication only (OAuth commented out)
        // let auth_type: &str;
        // if let Some(auth_token) = &self.config.auth_token {
        //     auth_type = "OAuth Bearer";
        //     request_builder = request_builder
        //         .header("Authorization", format!("Bearer {}", auth_token))
        //         .header("anthropic-beta", "claude-code-20250219,oauth-2025-04-20");
        // } else
        let auth_type: &str;
        if !self.config.api_key.is_empty() {
            auth_type = "API Key";
            request_builder = request_builder.header("x-api-key", &self.config.api_key);
        } else {
            return Err(Error::Auth("No authentication credentials available. Please set ANTHROPIC_API_KEY environment variable.".to_string()));
        }

        // DEBUG: Log headers to file
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("/tmp/llminate-debug.log")
        {
            use std::io::Write;
            let _ = writeln!(file, "Auth type: {}", auth_type);
            let _ = writeln!(file, "Headers: anthropic-version=2023-06-01, content-type=application/json, x-app=cli");
            let _ = writeln!(file, "User-Agent: claude-cli/2.0.72 (external, cli)");
            let _ = writeln!(file, "X-Stainless-Helper-Method: stream");
            if auth_type == "OAuth Bearer" {
                let _ = writeln!(file, "anthropic-beta: claude-code-20250219,oauth-2025-04-20");
                let _ = writeln!(file, "Authorization: Bearer <token>");
            }
            let _ = writeln!(file, "---");
        }
        
        let response = request_builder
            .json(&request)
            .send()
            .await
            .context("Failed to send request")?;
        
        info!("Response status: {}", response.status());
        
        if !response.status().is_success() {
            error!("Request failed with status: {}", response.status());
            let error = self.handle_error_response(response).await?;
            return Err(error);
        }
        
        let stream = response.bytes_stream();
        Ok(parse_sse_stream(stream))
    }
    
    /// Send a request with retry logic
    async fn send_request<T: DeserializeOwned>(
        &self,
        url: &str,
        body: &ChatRequest,
    ) -> Result<T> {
        let mut retries = 0;
        let mut delay = self.config.retry_config.initial_delay_ms;
        
        loop {
            // Default headers matching JavaScript SDK initialization (cli-jsdef-fixed.js:272469-272484)
            // X-Stainless headers: SDK telemetry headers (cli-jsdef-fixed.js:189829-189835)
            let mut request_builder = self
                .http_client
                .post(url)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .header("Accept", "application/json")
                .header("x-app", "cli")
                .header("User-Agent", "claude-cli/2.0.72 (external, cli)")
                .header("X-Stainless-Lang", "js")
                .header("X-Stainless-Package-Version", "0.70.0")
                .header("X-Stainless-OS", "MacOS")
                .header("X-Stainless-Arch", "arm64")
                .header("X-Stainless-Runtime", "node")
                .header("X-Stainless-Runtime-Version", "v22.0.0")
                .header("X-Stainless-Retry-Count", "0");

            // OAUTH DISABLED: Anthropic has disabled 3rd party OAuth support for Claude Code CLI
            // Use API key authentication only (OAuth commented out)
            // if let Some(auth_token) = &self.config.auth_token {
            //     // OAuth token authentication with required betas
            //     request_builder = request_builder
            //         .header("Authorization", format!("Bearer {}", auth_token))
            //         .header("anthropic-beta", "claude-code-20250219,oauth-2025-04-20");
            // } else
            if !self.config.api_key.is_empty() {
                request_builder = request_builder.header("x-api-key", &self.config.api_key);
            } else {
                return Err(Error::Auth("No authentication credentials available. Please set ANTHROPIC_API_KEY environment variable.".to_string()));
            }
            
            let response = match request_builder
                .json(body)
                .send()
                .await {
                    Ok(resp) => resp,
                    Err(e) => {
                        // Provide detailed error information
                        let error_msg = if e.is_connect() {
                            format!("Connection failed: {}", e)
                        } else if e.is_timeout() {
                            format!("Request timed out after {}s: {}", self.config.timeout_secs, e)
                        } else if e.is_builder() {
                            format!("Invalid request configuration: {}", e)
                        } else if e.is_redirect() {
                            format!("Too many redirects: {}", e)
                        } else if let Some(url) = e.url() {
                            format!("Failed to send request to {}: {}", url, e)
                        } else {
                            format!("Failed to send request: {} (error type: {:?})", e, e)
                        };
                        return Err(Error::Other(format!("{}: {}", error_msg, e)));
                    }
                };
            
            match response.status() {
                StatusCode::OK => {
                    return response
                        .json()
                        .await
                        .map_err(|e| Error::Api {
                            status: 0,
                            error_type: "parse_error".to_string(),
                            message: format!("Failed to parse response: {}", e),
                        });
                }
                StatusCode::TOO_MANY_REQUESTS | StatusCode::SERVICE_UNAVAILABLE => {
                    if retries >= self.config.retry_config.max_retries {
                        let error = self.handle_error_response(response).await?;
                        return Err(error);
                    }
                    
                    // Check for retry-after header
                    if let Some(retry_after) = response.headers().get("retry-after") {
                        if let Ok(seconds) = retry_after.to_str().unwrap_or("0").parse::<u64>() {
                            delay = seconds * 1000;
                        }
                    }
                    
                    retries += 1;
                    sleep(Duration::from_millis(delay)).await;
                    
                    delay = (delay as f64 * self.config.retry_config.backoff_multiplier as f64)
                        .min(self.config.retry_config.max_delay_ms as f64) as u64;
                }
                _ => {
                    let error = self.handle_error_response(response).await?;
                    return Err(error);
                }
            }
        }
    }
    
    /// Handle error response
    async fn handle_error_response(&self, response: Response) -> Result<Error> {
        let status = response.status();
        
        match response.json::<ErrorResponse>().await {
            Ok(error_response) => Ok(Error::Api {
                status: status.as_u16(),
                error_type: error_response.error.error_type,
                message: error_response.error.message,
            }),
            Err(_) => Ok(Error::Api {
                status: status.as_u16(),
                error_type: "unknown".to_string(),
                message: format!("HTTP {} error", status),
            }),
        }
    }
    
    /// Create a chat request builder
    pub fn create_chat_request(&self) -> ChatRequestBuilder {
        ChatRequestBuilder::new(self.config.default_model.clone())
    }
}

/// Builder for chat requests
pub struct ChatRequestBuilder {
    request: ChatRequest,
}

impl ChatRequestBuilder {
    /// Create a new builder
    pub fn new(model: String) -> Self {
        Self {
            request: ChatRequest {
                model,
                messages: Vec::new(),
                max_tokens: None,
                temperature: None,
                top_p: None,
                top_k: None,
                stop_sequences: None,
                stream: None,
                system: None,
                tools: None,
                tool_choice: None,
                metadata: None,
                betas: None,
            },
        }
    }
    
    /// Set the model
    pub fn model(mut self, model: &str) -> Self {
        self.request.model = model.to_string();
        self
    }

    /// Add a system message
    pub fn system(mut self, content: String) -> Self {
        self.request.system = Some(content);
        self
    }

    /// Add a message
    pub fn message(mut self, role: MessageRole, content: String) -> Self {
        self.request.messages.push(Message {
            role,
            content: MessageContent::Text(content),
            name: None,
        });
        self
    }
    
    /// Add a user message
    pub fn user(self, content: String) -> Self {
        self.message(MessageRole::User, content)
    }
    
    /// Add an assistant message
    pub fn assistant(self, content: String) -> Self {
        self.message(MessageRole::Assistant, content)
    }
    
    /// Add messages
    pub fn messages(mut self, messages: Vec<Message>) -> Self {
        self.request.messages.extend(messages);
        self
    }
    
    /// Set max tokens
    pub fn max_tokens(mut self, max_tokens: u32) -> Self {
        self.request.max_tokens = Some(max_tokens);
        self
    }
    
    /// Set temperature
    pub fn temperature(mut self, temperature: f32) -> Self {
        self.request.temperature = Some(temperature);
        self
    }
    
    /// Set tools
    pub fn tools(mut self, tools: Vec<Tool>) -> Self {
        self.request.tools = Some(tools);
        self
    }
    
    /// Set tool choice
    pub fn tool_choice(mut self, tool_choice: ToolChoice) -> Self {
        self.request.tool_choice = Some(tool_choice);
        self
    }
    
    /// Enable streaming
    pub fn stream(mut self) -> Self {
        self.request.stream = Some(true);
        self
    }
    
    /// Build the request
    pub fn build(self) -> ChatRequest {
        self.request
    }
}

/// Server-sent event for streaming
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// Message start event
    MessageStart {
        message: StreamMessage,
    },
    /// Content block start
    ContentBlockStart {
        index: usize,
        content_block: ContentBlock,
    },
    /// Content block delta
    ContentBlockDelta {
        index: usize,
        delta: ContentDelta,
    },
    /// Content block stop
    ContentBlockStop {
        index: usize,
    },
    /// Message delta
    MessageDelta {
        delta: MessageDelta,
        usage: Usage,
    },
    /// Message stop
    MessageStop,
    /// Content start (for simple content)
    ContentStart {
        content: String,
    },
    /// Content delta (for simple content)
    ContentDelta {
        delta: ContentDelta,
    },
    /// Content stop (for simple content) 
    ContentStop,
    /// Tool use start
    ToolUseStart {
        id: String,
        name: String,
    },
    /// Tool use delta
    ToolUseDelta {
        id: String,
        delta: String,
    },
    /// Tool use stop
    ToolUseStop {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    /// Ping event
    Ping,
    /// Error event
    Error(String),
}

/// Stream message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamMessage {
    pub id: String,
    pub model: String,
    pub role: MessageRole,
    pub content: Vec<ContentPart>,
    pub stop_reason: Option<StopReason>,
    pub stop_sequence: Option<String>,
    pub usage: Usage,
}

/// Content block
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    /// Thinking block for interleaved-thinking-2025-05-14 beta
    #[serde(rename = "thinking")]
    Thinking {
        thinking: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
    },
    /// Redacted thinking block
    #[serde(rename = "redacted_thinking")]
    RedactedThinking {
        data: String,
    },
}

/// Content delta
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentDelta {
    #[serde(rename = "text_delta")]
    TextDelta { text: String },
    #[serde(rename = "input_json_delta")]
    InputJsonDelta { partial_json: String },
    /// Thinking delta for interleaved-thinking-2025-05-14 beta
    #[serde(rename = "thinking_delta")]
    ThinkingDelta { thinking: String },
    /// Signature delta for thinking blocks
    #[serde(rename = "signature_delta")]
    SignatureDelta { signature: String },
}

/// Message delta
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageDelta {
    pub stop_reason: Option<StopReason>,
    pub stop_sequence: Option<String>,
}

use futures::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};

/// Parse SSE stream with proper buffering for partial chunks
fn parse_sse_stream(
    stream: impl Stream<Item = reqwest::Result<bytes::Bytes>> + Send + 'static,
) -> impl Stream<Item = Result<StreamEvent>> + Send {
    use futures::stream;
    use std::pin::Pin;
    use std::collections::VecDeque;
    
    // Wrap the stream in Pin<Box<...>> for proper async handling
    let pinned_stream = Box::pin(stream);
    
    // State for SSE parsing - holds buffer and event queue
    struct SseParserState {
        buffer: String,
        event_queue: VecDeque<Result<StreamEvent>>,
    }
    
    impl SseParserState {
        fn new() -> Self {
            Self {
                buffer: String::new(),
                event_queue: VecDeque::new(),
            }
        }
        
        fn process_buffer(&mut self) {
            // SSE protocol specification:
            // - Events are separated by double newline (\n\n)
            // - Each event consists of field:value pairs
            // - Field names include: data, event, id, retry
            // - Comments start with :
            
            // Process all complete events in the buffer
            while let Some(event_boundary) = self.buffer.find("\n\n") {
                // Extract one complete event (up to and including the \n\n)
                let event_text: String = self.buffer.drain(..=event_boundary + 1).collect();
                
                // Parse the event fields
                let mut data_fields = Vec::new();
                let mut event_type: Option<String> = None;
                
                for line in event_text.lines() {
                    if let Some(colon_pos) = line.find(':') {
                        let field = &line[..colon_pos];
                        let value = if colon_pos + 1 < line.len() {
                            // Skip optional space after colon
                            if line.chars().nth(colon_pos + 1) == Some(' ') {
                                &line[colon_pos + 2..]
                            } else {
                                &line[colon_pos + 1..]
                            }
                        } else {
                            ""
                        };
                        
                        match field {
                            "data" => {
                                data_fields.push(value);
                            }
                            "event" => {
                                event_type = Some(value.to_string());
                            }
                            "" => {
                                // Comment line starting with :, ignore
                            }
                            _ => {
                                // Other fields like id, retry - ignore for now
                            }
                        }
                    }
                }
                
                // Process the collected data fields
                // Multiple data fields are concatenated with \n between them
                if !data_fields.is_empty() {
                    let combined_data = data_fields.join("\n");
                    
                    // Check for stream termination
                    if combined_data == "[DONE]" {
                        continue;
                    }
                    
                    // Parse the JSON data
                    match serde_json::from_str::<SseEvent>(&combined_data) {
                        Ok(sse_event) => {
                            // Successfully parsed, convert to StreamEvent
                            self.event_queue.push_back(parse_sse_event(sse_event));
                        }
                        Err(parse_error) => {
                            // JSON parsing failed - this is a real error
                            self.event_queue.push_back(Err(Error::Other(format!(
                                "Failed to parse SSE event JSON: {}. Data was: '{}'",
                                parse_error, combined_data
                            ))));
                        }
                    }
                }
            }
            // Anything left in buffer is an incomplete event - keep for next iteration
        }
    }
    
    // Use unfold to create a stream from stateful async computation
    stream::unfold(
        (pinned_stream, SseParserState::new()),
        |(mut stream, mut state)| async move {
            // First, return any queued events from previous processing
            if let Some(event) = state.event_queue.pop_front() {
                return Some((event, (stream, state)));
            }
            
            // Read chunks from the stream until we get an event or stream ends
            loop {
                match stream.next().await {
                    Some(Ok(bytes)) => {
                        // Convert bytes to UTF-8 string
                        let text = match std::str::from_utf8(&bytes) {
                            Ok(text) => text,
                            Err(utf8_error) => {
                                return Some((
                                    Err(Error::Other(format!(
                                        "Invalid UTF-8 in SSE stream: {}",
                                        utf8_error
                                    ))),
                                    (stream, state),
                                ));
                            }
                        };
                        
                        // Append to buffer
                        state.buffer.push_str(text);
                        
                        // Process the buffer to extract complete events
                        state.process_buffer();
                        
                        // If we now have events, return the first one
                        if let Some(event) = state.event_queue.pop_front() {
                            return Some((event, (stream, state)));
                        }
                        // Otherwise continue reading more chunks
                    }
                    Some(Err(network_error)) => {
                        // Network error while reading stream
                        return Some((
                            Err(Error::Network(format!(
                                "Network error in SSE stream: {}",
                                network_error
                            ))),
                            (stream, state),
                        ));
                    }
                    None => {
                        // Stream has ended
                        
                        // Check for incomplete data in buffer
                        if !state.buffer.trim().is_empty() {
                            return Some((
                                Err(Error::Other(format!(
                                    "SSE stream ended with incomplete event: '{}'",
                                    state.buffer
                                ))),
                                (stream, state),
                            ));
                        }
                        
                        // Return any remaining queued events
                        if let Some(event) = state.event_queue.pop_front() {
                            return Some((event, (stream, state)));
                        }
                        
                        // Stream is fully consumed
                        return None;
                    }
                }
            }
        },
    )
}

/// Parse SSE chunk
fn parse_sse_chunk(chunk: &[u8]) -> Vec<Result<StreamEvent>> {
    let text = match std::str::from_utf8(chunk) {
        Ok(text) => text,
        Err(e) => return vec![Err(Error::Other(format!("Invalid UTF-8: {}", e)))],
    };
    
    let mut events = Vec::new();
    
    for line in text.lines() {
        if line.starts_with("data: ") {
            let data = &line[6..];
            
            if data == "[DONE]" {
                continue;
            }
            
            match serde_json::from_str::<SseEvent>(data) {
                Ok(event) => events.push(parse_sse_event(event)),
                Err(e) => events.push(Err(Error::Other(format!("Failed to parse SSE: {}", e)))),
            }
        }
    }
    
    events
}

/// SSE event
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum SseEvent {
    #[serde(rename = "message_start")]
    MessageStart { message: StreamMessage },
    #[serde(rename = "content_block_start")]
    ContentBlockStart {
        index: usize,
        content_block: ContentBlock,
    },
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta {
        index: usize,
        delta: ContentDelta,
    },
    #[serde(rename = "content_block_stop")]
    ContentBlockStop { index: usize },
    #[serde(rename = "message_delta")]
    MessageDelta {
        delta: MessageDelta,
        usage: Usage,
    },
    #[serde(rename = "message_stop")]
    MessageStop,
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "error")]
    Error { error: ErrorDetail },
}

use crate::ai::ErrorDetail;

/// Parse SSE event
fn parse_sse_event(event: SseEvent) -> Result<StreamEvent> {
    Ok(match event {
        SseEvent::MessageStart { message } => StreamEvent::MessageStart { message },
        SseEvent::ContentBlockStart {
            index,
            content_block,
        } => StreamEvent::ContentBlockStart {
            index,
            content_block,
        },
        SseEvent::ContentBlockDelta { index, delta } => {
            StreamEvent::ContentBlockDelta { index, delta }
        }
        SseEvent::ContentBlockStop { index } => StreamEvent::ContentBlockStop { index },
        SseEvent::MessageDelta { delta, usage } => StreamEvent::MessageDelta { delta, usage },
        SseEvent::MessageStop => StreamEvent::MessageStop,
        SseEvent::Ping => StreamEvent::Ping,
        SseEvent::Error { error } => StreamEvent::Error(error.message),
    })
}