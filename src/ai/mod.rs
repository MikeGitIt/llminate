pub mod client;
pub mod client_adapter;
pub mod models;
pub mod conversation;
pub mod streaming;
pub mod system_prompt;
pub mod tools;
pub mod agent_tool;
pub mod todo_tool;
pub mod web_tools;
pub mod notebook_tools;
pub mod exit_plan_mode_tool;
pub mod summarization;
pub mod git_prompts;
pub mod github_prompts;
pub mod security_prompts;
pub mod diff_display;

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// AI provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIConfig {
    /// API key for authentication
    pub api_key: String,
    /// Base URL for API endpoint
    pub base_url: String,
    /// OAuth auth token (for Claude Desktop authentication)
    pub auth_token: Option<String>,
    /// Default model to use
    pub default_model: String,
    /// Maximum tokens for responses
    pub max_tokens: u32,
    /// Temperature for sampling
    pub temperature: f32,
    /// Request timeout in seconds
    pub timeout_secs: u64,
    /// Maximum retry attempts
    pub max_retries: Option<u32>,
    /// Logging level
    pub log_level: Option<String>,
    /// Allow browser environment (dangerous)
    pub dangerously_allow_browser: Option<bool>,
    /// Retry configuration
    pub retry_config: RetryConfig,
}

impl Default for AIConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: "https://api.anthropic.com/v1".to_string(),
            auth_token: None,
            default_model: "claude-opus-4-1-20250805".to_string(),
            max_tokens: 4096,
            temperature: 0.7,
            timeout_secs: 300,
            max_retries: None,
            log_level: None,
            dangerously_allow_browser: None,
            retry_config: RetryConfig::default(),
        }
    }
}

/// Retry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of retries
    pub max_retries: u32,
    /// Initial retry delay in milliseconds
    pub initial_delay_ms: u64,
    /// Maximum retry delay in milliseconds
    pub max_delay_ms: u64,
    /// Exponential backoff multiplier
    pub backoff_multiplier: f32,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay_ms: 1000,
            max_delay_ms: 30000,
            backoff_multiplier: 2.0,
        }
    }
}

/// Message role in conversation
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

/// Message in a conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: MessageRole,
    pub content: MessageContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Message content types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Multipart(Vec<ContentPart>),
}

/// Content part for multipart messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentPart {
    #[serde(rename = "text")]
    Text { 
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        citations: Option<Vec<Citation>>,
    },
    #[serde(rename = "image")]
    Image { source: ImageSource },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
    #[serde(rename = "server_tool_use")]
    ServerToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "web_search_tool_result")]
    WebSearchToolResult {
        tool_use_id: String,
        content: WebSearchContent,
    },
}

/// Citation information for text content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Citation {
    #[serde(rename = "type")]
    citation_type: String,
    url: String,
    title: String,
    encrypted_index: String,
    cited_text: String,
}

/// Web search content - either results or error (matching JavaScript behavior)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WebSearchContent {
    Results(Vec<WebSearchResult>),
    Error { error_code: String },
}

/// Web search result (matching JavaScript structure exactly)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    title: Option<String>, // Handle null values from API
    #[serde(default, skip_serializing_if = "Option::is_none")]
    url: Option<String>,   // Handle null values from API
}

/// Image source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageSource {
    #[serde(rename = "type")]
    pub source_type: String,
    pub media_type: String,
    pub data: String,
}

/// Tool definition
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Tool {
    /// Standard tool with name, description, and input schema
    Standard {
        name: String,
        description: String,
        input_schema: serde_json::Value,
    },
    /// Web search tool (special format for Claude API)
    WebSearch {
        #[serde(rename = "type")]
        tool_type: String,
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        allowed_domains: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        blocked_domains: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        max_uses: Option<u32>,
    },
}

impl Tool {
    /// Get the name of the tool
    pub fn name(&self) -> &str {
        match self {
            Tool::Standard { name, .. } => name,
            Tool::WebSearch { name, .. } => name,
        }
    }
}

/// Chat completion request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
    /// Beta features to enable for this request (passed in body for beta.messages.create)
    /// JavaScript SDK (cli-jsdef-fixed.js:272970-272972) passes this in request body
    #[serde(skip_serializing_if = "Option::is_none")]
    pub betas: Option<Vec<String>>,
}

/// Tool choice configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolChoice {
    Auto,
    Any,
    Tool { name: String },
}

