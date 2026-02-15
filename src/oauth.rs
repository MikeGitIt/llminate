use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info, error};
use std::collections::HashMap;
use rand::{thread_rng, Rng};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json;

/// OAuth configuration matching JavaScript uHA/obj16 objects
#[derive(Debug, Clone)]
pub struct OAuthConfig {
    pub redirect_port: u16,
    pub scopes: Vec<String>,
    pub base_api_url: String,
    pub console_authorize_url: String,
    pub claude_ai_authorize_url: String,
    pub token_url: String,
    pub api_key_url: String,
    pub roles_url: String,
    pub console_success_url: String,
    pub claudeai_success_url: String,
    pub manual_redirect_url: String,
    pub client_id: String,
}

impl Default for OAuthConfig {
    fn default() -> Self {
        // Production config matching JavaScript (cli-jsdef-fixed.js lines 66733-66754)
        Self {
            redirect_port: 54545,
            // SCOPES from JavaScript (cli-jsdef-fixed.js line 66741): variable8259
            // Includes: org:create_api_key, user:profile, user:inference, user:sessions:claude_code
            scopes: vec![
                "org:create_api_key".to_string(),
                "user:profile".to_string(),
                "user:inference".to_string(),
                "user:sessions:claude_code".to_string(),
            ],
            base_api_url: "https://api.anthropic.com".to_string(),
            console_authorize_url: "https://console.anthropic.com/oauth/authorize".to_string(),
            claude_ai_authorize_url: "https://claude.ai/oauth/authorize".to_string(),
            token_url: "https://console.anthropic.com/v1/oauth/token".to_string(),
            api_key_url: "https://api.anthropic.com/api/oauth/claude_cli/create_api_key".to_string(),
            roles_url: "https://api.anthropic.com/api/oauth/claude_cli/roles".to_string(),
            console_success_url: "https://console.anthropic.com/buy_credits?returnUrl=/oauth/code/success%3Fapp%3Dclaude-code".to_string(),
            claudeai_success_url: "https://console.anthropic.com/oauth/code/success?app=claude-code".to_string(),
            manual_redirect_url: "https://console.anthropic.com/oauth/code/callback".to_string(),
            client_id: "9d1c250a-e61b-44d9-88ed-5944d1962f5e".to_string(),
        }
    }
}

/// OAuth token response from token endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: Option<i64>,
    pub token_type: String,
    pub scope: Option<String>,
}

/// API key creation response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyResponse {
    pub raw_key: String,
    pub api_key_id: String,
    pub name: Option<String>,
}

/// Result of OAuth authentication - either an API key or OAuth token
/// JavaScript (cli-jsdef-fixed.js line 71054-71056):
/// When token has 'user:inference' scope, use OAuth token directly (Claude Max)
/// Otherwise, create an API key (Console login)
#[derive(Debug, Clone)]
pub enum OAuthCredential {
    /// API key from console.anthropic.com (uses x-api-key header)
    ApiKey(String),
    /// OAuth token from claude.ai / Claude Max (uses Authorization: Bearer header)
    OAuthToken {
        access_token: String,
        refresh_token: String,
        expires_in: Option<i64>,
        scopes: Vec<String>,
        /// Account UUID from /api/oauth/profile (cli-jsdef-fixed.js line 71217)
        account_uuid: Option<String>,
    },
}

/// Account info from OAuth profile (cli-jsdef-fixed.js line 71217)
/// JavaScript: variable29010.account from /api/oauth/profile response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthAccount {
    pub uuid: String,
    #[serde(default)]
    pub email: Option<String>,
    pub display_name: Option<String>,
}

/// Organization roles response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RolesResponse {
    pub organization_role: Option<String>,
    pub workspace_role: Option<String>,
    pub organization_name: Option<String>,
    pub organization_uuid: Option<String>,
}

/// OAuth flow state
#[derive(Debug, Clone)]
pub enum OAuthFlowState {
    NotStarted,
    WaitingForLogin { url: String },
    CreatingApiKey,
    AboutToRetry,
    Success { api_key: String },
    Error { message: String, retry_state: Box<OAuthFlowState> },
}

