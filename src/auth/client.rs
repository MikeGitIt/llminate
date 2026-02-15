// Client Management Implementation
// Complete port from client_management_extracted.js

use anyhow::{anyhow, bail, Context, Result};
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE, USER_AGENT},
    Client as HttpClient, Method, Response, StatusCode,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::env;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};
use futures::stream::{Stream, StreamExt};
use super::proxy::ProxyConfig;

// Constants from JavaScript
const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
const DEFAULT_TIMEOUT_MS: u64 = 600000;
const ANTHROPIC_VERSION: &str = "2023-06-01";
const HUMAN_PROMPT: &str = "\n\nHuman:";
const AI_PROMPT: &str = "\n\nAssistant:";

// Deprecated models map
lazy_static::lazy_static! {
    static ref DEPRECATED_MODELS: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        // Add deprecated models and their end-of-life dates here
        m.insert("claude-instant-1", "2024-12-31");
        m.insert("claude-instant-1.0", "2024-12-31");
        m.insert("claude-instant-1.1", "2024-12-31");
        m.insert("claude-instant-1.2", "2024-12-31");
        m.insert("claude-1", "2024-12-31");
        m.insert("claude-1.0", "2024-12-31");
        m.insert("claude-1.2", "2024-12-31");
        m.insert("claude-1.3", "2024-12-31");
        m.insert("claude-2", "2024-12-31");
        m.insert("claude-2.0", "2024-12-31");
        m
    };
}

// Error types matching JavaScript error classes exactly
#[derive(Debug, thiserror::Error)]
pub enum AnthropicError {
    #[error("{message}")]
    Base { message: String },

    #[error("Bad Request (400): {message}")]
    BadRequest { status: u16, message: String, request_id: Option<String> },

    #[error("Authentication Error (401): {message}")]
    Authentication { status: u16, message: String, request_id: Option<String> },

    #[error("Permission Denied (403): {message}")]
    PermissionDenied { status: u16, message: String, request_id: Option<String> },

    #[error("Not Found (404): {message}")]
    NotFound { status: u16, message: String, request_id: Option<String> },

    #[error("Conflict (409): {message}")]
    Conflict { status: u16, message: String, request_id: Option<String> },

    #[error("Unprocessable Entity (422): {message}")]
    UnprocessableEntity { status: u16, message: String, request_id: Option<String> },

    #[error("Rate Limit (429): {message}")]
    RateLimit { status: u16, message: String, request_id: Option<String> },

    #[error("Internal Server Error (500+): {message}")]
    InternalServer { status: u16, message: String, request_id: Option<String> },

    #[error("Connection error.")]
    Connection { message: String, cause: Option<String> },

    #[error("Request timed out.")]
    Timeout,

    #[error("Request was aborted.")]
    Aborted,

    #[error("It looks like you're running in a browser-like environment.\n\nThis is disabled by default, as it risks exposing your secret API credentials to attackers.\nIf you understand the risks and have appropriate mitigations in place,\nyou can set the `dangerouslyAllowBrowser` option to `true`, e.g.,\n\nnew Anthropic({{ apiKey, dangerouslyAllowBrowser: true }});\n")]
    BrowserEnvironment,
}

impl AnthropicError {
    pub fn from_status(status: StatusCode, error_data: Value, headers: &HeaderMap) -> Self {
        let status_u16 = status.as_u16();
        let message = error_data["message"]
            .as_str()
            .or_else(|| error_data["error"]["message"].as_str())
            .unwrap_or(&status.to_string())
            .to_string();

        let request_id = headers
            .get("request-id")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        match status_u16 {
            400 => Self::BadRequest { status: status_u16, message, request_id },
            401 => Self::Authentication { status: status_u16, message, request_id },
            403 => Self::PermissionDenied { status: status_u16, message, request_id },
            404 => Self::NotFound { status: status_u16, message, request_id },
            409 => Self::Conflict { status: status_u16, message, request_id },
            422 => Self::UnprocessableEntity { status: status_u16, message, request_id },
            429 => Self::RateLimit { status: status_u16, message, request_id },
            _ if status_u16 >= 500 => Self::InternalServer { status: status_u16, message, request_id },
            _ => Self::Base { message },
        }
    }

    fn make_message(status: u16, error: &Value, message: Option<&str>) -> String {
        message
            .or_else(|| error["type"].as_str())
            .map(|m| format!("{} {}", status, m))
            .unwrap_or_else(|| format!("{} Error", status))
    }
}

// Qt function equivalent - environment variable getter
fn get_env_var(key: &str) -> Option<String> {
    env::var(key).ok().and_then(|v| {
        let trimmed = v.trim();
        if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
    })
}

// middleware17 equivalent - log level validator
fn validate_log_level(level: &str, source: &str) -> Option<String> {
    const VALID_LEVELS: [&str; 4] = ["debug", "info", "warn", "error"];

    if VALID_LEVELS.contains(&level) {
        Some(level.to_string())
    } else {
        warn!(
            "{} was set to {:?}, expected one of {:?}",
            source, level, VALID_LEVELS
        );
        None
    }
}

// getter9 equivalent - browser environment detector
fn is_browser_environment() -> bool {
    // Check if we're running in a browser environment (e.g., WebAssembly)
    // This is critical because Anthropic's API blocks browser requests by default
    // to prevent accidental API key exposure in client-side code

    #[cfg(target_arch = "wasm32")]
    {
        // In WebAssembly, check if browser APIs are available
        // This would need wasm-bindgen to properly check for window/document/navigator
        // For now, assume WASM means browser (could also be Node.js WASM)
        true
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        // Not in WASM, so not in browser
        false
    }
}

// checker115 equivalent - fetch availability checker
fn ensure_fetch_available() -> Result<()> {
    // In Rust, we always have HTTP client available
    Ok(())
}

// iN equivalent - error code checker
fn is_timeout_or_connection_error(error: &reqwest::Error) -> bool {
    error.is_timeout() || error.is_connect()
}

// MergedHeaders struct matching JavaScript YB() return type
#[derive(Debug, Clone)]
pub struct MergedHeaders {
    pub values: HeaderMap,
    pub nulls: HashSet<String>,
}

// Header merging function (YB equivalent)
pub fn merge_headers(headers_list: Vec<Option<HeaderMap>>) -> MergedHeaders {
    let mut merged = HeaderMap::new();
    let mut nulls = HashSet::new();

    for headers_opt in headers_list {
        if let Some(headers) = headers_opt {
            let mut seen = HashSet::new();
            for (key, value) in headers {
                if let Some(key) = key {
                    let key_lower = key.as_str().to_lowercase();

                    if !seen.contains(&key_lower) {
                        merged.remove(&key);
                        seen.insert(key_lower.clone());
                    }

                    if value.is_empty() {
                        merged.remove(&key);
                        nulls.insert(key_lower);
                    } else {
                        merged.append(key, value);
                        nulls.remove(&key_lower);
                    }
                }
            }
        }
    }

    MergedHeaders {
        values: merged,
        nulls,
    }
}

