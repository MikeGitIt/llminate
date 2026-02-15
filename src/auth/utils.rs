use std::env;
use std::collections::HashMap;
use tracing::debug;

/// Mask API key for secure display
/// Shows first 3 and last 4 characters with asterisks in between
/// JavaScript: maskApiKey function equivalent
pub fn mask_api_key(api_key: &str) -> String {
    if api_key.len() <= 8 {
        return "*".repeat(api_key.len());
    }

    // Fixed: Show exactly first 3 and last 4 characters
    let prefix_len = 3;
    let suffix_len = 4;

    // Make sure we have enough characters
    if api_key.len() <= prefix_len + suffix_len {
        return "*".repeat(api_key.len());
    }

    let masked_len = api_key.len() - prefix_len - suffix_len;

    format!(
        "{}{}{}",
        &api_key[..prefix_len],
        "*".repeat(masked_len),
        &api_key[api_key.len() - suffix_len..]
    )
}

/// Check if API key is in approved list by comparing masked versions
pub fn is_api_key_approved(api_key: &str, approved_keys: &[String]) -> bool {
    let masked = mask_api_key(api_key);
    approved_keys.contains(&masked)
}

/// Parse custom headers from a string
/// Format: "Header1:Value1,Header2:Value2"
pub fn parse_custom_headers_from_string(input: &str) -> Option<HashMap<String, String>> {
    if input.is_empty() {
        return None;
    }

    let mut headers = HashMap::new();
    for header_pair in input.split(',') {
        let parts: Vec<&str> = header_pair.splitn(2, ':').collect();
        if parts.len() == 2 {
            let name = parts[0].trim().to_lowercase();
            let value = parts[1].trim().to_string();

            // Skip empty values
            if !name.is_empty() && !value.is_empty() {
                headers.insert(name, value);
            }
        }
    }

    if headers.is_empty() {
        None
    } else {
        debug!("Parsed {} custom headers", headers.len());
        Some(headers)
    }
}

/// Parse custom headers from ANTHROPIC_CUSTOM_HEADERS environment variable
/// Format: "Header1:Value1,Header2:Value2"
/// JavaScript: parseCustomHeaders equivalent
pub fn parse_custom_headers() -> Option<HashMap<String, String>> {
    parse_custom_headers_with_reader(&SystemEnvReader)
}

/// Parse custom headers with dependency injection for testing
pub fn parse_custom_headers_with_reader(env_reader: &dyn EnvReader) -> Option<HashMap<String, String>> {
    let custom_headers_str = env_reader.get_var("ANTHROPIC_CUSTOM_HEADERS")?;
    debug!("Parsing custom headers from environment");
    parse_custom_headers_from_string(&custom_headers_str)
}

/// API key source information for tracking where credentials came from
#[derive(Debug, Clone)]
pub struct ApiKeySource {
    pub key: Option<String>,
    pub source: ApiKeySourceType,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ApiKeySourceType {
    EnvironmentVariable,
    ApiKeyHelper,
    ClaudeDesktop,
    StoredCredentials,
    None,
}

impl ApiKeySourceType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::EnvironmentVariable => "ANTHROPIC_API_KEY",
            Self::ApiKeyHelper => "apiKeyHelper",
            Self::ClaudeDesktop => "claudeDesktop",
            Self::StoredCredentials => "storedCredentials",
            Self::None => "none",
        }
    }
}

/// Trait for reading environment variables (for testing)
pub trait EnvReader: Send + Sync {
    fn get_var(&self, key: &str) -> Option<String>;
}

/// Default environment reader using std::env
pub struct SystemEnvReader;

impl EnvReader for SystemEnvReader {
    fn get_var(&self, key: &str) -> Option<String> {
        env::var(key).ok()
    }
}

/// Comprehensive API key resolution from multiple sources
/// Checks environment variables, stored credentials, and helper scripts
/// JavaScript: resolveApiKey function equivalent
pub async fn resolve_api_key_with_reader(
    require_key: bool,
    approved_keys: Option<&[String]>,
    env_reader: &dyn EnvReader,
) -> ApiKeySource {
    // Priority 1: Environment variable (if required or approved)
    if let Some(api_key) = env_reader.get_var("ANTHROPIC_API_KEY") {
        if !api_key.is_empty() {
            // If key is required, always use it
            if require_key {
                debug!("Using required API key from environment");
                return ApiKeySource {
                    key: Some(api_key),
                    source: ApiKeySourceType::EnvironmentVariable,
                };
            }

            // Check if key is approved
            if let Some(approved) = approved_keys {
                if is_api_key_approved(&api_key, approved) {
                    debug!("Using approved API key from environment");
                    return ApiKeySource {
                        key: Some(api_key),
                        source: ApiKeySourceType::EnvironmentVariable,
                    };
                }
            } else {
                // No approval list, use the key
                debug!("Using API key from environment (no approval required)");
                return ApiKeySource {
                    key: Some(api_key),
                    source: ApiKeySourceType::EnvironmentVariable,
                };
            }
        }
    }

    // Priority 2: Check stored credentials via helper
    // This would integrate with the storage module
    // For now, return None as this needs integration

    ApiKeySource {
        key: None,
        source: ApiKeySourceType::None,
    }
}