/// Organization profile response from OAuth profile endpoint
/// JavaScript (cli-jsdef-fixed.js lines 71032-71044):
/// Returns { account: { uuid, email, display_name }, organization: { ... } }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthProfile {
    /// Account info including the critical accountUuid (line 71217)
    pub account: Option<OAuthAccount>,
    pub organization: Option<Organization>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Organization {
    pub organization_type: Option<String>,
    #[serde(alias = "uuid")]
    pub organization_uuid: Option<String>,
    #[serde(alias = "name")]
    pub organization_name: Option<String>,
    pub has_extra_usage_enabled: Option<bool>,
}

/// OAuth manager handling the complete authentication flow
pub struct OAuthManager {
    config: OAuthConfig,
    state: Arc<Mutex<OAuthFlowState>>,
    pkce_verifier: Option<String>,
    oauth_state: Option<String>,
    is_manual_flow: bool,
}

impl OAuthManager {
    /// Create new OAuth manager with default production config
    pub fn new() -> Self {
        Self::with_config(OAuthConfig::default())
    }

    /// Create OAuth manager with custom config (for testing)
    pub fn with_config(config: OAuthConfig) -> Self {
        Self {
            config,
            state: Arc::new(Mutex::new(OAuthFlowState::NotStarted)),
            pkce_verifier: None,
            oauth_state: None,
            is_manual_flow: false,
        }
    }
    
    /// Generate PKCE challenge and verifier matching JavaScript implementation EXACTLY
    /// JavaScript (cli-jsdef-fixed.js lines 362417-362425):
    /// - variable13715(): 32 random bytes, base64url encoded (standard: +→-, /→_, =→removed)
    /// - variable11354(): SHA256 hash of verifier, base64url encoded
    fn generate_pkce() -> (String, String) {
        let mut rng = thread_rng();
        // JavaScript uses 32 random bytes for verifier (line 362421: TPA.randomBytes(32))
        let verifier_bytes: Vec<u8> = (0..32).map(|_| rng.gen()).collect();

        // JavaScript uses STANDARD base64url encoding (line 362418):
        // .replace(/\+/g, "-")  -> + becomes -
        // .replace(/\//g, "_")  -> / becomes _
        // .replace(/=/g, "")    -> = is REMOVED (not replaced)
        let verifier_base64 = STANDARD.encode(&verifier_bytes);
        let verifier = verifier_base64
            .replace('+', "-")
            .replace('/', "_")
            .replace('=', "");  // Remove padding entirely

        // Create SHA256 hash of verifier for challenge (line 362424)
        // JavaScript hashes the verifier STRING (not the original bytes)
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        let hash = hasher.finalize();

        // Challenge uses same base64url encoding (line 362425 calls variable6043)
        let challenge_base64 = STANDARD.encode(hash);
        let challenge = challenge_base64
            .replace('+', "-")
            .replace('/', "_")
            .replace('=', "");  // Remove padding entirely

        debug!("PKCE generation debug:");
        debug!("  - Verifier length: {} chars", verifier.len());
        debug!("  - Challenge length: {} chars", challenge.len());

        (verifier, challenge)
    }
    
    /// Generate random state parameter for OAuth
    /// JavaScript (cli-jsdef-fixed.js lines 362427-362428): variable33302() = variable6043(TPA.randomBytes(32))
    fn generate_state() -> String {
        let mut rng = thread_rng();
        // JavaScript uses 32 random bytes for state (same as verifier)
        let state_bytes: Vec<u8> = (0..32).map(|_| rng.gen()).collect();
        // Standard base64url encoding (same as PKCE)
        STANDARD.encode(&state_bytes)
            .replace('+', "-")
            .replace('/', "_")
            .replace('=', "")
    }
    
