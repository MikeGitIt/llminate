pub mod agent_tool;
pub mod ask_user_question_tool;
pub mod client;
pub mod client_adapter;
pub mod conversation;
pub mod diff_display;
pub mod emacs_tool;
pub mod enter_plan_mode_tool;
pub mod exit_plan_mode_tool;
pub mod git_prompts;
pub mod github_prompts;
pub mod models;
pub mod notebook_tools;
pub mod security_prompts;
pub mod skill_tool;
pub mod streaming;
pub mod summarization;
pub mod system_prompt;
pub mod task_tools;
pub mod todo_tool;
pub mod tools;
pub mod web_tools;

use crate::auth::aws::CredentialProvider;
use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// AI provider type — matches JavaScript getProvider() at cli-jsdef-fixed.js ~93135
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Provider {
    /// First-party Anthropic API (default)
    FirstParty,
    /// AWS Bedrock
    Bedrock,
    /// Google Vertex AI
    Vertex,
}

impl Default for Provider {
    fn default() -> Self {
        Self::FirstParty
    }
}

/// Detect provider from environment variables (matching JavaScript getProvider())
pub fn determine_provider() -> Provider {
    if std::env::var("CLAUDE_CODE_USE_BEDROCK").is_ok() {
        Provider::Bedrock
    } else if std::env::var("CLAUDE_CODE_USE_VERTEX").is_ok() {
        Provider::Vertex
    } else {
        Provider::FirstParty
    }
}

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
    /// Provider type (FirstParty, Bedrock, Vertex)
    #[serde(default)]
    pub provider: Provider,
    /// AWS region for Bedrock
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aws_region: Option<String>,
    /// AWS access key ID for Bedrock
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aws_access_key: Option<String>,
    /// AWS secret access key for Bedrock
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aws_secret_key: Option<String>,
    /// AWS session token for Bedrock
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aws_session_token: Option<String>,
    /// Skip Bedrock authentication (proxy mode)
    #[serde(default)]
    pub skip_bedrock_auth: bool,
    /// Bearer token for Bedrock (skips SigV4)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bedrock_bearer_token: Option<String>,
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
            provider: Provider::default(),
            aws_region: None,
            aws_access_key: None,
            aws_secret_key: None,
            aws_session_token: None,
            skip_bedrock_auth: false,
            bedrock_bearer_token: None,
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
    url: Option<String>, // Handle null values from API
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

    /// Get the input schema of the tool
    pub fn input_schema(&self) -> serde_json::Value {
        match self {
            Tool::Standard { input_schema, .. } => input_schema.clone(),
            Tool::WebSearch { .. } => serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query"
                    }
                },
                "required": ["query"]
            }),
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

    // Validate configuration — skip API key check for non-first-party providers
    let provider = determine_provider();
    if provider == Provider::FirstParty && config.api_key.is_empty() {
        return Err(Error::Config(
            "API key not found. Set ANTHROPIC_API_KEY environment variable or configure in settings.".to_string()
        ));
    }
    config.provider = provider;

    Ok(config)
}

/// Create a client with default configuration
/// Uses AIClientAdapter which wraps AnthropicClient or BedrockClient depending on provider
pub async fn create_client() -> Result<client_adapter::AIClientAdapter> {
    match determine_provider() {
        Provider::Bedrock => create_bedrock_client().await,
        Provider::Vertex => Err(Error::Config(
            "Vertex AI provider is not yet supported".to_string(),
        )),
        Provider::FirstParty => create_first_party_client().await,
    }
}