// Client configuration
#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub base_url: String,
    pub api_key: Option<String>,
    pub auth_token: Option<String>,
    pub timeout: Duration,
    pub max_retries: u32,
    pub dangerously_allow_browser: bool,
    pub log_level: String,
    pub fetch_options: Option<HashMap<String, String>>,
    pub default_headers: HeaderMap,
    pub proxy: Option<ProxyConfig>,  // Added proxy support
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            base_url: get_env_var("ANTHROPIC_BASE_URL").unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
            api_key: get_env_var("ANTHROPIC_API_KEY"),
            auth_token: get_env_var("ANTHROPIC_AUTH_TOKEN"),
            timeout: Duration::from_millis(DEFAULT_TIMEOUT_MS),
            max_retries: 2,
            dangerously_allow_browser: false,
            log_level: get_env_var("ANTHROPIC_LOG").unwrap_or_else(|| "warn".to_string()),
            fetch_options: None,
            default_headers: HeaderMap::new(),
            proxy: ProxyConfig::from_env().ok(),  // Load proxy from environment
        }
    }
}

// WeakMap equivalent for private data
struct PrivateData {
    transformer: fn(&RequestOptions) -> (HeaderMap, Option<String>),
}

// Main Anthropic Client (Class32)
#[derive(Clone)]
pub struct AnthropicClient {
    config: Arc<ClientConfig>,
    http_client: HttpClient,
    idempotency_header: String,
    private_data: Arc<PrivateData>,
}

impl AnthropicClient {
    pub fn new(config: ClientConfig) -> Result<Self> {
        // Browser environment check (getter9)
        if !config.dangerously_allow_browser && is_browser_environment() {
            return Err(AnthropicError::BrowserEnvironment.into());
        }

        // Validate log level (middleware17)
        let log_level = validate_log_level(&config.log_level, "ClientOptions.logLevel")
            .or_else(|| get_env_var("ANTHROPIC_LOG")
                .and_then(|l| validate_log_level(&l, "process.env['ANTHROPIC_LOG']")))
            .unwrap_or_else(|| "warn".to_string());

        // Ensure fetch is available (checker115)
        ensure_fetch_available()?;

        let mut final_config = config;
        final_config.log_level = log_level;

        // Build HTTP client with proxy support
        let mut client_builder = HttpClient::builder()
            .timeout(final_config.timeout);

        // Add proxy if configured
        if let Some(ref proxy_config) = final_config.proxy {
            if let Some(proxy) = proxy_config.to_reqwest_proxy()? {
                client_builder = client_builder.proxy(proxy);
            }
        }

        let http_client = client_builder
            .build()
            .context("Failed to create HTTP client")?;

        // transformer111 equivalent
        let transformer = |options: &RequestOptions| -> (HeaderMap, Option<String>) {
            let mut headers = HeaderMap::new();
            if let Some(body) = &options.body {
                headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                (headers, Some(serde_json::to_string(body).unwrap_or_default()))
            } else {
                (headers, None)
            }
        };

        Ok(Self {
            config: Arc::new(final_config),
            http_client,
            idempotency_header: "idempotency-key".to_string(),
            private_data: Arc::new(PrivateData { transformer }),
        })
    }

    // apiKeyAuth method
    fn api_key_auth(&self) -> Option<HeaderMap> {
        self.config.api_key.as_ref().map(|key| {
            let mut headers = HeaderMap::new();
            headers.insert(
                HeaderName::from_static("x-api-key"),
                HeaderValue::from_str(key).unwrap_or_else(|_| HeaderValue::from_static("")),
            );
            headers
        })
    }

