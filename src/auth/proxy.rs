// Proxy Authentication Implementation
// Complete port from proxy_auth_extracted.js

use anyhow::{Context, Result};
use base64::Engine;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, PROXY_AUTHORIZATION};
use serde::{Deserialize, Serialize};
use std::env;
use url::Url;

// ProxyConfig matching JavaScript implementations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    pub url: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub auth_token: Option<String>,  // For Bearer authentication
    pub no_proxy: Vec<String>,       // Bypass list
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            url: None,
            username: None,
            password: None,
            auth_token: None,
            no_proxy: Vec::new(),
        }
    }
}

impl ProxyConfig {
    // Create ProxyConfig from environment variables
    // Matches JavaScript proxy detection logic
    pub fn from_env() -> Result<Self> {
        let mut config = Self::default();

        // Check for proxy URL in priority order (matches JS lines 366982-366985)
        config.url = env::var("https_proxy")
            .or_else(|_| env::var("HTTPS_PROXY"))
            .or_else(|_| env::var("http_proxy"))
            .or_else(|_| env::var("HTTP_PROXY"))
            .ok();

        // Parse proxy URL if present
        if let Some(ref proxy_url) = config.url {
            if let Ok(parsed) = Url::parse(proxy_url) {
                // Extract username and password from URL
                let username = parsed.username();
                if !username.is_empty() {
                    config.username = Some(urlencoding::decode(username)
                        .unwrap_or_else(|_| username.into())
                        .into_owned());
                }

                config.password = parsed.password()
                    .map(|s| urlencoding::decode(s).unwrap_or_else(|_| s.into()).into_owned());
            }
        }

        // Check for Bearer token (Anthropic-specific, JS line 378630)
        config.auth_token = env::var("ANTHROPIC_AUTH_TOKEN").ok();

        // Parse NO_PROXY list (JS lines 271062-271067)
        if let Ok(no_proxy) = env::var("NO_PROXY").or_else(|_| env::var("no_proxy")) {
            config.no_proxy = no_proxy
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }

        Ok(config)
    }

    // Check if a host should bypass the proxy
    pub fn should_bypass(&self, host: &str) -> bool {
        for pattern in &self.no_proxy {
            // Handle wildcard patterns
            if pattern == "*" {
                return true;
            }

            // Handle wildcard domain patterns (e.g., *.local matches service.local)
            if pattern.starts_with("*.") {
                let suffix = &pattern[1..]; // Get .local from *.local
                if host.ends_with(suffix) {
                    return true;
                }
            }
            // Handle domain suffix matching (e.g., .example.com matches sub.example.com)
            else if pattern.starts_with('.') {
                if host.ends_with(pattern) || host == &pattern[1..] {
                    return true;
                }
            } else if host == pattern || host.ends_with(&format!(".{}", pattern)) {
                return true;
            }
        }
        false
    }

    // Add proxy authentication headers to request
    // Implements logic from multiple JS locations (14515-14518, 339079-339086, 221654-221659)
    pub fn add_proxy_auth(&self, headers: &mut HeaderMap) -> Result<()> {
        // Priority 1: Bearer token (for Anthropic API)
        if let Some(ref token) = self.auth_token {
            headers.insert(
                PROXY_AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {}", token))
                    .context("Invalid Bearer token")?
            );
            return Ok(());
        }

        // Priority 2: Basic authentication from username/password
        if let (Some(ref username), Some(ref password)) = (&self.username, &self.password) {
            let credentials = format!("{}:{}", username, password);
            let encoded = base64::engine::general_purpose::STANDARD.encode(credentials.as_bytes());

            headers.insert(
                PROXY_AUTHORIZATION,
                HeaderValue::from_str(&format!("Basic {}", encoded))
                    .context("Invalid Basic auth credentials")?
            );
            return Ok(());
        }

        // Priority 3: Extract credentials from proxy URL if not already set
        if let Some(ref proxy_url) = self.url {
            if let Ok(parsed) = Url::parse(proxy_url) {
                let username = parsed.username();
                let password = parsed.password().unwrap_or("");

                if !username.is_empty() {
                    // Decode URI components (handles special characters)
                    let decoded_username = urlencoding::decode(username)
                        .unwrap_or_else(|_| username.into())
                        .into_owned();
                    let decoded_password = urlencoding::decode(password)
                        .unwrap_or_else(|_| password.into())
                        .into_owned();

                    let credentials = format!("{}:{}", decoded_username, decoded_password);
                    let encoded = base64::engine::general_purpose::STANDARD.encode(credentials.as_bytes());

                    headers.insert(
                        PROXY_AUTHORIZATION,
                        HeaderValue::from_str(&format!("Basic {}", encoded))
                            .context("Invalid proxy credentials")?
                    );
                }
            }
        }

        Ok(())
    }

    // Create a reqwest::Proxy from this configuration
    pub fn to_reqwest_proxy(&self) -> Result<Option<reqwest::Proxy>> {
        if let Some(ref proxy_url) = self.url {
            let mut proxy = reqwest::Proxy::all(proxy_url)
                .context("Invalid proxy URL")?;

            // Add authentication if available
            if let (Some(ref username), Some(ref password)) = (&self.username, &self.password) {
                proxy = proxy.basic_auth(username, password);
            }

            // Add no_proxy list - reqwest DOES support this
            // NoProxy::from_string expects comma-separated hosts
            if !self.no_proxy.is_empty() {
                let no_proxy_string = self.no_proxy.join(",");
                let no_proxy = reqwest::NoProxy::from_string(&no_proxy_string);
                proxy = proxy.no_proxy(no_proxy);
            }

            Ok(Some(proxy))
        } else {
            Ok(None)
        }
    }
}