    /// Start OAuth login flow
    pub async fn start_oauth_flow(&mut self, use_claude_ai: bool) -> Result<String> {
        let (verifier, challenge) = Self::generate_pkce();
        let state = Self::generate_state();
        
        self.pkce_verifier = Some(verifier);
        self.oauth_state = Some(state.clone());
        
        // Build authorization URL - MUST match JavaScript exactly
        let authorize_url = if use_claude_ai {
            &self.config.claude_ai_authorize_url
        } else {
            &self.config.console_authorize_url
        };
        
        let redirect_uri = format!("http://localhost:{}/callback", self.config.redirect_port);
        
        // Build URL with parameters in EXACT order from JavaScript
        let mut url = url::Url::parse(authorize_url)
            .context("Failed to parse authorize URL")?;
        
        debug!("Building OAuth URL with base: {}", authorize_url);
        debug!("Client ID from config: '{}'", self.config.client_id);
        debug!("Client ID length: {}", self.config.client_id.len());
        
        // Ensure we're using the correct production client_id
        assert_eq!(self.config.client_id, "9d1c250a-e61b-44d9-88ed-5944d1962f5e", "Wrong client_id!");
        
        // Use ALL scopes - matching actual Claude Code behavior
        let scope = self.config.scopes.join(" ");

        // Parameters matching JavaScript stringDecoder90 function exactly
        url.query_pairs_mut()
            .clear()
            .append_pair("code", "true")
            .append_pair("client_id", &self.config.client_id)
            .append_pair("response_type", "code")
            .append_pair("redirect_uri", &redirect_uri)
            .append_pair("scope", &scope)
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", &state);
        
        let auth_url = url.to_string();
        
        // Log the exact URL being generated for debugging
        info!("=== OAUTH URL DEBUG ===");
        info!("Full OAuth URL: {}", auth_url);
        info!("");
        info!("URL Parts:");
        info!("  Base: {}", authorize_url);
        info!("  Query String: {}", url.query().unwrap_or("NONE"));
        info!("");
        info!("OAuth parameters being sent:");
        info!("  - client_id: '{}'", self.config.client_id);
        info!("  - response_type: code");
        info!("  - redirect_uri: {}", redirect_uri);
        info!("  - scope: {}", scope);
        info!("  - code_challenge: {} (length: {})", challenge, challenge.len());
        info!("  - code_challenge_method: S256");
        info!("  - state: {} (length: {})", state, state.len());
        info!("=== END OAUTH URL DEBUG ===");

        // Update state
        let mut flow_state = self.state.lock().await;
        *flow_state = OAuthFlowState::WaitingForLogin { url: auth_url.clone() };

        Ok(auth_url)
    }

    /// Start OAuth login flow with manual redirect (uses Anthropic's callback URL)
    /// This is more reliable when localhost callbacks don't work
    pub async fn start_oauth_flow_manual(&mut self) -> Result<String> {
        let (verifier, challenge) = Self::generate_pkce();
        let state = Self::generate_state();

        self.pkce_verifier = Some(verifier);
        self.oauth_state = Some(state.clone());
        self.is_manual_flow = true;  // Mark this as manual flow for token exchange

        // Use claude.ai authorize URL for Claude Desktop login
        let authorize_url = &self.config.claude_ai_authorize_url;

        // Use localhost callback - matching actual Claude Code behavior
        let redirect_uri = format!("http://localhost:{}/callback", self.config.redirect_port);

        // Use ALL scopes - matching actual Claude Code behavior
        let scope = self.config.scopes.join(" ");

        // Build URL with parameters matching JavaScript stringDecoder90 (isManual=true)
        let mut url = url::Url::parse(authorize_url)
            .context("Failed to parse authorize URL")?;

        url.query_pairs_mut()
            .clear()
            .append_pair("code", "true")
            .append_pair("client_id", &self.config.client_id)
            .append_pair("response_type", "code")
            .append_pair("redirect_uri", &redirect_uri)
            .append_pair("scope", &scope)
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", &state);

        let auth_url = url.to_string();

        info!("Manual OAuth URL: {}", auth_url);

        // Update state
        let mut flow_state = self.state.lock().await;
        *flow_state = OAuthFlowState::WaitingForLogin { url: auth_url.clone() };

        Ok(auth_url)
    }