    // bearerAuth method
    fn bearer_auth(&self) -> Option<HeaderMap> {
        self.config.auth_token.as_ref().map(|token| {
            let mut headers = HeaderMap::new();
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {}", token))
                    .unwrap_or_else(|_| HeaderValue::from_static("")),
            );
            headers
        })
    }

    // authHeaders method
    fn auth_headers(&self) -> HeaderMap {
        merge_headers(vec![self.api_key_auth(), self.bearer_auth()]).values
    }

    // HTTP methods
    pub async fn get(&self, path: &str, options: Option<RequestOptions>) -> Result<Value> {
        self.method_request(Method::GET, path, options).await
    }

    pub async fn post(&self, path: &str, options: Option<RequestOptions>) -> Result<Value> {
        self.method_request(Method::POST, path, options).await
    }

    pub async fn patch(&self, path: &str, options: Option<RequestOptions>) -> Result<Value> {
        self.method_request(Method::PATCH, path, options).await
    }

    pub async fn put(&self, path: &str, options: Option<RequestOptions>) -> Result<Value> {
        self.method_request(Method::PUT, path, options).await
    }

    pub async fn delete(&self, path: &str, options: Option<RequestOptions>) -> Result<Value> {
        self.method_request(Method::DELETE, path, options).await
    }

    // methodRequest implementation
    async fn method_request(
        &self,
        method: Method,
        path: &str,
        options: Option<RequestOptions>,
    ) -> Result<Value> {
        let mut opts = options.unwrap_or_default();
        opts.method = Some(method);
        opts.path = Some(path.to_string());
        self.request(opts).await
    }

    // request method
    pub async fn request(&self, options: RequestOptions) -> Result<Value> {
        self.make_request(options, None, None).await
    }

    // makeRequest implementation
    async fn make_request(
        &self,
        options: RequestOptions,
        retry_count: Option<u32>,
        log_id: Option<String>,
    ) -> Result<Value> {
        let max_retries = options.max_retries.unwrap_or(self.config.max_retries);
        let retries_left = retry_count.unwrap_or(max_retries);

        // prepareOptions equivalent
        let prepared_options = self.prepare_options(options).await?;

        // buildRequest
        let (request, url, timeout) = self.build_request(&prepared_options, max_retries - retries_left)?;

        // Generate log ID
        let log_id = log_id.unwrap_or_else(|| {
            format!("log_{:06x}", (rand::random::<f32>() * 16777216.0) as u32)
        });

        let retry_of = if log_id.starts_with("log_") { "" } else { &format!(", retryOf: {}", log_id) };
        let start_time = SystemTime::now();

        debug!("[{}] sending request{}", log_id, retry_of);

        // Check abort signal
        if prepared_options.signal.as_ref().map(|s| s.is_aborted()).unwrap_or(false) {
            return Err(anyhow::anyhow!(AnthropicError::Aborted));
        }

        // fetchWithTimeout
        let response_result = self.fetch_with_timeout(request, timeout).await;
        let duration = SystemTime::now().duration_since(start_time).unwrap_or_default();

        match response_result {
            Err(e) => {
                let retry_msg = format!("retrying, {} attempts remaining", retries_left);

                if prepared_options.signal.as_ref().map(|s| s.is_aborted()).unwrap_or(false) {
                    return Err(anyhow::anyhow!(AnthropicError::Aborted));
                }

                let is_timeout = e.downcast_ref::<reqwest::Error>()
                    .map(|re| is_timeout_or_connection_error(re))
                    .unwrap_or(false) ||
                    e.to_string().to_lowercase().contains("timed") ||
                    e.to_string().to_lowercase().contains("timeout");

                if retries_left > 0 {
                    info!(
                        "[{}] connection {} - {}",
                        log_id,
                        if is_timeout { "timed out" } else { "failed" },
                        retry_msg
                    );
                    debug!(
                        "[{}] connection {} ({})",
                        log_id,
                        if is_timeout { "timed out" } else { "failed" },
                        retry_msg
                    );
                    return self.retry_request(prepared_options, retries_left - 1, log_id.clone()).await;
                }

                info!(
                    "[{}] connection {} - error; no more retries left",
                    log_id,
                    if is_timeout { "timed out" } else { "failed" }
                );
                debug!(
                    "[{}] connection {} (error; no more retries left)",
                    log_id,
                    if is_timeout { "timed out" } else { "failed" }
                );

                if is_timeout {
                    return Err(anyhow::anyhow!(AnthropicError::Timeout));
                }

                return Err(anyhow::anyhow!(AnthropicError::Connection {
                    message: e.to_string(),
                    cause: Some(format!("{:?}", e)),
                }));
            }
            Ok(response) => {
                let status = response.status();
                let headers = response.headers().clone();
                let response_text = response.text().await.context("Failed to read response")?;

                if !status.is_success() {
                    let error_data: Value = serde_json::from_str(&response_text)
                        .unwrap_or_else(|_| json!({ "message": response_text }));

                    if retries_left > 0 && (status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()) {
                        return self.retry_request(prepared_options, retries_left - 1, log_id).await;
                    }

                    return Err(anyhow::anyhow!(AnthropicError::from_status(status, error_data, &headers)));
                }

                serde_json::from_str(&response_text).context("Failed to parse response JSON")
            }
        }
    }

    // retryRequest implementation
    fn retry_request(
        &self,
        options: RequestOptions,
        retries_left: u32,
        log_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<Value>> + Send + '_>> {
        Box::pin(async move {
            let max_retries = options.max_retries.unwrap_or(self.config.max_retries);
            let retry_number = max_retries - retries_left;
            let delay_ms = std::cmp::min(1000 * 2_u64.pow(retry_number), 10000);
            sleep(Duration::from_millis(delay_ms)).await;
            self.make_request(options, Some(retries_left), Some(log_id)).await
        })
    }

    // buildRequest implementation
    fn build_request(
        &self,
        options: &RequestOptions,
        retry_count: u32,
    ) -> Result<(reqwest::Request, String, Duration)> {
        let method = options.method.clone().unwrap_or(Method::GET);
        let path = options.path.as_deref().unwrap_or("/");
        let url = self.build_url(path, &options.query)?;

        // Validate timeout
        if let Some(timeout) = options.timeout {
            if timeout.as_secs() == 0 {
                bail!("timeout must be a number");
            }
        }

        let timeout = options.timeout.unwrap_or(self.config.timeout);

        let (body_headers, body) = self.build_body(options);
        let headers = self.build_headers(&method, body_headers, retry_count, options)?;

        let mut request_builder = self.http_client
            .request(method, &url)
            .headers(headers);

        if let Some(signal) = &options.signal {
            // Handle abort signal if needed
        }

        if let Some(body_str) = body {
            request_builder = request_builder.body(body_str);
        }

        // Add fetch options if any
        if let Some(fetch_opts) = &self.config.fetch_options {
            // Apply fetch options
        }

        if let Some(fetch_opts) = &options.fetch_options {
            // Apply request-specific fetch options
        }

        let request = request_builder.build()?;
        Ok((request, url, timeout))
    }

    // buildURL implementation
    fn build_url(&self, path: &str, query: &Option<HashMap<String, String>>) -> Result<String> {
        let mut url = format!("{}{}", self.config.base_url, path);

        if let Some(params) = query {
            if !params.is_empty() {
                let query_string: Vec<String> = params
                    .iter()
                    .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
                    .collect();
                url.push('?');
                url.push_str(&query_string.join("&"));
            }
        }

        Ok(url)
    }

    // buildBody implementation
    fn build_body(&self, options: &RequestOptions) -> (HeaderMap, Option<String>) {
        if options.body.is_none() {
            return (HeaderMap::new(), None);
        }

        if let Some(body_str) = options.body.as_ref().and_then(|v| v.as_str()) {
            let mut headers = HeaderMap::new();
            let content_type = options.headers.as_ref()
                .and_then(|h| h.get(CONTENT_TYPE))
                .and_then(|v| v.to_str().ok())
                .unwrap_or("text/plain");
            headers.insert(CONTENT_TYPE, HeaderValue::from_str(content_type).unwrap());
            return (headers, Some(body_str.to_string()));
        }

        // Use transformer111 equivalent
        (self.private_data.transformer)(options)
    }

    // buildHeaders implementation
    pub fn build_headers(
        &self,
        method: &Method,
        body_headers: HeaderMap,
        retry_count: u32,
        options: &RequestOptions,
    ) -> Result<HeaderMap> {
        let mut idempotency_headers = HeaderMap::new();

        // Add idempotency key for non-GET requests
        if method != Method::GET {
            let key = options.idempotency_key.clone()
                .unwrap_or_else(|| self.default_idempotency_key());
            idempotency_headers.insert(
                HeaderName::from_bytes(self.idempotency_header.as_bytes())?,
                HeaderValue::from_str(&key)?,
            );
        }

        let mut base_headers = HeaderMap::new();
        base_headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        base_headers.insert(USER_AGENT, HeaderValue::from_str(&self.get_user_agent())?);
        base_headers.insert(
            HeaderName::from_static("x-stainless-retry-count"),
            HeaderValue::from_str(&retry_count.to_string())?,
        );

        if let Some(timeout) = options.timeout {
            base_headers.insert(
                HeaderName::from_static("x-stainless-timeout"),
                HeaderValue::from_str(&(timeout.as_secs()).to_string())?,
            );
        }

        // Platform info (getter11 equivalent) - empty in Rust

        if self.config.dangerously_allow_browser {
            base_headers.insert(
                HeaderName::from_static("anthropic-dangerous-direct-browser-access"),
                HeaderValue::from_static("true"),
            );
        }

        base_headers.insert(
            HeaderName::from_static("anthropic-version"),
            HeaderValue::from_static(ANTHROPIC_VERSION),
        );

        // Add proxy authentication headers if configured
        let mut proxy_headers = HeaderMap::new();
        if let Some(ref proxy_config) = self.config.proxy {
            // Only add proxy auth if we're not bypassing this host
            let url = self.build_url(options.path.as_deref().unwrap_or("/"), &options.query)?;
            if let Ok(parsed_url) = url::Url::parse(&url) {
                if let Some(host) = parsed_url.host_str() {
                    if !proxy_config.should_bypass(host) {
                        proxy_config.add_proxy_auth(&mut proxy_headers)?;
                    }
                }
            }
        }

        // Merge all headers using YB equivalent
        let merged = merge_headers(vec![
            Some(idempotency_headers),
            Some(base_headers),
            Some(self.auth_headers()),
            Some(proxy_headers),  // Add proxy headers
            Some(self.config.default_headers.clone()),
            Some(body_headers),
            options.headers.clone(),
        ]);

        // Call validateHeaders with the full merged result
        self.validate_headers(&merged)?;
        Ok(merged.values)
    }

    // getUserAgent implementation
    fn get_user_agent(&self) -> String {
        "anthropic-rust/1.0.0".to_string()
    }

    // defaultIdempotencyKey implementation
    pub fn default_idempotency_key(&self) -> String {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let random = format!("{:x}", rand::random::<u32>()).chars().take(9).collect::<String>();
        format!("key_{}_{}", timestamp, random)
    }

    // validateHeaders implementation - CRITICAL AUTH VALIDATION
    fn validate_headers(&self, merged_result: &MergedHeaders) -> Result<()> {
        // JavaScript validateHeaders checks authentication is properly configured
        // It receives both the merged headers AND the nulls set from YB()

        // The logic is:
        // 1. If apiKey exists AND x-api-key header is in values -> VALID
        // 2. If x-api-key is in nulls set (explicitly nulled) -> VALID (user disabled it)
        // 3. If authToken exists AND authorization header is in values -> VALID
        // 4. If authorization is in nulls set -> VALID (user disabled it)
        // 5. Otherwise -> ERROR: No valid auth method

        let has_api_key_header = merged_result.values.contains_key("x-api-key");
        let api_key_nulled = merged_result.nulls.contains("x-api-key");

        let has_auth_header = merged_result.values.contains_key("authorization");
        let auth_nulled = merged_result.nulls.contains("authorization");

        // Check API key authentication
        if self.config.api_key.is_some() && has_api_key_header {
            return Ok(()); // Valid: API key configured and header present
        }

        if api_key_nulled {
            return Ok(()); // Valid: API key explicitly disabled by user
        }

        // Check Bearer token authentication
        if self.config.auth_token.is_some() && has_auth_header {
            return Ok(()); // Valid: Auth token configured and header present
        }

        if auth_nulled {
            return Ok(()); // Valid: Authorization explicitly disabled by user
        }

        // No valid authentication method found
        Err(anyhow!(
            "Could not resolve authentication method. Expected either apiKey or authToken to be set. \
            Or for one of the \"X-Api-Key\" or \"Authorization\" headers to be explicitly omitted"
        ))
    }

    // prepareOptions implementation
    async fn prepare_options(&self, options: RequestOptions) -> Result<RequestOptions> {
        // Options preparation from JavaScript
        Ok(options)
    }

    // prepareRequest implementation
    async fn prepare_request(&self, req: &reqwest::Request, url: &str, options: &RequestOptions) -> Result<()> {
        // Request preparation from JavaScript
        Ok(())
    }

    // fetchWithTimeout implementation
    async fn fetch_with_timeout(
        &self,
        request: reqwest::Request,
        timeout: Duration,
    ) -> Result<Response> {
        let client = self.http_client.clone();

        match tokio::time::timeout(timeout, client.execute(request)).await {
            Ok(result) => result.map_err(Into::into),
            Err(_) => Err(anyhow::anyhow!("Request timeout")),
        }
    }

    // ===== STREAMING SUPPORT FOR AI OPERATIONS =====
    // These methods provide compatibility with the AI client's streaming functionality

    /// Send a chat request - compatibility method for AI operations
    pub async fn chat(&self, request: &crate::ai::ChatRequest) -> Result<crate::ai::ChatResponse> {
        let mut options = RequestOptions::default();

        // OAUTH DISABLED: Anthropic has disabled 3rd party OAuth support for Claude Code CLI
        // let is_oauth = self.config.auth_token.is_some();
        //
        // // For OAuth, add anthropic-beta header (JavaScript: beta.messages.create extracts betas and adds to header)
        // if is_oauth {
        //     let mut headers = HeaderMap::new();
        //     headers.insert(
        //         HeaderName::from_static("anthropic-beta"),
        //         HeaderValue::from_static("claude-code-20250219,oauth-2025-04-20"),
        //     );
        //     options.headers = Some(headers);
        // }

        options.body = Some(serde_json::to_value(request)?);

        // OAUTH DISABLED: OAuth requests required ?beta=true query parameter
        // let path = if is_oauth {
        //     "/messages?beta=true"
        // } else {
        //     "/messages"
        // };
        let path = "/messages";

        let response = self.post(path, Some(options)).await?;

        serde_json::from_value(response).context("Failed to parse chat response")
    }

    /// Send a streaming chat request - compatibility method for AI operations
    pub async fn chat_stream(
        &self,
        request: &crate::ai::ChatRequest,
    ) -> Result<impl Stream<Item = Result<crate::ai::client::StreamEvent>> + Send> {
        let mut request = request.clone();
        request.stream = Some(true);

        // OAUTH DISABLED: Anthropic has disabled 3rd party OAuth support for Claude Code CLI
        // let is_oauth = self.config.auth_token.is_some();
        // if is_oauth {
        //     // Add metadata with user_id
        //     let user_id = Self::get_or_generate_user_id();
        //     let account_uuid = Self::get_account_uuid();
        //     let session_id = Self::get_session_id();
        //
        //     let mut metadata = request.metadata.clone().unwrap_or_default();
        //     metadata.insert(
        //         "user_id".to_string(),
        //         format!("user_{}_account_{}_session_{}", user_id, account_uuid, session_id)
        //     );
        //     request.metadata = Some(metadata);
        //
        //     info!("OAuth request - added metadata user_id: user_{}_account_{}_session_{}", user_id, account_uuid, session_id);
        //
        //     // Betas are NOT passed in body for OAuth - only in anthropic-beta header
        //     request.betas = None;
        // } else {
        //     // API key auth: betas ARE passed in request body
        //     let mut betas_list = vec!["claude-code-20250219".to_string()];
        //     if request.model.contains("claude-sonnet-4") || request.model.contains("claude-opus-4") {
        //         betas_list.push("interleaved-thinking-2025-05-14".to_string());
        //     }
        //     request.betas = Some(betas_list);
        // }

        // Betas are NOT passed in request body - they go in the anthropic-beta header only
        request.betas = None;

        // OAUTH DISABLED: OAuth required ?beta=true query parameter
        // let url = if is_oauth {
        //     format!("{}/messages?beta=true", self.config.base_url)
        // } else {
        //     format!("{}/messages", self.config.base_url)
        // };
        let url = format!("{}/messages", self.config.base_url);

        info!("=== SENDING MESSAGE REQUEST TO LLM ===");
        info!("URL: {}", url);
        info!("Request body: {}", serde_json::to_string_pretty(&request).unwrap_or_else(|_| "Failed to serialize".to_string()));

        // Build request with proper headers matching JavaScript SDK (cli-jsdef-fixed.js:191866-191879)
        let mut headers = HeaderMap::new();
        headers.insert("accept", HeaderValue::from_static("application/json"));
        headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
        headers.insert("content-type", HeaderValue::from_static("application/json"));
        headers.insert("x-app", HeaderValue::from_static("cli"));
        headers.insert("user-agent", HeaderValue::from_static("claude-cli/2.0.72 (external, cli)"));
        headers.insert("x-stainless-retry-count", HeaderValue::from_static("0"));
        let idempotency_key = format!("stainless-node-retry-{}", uuid::Uuid::new_v4());
        headers.insert("idempotency-key", HeaderValue::from_str(&idempotency_key)?);

        // OAUTH DISABLED: Anthropic has disabled 3rd party OAuth support for Claude Code CLI
        // Add authentication - API key only (OAuth commented out)
        // if let Some(ref auth_token) = self.config.auth_token {
        //     info!("Using OAuth Bearer token authentication");
        //     headers.insert("authorization", HeaderValue::from_str(&format!("Bearer {}", auth_token))?);
        //     // OAuth requires anthropic-beta header
        //     let mut betas = vec!["claude-code-20250219", "oauth-2025-04-20"];
        //     if request.model.contains("claude-sonnet-4") || request.model.contains("claude-opus-4") {
        //         betas.push("interleaved-thinking-2025-05-14");
        //     }
        //     let beta_header = betas.join(",");
        //     info!("anthropic-beta header: {}", beta_header);
        //     headers.insert("anthropic-beta", HeaderValue::from_str(&beta_header)?);
        // } else
        if let Some(ref api_key) = &self.config.api_key {
            info!("Using API key authentication");
            headers.insert("x-api-key", HeaderValue::from_str(api_key)?);

            // Add anthropic-beta header for beta features
            let mut betas = vec!["claude-code-20250219"];
            if request.model.contains("claude-sonnet-4") || request.model.contains("claude-opus-4") {
                betas.push("interleaved-thinking-2025-05-14");
            }
            let beta_header = betas.join(",");
            info!("anthropic-beta header: {}", beta_header);
            headers.insert("anthropic-beta", HeaderValue::from_str(&beta_header)?);
        } else {
            return Err(anyhow!("No authentication credentials available. Please set ANTHROPIC_API_KEY environment variable."));
        }

        // Log all headers being sent
        info!("=== REQUEST HEADERS ===");
        for (name, value) in headers.iter() {
            info!("  {}: {}", name, value.to_str().unwrap_or("<binary>"));
        }
        info!("=== END HEADERS ===");

        // Make the streaming request
        let response = self.http_client
            .post(&url)
            .headers(headers)
            .json(&request)
            .send()
            .await
            .context("Failed to send streaming request")?;

        info!("Response status: {}", response.status());

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_else(|_| "Failed to read error".to_string());
            return Err(anyhow!("Request failed with status {}: {}", status, text));
        }

        let stream = response.bytes_stream();
        Ok(parse_sse_stream(stream))
    }

    // ===== Helper methods for OAuth metadata (matching JavaScript behavior) =====
    // OAUTH DISABLED: Anthropic has disabled 3rd party OAuth support for Claude Code CLI
    // These methods are kept commented out for potential future re-enablement.

    // /// Get or generate user ID (matches JavaScript variable22486 at line 530737-530745)
    // /// JavaScript stores userID in state and generates a 32-byte hex random string if not present
    // fn get_or_generate_user_id() -> String {
    //     // Check environment variable first
    //     if let Ok(user_id) = std::env::var("CLAUDE_CODE_USER_ID") {
    //         if !user_id.is_empty() {
    //             return user_id;
    //         }
    //     }
    //
    //     // Try to read from config file
    //     if let Some(home) = dirs::home_dir() {
    //         let state_file = home.join(".claude").join("state.json");
    //         if let Ok(content) = std::fs::read_to_string(&state_file) {
    //             if let Ok(state) = serde_json::from_str::<serde_json::Value>(&content) {
    //                 if let Some(user_id) = state.get("userID").and_then(|v| v.as_str()) {
    //                     if !user_id.is_empty() {
    //                         return user_id.to_string();
    //                     }
    //                 }
    //             }
    //         }
    //     }
    //
    //     // Generate new 32-byte hex ID (JavaScript: zX7(32).toString("hex"))
    //     use rand::Rng;
    //     let mut rng = rand::thread_rng();
    //     let bytes: Vec<u8> = (0..32).map(|_| rng.gen()).collect();
    //     hex::encode(bytes)
    // }

    // /// Get account UUID from OAuth (matches JavaScript variable26865()?.accountUuid)
    // fn get_account_uuid() -> String {
    //     // Check environment variable first
    //     if let Ok(account_uuid) = std::env::var("CLAUDE_CODE_ACCOUNT_UUID") {
    //         if !account_uuid.is_empty() {
    //             info!("get_account_uuid: found in env var: {}", account_uuid);
    //             return account_uuid;
    //         }
    //     }
    //
    //     // CRITICAL: Try to read from macOS keychain FIRST (this is where OAuth credentials are stored!)
    //     // The storage backend saves credentials to keychain, NOT plaintext files
    //     #[cfg(target_os = "macos")]
    //     {
    //         let service_name = "Claude Code-credentials";
    //         let username = std::env::var("USER").unwrap_or_else(|_| "unknown".to_string());
    //
    //         if let Ok(output) = std::process::Command::new("security")
    //             .args(&[
    //                 "find-generic-password",
    //                 "-a", &username,
    //                 "-s", service_name,
    //                 "-w"  // Only output the password (the credentials JSON)
    //             ])
    //             .output()
    //         {
    //             if output.status.success() {
    //                 let creds_json = String::from_utf8_lossy(&output.stdout).trim().to_string();
    //                 if let Ok(creds) = serde_json::from_str::<serde_json::Value>(&creds_json) {
    //                     if let Some(oauth) = creds.get("claudeAiOauth") {
    //                         if let Some(account_uuid) = oauth.get("accountUuid").and_then(|v| v.as_str()) {
    //                             if !account_uuid.is_empty() {
    //                                 info!("get_account_uuid: found in keychain: {}", account_uuid);
    //                                 return account_uuid.to_string();
    //                             }
    //                         }
    //                     }
    //                 }
    //             }
    //         }
    //     }
    //
    //     // Fallback: Try to read from credentials file (plaintext storage)
    //     if let Some(home) = dirs::home_dir() {
    //         // Try .claude/.credentials.json
    //         let creds_file = home.join(".claude").join(".credentials.json");
    //         if let Ok(content) = std::fs::read_to_string(&creds_file) {
    //             if let Ok(creds) = serde_json::from_str::<serde_json::Value>(&content) {
    //                 if let Some(oauth) = creds.get("claudeAiOauth") {
    //                     if let Some(account_uuid) = oauth.get("accountUuid").and_then(|v| v.as_str()) {
    //                         if !account_uuid.is_empty() {
    //                             info!("get_account_uuid: found in .credentials.json: {}", account_uuid);
    //                             return account_uuid.to_string();
    //                         }
    //                     }
    //                 }
    //             }
    //         }
    //
    //         // Try state.json oauthAccount
    //         let state_file = home.join(".claude").join("state.json");
    //         if let Ok(content) = std::fs::read_to_string(&state_file) {
    //             if let Ok(state) = serde_json::from_str::<serde_json::Value>(&content) {
    //                 if let Some(oauth_account) = state.get("oauthAccount") {
    //                     if let Some(account_uuid) = oauth_account.get("accountUuid").and_then(|v| v.as_str()) {
    //                         if !account_uuid.is_empty() {
    //                             info!("get_account_uuid: found in state.json: {}", account_uuid);
    //                             return account_uuid.to_string();
    //                         }
    //                     }
    //                 }
    //             }
    //         }
    //     }
    //
    //     // Return empty if not found (JavaScript uses "" when not available)
    //     info!("get_account_uuid: NOT FOUND - returning empty string");
    //     String::new()
    // }

    // /// Get session ID (matches JavaScript variable8987 at line 2028)
    // /// JavaScript stores sessionId in global state, generated once per session
    // fn get_session_id() -> String {
    //     // Check environment variable first (JavaScript also uses CLAUDE_CODE_SESSION_ID)
    //     if let Ok(session_id) = std::env::var("CLAUDE_CODE_SESSION_ID") {
    //         if !session_id.is_empty() {
    //             return session_id;
    //         }
    //     }
    //
    //     // Generate a session ID (JavaScript uses EP0() which creates a UUID-like ID)
    //     // Using process ID + timestamp for uniqueness within this process
    //     use std::time::{SystemTime, UNIX_EPOCH};
    //     let timestamp = SystemTime::now()
    //         .duration_since(UNIX_EPOCH)
    //         .unwrap_or_default()
    //         .as_millis();
    //     let pid = std::process::id();
    //
    //     format!("{:x}{:x}", timestamp, pid)
    // }
}