/// Public API that uses the system environment
pub async fn resolve_api_key(
    require_key: bool,
    approved_keys: Option<&[String]>,
) -> ApiKeySource {
    resolve_api_key_with_reader(require_key, approved_keys, &SystemEnvReader).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_api_key() {
        // Test normal key (15 chars: show first 3, mask 8, show last 4)
        assert_eq!(mask_api_key("sk-ant-12345678"), "sk-********5678");

        // Test short key
        assert_eq!(mask_api_key("short"), "*****");

        // Test long key (23 chars: show first 3, mask 16, show last 4)
        assert_eq!(mask_api_key("verylongapikey123456789"), "ver****************6789");

        // Test empty
        assert_eq!(mask_api_key(""), "");
    }

    #[test]
    fn test_is_api_key_approved() {
        let approved = vec![
            "sk-********5678".to_string(),  // Masked version of "sk-ant-12345678"
            "ant***wxyz".to_string(),
        ];

        assert!(is_api_key_approved("sk-ant-12345678", &approved));
        assert!(!is_api_key_approved("sk-different-key", &approved));
        assert!(!is_api_key_approved("", &approved));
    }

    #[test]
    fn test_parse_custom_headers() {
        // Test parsing from string directly
        let input = "X-Custom-Header:value1,X-Another:value2,Invalid,X-Empty:";
        let headers = parse_custom_headers_from_string(input);
        assert!(headers.is_some());

        let headers = headers.unwrap();
        assert_eq!(headers.len(), 2);
        assert_eq!(headers.get("x-custom-header"), Some(&"value1".to_string()));
        assert_eq!(headers.get("x-another"), Some(&"value2".to_string()));
    }

    #[test]
    fn test_parse_custom_headers_with_env_reader() {
        let mut mock_env = MockEnvReader {
            vars: HashMap::new(),
        };
        mock_env.vars.insert(
            "ANTHROPIC_CUSTOM_HEADERS".to_string(),
            "X-Test:testvalue,X-Another:anothervalue".to_string()
        );

        let headers = parse_custom_headers_with_reader(&mock_env);
        assert!(headers.is_some());

        let headers = headers.unwrap();
        assert_eq!(headers.len(), 2);
        assert_eq!(headers.get("x-test"), Some(&"testvalue".to_string()));
        assert_eq!(headers.get("x-another"), Some(&"anothervalue".to_string()));
    }

    #[test]
    fn test_parse_custom_headers_empty() {
        // Test with empty string
        let headers = parse_custom_headers_from_string("");
        assert!(headers.is_none());

        // Test with mock env reader
        let mock_env = MockEnvReader {
            vars: HashMap::new(),
        };
        assert!(parse_custom_headers_with_reader(&mock_env).is_none());
    }

    // Mock environment reader for testing
    struct MockEnvReader {
        vars: HashMap<String, String>,
    }

    impl EnvReader for MockEnvReader {
        fn get_var(&self, key: &str) -> Option<String> {
            self.vars.get(key).cloned()
        }
    }

    #[tokio::test]
    async fn test_resolve_api_key_with_env() {
        let mut mock_env = MockEnvReader {
            vars: HashMap::new(),
        };
        mock_env.vars.insert("ANTHROPIC_API_KEY".to_string(), "test-api-key".to_string());

        // Test with require_key = true
        let result = resolve_api_key_with_reader(true, None, &mock_env).await;
        assert_eq!(result.source, ApiKeySourceType::EnvironmentVariable);
        assert_eq!(result.key, Some("test-api-key".to_string()));

        // Test with require_key = false, no approval
        let result = resolve_api_key_with_reader(false, None, &mock_env).await;
        assert_eq!(result.source, ApiKeySourceType::EnvironmentVariable);
        assert_eq!(result.key, Some("test-api-key".to_string()));
    }

    #[tokio::test]
    async fn test_resolve_api_key_with_approval() {
        let mut mock_env = MockEnvReader {
            vars: HashMap::new(),
        };
        mock_env.vars.insert("ANTHROPIC_API_KEY".to_string(), "sk-ant-12345678".to_string());

        let approved = vec!["sk-********5678".to_string()];  // Correct masked format

        let result = resolve_api_key_with_reader(false, Some(&approved), &mock_env).await;
        assert_eq!(result.source, ApiKeySourceType::EnvironmentVariable);
        assert_eq!(result.key, Some("sk-ant-12345678".to_string()));

        // Test with non-approved key
        mock_env.vars.insert("ANTHROPIC_API_KEY".to_string(), "different-key".to_string());
        let result = resolve_api_key_with_reader(false, Some(&approved), &mock_env).await;
        assert_eq!(result.source, ApiKeySourceType::None);
        assert_eq!(result.key, None);
    }

    #[tokio::test]
    async fn test_resolve_api_key_no_env() {
        let mock_env = MockEnvReader {
            vars: HashMap::new(),
        };

        // Test when no API key is set
        let result = resolve_api_key_with_reader(false, None, &mock_env).await;
        assert_eq!(result.source, ApiKeySourceType::None);
        assert_eq!(result.key, None);
    }
}