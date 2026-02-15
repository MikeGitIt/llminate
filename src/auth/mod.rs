pub mod storage;
pub mod signers;
pub mod checksum;
pub mod aws;
pub mod aws_providers;
pub mod client;
pub mod http;
pub mod session;
pub mod proxy;
pub mod utils;

use crate::error::{Error, Result};
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;
use tracing::{debug, info, error};
use self::storage::{CredentialsStorage, StorageBackend, PlaintextStorage};

/// Authentication methods supported
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthMethod {
    /// Direct API key authentication
    ApiKey(String),
    /// Claude.ai OAuth authentication (for Max subscribers)  
    ClaudeAiOauth(ClaudeAiOauth),
}

/// Claude.ai OAuth authentication details (matches JavaScript UZ function)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeAiOauth {
    #[serde(rename = "accessToken")]
    pub access_token: String,
    #[serde(rename = "refreshToken")]
    pub refresh_token: String,
    #[serde(rename = "expiresAt", skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
    pub scopes: Vec<String>,
    #[serde(rename = "subscriptionType", skip_serializing_if = "Option::is_none")]
    pub subscription_type: Option<String>,
    /// Account UUID from OAuth profile (cli-jsdef-fixed.js line 71217)
    /// JavaScript: variable29010.account.uuid from /api/oauth/profile response
    #[serde(rename = "accountUuid", skip_serializing_if = "Option::is_none")]
    pub account_uuid: Option<String>,
}

/// Configuration file structure matching JavaScript
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    #[serde(rename = "primaryApiKey", skip_serializing_if = "Option::is_none")]
    pub primary_api_key: Option<String>,
    #[serde(rename = "apiKeyHelper", skip_serializing_if = "Option::is_none")]
    pub api_key_helper: Option<String>,
    #[serde(rename = "customApiKeyResponses", skip_serializing_if = "Option::is_none")]
    pub custom_api_key_responses: Option<CustomApiKeyResponses>,
    #[serde(rename = "oauth", skip_serializing_if = "Option::is_none")]
    pub oauth: Option<ClaudeAiOauth>,
}

/// Custom API key response tracking (JavaScript YA function)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomApiKeyResponses {
    #[serde(default)]
    pub approved: Vec<String>,
    #[serde(default)]
    pub rejected: Vec<String>,
}

/// Authentication source information
#[derive(Debug, Clone)]
struct AuthSource {
    key: Option<String>,
    source: String,
}

/// Main authentication manager
pub struct AuthManager {
    config_path: PathBuf,
    config_cache: Option<(AuthConfig, std::time::SystemTime)>,
    storage_backend: Box<dyn CredentialsStorage>,
    credentials_cache: Option<(Option<storage::Credentials>, std::time::SystemTime)>,
}

impl AuthManager {
    /// Create a new authentication manager
    pub fn new() -> Result<Self> {
        let config_path = Self::get_config_file_path()?;
        let storage_backend = storage::get_storage_backend()?;
        
        Ok(Self {
            config_path,
            config_cache: None,
            storage_backend,
            credentials_cache: None,
        })
    }
    
    /// Create a new authentication manager with custom config directory (for testing)
    pub fn new_with_config_dir(config_dir: PathBuf) -> Result<Self> {
        let config_path = config_dir.join("config.json");
        let storage_backend = Box::new(storage::PlaintextStorage::new_with_path(
            config_dir.join(".credentials.json")
        ));
        
        Ok(Self {
            config_path,
            config_cache: None,
            storage_backend,
            credentials_cache: None,
        })
    }

    /// Get config file path with JavaScript compatibility (wX function)
    fn get_config_file_path() -> Result<PathBuf> {
        // Priority 1: Check for .config.json in config directory
        let config_dir = Self::get_config_directory()?;
        let primary_config = config_dir.join(".config.json");
        
        if primary_config.exists() {
            debug!("Using primary config file: {:?}", primary_config);
            return Ok(primary_config);
        }
        
        // Priority 2: Fallback to .claude.json in appropriate location
        if let Ok(claude_config_dir) = std::env::var("CLAUDE_CONFIG_DIR") {
            Ok(PathBuf::from(claude_config_dir).join(".claude.json"))
        } else if let Some(home_dir) = dirs::home_dir() {
            Ok(home_dir.join(".claude.json"))
        } else {
            Err(Error::Config("Cannot determine home directory for config file".to_string()))
        }
    }

    /// Get config directory with exact JavaScript precedence (checker64 function)
    fn get_config_directory() -> Result<PathBuf> {
        if let Ok(claude_config_dir) = std::env::var("CLAUDE_CONFIG_DIR") {
            return Ok(PathBuf::from(claude_config_dir));
        }
        
        if let Ok(xdg_config_home) = std::env::var("XDG_CONFIG_HOME") {
            return Ok(PathBuf::from(xdg_config_home).join("claude"));
        }
        
        if let Some(home_dir) = dirs::home_dir() {
            return Ok(home_dir.join(".claude"));
        }
        
        Err(Error::Config("Cannot determine config directory".to_string()))
    }