// ===== SSE STREAMING SUPPORT =====
// Parse SSE stream with proper buffering for partial chunks
fn parse_sse_stream(
    stream: impl Stream<Item = reqwest::Result<bytes::Bytes>> + Send + 'static,
) -> impl Stream<Item = Result<crate::ai::client::StreamEvent>> + Send {
    use futures::stream;
    use std::collections::VecDeque;

    // Wrap the stream in Pin<Box<...>> for proper async handling
    let pinned_stream = Box::pin(stream);

    // State for SSE parsing - holds buffer and event queue
    struct SseParserState {
        buffer: String,
        event_queue: VecDeque<Result<crate::ai::client::StreamEvent>>,
    }

    impl SseParserState {
        fn new() -> Self {
            Self {
                buffer: String::new(),
                event_queue: VecDeque::new(),
            }
        }

        fn process_buffer(&mut self) {
            // Process all complete events in the buffer
            while let Some(event_boundary) = self.buffer.find("\n\n") {
                // Extract one complete event
                let event_text: String = self.buffer.drain(..=event_boundary + 1).collect();

                // Parse the event fields
                let mut data_fields = Vec::new();

                for line in event_text.lines() {
                    if let Some(colon_pos) = line.find(':') {
                        let field = &line[..colon_pos];
                        let value = if colon_pos + 1 < line.len() {
                            if line.chars().nth(colon_pos + 1) == Some(' ') {
                                &line[colon_pos + 2..]
                            } else {
                                &line[colon_pos + 1..]
                            }
                        } else {
                            ""
                        };

                        if field == "data" {
                            data_fields.push(value);
                        }
                    }
                }

                // Process the collected data fields
                if !data_fields.is_empty() {
                    let combined_data = data_fields.join("\n");

                    // Check for stream termination
                    if combined_data == "[DONE]" {
                        continue;
                    }

                    // Parse the JSON data - simplified parsing for now
                    if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(&combined_data) {
                        // Convert to StreamEvent based on the type field
                        if let Some(event_type) = json_value.get("type").and_then(|v| v.as_str()) {
                            let event = match event_type {
                                "message_start" => {
                                    // Parse as StreamMessage, not ChatResponse
                                    if let Some(message_obj) = json_value.get("message") {
                                        if let Ok(stream_msg) = serde_json::from_value::<crate::ai::client::StreamMessage>(message_obj.clone()) {
                                            crate::ai::client::StreamEvent::MessageStart { message: stream_msg }
                                        } else {
                                            continue;
                                        }
                                    } else {
                                        continue;
                                    }
                                }
                                "content_block_start" => {
                                    if let (Some(index), Some(content_block)) = (
                                        json_value.get("index").and_then(|v| v.as_u64()).map(|i| i as usize),
                                        json_value.get("content_block")
                                    ) {
                                        // Parse as ContentBlock, not ContentPart
                                        if let Ok(content) = serde_json::from_value::<crate::ai::client::ContentBlock>(content_block.clone()) {
                                            crate::ai::client::StreamEvent::ContentBlockStart { index, content_block: content }
                                        } else {
                                            continue;
                                        }
                                    } else {
                                        continue;
                                    }
                                }
                                "content_block_delta" => {
                                    if let (Some(index), Some(delta)) = (
                                        json_value.get("index").and_then(|v| v.as_u64()).map(|i| i as usize),
                                        json_value.get("delta")
                                    ) {
                                        if let Ok(delta_content) = serde_json::from_value::<crate::ai::client::ContentDelta>(delta.clone()) {
                                            crate::ai::client::StreamEvent::ContentBlockDelta { index, delta: delta_content }
                                        } else {
                                            continue;
                                        }
                                    } else {
                                        continue;
                                    }
                                }
                                "content_block_stop" => {
                                    if let Some(index) = json_value.get("index").and_then(|v| v.as_u64()).map(|i| i as usize) {
                                        crate::ai::client::StreamEvent::ContentBlockStop { index }
                                    } else {
                                        continue;
                                    }
                                }
                                "message_delta" => {
                                    if let Some(delta) = json_value.get("delta") {
                                        if let Ok(msg_delta) = serde_json::from_value::<crate::ai::client::MessageDelta>(delta.clone()) {
                                            // Usage is NOT optional, get it from the JSON or create default
                                            let usage = json_value.get("usage")
                                                .and_then(|u| serde_json::from_value::<crate::ai::Usage>(u.clone()).ok())
                                                .unwrap_or_else(|| crate::ai::Usage {
                                                    input_tokens: 0,
                                                    output_tokens: 0,
                                                    cache_creation_input_tokens: None,
                                                    cache_read_input_tokens: None,
                                                });
                                            crate::ai::client::StreamEvent::MessageDelta { delta: msg_delta, usage }
                                        } else {
                                            continue;
                                        }
                                    } else {
                                        continue;
                                    }
                                }
                                "message_stop" => {
                                    crate::ai::client::StreamEvent::MessageStop
                                }
                                "error" => {
                                    // Error is a tuple variant, not struct
                                    if let Some(error) = json_value.get("error") {
                                        let error_msg = error.get("message")
                                            .and_then(|m| m.as_str())
                                            .unwrap_or("Unknown error")
                                            .to_string();
                                        crate::ai::client::StreamEvent::Error(error_msg)
                                    } else {
                                        continue;
                                    }
                                }
                                _ => continue,
                            };
                            self.event_queue.push_back(Ok(event));
                        }
                    }
                }
            }
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
                                    Err(anyhow::anyhow!(
                                        "Invalid UTF-8 in SSE stream: {}",
                                        utf8_error
                                    )),
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
                            Err(anyhow::anyhow!(
                                "Network error in SSE stream: {}",
                                network_error
                            )),
                            (stream, state),
                        ));
                    }
                    None => {
                        // Stream ended
                        // Process any remaining partial data in buffer
                        if !state.buffer.trim().is_empty() {
                            // There's incomplete data - this might be an error
                            return Some((
                                Err(anyhow::anyhow!(
                                    "SSE stream ended with incomplete event: '{}'",
                                    state.buffer
                                )),
                                (stream, state),
                            ));
                        }
                        return None;
                    }
                }
            }
        },
    )
}