    /// Start local HTTP server to receive OAuth callback
    /// If auth_url is provided, opens browser AFTER server is bound (matching JavaScript behavior)
    pub async fn start_callback_server(&self, auth_url: Option<&str>) -> Result<(String, String)> {
        use warp::Filter;
        use std::sync::Arc;
        use tokio::sync::Mutex;
        
        let (tx, mut rx) = tokio::sync::oneshot::channel::<(String, String)>();
        let port = self.config.redirect_port;
        
        // Wrap the sender in Arc<Mutex<Option>> so it can be shared and used once
        let tx = Arc::new(Mutex::new(Some(tx)));
        
        // Create callback route - handle all OAuth response scenarios
        let callback = warp::path("callback")
            .and(warp::query::<HashMap<String, String>>())
            .and_then(move |params: HashMap<String, String>| {
                let tx = tx.clone();
                async move {
                    // Log all received parameters for debugging
                    info!("OAuth callback received parameters: {:?}", params);
                    
                    // Check for error parameters first (OAuth error response)
                    if let Some(error) = params.get("error") {
                        let error_desc = params.get("error_description")
                            .map(|s| s.as_str())
                            .unwrap_or("No description provided");
                        
                        error!("OAuth error response: {} - {}", error, error_desc);
                        
                        // Still try to send error to the channel so we can handle it
                        let mut tx_guard = tx.lock().await;
                        if let Some(sender) = tx_guard.take() {
                            // Send empty strings to signal error
                            let _ = sender.send((String::new(), String::new()));
                        }
                        
                        return Ok::<_, warp::Rejection>(warp::reply::html(format!(
                            r#"<html>
                            <head><title>Authentication Error</title></head>
                            <body style="font-family: sans-serif; padding: 20px;">
                                <h1>Authentication Error</h1>
                                <p style="color: red;">Error: {}</p>
                                <p>{}</p>
                                <p>Please close this window and try again.</p>
                            </body>
                            </html>"#, 
                            error, error_desc
                        )));
                    }
                    
                    // Check for successful authorization with code and state
                    if let (Some(code), Some(state)) = (params.get("code"), params.get("state")) {
                        info!("OAuth successful - received code and state");
                        
                        // Try to send the result, but only if we haven't already
                        let mut tx_guard = tx.lock().await;
                        if let Some(sender) = tx_guard.take() {
                            let _ = sender.send((code.clone(), state.clone()));
                        }
                        
                        Ok::<_, warp::Rejection>(warp::reply::html(
                            r#"<html>
                            <head><title>Authentication Successful</title></head>
                            <body style="font-family: sans-serif; padding: 20px;">
                                <h1>Authentication successful!</h1>
                                <p>You can close this window and return to Claude Code.</p>
                                <script>
                                    // Try to close the window
                                    window.close();
                                    // If that doesn't work, show a message
                                    setTimeout(function() {
                                        document.body.innerHTML += '<p><small>You may need to close this window manually.</small></p>';
                                    }, 500);
                                </script>
                            </body>
                            </html>"#.to_string()
                        ))
                    } else {
                        // Missing required parameters
                        error!("OAuth callback missing required parameters. Received: {:?}", params);
                        
                        Ok::<_, warp::Rejection>(warp::reply::html(
                            r#"<html>
                            <head><title>Authentication Failed</title></head>
                            <body style="font-family: sans-serif; padding: 20px;">
                                <h1>Authentication failed</h1>
                                <p>Missing authorization code or state. Please try again.</p>
                                <p>If you're using a Max account, please make sure to click "Approve" when prompted.</p>
                            </body>
                            </html>"#.to_string()
                        ))
                    }
                }
            });
        
        // Start server in background
        // Use bind_ephemeral() instead of run() so we know when the server is actually listening
        // This matches JavaScript behavior where start() resolves only after server.listen() callback
        // IMPORTANT: Bind to 0.0.0.0 to accept connections on all interfaces
        // JavaScript uses "localhost" which can resolve to either IPv4 or IPv6
        // Safari on macOS may prefer IPv6 (::1), so binding only to 127.0.0.1 can fail
        let (addr, server) = warp::serve(callback).bind_ephemeral(([0, 0, 0, 0], port));
        tokio::spawn(server);

        info!("OAuth callback server listening on {}", addr);

        // Open browser AFTER server is bound (matching JavaScript cli-jsdef-fixed.js lines 393500-393501)
        // JavaScript: await variable148(variable1729) is called AFTER server.start() resolves
        if let Some(url) = auth_url {
            info!("Opening browser for OAuth: {}", url);
            if let Err(e) = Self::open_browser(url) {
                error!("Failed to open browser: {}", e);
                // Don't fail here, user can still manually navigate
            }
        }
        
        // Wait for callback with timeout
        match tokio::time::timeout(
            std::time::Duration::from_secs(300), // 5 minute timeout
            rx
        ).await {
            Ok(Ok((code, state))) => {
                // Check if we received an error signal (empty strings)
                if code.is_empty() && state.is_empty() {
                    bail!("OAuth authentication failed. Please check the browser for error details and try again.");
                }
                debug!("Received OAuth callback with code");
                Ok((code, state))
            }
            Ok(Err(_)) => bail!("OAuth callback channel closed unexpectedly"),
            Err(_) => bail!("OAuth authentication timed out after 5 minutes"),
        }
    }
    