    /// Read and cache configuration file (JavaScript helperFunc5 equivalent)
    async fn get_config(&mut self) -> Result<AuthConfig> {
        // Check cache validity
        if let Some((ref config, cached_time)) = self.config_cache {
            if let Ok(metadata) = fs::metadata(&self.config_path).await {
                if let Ok(modified) = metadata.modified() {
                    if modified <= cached_time {
                        debug!("Using cached config");
                        return Ok(config.clone());
                    }
                }
            }
        }

        debug!("Reading config file: {:?}", self.config_path);
        
        if !self.config_path.exists() {
            debug!("Config file does not exist, using defaults");
            let default_config = AuthConfig {
                primary_api_key: None,
                api_key_helper: None,
                custom_api_key_responses: None,
                oauth: None,
            };
            return Ok(default_config);
        }
        
        let content = fs::read_to_string(&self.config_path).await
            .context("Failed to read config file")?;
            
        let config: AuthConfig = serde_json::from_str(&content)
            .context("Failed to parse config file")?;
            
        // Update cache
        if let Ok(metadata) = fs::metadata(&self.config_path).await {
            if let Ok(modified) = metadata.modified() {
                self.config_cache = Some((config.clone(), modified));
            }
        }
        
        debug!("Loaded config: has_primary_api_key={}, has_api_key_helper={}, has_oauth={}", 
               config.primary_api_key.is_some(),
               config.api_key_helper.is_some(),
               config.oauth.is_some());
        
        Ok(config)
    }

    /// Read credentials from storage backend (combines keychain + plaintext)
    async fn get_credentials(&mut self) -> Result<Option<storage::Credentials>> {
        // Check cache (1 minute TTL)
        if let Some((ref creds, cached_time)) = self.credentials_cache {
            if cached_time.elapsed().unwrap_or(std::time::Duration::from_secs(3600)) < std::time::Duration::from_secs(60) {
                debug!("Using cached credentials");
                return Ok(creds.clone());
            }
        }

        debug!("Reading credentials from storage backend");
        let credentials = self.storage_backend.read().await?;
        
        self.credentials_cache = Some((credentials.clone(), std::time::SystemTime::now()));
        Ok(credentials)
    }

    /// Get Claude.ai OAuth from credentials storage
    async fn get_claude_ai_oauth(&mut self) -> Result<Option<ClaudeAiOauth>> {
        if let Some(creds) = self.get_credentials().await? {
            if let Some(oauth) = creds.claude_ai_oauth {
                // Check if token is already expired
                if self.token_is_expired(&oauth) {
                    debug!("OAuth token is expired, attempting refresh");
                    match self.refresh_oauth_token(&oauth).await {
                        Ok(Some(refreshed)) => return Ok(Some(refreshed)),
                        Ok(None) => {
                            // Refresh returned None (no refresh token or other issue)
                            error!("OAuth token expired and refresh failed (no refresh token)");
                            return Ok(None);  // Don't return expired token
                        }
                        Err(e) => {
                            // Refresh failed with error
                            error!("OAuth token expired and refresh failed: {}", e);
                            return Ok(None);  // Don't return expired token
                        }
                    }
                }
                // Check if token needs refresh (within 5 minute buffer)
                else if self.token_needs_refresh(&oauth) {
                    debug!("OAuth token needs refresh (expires soon)");
                    if let Some(refreshed) = self.refresh_oauth_token(&oauth).await? {
                        return Ok(Some(refreshed));
                    }
                    // If refresh fails but token isn't expired yet, we can still use it
                    debug!("Refresh failed but token not yet expired, using existing token");
                }
                return Ok(Some(oauth));
            }
        }
        Ok(None)
    }

    /// Check if OAuth token is already expired
    fn token_is_expired(&self, oauth: &ClaudeAiOauth) -> bool {
        if let Some(expires_at) = oauth.expires_at {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            return expires_at <= now;
        }
        false
    }

    /// Check if OAuth token needs refresh (within 5 minute buffer, but not yet expired)
    fn token_needs_refresh(&self, oauth: &ClaudeAiOauth) -> bool {
        if let Some(expires_at) = oauth.expires_at {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            // Token needs refresh if it expires within 5 minutes but isn't expired yet
            let time_until_expiry = expires_at - now;
            return time_until_expiry > 0 && time_until_expiry < 300;
        }
        false
    }

    /// Get stored OAuth token (matches JavaScript getOAuthToken function)
    pub async fn get_oauth_token(&mut self) -> Result<Option<ClaudeAiOauth>> {
        debug!("Retrieving stored OAuth token");
        
        // Check environment variable first (CLAUDE_CODE_OAUTH_TOKEN)
        if let Ok(oauth_token_str) = std::env::var("CLAUDE_CODE_OAUTH_TOKEN") {
            if !oauth_token_str.is_empty() {
                debug!("Found CLAUDE_CODE_OAUTH_TOKEN environment variable");
                // Try to parse as JSON OAuth object
                if let Ok(oauth) = serde_json::from_str::<ClaudeAiOauth>(&oauth_token_str) {
                    return Ok(Some(oauth));
                }
                // Otherwise treat as access token only
                return Ok(Some(ClaudeAiOauth {
                    access_token: oauth_token_str,
                    refresh_token: String::new(),
                    expires_at: None,
                    scopes: vec!["user:inference".to_string()],
                    subscription_type: None,
                    account_uuid: None,
                }));
            }
        }
        
        // Check stored credentials
        self.get_claude_ai_oauth().await
    }
    