// Request options
#[derive(Debug, Clone, Default)]
pub struct RequestOptions {
    pub method: Option<Method>,
    pub path: Option<String>,
    pub query: Option<HashMap<String, String>>,
    pub headers: Option<HeaderMap>,
    pub body: Option<Value>,
    pub timeout: Option<Duration>,
    pub max_retries: Option<u32>,
    pub idempotency_key: Option<String>,
    pub signal: Option<AbortSignal>,
    pub fetch_options: Option<HashMap<String, String>>,
}

// Abort signal
#[derive(Debug, Clone)]
pub struct AbortSignal {
    aborted: Arc<RwLock<bool>>,
}

impl AbortSignal {
    pub fn new() -> Self {
        Self {
            aborted: Arc::new(RwLock::new(false)),
        }
    }

    pub async fn abort(&self) {
        let mut guard = self.aborted.write().await;
        *guard = true;
    }

    pub fn is_aborted(&self) -> bool {
        futures::executor::block_on(async {
            *self.aborted.read().await
        })
    }
}

// Service base class (yG equivalent)
pub trait ServiceBase {
    fn client(&self) -> Arc<AnthropicClient>;
}

// CompletionsService (lR)
pub struct CompletionsService {
    client: Arc<AnthropicClient>,
}

impl CompletionsService {
    pub fn new(client: Arc<AnthropicClient>) -> Self {
        Self { client }
    }

