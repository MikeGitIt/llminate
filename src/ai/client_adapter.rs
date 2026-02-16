// Adapter to use the robust auth::client::AnthropicClient in place of AIClient
// This provides a safe migration path without breaking existing functionality

use crate::ai::{AIConfig, ChatRequest, ChatResponse};
use crate::ai::client::{StreamEvent, ContentDelta, MessageDelta, ChatRequestBuilder};
use crate::auth::client::{AnthropicClient, ClientConfig};
use crate::error::Result;
use std::sync::Arc;
use futures::Stream;

/// Create an AnthropicClient from AIConfig for drop-in replacement
pub fn create_anthropic_from_ai_config(config: AIConfig) -> Result<Arc<AnthropicClient>> {
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
        client_config.api_key = Some(config.api_key);
    }
    if let Some(auth_token) = config.auth_token {
        client_config.auth_token = Some(auth_token);
    }

    // Transfer base URL
    if !config.base_url.is_empty() {
        client_config.base_url = config.base_url;
    }

    // Transfer timeout
    client_config.timeout = std::time::Duration::from_secs(config.timeout_secs);

    // Transfer retry settings
    client_config.max_retries = config.max_retries.unwrap_or(2);

    // Transfer browser settings
    client_config.dangerously_allow_browser = config.dangerously_allow_browser.unwrap_or(false);

    // Transfer log level
    if let Some(log_level) = config.log_level {
        client_config.log_level = log_level;
    }

    // Create the client - convert from anyhow::Result to crate::error::Result
    let client = AnthropicClient::new(client_config)
        .map_err(|e| crate::error::Error::Other(e.to_string()))?;
    Ok(Arc::new(client))
}

/// Wrapper that makes AnthropicClient compatible with AIClient interface
pub struct AIClientAdapter {
    inner: Arc<AnthropicClient>,
    config: AIConfig,  // Keep original config for compatibility
}

impl AIClientAdapter {
    pub fn new(config: AIConfig) -> Result<Self> {
        let inner = create_anthropic_from_ai_config(config.clone())?;
        Ok(Self { inner, config })
    }

    /// Send a chat completion request
    pub async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        // Convert from anyhow::Result to crate::error::Result
        self.inner.chat(&request).await
            .map_err(|e| crate::error::Error::Other(e.to_string()))
    }

    /// Send a streaming chat completion request
    pub async fn chat_stream(
        &self,
        request: ChatRequest,
    ) -> Result<impl Stream<Item = Result<StreamEvent>> + Send> {
        // Convert the stream result from anyhow::Result to crate::error::Result
        use futures::StreamExt;

        let stream = self.inner.chat_stream(&request).await
            .map_err(|e| crate::error::Error::Other(e.to_string()))?;

        // Wrap the stream to convert each item from anyhow::Result to crate::error::Result
        Ok(stream.map(|item| {
            item.map_err(|e| crate::error::Error::Other(e.to_string()))
        }))
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
    pub async fn count_tokens(
        &self,
        request: crate::auth::client::CountTokensRequest,
    ) -> Result<crate::auth::client::CountTokensResponse> {
        self.inner.count_tokens(&request).await
            .map_err(|e| crate::error::Error::Other(e.to_string()))
    }
}

// Re-export the types that are used in the AI module
pub use crate::ai::client::{StreamEvent as AIStreamEvent, ContentDelta as AIContentDelta, MessageDelta as AIMessageDelta};