    /// Check if OAuth token has required scopes (matches JavaScript hasValidScopes function)
    pub fn has_valid_scopes(token: &Option<ClaudeAiOauth>) -> bool {
        const REQUIRED_SCOPE: &str = "user:inference";
        
        if let Some(oauth) = token {
            return oauth.scopes.contains(&REQUIRED_SCOPE.to_string());
        }
        false
    }
    
    /// Check if OAuth is available and valid (matches JavaScript hasOAuthAccess function)
    pub async fn has_oauth_access(&mut self) -> bool {
        // Get OAuth token if available
        if let Ok(token) = self.get_oauth_token().await {
            // Check if token has valid scopes
            return Self::has_valid_scopes(&token);
        }
        false
    }

    /// Refresh OAuth token using refresh token
    async fn refresh_oauth_token(&mut self, oauth: &ClaudeAiOauth) -> Result<Option<ClaudeAiOauth>> {
        if oauth.refresh_token.is_empty() {
            debug!("No refresh token available");
            return Ok(None);
        }

        debug!("Attempting to refresh OAuth token");

        // Use the same client_id as the original OAuth flow
        let oauth_config = crate::oauth::OAuthConfig::default();

        let client = reqwest::Client::new();
        let response = client
            .post(&oauth_config.token_url)
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "grant_type": "refresh_token",
                "refresh_token": oauth.refresh_token,
                "client_id": oauth_config.client_id
            }))
            .send()
            .await
            .context("Failed to send refresh request")?;

        if !response.status().is_success() {
            debug!("Token refresh failed with status: {}", response.status());
            return Ok(None);
        }

        let token_response: serde_json::Value = response.json().await
            .context("Failed to parse refresh response")?;

        let new_oauth = ClaudeAiOauth {
            access_token: token_response["access_token"]
                .as_str()
                .ok_or_else(|| Error::Authentication("No access_token in refresh response".to_string()))?
                .to_string(),
            refresh_token: token_response.get("refresh_token")
                .and_then(|v| v.as_str())
                .unwrap_or(&oauth.refresh_token)
                .to_string(),
            expires_at: token_response.get("expires_in")
                .and_then(|v| v.as_i64())
                .map(|expires_in| {
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64 + expires_in
                }),
            scopes: oauth.scopes.clone(),
            subscription_type: oauth.subscription_type.clone(),
            account_uuid: oauth.account_uuid.clone(),
        };

        // Update stored credentials
        if let Some(mut creds) = self.get_credentials().await? {
            creds.claude_ai_oauth = Some(new_oauth.clone());
            self.storage_backend.update(creds).await?;
            // Clear cache to force reload
            self.credentials_cache = None;
        }

        info!("Successfully refreshed OAuth token");
        Ok(Some(new_oauth))
    }

    /// Exchange OAuth token for API key (JavaScript function at line 355319)
    async fn exchange_oauth_for_api_key(&self, oauth_token: &str) -> Result<String> {
        debug!("Exchanging OAuth token for API key");
        
        let client = reqwest::Client::new();
        let response = client
            .post("https://api.anthropic.com/api/oauth/claude_cli/create_api_key")
            .header("authorization", format!("Bearer {}", oauth_token))
            .header("content-type", "application/json")
            .header("x-app", "cli")
            .send()
            .await
            .context("Failed to exchange OAuth token for API key")?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            error!("API key exchange failed with status {}: {}", status, error_text);
            return Err(Error::Auth(format!("Failed to exchange OAuth token: {} - {}", status, error_text)));
        }
        
        let response_data: serde_json::Value = response.json().await
            .context("Failed to parse API key exchange response")?;
        
        // Extract the raw_key from response
        if let Some(raw_key) = response_data.get("raw_key").and_then(|v| v.as_str()) {
            info!("Successfully exchanged OAuth token for API key");
            Ok(raw_key.to_string())
        } else {
            error!("API key exchange response missing raw_key field");
            Err(Error::Auth("API key exchange response missing raw_key".to_string()))
        }
    }

    /// Execute apiKeyHelper command (JavaScript MS function)
    async fn execute_api_key_helper(&mut self, helper_command: &str) -> Result<Option<String>> {
        debug!("Executing apiKeyHelper: {}", helper_command);
        
        match tokio::process::Command::new("sh")
            .arg("-c")
            .arg(helper_command)
            .output()
            .await
        {
            Ok(output) => {
                if output.status.success() {
                    match String::from_utf8(output.stdout) {
                        Ok(result) => {
                            let trimmed = result.trim();
                            if !trimmed.is_empty() {
                                debug!("apiKeyHelper returned valid key");
                                Ok(Some(trimmed.to_string()))
                            } else {
                                debug!("apiKeyHelper returned empty output");
                                // JavaScript MS() returns " " (space) when empty
                                Ok(Some(" ".to_string()))
                            }
                        }
                        Err(_) => {
                            debug!("apiKeyHelper returned invalid UTF-8");
                            // JavaScript MS() returns " " (space) on error
                            Ok(Some(" ".to_string()))
                        }
                    }
                } else {
                    debug!("apiKeyHelper execution failed with status: {}", output.status);
                    // JavaScript MS() returns " " (space) on failure
                    Ok(Some(" ".to_string()))
                }
            }
            Err(e) => {
                debug!("Failed to execute apiKeyHelper: {}", e);
                // JavaScript MS() returns " " (space) on execution error
                Ok(Some(" ".to_string()))
            }
        }
    }

    /// Check if API key is approved by user (JavaScript YA function)
    async fn is_api_key_approved(&mut self, api_key: &str) -> Result<bool> {
        let config = self.get_config().await?;
        
        if let Some(responses) = config.custom_api_key_responses {
            // Check last 20 characters of API key (JavaScript VJ function)
            let suffix = if api_key.len() > 20 { 
                &api_key[api_key.len() - 20..] 
            } else { 
                api_key 
            };
            
            let is_approved = responses.approved.contains(&suffix.to_string());
            debug!("API key approval check: suffix={}, approved={}", suffix, is_approved);
            return Ok(is_approved);
        }
        
        debug!("No customApiKeyResponses found, considering not approved");
        Ok(false)
    }

    /// Get authentication source with priority (JavaScript QX function)
    async fn get_auth_source(&mut self) -> Result<AuthSource> {
        debug!("Determining authentication source");
        
        // Priority 1: ANTHROPIC_AUTH_TOKEN environment variable
        if let Ok(auth_token) = std::env::var("ANTHROPIC_AUTH_TOKEN") {
            if !auth_token.is_empty() {
                debug!("Using ANTHROPIC_AUTH_TOKEN");
                // This returns an OAuth token directly, not an API key
                return Ok(AuthSource {
                    key: None,
                    source: "ANTHROPIC_AUTH_TOKEN".to_string(),
                });
            }
        }

        // Priority 2: CLAUDE_CODE_OAUTH_TOKEN environment variable
        if let Ok(oauth_token) = std::env::var("CLAUDE_CODE_OAUTH_TOKEN") {
            if !oauth_token.is_empty() {
                debug!("Using CLAUDE_CODE_OAUTH_TOKEN");
                return Ok(AuthSource {
                    key: None,
                    source: "CLAUDE_CODE_OAUTH_TOKEN".to_string(),
                });
            }
        }

        // Priority 3: ANTHROPIC_API_KEY with approval check
        if let Ok(api_key) = std::env::var("ANTHROPIC_API_KEY") {
            if !api_key.is_empty() {
                if self.is_api_key_approved(&api_key).await? {
                    debug!("Using approved ANTHROPIC_API_KEY");
                    return Ok(AuthSource {
                        key: Some(api_key),
                        source: "ANTHROPIC_API_KEY".to_string(),
                    });
                } else {
                    debug!("Found unapproved ANTHROPIC_API_KEY");
                    // Still return it but mark as unapproved
                    return Ok(AuthSource {
                        key: Some(api_key),
                        source: "ANTHROPIC_API_KEY".to_string(),
                    });
                }
            }
        }

        // Priority 4: apiKeyHelper
        let config = self.get_config().await?;
        if let Some(helper_command) = config.api_key_helper {
            if let Some(api_key) = self.execute_api_key_helper(&helper_command).await? {
                if api_key != " " {  // Space is the error sentinel
                    debug!("Using apiKeyHelper");
                    return Ok(AuthSource {
                        key: Some(api_key),
                        source: "apiKeyHelper".to_string(),
                    });
                }
            }
        }

        // Priority 5: Login managed key from keychain (macOS) or config
        if cfg!(target_os = "macos") {
            // Try keychain for API key (not OAuth)
            let service_name = storage::get_service_name_for_api_key()?;
            let username = std::env::var("USER").unwrap_or_else(|_| "unknown".to_string());
            
            if let Ok(output) = tokio::process::Command::new("security")
                .args(&[
                    "find-generic-password",
                    "-a", &username,
                    "-w",
                    "-s", &service_name
                ])
                .output()
                .await
            {
                if output.status.success() {
                    if let Ok(api_key) = String::from_utf8(output.stdout) {
                        let api_key = api_key.trim();
                        if !api_key.is_empty() {
                            debug!("Using /login managed key from keychain");
                            return Ok(AuthSource {
                                key: Some(api_key.to_string()),
                                source: "/login managed key".to_string(),
                            });
                        }
                    }
                }
            }
        } else if cfg!(target_os = "linux") {
            // Linux: Try secret-tool for GNOME Keyring/KWallet
            let service_name = storage::get_service_name_for_api_key()?;
            let username = std::env::var("USER").unwrap_or_else(|_| "unknown".to_string());
            
            // Try secret-tool (GNOME Keyring)
            if let Ok(output) = tokio::process::Command::new("secret-tool")
                .args(&[
                    "lookup",
                    "service", &service_name,
                    "username", &username
                ])
                .output()
                .await
            {
                if output.status.success() {
                    if let Ok(api_key) = String::from_utf8(output.stdout) {
                        let api_key = api_key.trim();
                        if !api_key.is_empty() {
                            debug!("Using /login managed key from secret-tool");
                            return Ok(AuthSource {
                                key: Some(api_key.to_string()),
                                source: "/login managed key".to_string(),
                            });
                        }
                    }
                }
            }
            
            // Try kwallet (KDE Wallet)
            if let Ok(output) = tokio::process::Command::new("kwallet-query")
                .args(&[
                    "--read-password",
                    &service_name,
                    "--folder", "Claude Code",
                    "kdewallet"
                ])
                .output()
                .await
            {
                if output.status.success() {
                    if let Ok(api_key) = String::from_utf8(output.stdout) {
                        let api_key = api_key.trim();
                        if !api_key.is_empty() {
                            debug!("Using /login managed key from kwallet");
                            return Ok(AuthSource {
                                key: Some(api_key.to_string()),
                                source: "/login managed key".to_string(),
                            });
                        }
                    }
                }
            }
        } else if cfg!(target_os = "windows") {
            // Windows: Use Windows Credential Manager via PowerShell
            let service_name = storage::get_service_name_for_api_key()?;
            
            // PowerShell command to retrieve credential
            let ps_script = format!(
                "$cred = Get-StoredCredential -Target '{}' -AsCredentialObject -ErrorAction SilentlyContinue; \
                 if ($cred) {{ $cred.GetNetworkCredential().Password }}",
                service_name
            );
            
            if let Ok(output) = tokio::process::Command::new("powershell")
                .args(&[
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command", &ps_script
                ])
                .output()
                .await
            {
                if output.status.success() {
                    if let Ok(api_key) = String::from_utf8(output.stdout) {
                        let api_key = api_key.trim();
                        if !api_key.is_empty() {
                            debug!("Using /login managed key from Windows Credential Manager");
                            return Ok(AuthSource {
                                key: Some(api_key.to_string()),
                                source: "/login managed key".to_string(),
                            });
                        }
                    }
                }
            }
        }

        // Priority 6: Config file primaryApiKey
        if let Some(api_key) = config.primary_api_key {
            if !api_key.is_empty() {
                debug!("Using /login managed key from config");
                return Ok(AuthSource {
                    key: Some(api_key),
                    source: "/login managed key".to_string(),
                });
            }
        }

        debug!("No authentication source found");
        Ok(AuthSource {
            key: None,
            source: "none".to_string(),
        })
    }

    /// Check if OAuth should be preferred over API key
    async fn should_prefer_oauth(&mut self) -> Result<bool> {
        debug!("Checking if OAuth should be preferred");
        
        // Check environment OAuth tokens first
        if std::env::var("ANTHROPIC_AUTH_TOKEN").is_ok() || 
           std::env::var("CLAUDE_CODE_OAUTH_TOKEN").is_ok() {
            debug!("Environment OAuth token found - OAuth preferred");
            return Ok(true);
        }

        // If API key exists and is approved, don't use OAuth
        // BUT: if the "API key" is actually an OAuth token (starts with sk-ant-oat), prefer OAuth
        if let Ok(api_key) = std::env::var("ANTHROPIC_API_KEY") {
            if !api_key.is_empty() {
                // Detect OAuth token accidentally stored as API key
                if api_key.starts_with("sk-ant-oat") {
                    debug!("ANTHROPIC_API_KEY contains OAuth token - clearing and preferring OAuth");
                    // Clear the misplaced OAuth token from API key env var
                    std::env::remove_var("ANTHROPIC_API_KEY");
                    return Ok(true);
                }
                if self.is_api_key_approved(&api_key).await? {
                    debug!("Approved API key found - OAuth not preferred");
                    return Ok(false);
                }
            }
        }

        // Check if OAuth credentials are available
        if let Some(oauth) = self.get_claude_ai_oauth().await? {
            if !oauth.access_token.is_empty() && 
               oauth.scopes.contains(&"user:inference".to_string()) {
                debug!("Valid OAuth credentials found - OAuth preferred");
                return Ok(true);
            }
        }
        
        debug!("No OAuth credentials or unapproved API key - OAuth not preferred");
        Ok(false)
    }

    /// Determine the authentication method to use (main entry point)
    pub async fn determine_auth_method(&mut self) -> Result<AuthMethod> {
        debug!("Starting authentication determination");

        // OAUTH DISABLED: Anthropic has disabled 3rd party OAuth support for Claude Code CLI
        // Priority is now: ANTHROPIC_API_KEY > other API key sources
        // OAuth environment variables and stored credentials are ignored.

        // Check for environment OAuth tokens - these are now DISABLED
        // if let Ok(auth_token) = std::env::var("ANTHROPIC_AUTH_TOKEN") {
        //     if !auth_token.is_empty() {
        //         info!("✅ Using ANTHROPIC_AUTH_TOKEN as OAuth Bearer token");
        //         let account_uuid = std::env::var("CLAUDE_CODE_ACCOUNT_UUID").ok();
        //         return Ok(AuthMethod::ClaudeAiOauth(ClaudeAiOauth {
        //             access_token: auth_token,
        //             refresh_token: String::new(),
        //             expires_at: None,
        //             scopes: vec!["user:inference".to_string()],
        //             subscription_type: None,
        //             account_uuid,
        //         }));
        //     }
        // }
        //
        // if let Ok(oauth_token) = std::env::var("CLAUDE_CODE_OAUTH_TOKEN") {
        //     if !oauth_token.is_empty() {
        //         info!("✅ Using CLAUDE_CODE_OAUTH_TOKEN as OAuth Bearer token");
        //         let account_uuid = std::env::var("CLAUDE_CODE_ACCOUNT_UUID").ok();
        //         return Ok(AuthMethod::ClaudeAiOauth(ClaudeAiOauth {
        //             access_token: oauth_token,
        //             refresh_token: String::new(),
        //             expires_at: None,
        //             scopes: vec!["user:inference".to_string()],
        //             subscription_type: None,
        //             account_uuid,
        //         }));
        //     }
        // }
        //
        // // Check if we should prefer OAuth over API key - DISABLED
        // if self.should_prefer_oauth().await? {
        //     if let Some(oauth) = self.get_claude_ai_oauth().await? {
        //         info!("✅ Using Claude.ai OAuth authentication");
        //         return Ok(AuthMethod::ClaudeAiOauth(oauth));
        //     }
        // }

        // Try to get API key (now the primary authentication method)
        let auth_source = self.get_auth_source().await?;

        if let Some(api_key) = auth_source.key {
            // Filter out space character sentinel value from apiKeyHelper
            if api_key == " " {
                error!("apiKeyHelper failed, no valid API key");
                return Err(Error::Authentication("apiKeyHelper failed to provide valid key".to_string()));
            }

            info!("✅ Using API key from source: {}", auth_source.source);
            return Ok(AuthMethod::ApiKey(api_key));
        }

        // OAUTH DISABLED: Don't fall back to OAuth anymore
        // // Last resort: try OAuth even if not preferred
        // if let Some(oauth) = self.get_claude_ai_oauth().await? {
        //     if oauth.scopes.contains(&"user:inference".to_string()) {
        //         info!("✅ Using Claude.ai OAuth token with user:inference scope (fallback)");
        //         return Ok(AuthMethod::ClaudeAiOauth(oauth));
        //     } else {
        //         info!("✅ OAuth token lacks user:inference scope - attempting API key exchange");
        //         match self.exchange_oauth_for_api_key(&oauth.access_token).await {
        //             Ok(api_key) => {
        //                 info!("✅ Successfully exchanged OAuth token for API key");
        //                 return Ok(AuthMethod::ApiKey(api_key));
        //             }
        //             Err(e) => {
        //                 error!("Failed to exchange OAuth token: {}", e);
        //                 info!("Using OAuth token as Bearer (last resort after exchange failure)");
        //                 return Ok(AuthMethod::ClaudeAiOauth(oauth));
        //             }
        //         }
        //     }
        // }

        error!("No authentication method available. Please set ANTHROPIC_API_KEY environment variable.");
        Err(Error::Authentication("No valid authentication method found. Please set ANTHROPIC_API_KEY environment variable.".to_string()))
    }

    /// Check if Claude Desktop is available (matches JavaScript yP() and v3() functions)
    /// Returns true if:
    /// 1. OAuth environment variables are set, OR
    /// 2. Claude Desktop app is installed on the system, OR
    /// 3. Existing OAuth credentials are stored
    pub async fn is_desktop_available(&self) -> bool {
        debug!("Checking Claude Desktop availability");

        // Check environment variables
        if std::env::var("ANTHROPIC_AUTH_TOKEN").is_ok() ||
           std::env::var("CLAUDE_CODE_OAUTH_TOKEN").is_ok() {
            debug!("✅ Found OAuth environment variable - Claude Desktop available");
            return true;
        }

        // Check if Claude Desktop app is installed on the system
        // This allows OAuth login even without existing credentials
        if Self::is_claude_desktop_installed() {
            debug!("✅ Claude Desktop app is installed - OAuth available");
            return true;
        }

        // Check storage for existing OAuth credentials
        let mut manager = match Self::new() {
            Ok(m) => m,
            Err(_) => return false,
        };

        if let Ok(Some(oauth)) = manager.get_claude_ai_oauth().await {
            if !oauth.access_token.is_empty() {
                debug!("✅ Found OAuth credentials - Claude Desktop available");
                return true;
            }
        }

        debug!("❌ No OAuth credentials or Claude Desktop found");
        false
    }

    /// Check if Claude Desktop application is installed on the system
    fn is_claude_desktop_installed() -> bool {
        #[cfg(target_os = "macos")]
        {
            // Check common macOS installation locations
            if std::path::Path::new("/Applications/Claude.app").exists() {
                debug!("Found Claude Desktop at: /Applications/Claude.app");
                return true;
            }
            if let Ok(home) = std::env::var("HOME") {
                let user_app = format!("{}/Applications/Claude.app", home);
                if std::path::Path::new(&user_app).exists() {
                    debug!("Found Claude Desktop at: {}", user_app);
                    return true;
                }
            }
        }

        #[cfg(target_os = "windows")]
        {
            // Check common Windows installation locations
            if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
                let path = format!("{}\\Programs\\Claude\\Claude.exe", local_app_data);
                if std::path::Path::new(&path).exists() {
                    debug!("Found Claude Desktop at: {}", path);
                    return true;
                }
            }
            if let Ok(program_files) = std::env::var("PROGRAMFILES") {
                let path = format!("{}\\Claude\\Claude.exe", program_files);
                if std::path::Path::new(&path).exists() {
                    debug!("Found Claude Desktop at: {}", path);
                    return true;
                }
            }
        }

        #[cfg(target_os = "linux")]
        {
            // Check common Linux installation locations
            for path in &["/usr/bin/claude", "/usr/local/bin/claude", "/opt/Claude/claude"] {
                if std::path::Path::new(path).exists() {
                    debug!("Found Claude Desktop at: {}", path);
                    return true;
                }
            }
            if let Ok(home) = std::env::var("HOME") {
                let user_bin = format!("{}/.local/bin/claude", home);
                if std::path::Path::new(&user_bin).exists() {
                    debug!("Found Claude Desktop at: {}", user_bin);
                    return true;
                }
            }
        }

        false
    }

    /// Verify authentication is valid
    pub async fn verify_auth(&mut self) -> Result<bool> {
        match self.determine_auth_method().await {
            Ok(_) => {
                debug!("Authentication verification successful");
                Ok(true)
            }
            Err(e) => {
                debug!("Authentication verification failed: {}", e);
                Ok(false)
            }
        }
    }

    /// Save authentication configuration  
    pub async fn save_auth(&mut self) -> Result<()> {
        // This would typically save current auth state to config file
        // Implementation depends on what needs to be persisted
        debug!("Auth save requested (not yet implemented)");
        Ok(())
    }

    /// Prompt user for Claude Desktop authentication setup
    pub async fn prompt_desktop_auth(&mut self) -> Result<()> {
        debug!("Setting up Claude Desktop authentication");
        
        // Try to read existing Claude Desktop OAuth tokens
        if let Some(oauth) = self.get_claude_ai_oauth().await? {
            debug!("Found existing Claude Desktop OAuth tokens");
            
            // Validate scopes
            if !oauth.scopes.contains(&"user:inference".to_string()) {
                return Err(Error::Authentication(
                    "Claude Desktop OAuth tokens missing required 'user:inference' scope. Please re-authenticate in Claude Desktop.".to_string()
                ));
            }
            
            info!("✅ Successfully configured Claude Desktop authentication");
            return Ok(());
        }
        
        Err(Error::Authentication(
            "No Claude Desktop OAuth tokens found. Please sign in to Claude Desktop first.".to_string()
        ))
    }

    /// Set authentication method (for CLI integration)
    pub fn set_auth(&mut self, _auth_method: AuthMethod) {
        // This would typically cache the auth method
        // Implementation depends on how CLI wants to manage auth state
        debug!("Auth method set (not yet implemented)");
    }

    /// Save API key from OAuth login to keychain or config file
    pub async fn save_api_key_from_oauth(&mut self, api_key: &str) -> Result<()> {
        debug!("Saving API key from OAuth login");
        
        // On macOS, save to keychain
        if cfg!(target_os = "macos") {
            let service_name = storage::get_service_name_for_api_key()?;
            let username = std::env::var("USER").unwrap_or_else(|_| "unknown".to_string());
            
            // Delete existing entry first
            let _ = tokio::process::Command::new("security")
                .args(&[
                    "delete-generic-password",
                    "-a", &username,
                    "-s", &service_name
                ])
                .output()
                .await;
            
            // Add new entry
            let output = tokio::process::Command::new("security")
                .args(&[
                    "add-generic-password",
                    "-a", &username,
                    "-s", &service_name,
                    "-w", api_key,
                    "-U"  // Update if exists
                ])
                .output()
                .await
                .map_err(|e| Error::Config(format!("Failed to save to keychain: {}", e)))?;
            
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(Error::Config(format!("Keychain save failed: {}", stderr)));
            }
            
            debug!("Saved API key to keychain");
        } else {
            // Save to config file
            let mut config = self.get_config().await?;
            config.primary_api_key = Some(api_key.to_string());
            
            let json = serde_json::to_string_pretty(&config)
                .map_err(|e| Error::Config(format!("Failed to serialize config: {}", e)))?;
            
            fs::write(&self.config_path, json).await
                .map_err(|e| Error::Config(format!("Failed to write config: {}", e)))?;
            
            debug!("Saved API key to config file");
        }
        
        // Clear cache to force reload
        self.config_cache = None;
        Ok(())
    }

    /// Save OAuth token from Claude Max login to storage backend
    /// JavaScript (cli-jsdef-fixed.js lines 134479-134497):
    /// Saves claudeAiOauth to storage backend via variable35391().update()
    ///
    /// CRITICAL: account_uuid is required for metadata user_id construction
    /// JavaScript (cli-jsdef-fixed.js line 71217): stores accountUuid from profile
    pub async fn save_oauth_token(
        &mut self,
        access_token: &str,
        refresh_token: &str,
        expires_in: Option<i64>,
        scopes: &[String],
        account_uuid: Option<&str>,
    ) -> Result<()> {
        debug!("Saving OAuth token from Claude Max login");
        if let Some(uuid) = account_uuid {
            debug!("Account UUID: {}", uuid);
        } else {
            debug!("WARNING: No account UUID provided - metadata may be incomplete");
        }

        // Clear any stale API key from keychain (prevents OAuth token being used as API key)
        // JavaScript (line 272551-272560): sets apiKey: null when OAuth is active
        if cfg!(target_os = "macos") {
            if let Ok(service_name) = storage::get_service_name_for_api_key() {
                let username = std::env::var("USER").unwrap_or_else(|_| "unknown".to_string());
                debug!("Clearing stale API key from keychain service: {}", service_name);
                let _ = tokio::process::Command::new("security")
                    .args(&["delete-generic-password", "-a", &username, "-s", &service_name])
                    .output()
                    .await;
            }
        }

        // Calculate expires_at from expires_in
        // JavaScript (cli-jsdef-fixed.js line 134485): expiresAt: variable22124.expiresAt
        let expires_at = expires_in.map(|exp| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64 + exp
        });

        // Create OAuth token data matching JavaScript structure
        // JavaScript (cli-jsdef-fixed.js lines 134482-134489):
        // variable21016.claudeAiOauth = {
        //     accessToken, refreshToken, expiresAt, scopes, subscriptionType, rateLimitTier, accountUuid
        // }
        let oauth_data = ClaudeAiOauth {
            access_token: access_token.to_string(),
            refresh_token: refresh_token.to_string(),
            expires_at,
            scopes: scopes.to_vec(),
            subscription_type: Some("max".to_string()), // Claude Max
            account_uuid: account_uuid.map(|s| s.to_string()),
        };

        // JavaScript saves to storage backend via variable35391().update()
        // This uses the SAME storage backend that read() uses (keychain on macOS, plaintext otherwise)
        let credentials = storage::Credentials {
            claude_ai_oauth: Some(oauth_data),
        };

        // Save to storage backend (matches JavaScript exactly)
        self.storage_backend.update(credentials).await?;
        info!("Saved OAuth token to storage backend with accountUuid: {:?}", account_uuid);

        // Clear caches to force reload
        self.config_cache = None;
        self.credentials_cache = None;
        Ok(())
    }

    /// Logout and clear all stored credentials
    pub async fn logout(&mut self) -> Result<()> {
        info!("Logging out - clearing all stored credentials");

        // Delete OAuth credentials from storage backend
        if let Err(e) = self.storage_backend.delete().await {
            debug!("Failed to delete from storage backend: {}", e);
            // Continue anyway - we still want to clear other credentials
        }

        // Clear API key from keychain (macOS)
        if cfg!(target_os = "macos") {
            let service_name = storage::get_service_name_for_api_key()?;
            let username = std::env::var("USER").unwrap_or_else(|_| "unknown".to_string());

            // Try to delete API key from keychain
            let _ = tokio::process::Command::new("security")
                .args(&[
                    "delete-generic-password",
                    "-a", &username,
                    "-s", &service_name
                ])
                .output()
                .await;

            debug!("Attempted to delete API key from keychain");
        }

        // Clear config file OAuth entry
        let mut config = self.get_config().await?;
        if config.oauth.is_some() || config.primary_api_key.is_some() {
            config.oauth = None;
            config.primary_api_key = None;

            let json = serde_json::to_string_pretty(&config)
                .map_err(|e| Error::Config(format!("Failed to serialize config: {}", e)))?;

            fs::write(&self.config_path, json).await
                .map_err(|e| Error::Config(format!("Failed to write config: {}", e)))?;

            debug!("Cleared OAuth and API key from config file");
        }

        // Clear all caches
        self.config_cache = None;
        self.credentials_cache = None;

        info!("Successfully logged out and cleared all credentials");
        Ok(())
    }
}