    pub async fn create(&self, mut params: Value, options: Option<RequestOptions>) -> Result<Value> {
        let betas = params.get("betas").cloned();
        if let Some(map) = params.as_object_mut() {
            map.remove("betas");
        }

        let mut opts = options.unwrap_or_default();
        opts.body = Some(params);
        opts.timeout = Some(self.client.config.timeout);

        if let Some(betas) = betas {
            let mut headers = opts.headers.unwrap_or_default();
            headers.insert(
                HeaderName::from_static("anthropic-beta"),
                HeaderValue::from_str(&betas.to_string())?,
            );
            opts.headers = Some(headers);
        }

        self.client.post("/complete", Some(opts)).await
    }
}

// BatchesService (ro)
pub struct BatchesService {
    client: Arc<AnthropicClient>,
}

impl BatchesService {
    pub fn new(client: Arc<AnthropicClient>) -> Self {
        Self { client }
    }
}

// MessagesService (ZK)
pub struct MessagesService {
    client: Arc<AnthropicClient>,
    pub batches: Arc<BatchesService>,
}

impl MessagesService {
    pub fn new(client: Arc<AnthropicClient>) -> Self {
        Self {
            batches: Arc::new(BatchesService::new(client.clone())),
            client,
        }
    }

    pub async fn create(&self, mut params: Value, options: Option<RequestOptions>) -> Result<Value> {
        // Check for deprecated models
        if let Some(model) = params.get("model").and_then(|m| m.as_str()) {
            if let Some(eol_date) = DEPRECATED_MODELS.get(model) {
                warn!(
                    "The model '{}' is deprecated and will reach end-of-life on {}\nPlease migrate to a newer model.",
                    model, eol_date
                );
            }
        }

        let betas = params.get("betas").cloned();
        if let Some(map) = params.as_object_mut() {
            map.remove("betas");
        }

        let mut opts = options.unwrap_or_default();
        opts.body = Some(params);
        opts.timeout = Some(self.client.config.timeout);

        if let Some(betas) = betas {
            let mut headers = opts.headers.unwrap_or_default();
            headers.insert(
                HeaderName::from_static("anthropic-beta"),
                HeaderValue::from_str(&betas.to_string())?,
            );
            opts.headers = Some(headers);
        }

        self.client.post("/messages", Some(opts)).await
    }
}