    /// Handle manual authorization code input (when browser can't be opened)
    pub async fn handle_manual_auth_code(&mut self, full_code: &str) -> Result<String> {
        // Parse format: "code#state"
        let parts: Vec<&str> = full_code.split('#').collect();
        if parts.len() != 2 {
            bail!("Invalid code format. Expected format: code#state");
        }
        
        let code = parts[0];
        let state = parts[1];
        
        // Verify state matches
        if self.oauth_state.as_ref() != Some(&state.to_string()) {
            bail!("Invalid state parameter - authentication may have been compromised");
        }
        
        self.exchange_code_for_api_key(code).await
    }
    
    /// Exchange authorization code for tokens
    async fn exchange_code_for_tokens(&self, code: &str) -> Result<TokenResponse> {
        let client = reqwest::Client::new();

        // Always use localhost callback - matching actual Claude Code behavior
        let redirect_uri = format!("http://localhost:{}/callback", self.config.redirect_port);

        debug!("Token exchange using redirect_uri: {}", redirect_uri);

        // Create JSON body matching JavaScript's MvA function exactly
        let mut body = serde_json::json!({
            "grant_type": "authorization_code",
            "code": code,
            "client_id": &self.config.client_id,
            "redirect_uri": &redirect_uri,
        });
        
        // Add PKCE verifier if present
        if let Some(verifier) = &self.pkce_verifier {
            body["code_verifier"] = serde_json::json!(verifier);
        }
        
        // CRITICAL: JavaScript includes state in token exchange
        if let Some(state) = &self.oauth_state {
            body["state"] = serde_json::json!(state);
        }
        
        debug!("Token exchange request body: {}", serde_json::to_string_pretty(&body).unwrap_or_default());
        
        debug!("Exchanging authorization code for tokens");
        
        // Send as JSON, not form data - matching JavaScript's MvA function
        let response = client
            .post(&self.config.token_url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Failed to exchange code for tokens")?;
        
        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            bail!("Token exchange failed: {}", error_text);
        }
        
        let token_response: TokenResponse = response
            .json()
            .await
            .context("Failed to parse token response")?;
        
        info!("Successfully obtained OAuth tokens");
        Ok(token_response)
    }
    
    /// Convert OAuth token to API key
    async fn create_api_key_from_token(&self, access_token: &str) -> Result<String> {
        let client = reqwest::Client::new();
        
        let mut state = self.state.lock().await;
        *state = OAuthFlowState::CreatingApiKey;
        drop(state);
        
        debug!("Creating API key from OAuth token");
        
        let response = client
            .post(&self.config.api_key_url)
            .header("Authorization", format!("Bearer {}", access_token))
            .send()
            .await
            .context("Failed to create API key")?;
        
        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            
            // Check if it's a scope error
            if error_text.contains("org:create_api_key") {
                bail!("OAuth token lacks required scope 'org:create_api_key'. Please ensure you have the correct permissions.");
            }
            
            bail!("API key creation failed: {}", error_text);
        }
        
        let api_key_response: ApiKeyResponse = response
            .json()
            .await
            .context("Failed to parse API key response")?;
        