/// Chat completion response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub id: String,
    pub model: String,
    pub role: MessageRole,
    pub content: Vec<ContentPart>,
    pub stop_reason: Option<StopReason>,
    pub stop_sequence: Option<String>,
    pub usage: Usage,
}

/// Stop reason
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    MaxTokens,
    StopSequence,
    ToolUse,
}

/// Token usage information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<u32>,
}

/// Error response from API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: ErrorDetail,
}

/// Error detail
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorDetail {
    #[serde(rename = "type")]
    pub error_type: String,
    pub message: String,
}

/// Load AI configuration from environment and config
pub fn load_config() -> Result<AIConfig> {
    let mut config = AIConfig::default();
    
    // Load from environment variables
    if let Ok(api_key) = std::env::var("ANTHROPIC_API_KEY") {
        config.api_key = api_key;
    }
    
    if let Ok(base_url) = std::env::var("ANTHROPIC_BASE_URL") {
        config.base_url = base_url;
    }
    
    if let Ok(model) = std::env::var("ANTHROPIC_MODEL") {
        config.default_model = model;
    }
    
    // Load from config file
    if let Ok(user_config) = crate::config::load_config(crate::config::ConfigScope::User) {
        if let Some(ai_config) = user_config.ai_config {
            if !ai_config.api_key.is_empty() {
                config.api_key = ai_config.api_key;
            }
            if !ai_config.base_url.is_empty() {
                config.base_url = ai_config.base_url;
            }
            if !ai_config.default_model.is_empty() {
                config.default_model = ai_config.default_model;
            }
            config.max_tokens = ai_config.max_tokens;
            config.temperature = ai_config.temperature;
            config.timeout_secs = ai_config.timeout_secs;
            config.retry_config = ai_config.retry_config;
        }
    }
    
    // Validate configuration
    if config.api_key.is_empty() {
        return Err(Error::Config(
            "API key not found. Set ANTHROPIC_API_KEY environment variable or configure in settings.".to_string()
        ));
    }
    
    Ok(config)
}

/// Create a client with default configuration
/// Uses AIClientAdapter which wraps AnthropicClient (has OAuth metadata helpers)
pub async fn create_client() -> Result<client_adapter::AIClientAdapter> {
    // Try to get authentication (API key or Claude Desktop)
    match crate::auth::get_or_prompt_auth().await {
        Ok(auth_method) => {
            let config = load_config_with_auth(auth_method)?;
            client_adapter::AIClientAdapter::new(config)
        }
        Err(_) => {
            // Fallback to environment-based config for backwards compatibility
            let config = load_config()?;
            client_adapter::AIClientAdapter::new(config)
        }
    }
}

/// Load AI configuration with authentication method
pub fn load_config_with_auth(auth_method: crate::auth::AuthMethod) -> Result<AIConfig> {
    let mut config = AIConfig::default();

    match auth_method {
        crate::auth::AuthMethod::ApiKey(api_key) => {
            config.api_key = api_key;
            config.base_url = "https://api.anthropic.com/v1".to_string();
        }
        // OAUTH DISABLED: Anthropic has disabled 3rd party OAuth support for Claude Code CLI
        // crate::auth::AuthMethod::ClaudeAiOauth(oauth_auth) => {
        //     // OAuth tokens use Bearer authentication
        //     config.auth_token = Some(oauth_auth.access_token);
        //     config.api_key = String::new(); // No API key for OAuth
        //     config.base_url = "https://api.anthropic.com/v1".to_string();
        // }
        crate::auth::AuthMethod::ClaudeAiOauth(_oauth_auth) => {
            // OAuth is disabled - return error instructing user to use API key
            return Err(Error::Auth(
                "OAuth authentication is no longer supported. Please use an API key instead.\n\
                Set the ANTHROPIC_API_KEY environment variable or configure it in settings.".to_string()
            ));
        }
    }

    // Load other settings from environment if available
    if let Ok(base_url) = std::env::var("ANTHROPIC_BASE_URL") {
        config.base_url = base_url;
    }

    if let Ok(model) = std::env::var("ANTHROPIC_MODEL") {
        config.default_model = model;
    }

    // Validate that we have API key
    if config.api_key.is_empty() {
        return Err(Error::Auth("No API key available. Please set ANTHROPIC_API_KEY environment variable.".to_string()));
    }

    Ok(config)
}