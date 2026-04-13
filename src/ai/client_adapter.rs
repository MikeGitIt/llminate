// Adapter to use the robust auth::client::AnthropicClient in place of AIClient
// This provides a safe migration path without breaking existing functionality
// Supports both first-party Anthropic and AWS Bedrock providers

use crate::ai::client::{ChatRequestBuilder, StreamEvent};
use crate::ai::{AIConfig, ChatRequest, ChatResponse};
use crate::auth::client::{AnthropicClient, BedrockClient, ClientConfig};
use crate::error::Result;
use futures::Stream;
use std::pin::Pin;
use std::sync::Arc;

/// Inner client enum — dispatches to either Anthropic or Bedrock
enum InnerClient {
    Anthropic(Arc<AnthropicClient>),
    Bedrock(Arc<BedrockClient>),
}

/// Create an AnthropicClient from AIConfig for drop-in replacement
pub fn create_anthropic_from_ai_config(config: &AIConfig) -> Result<Arc<AnthropicClient>> {
    // Convert AIConfig to ClientConfig
    let mut client_config = ClientConfig::default();

    // Set defaultHeaders matching JavaScript SDK (cli-jsdef-fixed.js:272469-272484)
    // JavaScript: { "x-app": "cli", "User-Agent": variable22811(), ... }
    use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
    let mut default_headers = HeaderMap::new();
    default_headers.insert(
        HeaderName::from_static("x-app"),
        HeaderValue::from_static("cli"),
    );
    default_headers.insert(
        HeaderName::from_static("user-agent"),
        HeaderValue::from_static("claude-cli/2.0.72 (external, cli)"),
    );
    client_config.default_headers = default_headers;

    // Transfer authentication
    if !config.api_key.is_empty() {
        client_config.api_key = Some(config.api_key.clone());
    }
    if let Some(ref auth_token) = config.auth_token {
        client_config.auth_token = Some(auth_token.clone());
    }

    // Transfer base URL
    if !config.base_url.is_empty() {
        client_config.base_url = config.base_url.clone();
    }

    // Transfer timeout
    client_config.timeout = std::time::Duration::from_secs(config.timeout_secs);

    // Transfer retry settings
    client_config.max_retries = config.max_retries.unwrap_or(2);

    // Transfer browser settings
    client_config.dangerously_allow_browser = config.dangerously_allow_browser.unwrap_or(false);

    // Transfer log level
    if let Some(ref log_level) = config.log_level {
        client_config.log_level = log_level.clone();
    }

    // Create the client - convert from anyhow::Result to crate::error::Result
    let client = AnthropicClient::new(client_config)
        .map_err(|e| crate::error::Error::Other(e.to_string()))?;
    Ok(Arc::new(client))
}

/// Create a BedrockClient from AIConfig
pub fn create_bedrock_from_ai_config(config: &AIConfig) -> Result<Arc<BedrockClient>> {
    let mut client_config = ClientConfig::default();

    // Bedrock doesn't use API key auth — disable it to pass validateHeaders
    client_config.api_key = None;
    client_config.auth_token = None;

    // Set default headers matching JavaScript SDK
    use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
    let mut default_headers = HeaderMap::new();
    default_headers.insert(
        HeaderName::from_static("x-app"),
        HeaderValue::from_static("cli"),
    );
    default_headers.insert(
        HeaderName::from_static("user-agent"),
        HeaderValue::from_static("claude-cli/2.0.72 (external, cli)"),
    );
    client_config.default_headers = default_headers;

    // Transfer timeout
    client_config.timeout = std::time::Duration::from_secs(config.timeout_secs);

    // Transfer retry settings
    client_config.max_retries = config.max_retries.unwrap_or(2);

    // Transfer log level
    if let Some(ref log_level) = config.log_level {
        client_config.log_level = log_level.clone();
    }

    let mut bedrock = BedrockClient::new(
        config.aws_region.clone(),
        config.aws_secret_key.clone(),
        config.aws_access_key.clone(),
        config.aws_session_token.clone(),
        client_config,
    )
    .map_err(|e| crate::error::Error::Other(e.to_string()))?;

    bedrock.skip_auth = config.skip_bedrock_auth;

    Ok(Arc::new(bedrock))
}

/// Wrapper that makes AnthropicClient/BedrockClient compatible with AIClient interface
pub struct AIClientAdapter {
    inner: InnerClient,
    config: AIConfig, // Keep original config for compatibility
}

impl AIClientAdapter {
    /// Create adapter for first-party Anthropic API
    pub fn new(config: AIConfig) -> Result<Self> {
        let inner = create_anthropic_from_ai_config(&config)?;
        Ok(Self {
            inner: InnerClient::Anthropic(inner),
            config,
        })
    }

    /// Create adapter for AWS Bedrock
    pub fn new_bedrock(config: AIConfig) -> Result<Self> {
        let inner = create_bedrock_from_ai_config(&config)?;
        Ok(Self {
            inner: InnerClient::Bedrock(inner),
            config,
        })
    }

    /// Send a chat completion request
    pub async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        match &self.inner {
            InnerClient::Anthropic(client) => client
                .chat(&request)
                .await
                .map_err(|e| crate::error::Error::Other(e.to_string())),
            InnerClient::Bedrock(client) => client
                .chat(&request)
                .await
                .map_err(|e| crate::error::Error::Other(e.to_string())),
        }
    }

    /// Send a streaming chat completion request
    /// Returns a boxed stream to unify the different concrete stream types
    /// from Anthropic and Bedrock providers
    pub async fn chat_stream(
        &self,
        request: ChatRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send>>> {
        use futures::StreamExt;

        match &self.inner {
            InnerClient::Anthropic(client) => {
                let stream = client
                    .chat_stream(&request)
                    .await
                    .map_err(|e| crate::error::Error::Other(e.to_string()))?;

                // Wrap the stream to convert each item from anyhow::Result to crate::error::Result
                let mapped =
                    stream.map(|item| item.map_err(|e| crate::error::Error::Other(e.to_string())));
                Ok(Box::pin(mapped))
            }
            InnerClient::Bedrock(client) => {
                let stream = client
                    .chat_stream(&request)
                    .await
                    .map_err(|e| crate::error::Error::Other(e.to_string()))?;

                let mapped =
                    stream.map(|item| item.map_err(|e| crate::error::Error::Other(e.to_string())));
                Ok(Box::pin(mapped))
            }
        }
    }

    /// Get the underlying config (for compatibility with existing code)
    pub fn config(&self) -> &AIConfig {
        &self.config
    }

    /// Create a chat request builder (for compatibility with AIClient)
    pub fn create_chat_request(&self) -> ChatRequestBuilder {
        ChatRequestBuilder::new(self.config.default_model.clone())
    }

    /// Count tokens for a message request
    /// Uses Anthropic's /v1/messages/count_tokens endpoint
    /// Note: Only available for first-party Anthropic, not Bedrock
    pub async fn count_tokens(
        &self,
        request: crate::auth::client::CountTokensRequest,
    ) -> Result<crate::auth::client::CountTokensResponse> {
        match &self.inner {
            InnerClient::Anthropic(client) => client
                .count_tokens(&request)
                .await
                .map_err(|e| crate::error::Error::Other(e.to_string())),
            InnerClient::Bedrock(_) => Err(crate::error::Error::Other(
                "Token counting is not supported with AWS Bedrock provider".to_string(),
            )),
        }
    }
}

// Re-export the types that are used in the AI module
pub use crate::ai::client::{
    ContentDelta as AIContentDelta, MessageDelta as AIMessageDelta, StreamEvent as AIStreamEvent,
};