        info!("Successfully created API key from OAuth token");
        Ok(api_key_response.raw_key)
    }
    
    /// Check if scopes include user:inference (for Claude Max direct token usage)
    /// JavaScript (cli-jsdef-fixed.js line 71054-71056):
    /// function variable11754(variable22124) {
    ///     return Boolean(variable22124?.includes(variable4301));  // variable4301 = "user:inference"
    /// }
    fn has_inference_scope(scopes: &Option<String>) -> bool {
        scopes.as_ref()
            .map(|s| s.split_whitespace().any(|scope| scope == "user:inference"))
            .unwrap_or(false)
    }

    /// Complete flow: exchange code for credential (API key or OAuth token)
    /// JavaScript (cli-jsdef-fixed.js lines 400798-400821):
    /// - If token has 'user:inference' scope, use OAuth token directly (Claude Max path)
    /// - Otherwise, create an API key (Console login path)
    ///
    /// CRITICAL: After getting tokens, fetch profile to get accountUuid
    /// JavaScript (cli-jsdef-fixed.js lines 71211-71223):
    /// variable10301(accessToken) fetches /api/oauth/profile and stores accountUuid
    pub async fn exchange_code_for_credential(&mut self, code: &str) -> Result<OAuthCredential> {
        // First get tokens
        let tokens = self.exchange_code_for_tokens(code).await?;

        // Parse scopes into a list
        let scopes: Vec<String> = tokens.scope.as_ref()
            .map(|s| s.split_whitespace().map(|s| s.to_string()).collect())
            .unwrap_or_default();

        info!("Token scopes: {:?}", scopes);

        // Check if token has user:inference scope (Claude Max path)
        // JavaScript: variable11754(variable1133.scopes) checks for user:inference
        if Self::has_inference_scope(&tokens.scope) {
            info!("Token has user:inference scope - using OAuth token directly (Claude Max)");

            // CRITICAL: Fetch profile to get accountUuid (cli-jsdef-fixed.js lines 71211-71223)
            // JavaScript: variable10301(variable6404.accessToken) -> variable16218({ accountUuid: ... })
            // This is REQUIRED for the metadata user_id to be correct
            let account_uuid = match Self::fetch_oauth_profile(&tokens.access_token).await {
                Ok(profile) => {
                    let uuid = profile.account.map(|a| a.uuid);
                    info!("Fetched OAuth profile - accountUuid: {:?}", uuid);
                    uuid
                }
                Err(e) => {
                    error!("Failed to fetch OAuth profile: {} - metadata may be incomplete", e);
                    None
                }
            };

            // For Claude Max, we use the OAuth token directly
            // No API key creation needed
            let mut state = self.state.lock().await;
            *state = OAuthFlowState::Success { api_key: tokens.access_token.clone() };

            return Ok(OAuthCredential::OAuthToken {
                access_token: tokens.access_token,
                refresh_token: tokens.refresh_token,
                expires_in: tokens.expires_in,
                scopes,
                account_uuid,
            });
        }

        // No user:inference scope - need to create an API key (Console path)
        info!("Token does not have user:inference scope - creating API key");

        let api_key = self.create_api_key_from_token(&tokens.access_token).await?;

        // Update state to success
        let mut state = self.state.lock().await;
        *state = OAuthFlowState::Success { api_key: api_key.clone() };

        Ok(OAuthCredential::ApiKey(api_key))
    }

    /// Complete flow: exchange code for API key (legacy method for backward compatibility)
    /// Use exchange_code_for_credential for new code
    pub async fn exchange_code_for_api_key(&mut self, code: &str) -> Result<String> {
        match self.exchange_code_for_credential(code).await? {
            OAuthCredential::ApiKey(key) => Ok(key),
            OAuthCredential::OAuthToken { access_token, .. } => {
                // For Claude Max, return the access token
                // The caller should use this with Bearer auth
                Ok(access_token)
            }
        }
    }
    
    /// Get current OAuth flow state
    pub async fn get_state(&self) -> OAuthFlowState {
        self.state.lock().await.clone()
    }
    
    /// Fetch OAuth profile to get organization details
    pub async fn fetch_oauth_profile(access_token: &str) -> Result<OAuthProfile> {
        let client = reqwest::Client::new();
        let url = format!("{}/api/oauth/profile", OAuthConfig::default().base_api_url);
        
        debug!("Fetching OAuth profile from {}", url);
        
        let response = client
            .get(&url)
            .header("Authorization", format!("Bearer {}", access_token))
            .header("Content-Type", "application/json")
            .send()
            .await
            .context("Failed to fetch OAuth profile")?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            bail!("Profile fetch failed with status {}: {}", status, error_text);
        }
        
        let profile: OAuthProfile = response
            .json()
            .await
            .context("Failed to parse OAuth profile")?;
        
        debug!("OAuth profile: {:?}", profile);
        Ok(profile)
    }
    
    /// Determine subscription type from OAuth token
    /// Returns: "max" | "pro" | "enterprise" | "team" | None
    pub async fn get_subscription_type(access_token: &str) -> Result<Option<String>> {
        let profile = Self::fetch_oauth_profile(access_token).await?;
        
        let subscription_type = match profile.organization.as_ref().and_then(|o| o.organization_type.as_ref()) {
            Some(org_type) => match org_type.as_str() {
                "claude_max" => Some("max".to_string()),
                "claude_pro" => Some("pro".to_string()),
                "claude_enterprise" => Some("enterprise".to_string()),
                "claude_team" => Some("team".to_string()),
                _ => None,
            },
            None => None,
        };
        
        debug!("Subscription type: {:?}", subscription_type);
        Ok(subscription_type)
    }
    
    /// Determine which OAuth endpoint to use based on existing token
    /// Returns true for claude.ai, false for console.anthropic.com
    pub async fn determine_oauth_endpoint(existing_token: Option<&str>) -> Result<bool> {
        match existing_token {
            None => {
                // No existing token, default to console.anthropic.com
                debug!("No existing token, defaulting to console.anthropic.com");
                Ok(false)
            },
            Some(token) => {
                match Self::get_subscription_type(token).await {
                    Ok(Some(sub_type)) => {
                        // Use claude.ai for Max and Pro subscriptions
                        let use_claude_ai = sub_type == "max" || sub_type == "pro";
                        debug!("Subscription type '{}' -> use_claude_ai: {}", sub_type, use_claude_ai);
                        Ok(use_claude_ai)
                    },
                    Ok(None) => {
                        debug!("Unknown subscription type, defaulting to console.anthropic.com");
                        Ok(false)
                    },
                    Err(e) => {
                        // If we can't fetch profile, default to console.anthropic.com
                        debug!("Failed to fetch profile: {}, defaulting to console.anthropic.com", e);
                        Ok(false)
                    }
                }
            }
        }
    }
    
    /// Open browser for authentication
    pub fn open_browser(url: &str) -> Result<()> {
        info!("Opening browser for OAuth authentication");
        debug!("=== BROWSER URL DEBUG ===");
        debug!("Opening browser with URL: {}", url);
        debug!("URL length: {} characters", url.len());
        
        // Check if URL contains client_id
        if !url.contains("client_id=") {
            error!("WARNING: URL does not contain 'client_id=' parameter!");
        } else {
            debug!("✓ URL contains client_id parameter");
        }
        debug!("=== END BROWSER URL DEBUG ===");
        
        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("open")
                .arg(url)
                .spawn()
                .context("Failed to open browser")?;
        }
        
        #[cfg(target_os = "linux")]
        {
            std::process::Command::new("xdg-open")
                .arg(url)
                .spawn()
                .context("Failed to open browser")?;
        }
        
        #[cfg(target_os = "windows")]
        {
            std::process::Command::new("cmd")
                .args(&["/C", "start", url])
                .spawn()
                .context("Failed to open browser")?;
        }
        
        Ok(())
    }
}

/// Fetch organization roles using OAuth token
pub async fn fetch_organization_roles(access_token: &str) -> Result<RolesResponse> {
    let client = reqwest::Client::new();
    let config = OAuthConfig::default();
    
    debug!("Fetching organization roles");
    
    let response = client
        .get(&config.roles_url)
        .header("Authorization", format!("Bearer {}", access_token))
        .header("Content-Type", "application/json")
        .send()
        .await
        .context("Failed to fetch organization roles")?;
    
    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        bail!("Failed to fetch roles: {}", error_text);
    }
    
    let roles: RolesResponse = response
        .json()
        .await
        .context("Failed to parse roles response")?;
    
    debug!("Organization roles: {:?}", roles);
    Ok(roles)
}