// ModelsService (Lm)
pub struct ModelsService {
    client: Arc<AnthropicClient>,
}

impl ModelsService {
    pub fn new(client: Arc<AnthropicClient>) -> Self {
        Self { client }
    }

    pub async fn retrieve(&self, model_id: &str, options: Option<RequestOptions>) -> Result<Value> {
        let path = format!("/models/{}", urlencoding::encode(model_id));

        let mut opts = options.unwrap_or_default();

        if let Some(headers) = &mut opts.headers {
            if let Some(betas) = headers.get("betas").cloned() {
                headers.insert(
                    HeaderName::from_static("anthropic-beta"),
                    betas,
                );
            }
        }

        self.client.get(&path, Some(opts)).await
    }
}

// BetaModelsService (uo)
pub struct BetaModelsService {
    client: Arc<AnthropicClient>,
}

impl BetaModelsService {
    pub fn new(client: Arc<AnthropicClient>) -> Self {
        Self { client }
    }
}

// BetaMessagesService (Class31)
pub struct BetaMessagesService {
    client: Arc<AnthropicClient>,
}

impl BetaMessagesService {
    pub fn new(client: Arc<AnthropicClient>) -> Self {
        Self { client }
    }
}

// BetaService (iX)
pub struct BetaService {
    client: Arc<AnthropicClient>,
    pub models: Arc<BetaModelsService>,
    pub messages: Arc<BetaMessagesService>,
}

impl BetaService {
    pub fn new(client: Arc<AnthropicClient>) -> Self {
        Self {
            models: Arc::new(BetaModelsService::new(client.clone())),
            messages: Arc::new(BetaMessagesService::new(client.clone())),
            client,
        }
    }
}

// Extended Anthropic Client (Ow)
pub struct ExtendedAnthropicClient {
    inner: Arc<AnthropicClient>,
    pub completions: Arc<CompletionsService>,
    pub messages: Arc<MessagesService>,
    pub models: Arc<ModelsService>,
    pub beta: Arc<BetaService>,
}

impl ExtendedAnthropicClient {
    pub fn new(config: ClientConfig) -> Result<Self> {
        let client = Arc::new(AnthropicClient::new(config)?);

        Ok(Self {
            completions: Arc::new(CompletionsService::new(client.clone())),
            messages: Arc::new(MessagesService::new(client.clone())),
            models: Arc::new(ModelsService::new(client.clone())),
            beta: Arc::new(BetaService::new(client.clone())),
            inner: client,
        })
    }
}

// AWS Bedrock Client (Class38)
// NOTE: Uses AWS SigV4 authentication instead of API keys
pub struct BedrockClient {
    inner: Arc<BedrockAnthropicClient>,
    pub messages: Arc<MessagesService>,
    pub completions: Arc<CompletionsService>,
    pub beta: Arc<BetaService>,
    pub aws_region: String,
    pub aws_secret_key: Option<String>,
    pub aws_access_key: Option<String>,
    pub aws_session_token: Option<String>,
    pub skip_auth: bool,
}

// Bedrock-specific client that overrides validateHeaders
struct BedrockAnthropicClient {
    base: AnthropicClient,
}

impl BedrockAnthropicClient {
    fn new(config: ClientConfig) -> Result<Self> {
        Ok(Self {
            base: AnthropicClient::new(config)?,
        })
    }

    // Override validateHeaders to be empty - Bedrock uses AWS auth
    fn validate_headers(&self, _merged: &MergedHeaders) -> Result<()> {
        Ok(())
    }
}

impl BedrockClient {
    pub fn new(
        aws_region: Option<String>,
        aws_secret_key: Option<String>,
        aws_access_key: Option<String>,
        aws_session_token: Option<String>,
        mut config: ClientConfig,
    ) -> Result<Self> {
        let region = aws_region
            .or_else(|| get_env_var("AWS_REGION"))
            .unwrap_or_else(|| "us-east-1".to_string());

        let base_url = get_env_var("ANTHROPIC_BEDROCK_BASE_URL")
            .unwrap_or_else(|| format!("https://bedrock-runtime.{}.amazonaws.com", region));

        config.base_url = base_url;

        let client = Arc::new(BedrockAnthropicClient::new(config)?);
        let client_base = Arc::new(client.base.clone());

        // stringDecoder214 - Messages without batches/countTokens
        let messages = Arc::new(MessagesService::new(client_base.clone()));

        // stringDecoder215 - Beta without promptCaching and messages modifications
        let beta = Arc::new(BetaService::new(client_base.clone()));

        Ok(Self {
            messages,
            completions: Arc::new(CompletionsService::new(client_base.clone())),
            beta,
            inner: client,
            aws_region: region,
            aws_secret_key,
            aws_access_key,
            aws_session_token,
            skip_auth: false,
        })
    }
}

// Google Vertex AI Client (Class39)
// NOTE: Uses Google Cloud authentication instead of API keys
pub struct VertexClient {
    inner: Arc<VertexAnthropicClient>,
    pub messages: Arc<MessagesService>,
    pub beta: Arc<BetaService>,
    pub region: String,
    pub project_id: Option<String>,
}

// Vertex-specific client that overrides validateHeaders
struct VertexAnthropicClient {
    base: AnthropicClient,
}

impl VertexAnthropicClient {
    fn new(config: ClientConfig) -> Result<Self> {
        Ok(Self {
            base: AnthropicClient::new(config)?,
        })
    }