/// Create a first-party Anthropic API client (existing behavior)
async fn create_first_party_client() -> Result<client_adapter::AIClientAdapter> {
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

/// Map first-party Anthropic model IDs to Bedrock model identifiers.
/// Matching JavaScript model table at cli-jsdef-fixed.js ~182998-183051
pub fn map_model_to_bedrock(model: &str) -> String {
    match model {
        "claude-3-7-sonnet-20250219" => "us.anthropic.claude-3-7-sonnet-20250219-v1:0".to_string(),
        "claude-3-5-sonnet-20241022" => "anthropic.claude-3-5-sonnet-20241022-v2:0".to_string(),
        "claude-3-5-haiku-20241022" => "us.anthropic.claude-3-5-haiku-20241022-v1:0".to_string(),
        "claude-haiku-4-5-20251001" => "us.anthropic.claude-haiku-4-5-20251001-v1:0".to_string(),
        "claude-sonnet-4-20250514" => "us.anthropic.claude-sonnet-4-20250514-v1:0".to_string(),
        "claude-sonnet-4-5-20250929" => "us.anthropic.claude-sonnet-4-5-20250929-v1:0".to_string(),
        "claude-opus-4-20250514" => "us.anthropic.claude-opus-4-20250514-v1:0".to_string(),
        "claude-opus-4-1-20250805" => "us.anthropic.claude-opus-4-1-20250805-v1:0".to_string(),
        "claude-opus-4-5-20251101" => "us.anthropic.claude-opus-4-5-20251101-v1:0".to_string(),
        "claude-opus-4-6" => "anthropic.claude-opus-4-6-v1:0".to_string(),
        // If the model is already a Bedrock ID (contains "anthropic."), pass through
        other if other.contains("anthropic.") => other.to_string(),
        // Unknown model — pass through as-is (user may have set a custom Bedrock model ID)
        other => other.to_string(),
    }
}

/// Create an AWS Bedrock client
/// Matching JS client creation at cli-jsdef-fixed.js ~350370
async fn create_bedrock_client() -> Result<client_adapter::AIClientAdapter> {
    let mut config = AIConfig::default();
    config.provider = Provider::Bedrock;

    // Resolve region: AWS_REGION || AWS_DEFAULT_REGION || "us-east-1"
    config.aws_region = Some(
        std::env::var("AWS_REGION")
            .or_else(|_| std::env::var("AWS_DEFAULT_REGION"))
            .unwrap_or_else(|_| "us-east-1".to_string()),
    );

    // Check skip-auth flag
    config.skip_bedrock_auth = std::env::var("CLAUDE_CODE_SKIP_BEDROCK_AUTH").is_ok();

    // Check bearer token
    config.bedrock_bearer_token = std::env::var("AWS_BEARER_TOKEN_BEDROCK").ok();

    // If not skipping auth and no bearer token, resolve AWS credentials
    let has_bearer = config
        .bedrock_bearer_token
        .as_ref()
        .map(|t| !t.trim().is_empty())
        .unwrap_or(false);

    if !config.skip_bedrock_auth && !has_bearer {
        let provider = crate::auth::aws::DefaultCredentialProvider::new();
        match provider.get_credentials().await {
            Ok(creds) => {
                config.aws_access_key = Some(creds.access_key_id);
                config.aws_secret_key = Some(creds.secret_access_key);
                config.aws_session_token = creds.session_token;
            }
            Err(e) => {
                return Err(Error::Auth(format!(
                    "Failed to resolve AWS credentials for Bedrock: {}. \
                    Set AWS_ACCESS_KEY_ID/AWS_SECRET_ACCESS_KEY environment variables, \
                    configure AWS CLI credentials, or set CLAUDE_CODE_SKIP_BEDROCK_AUTH=1 for proxy mode.",
                    e
                )));
            }
        }
    }

    // Load model from environment if set
    if let Ok(model) = std::env::var("ANTHROPIC_MODEL") {
        config.default_model = model;
    }

    // Map the model ID to Bedrock format
    // e.g. "claude-opus-4-1-20250805" -> "us.anthropic.claude-opus-4-1-20250805-v1:0"
    config.default_model = map_model_to_bedrock(&config.default_model);

    // Load timeout from environment if set
    if let Ok(timeout) = std::env::var("ANTHROPIC_TIMEOUT") {
        if let Ok(secs) = timeout.parse::<u64>() {
            config.timeout_secs = secs;
        }
    }

    client_adapter::AIClientAdapter::new_bedrock(config)
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
                Set the ANTHROPIC_API_KEY environment variable or configure it in settings."
                    .to_string(),
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
        return Err(Error::Auth(
            "No API key available. Please set ANTHROPIC_API_KEY environment variable.".to_string(),
        ));
    }

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests for determine_provider() — consolidated into one test to avoid
    /// env var pollution between parallel tests
    #[test]
    fn test_determine_provider() {
        // Save original state
        let orig_bedrock = std::env::var("CLAUDE_CODE_USE_BEDROCK").ok();
        let orig_vertex = std::env::var("CLAUDE_CODE_USE_VERTEX").ok();

        // Test 1: Default (no env vars) → FirstParty
        std::env::remove_var("CLAUDE_CODE_USE_BEDROCK");
        std::env::remove_var("CLAUDE_CODE_USE_VERTEX");
        assert_eq!(
            determine_provider(),
            Provider::FirstParty,
            "Default should be FirstParty"
        );

        // Test 2: Bedrock env var set → Bedrock
        std::env::set_var("CLAUDE_CODE_USE_BEDROCK", "1");
        std::env::remove_var("CLAUDE_CODE_USE_VERTEX");
        assert_eq!(
            determine_provider(),
            Provider::Bedrock,
            "Should detect Bedrock"
        );

        // Test 3: Vertex env var set → Vertex
        std::env::remove_var("CLAUDE_CODE_USE_BEDROCK");
        std::env::set_var("CLAUDE_CODE_USE_VERTEX", "1");
        assert_eq!(
            determine_provider(),
            Provider::Vertex,
            "Should detect Vertex"
        );

        // Test 4: Both set → Bedrock takes precedence
        std::env::set_var("CLAUDE_CODE_USE_BEDROCK", "1");
        std::env::set_var("CLAUDE_CODE_USE_VERTEX", "1");
        assert_eq!(
            determine_provider(),
            Provider::Bedrock,
            "Bedrock should take precedence"
        );

        // Restore original state
        match orig_bedrock {
            Some(v) => std::env::set_var("CLAUDE_CODE_USE_BEDROCK", v),
            None => std::env::remove_var("CLAUDE_CODE_USE_BEDROCK"),
        }
        match orig_vertex {
            Some(v) => std::env::set_var("CLAUDE_CODE_USE_VERTEX", v),
            None => std::env::remove_var("CLAUDE_CODE_USE_VERTEX"),
        }
    }

    #[test]
    fn test_ai_config_default_provider() {
        let config = AIConfig::default();
        assert_eq!(config.provider, Provider::FirstParty);
        assert!(config.aws_region.is_none());
        assert!(config.aws_access_key.is_none());
        assert!(config.aws_secret_key.is_none());
        assert!(config.aws_session_token.is_none());
        assert!(!config.skip_bedrock_auth);
        assert!(config.bedrock_bearer_token.is_none());
    }

    #[test]
    fn test_load_config_skips_api_key_check_for_bedrock() {
        // Save original state
        let orig_bedrock = std::env::var("CLAUDE_CODE_USE_BEDROCK").ok();
        let orig_api_key = std::env::var("ANTHROPIC_API_KEY").ok();

        // Set Bedrock provider
        std::env::set_var("CLAUDE_CODE_USE_BEDROCK", "1");
        // Ensure no API key
        std::env::remove_var("ANTHROPIC_API_KEY");

        let result = load_config();
        // Should NOT fail with "API key not found" for Bedrock
        assert!(
            result.is_ok(),
            "load_config should not require API key for Bedrock provider, got: {:?}",
            result.err()
        );
        let config = result.as_ref().ok();
        assert_eq!(config.map(|c| &c.provider), Some(&Provider::Bedrock));

        // Restore original state
        match orig_bedrock {
            Some(v) => std::env::set_var("CLAUDE_CODE_USE_BEDROCK", v),
            None => std::env::remove_var("CLAUDE_CODE_USE_BEDROCK"),
        }
        match orig_api_key {
            Some(v) => std::env::set_var("ANTHROPIC_API_KEY", v),
            None => std::env::remove_var("ANTHROPIC_API_KEY"),
        }
    }
}