/// Initialize authentication and return the method
pub async fn init_auth() -> Result<AuthMethod> {
    let mut auth_manager = AuthManager::new()?;
    auth_manager.determine_auth_method().await
}

/// Get or prompt for authentication (backward compatibility)
pub async fn get_or_prompt_auth() -> Result<AuthMethod> {
    init_auth().await
}

/// Load AI configuration with authentication method
pub fn load_config_with_auth(auth_method: AuthMethod) -> Result<crate::ai::AIConfig> {
    let mut config = crate::ai::AIConfig::default();
    
    match auth_method {
        AuthMethod::ApiKey(api_key) => {
            config.api_key = api_key;
            config.base_url = "https://api.anthropic.com/v1".to_string();
        }
        AuthMethod::ClaudeAiOauth(oauth_auth) => {
            // Use authToken for OAuth, not apiKey
            config.auth_token = Some(oauth_auth.access_token);
            config.api_key = String::new(); // No API key for OAuth
            config.base_url = "https://api.anthropic.com/v1".to_string(); // OAuth uses same endpoint as API keys
        }
    }
    
    // Load other settings from environment if available
    if let Ok(base_url) = std::env::var("ANTHROPIC_BASE_URL") {
        config.base_url = base_url;
    }
    
    if let Ok(model) = std::env::var("ANTHROPIC_MODEL") {
        config.default_model = model;
    }
    
    Ok(config)
}