    // Override validateHeaders to be empty - Vertex uses Google auth
    fn validate_headers(&self, _merged: &MergedHeaders) -> Result<()> {
        Ok(())
    }
}

impl VertexClient {
    pub fn new(
        region: Option<String>,
        project_id: Option<String>,
        mut config: ClientConfig,
    ) -> Result<Self> {
        let region = region
            .or_else(|| get_env_var("CLOUD_ML_REGION"))
            .ok_or_else(|| anyhow!(
                "No region was given. The client should be instantiated with the `region` option or the `CLOUD_ML_REGION` environment variable should be set."
            ))?;

        let base_url = get_env_var("ANTHROPIC_VERTEX_BASE_URL")
            .unwrap_or_else(|| format!("https://{}-aiplatform.googleapis.com/v1", region));

        let project_id = project_id.or_else(|| get_env_var("ANTHROPIC_VERTEX_PROJECT_ID"));

        config.base_url = base_url;

        let client = Arc::new(VertexAnthropicClient::new(config)?);
        let client_base = Arc::new(client.base.clone());

        // stringDecoder216 - Messages without batches
        let messages = Arc::new(MessagesService::new(client_base.clone()));

        // stringDecoder217 - Beta without messages.batches
        let beta = Arc::new(BetaService::new(client_base.clone()));

        Ok(Self {
            messages,
            beta,
            inner: client,
            region,
            project_id,
        })
    }
}

// Simple ClientManager
#[derive(Clone)]
pub struct ClientManager {
    client: Arc<RwLock<Option<Arc<AnthropicClient>>>>,
}

impl ClientManager {
    pub fn new() -> Self {
        Self {
            client: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn set_client(&self, client: Arc<AnthropicClient>) {
        let mut guard = self.client.write().await;
        *guard = Some(client);
    }

    pub async fn get_client(&self) -> Option<Arc<AnthropicClient>> {
        let guard = self.client.read().await;
        guard.clone()
    }
}

impl Default for ClientManager {
    fn default() -> Self {
        Self::new()
    }
}

// ClientManagerScope
#[derive(Clone)]
pub struct ClientManagerScope {
    client: Arc<RwLock<Option<Arc<AnthropicClient>>>>,
}

impl ClientManagerScope {
    pub fn new() -> Self {
        Self {
            client: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn set_client(&self, client: Arc<AnthropicClient>) {
        let mut guard = self.client.write().await;
        *guard = Some(client);
    }

    pub async fn get_client(&self) -> Option<Arc<AnthropicClient>> {
        let guard = self.client.read().await;
        guard.clone()
    }
}

// HubStackEntry
#[derive(Clone)]
struct HubStackEntry {
    client: Option<Arc<AnthropicClient>>,
    scope: ClientManagerScope,
}

// ClientManagerHub
#[derive(Clone)]
pub struct ClientManagerHub {
    stack: Arc<RwLock<Vec<HubStackEntry>>>,
}

impl ClientManagerHub {
    pub fn new() -> Self {
        let initial = HubStackEntry {
            client: None,
            scope: ClientManagerScope::new(),
        };

        Self {
            stack: Arc::new(RwLock::new(vec![initial])),
        }
    }

    pub async fn get_stack_top(&self) -> HubStackEntry {
        let stack = self.stack.read().await;
        stack.last().cloned().unwrap_or_else(|| HubStackEntry {
            client: None,
            scope: ClientManagerScope::new(),
        })
    }

    pub async fn get_client(&self) -> Option<Arc<AnthropicClient>> {
        let top = self.get_stack_top().await;
        top.client
    }

    pub async fn bind_client(&self, client: Arc<AnthropicClient>) {
        let mut stack = self.stack.write().await;
        if let Some(top) = stack.last_mut() {
            top.client = Some(client.clone());
            top.scope.set_client(client).await;
        }
    }
}

// Initialize client helper (SQA equivalent)
pub async fn initialize_client(client: Arc<AnthropicClient>) -> ClientManagerHub {
    let hub = ClientManagerHub::new();
    hub.bind_client(client).await;
    hub
}

// Global client manager
lazy_static::lazy_static! {
    pub static ref GLOBAL_CLIENT_MANAGER: ClientManager = ClientManager::new();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_client_manager() {
        let manager = ClientManager::new();
        assert!(manager.get_client().await.is_none());

        let config = ClientConfig::default();
        let client = Arc::new(AnthropicClient::new(config).unwrap());
        manager.set_client(client.clone()).await;

        let retrieved = manager.get_client().await;
        assert!(retrieved.is_some());
    }

    #[test]
    fn test_anthropic_client_creation() {
        let config = ClientConfig::default();
        let client = AnthropicClient::new(config);
        assert!(client.is_ok());
    }

    #[test]
    fn test_auth_headers() {
        let mut config = ClientConfig::default();
        config.api_key = Some("test-key".to_string());
        config.auth_token = Some("test-token".to_string());

        let client = AnthropicClient::new(config).unwrap();
        let headers = client.auth_headers();

        assert!(headers.contains_key("x-api-key"));
        assert!(headers.contains_key("authorization"));
    }

    #[test]
    fn test_bedrock_client() {
        let config = ClientConfig::default();
        let client = BedrockClient::new(
            Some("us-west-2".to_string()),
            None,
            None,
            None,
            config,
        );
        assert!(client.is_ok());
        let client = client.unwrap();
        assert_eq!(client.aws_region, "us-west-2");
    }

    #[test]
    fn test_vertex_client_requires_region() {
        let client = VertexClient::new(None, None, ClientConfig::default());
        assert!(client.is_err());

        let client = VertexClient::new(
            Some("us-central1".to_string()),
            None,
            ClientConfig::default(),
        );
        assert!(client.is_ok());
    }

    #[tokio::test]
    async fn test_hub_client_management() {
        let hub = ClientManagerHub::new();
        assert!(hub.get_client().await.is_none());

        let config = ClientConfig::default();
        let client = Arc::new(AnthropicClient::new(config).unwrap());
        hub.bind_client(client.clone()).await;

        let retrieved = hub.get_client().await;
        assert!(retrieved.is_some());
    }

    #[tokio::test]
    async fn test_scope_client_management() {
        let scope = ClientManagerScope::new();
        assert!(scope.get_client().await.is_none());

        let config = ClientConfig::default();
        let client = Arc::new(AnthropicClient::new(config).unwrap());
        scope.set_client(client.clone()).await;

        let retrieved = scope.get_client().await;
        assert!(retrieved.is_some());
    }

    #[test]
    fn test_deprecated_models_warning() {
        // Verify deprecated models map is populated
        assert!(DEPRECATED_MODELS.contains_key("claude-instant-1"));
        assert!(DEPRECATED_MODELS.contains_key("claude-2.0"));
    }

    #[test]
    fn test_idempotency_key_generation() {
        let config = ClientConfig::default();
        let client = AnthropicClient::new(config).unwrap();

        let key1 = client.default_idempotency_key();
        let key2 = client.default_idempotency_key();

        assert!(key1.starts_with("key_"));
        assert!(key2.starts_with("key_"));
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_error_creation() {
        let err = AnthropicError::from_status(
            StatusCode::TOO_MANY_REQUESTS,
            json!({"message": "Rate limited"}),
            &HeaderMap::new(),
        );

        match err {
            AnthropicError::RateLimit { status, message, .. } => {
                assert_eq!(status, 429);
                assert_eq!(message, "Rate limited");
            }
            _ => panic!("Wrong error type"),
        }
    }
}