// Helper function to add proxy authentication to existing headers
// Matches JavaScript pattern from line 339079-339086
pub fn add_proxy_authentication(
    headers: &mut HeaderMap,
    auth: Option<ProxyAuth>,
) -> Result<()> {
    if let Some(auth) = auth {
        match auth {
            ProxyAuth::Basic { username, password } => {
                let credentials = format!("{}:{}", username, password);
                let encoded = base64::engine::general_purpose::STANDARD.encode(credentials.as_bytes());

                headers.insert(
                    PROXY_AUTHORIZATION,
                    HeaderValue::from_str(&format!("Basic {}", encoded))
                        .context("Invalid Basic auth")?
                );
            }
            ProxyAuth::Bearer { token } => {
                headers.insert(
                    PROXY_AUTHORIZATION,
                    HeaderValue::from_str(&format!("Bearer {}", token))
                        .context("Invalid Bearer token")?
                );
            }
            ProxyAuth::Raw { value } => {
                headers.insert(
                    PROXY_AUTHORIZATION,
                    HeaderValue::from_str(&value)
                        .context("Invalid proxy auth header")?
                );
            }
        }
    }
    Ok(())
}

// Proxy authentication types
#[derive(Debug, Clone)]
pub enum ProxyAuth {
    Basic { username: String, password: String },
    Bearer { token: String },
    Raw { value: String },  // Pre-encoded or custom format
}

// Parse proxy URL and extract authentication
// Implements URL parsing logic from JavaScript
pub fn parse_proxy_url(proxy_url: &str) -> Result<(String, Option<ProxyAuth>)> {
    let parsed = Url::parse(proxy_url).context("Invalid proxy URL")?;

    let username = parsed.username();
    let password = parsed.password();

    // Build clean URL without credentials
    let mut clean_url = parsed.clone();
    clean_url.set_username("").ok();
    clean_url.set_password(None).ok();

    let auth = if !username.is_empty() {
        Some(ProxyAuth::Basic {
            username: urlencoding::decode(username)
                .unwrap_or_else(|_| username.into())
                .into_owned(),
            password: password.map(|p|
                urlencoding::decode(p)
                    .unwrap_or_else(|_| p.into())
                    .into_owned()
            ).unwrap_or_default(),
        })
    } else {
        None
    };

    Ok((clean_url.to_string(), auth))
}

// GRPC-specific proxy authentication
// From JavaScript line 201299-201301
pub fn add_grpc_proxy_auth(headers: &mut HeaderMap, credentials: &str) -> Result<()> {
    let encoded = base64::engine::general_purpose::STANDARD.encode(credentials.as_bytes());

    headers.insert(
        PROXY_AUTHORIZATION,
        HeaderValue::from_str(&format!("Basic {}", encoded))
            .context("Invalid GRPC proxy credentials")?
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_auth_generation() {
        let mut headers = HeaderMap::new();
        let config = ProxyConfig {
            username: Some("user".to_string()),
            password: Some("pass".to_string()),
            ..Default::default()
        };

        config.add_proxy_auth(&mut headers).unwrap();

        let auth_header = headers.get(PROXY_AUTHORIZATION).unwrap();
        assert_eq!(auth_header, "Basic dXNlcjpwYXNz");  // base64("user:pass")
    }

    #[test]
    fn test_bearer_token_auth() {
        let mut headers = HeaderMap::new();
        let config = ProxyConfig {
            auth_token: Some("test-token".to_string()),
            ..Default::default()
        };

        config.add_proxy_auth(&mut headers).unwrap();

        let auth_header = headers.get(PROXY_AUTHORIZATION).unwrap();
        assert_eq!(auth_header, "Bearer test-token");
    }

    #[test]
    fn test_proxy_url_parsing() {
        let (clean_url, auth) = parse_proxy_url("http://user:pass@proxy.example.com:8080").unwrap();

        assert_eq!(clean_url, "http://proxy.example.com:8080/");

        if let Some(ProxyAuth::Basic { username, password }) = auth {
            assert_eq!(username, "user");
            assert_eq!(password, "pass");
        } else {
            panic!("Expected Basic auth");
        }
    }

    #[test]
    fn test_url_encoded_credentials() {
        let (_, auth) = parse_proxy_url("http://user%40example:pass%23word@proxy.com").unwrap();

        if let Some(ProxyAuth::Basic { username, password }) = auth {
            assert_eq!(username, "user@example");
            assert_eq!(password, "pass#word");
        } else {
            panic!("Expected Basic auth with decoded credentials");
        }
    }

    #[test]
    fn test_no_proxy_bypass() {
        let config = ProxyConfig {
            no_proxy: vec![
                "localhost".to_string(),
                ".internal.com".to_string(),
                "192.168.1.1".to_string(),
            ],
            ..Default::default()
        };

        assert!(config.should_bypass("localhost"));
        assert!(config.should_bypass("api.internal.com"));
        assert!(config.should_bypass("internal.com"));
        assert!(config.should_bypass("192.168.1.1"));
        assert!(!config.should_bypass("example.com"));
    }

    #[test]
    fn test_proxy_from_env() {
        // This test would need to set environment variables
        // Skipping for now as it would affect other tests
    }
}