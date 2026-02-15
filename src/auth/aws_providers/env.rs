use super::{Credentials, CredentialProvider, CredentialsProviderError, parse_credential_expiration};
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::env;
use tracing::debug;

// Environment variable constants from JavaScript (lines 790-795)
pub const AWS_ACCESS_KEY_ID: &str = "AWS_ACCESS_KEY_ID";
pub const AWS_SECRET_ACCESS_KEY: &str = "AWS_SECRET_ACCESS_KEY";
pub const AWS_SESSION_TOKEN: &str = "AWS_SESSION_TOKEN";
pub const AWS_CREDENTIAL_EXPIRATION: &str = "AWS_CREDENTIAL_EXPIRATION";
pub const AWS_CREDENTIAL_SCOPE: &str = "AWS_CREDENTIAL_SCOPE";
pub const AWS_ACCOUNT_ID: &str = "AWS_ACCOUNT_ID";

/// Trait for reading environment variables - allows for dependency injection in tests
pub trait EnvReader: Send + Sync {
    fn get_var(&self, key: &str) -> Option<String>;
}

/// System environment reader - reads from actual environment variables
#[derive(Debug, Clone)]
pub struct SystemEnvReader;

impl EnvReader for SystemEnvReader {
    fn get_var(&self, key: &str) -> Option<String> {
        env::var(key).ok()
    }
}

/// Mock environment reader for tests - uses a provided HashMap
#[derive(Debug, Clone)]
pub struct MockEnvReader {
    vars: HashMap<String, String>,
}

impl MockEnvReader {
    pub fn new() -> Self {
        Self {
            vars: HashMap::new(),
        }
    }

    pub fn with_var(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.vars.insert(key.into(), value.into());
        self
    }

    pub fn set_var(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.vars.insert(key.into(), value.into());
    }
}

impl EnvReader for MockEnvReader {
    fn get_var(&self, key: &str) -> Option<String> {
        self.vars.get(key).cloned()
    }
}

/// Environment variable credential provider
///
/// Reads AWS credentials from environment variables following the JavaScript implementation
/// at lines 797-838 in the extracted code.
pub struct FromEnv<E: EnvReader = SystemEnvReader> {
    logger: Option<String>,
    env_reader: E,
}

impl FromEnv<SystemEnvReader> {
    /// Create a new environment variable credential provider
    pub fn new() -> Self {
        Self {
            logger: None,
            env_reader: SystemEnvReader,
        }
    }

    /// Create a new environment variable credential provider with logger
    pub fn with_logger(logger: impl Into<String>) -> Self {
        Self {
            logger: Some(logger.into()),
            env_reader: SystemEnvReader,
        }
    }
}

impl<E: EnvReader> FromEnv<E> {
    /// Create a new environment variable credential provider with custom env reader
    pub fn with_env_reader(env_reader: E) -> Self {
        Self {
            logger: None,
            env_reader,
        }
    }

    /// Create a new environment variable credential provider with custom env reader and logger
    pub fn with_env_reader_and_logger(env_reader: E, logger: impl Into<String>) -> Self {
        Self {
            logger: Some(logger.into()),
            env_reader,
        }
    }
}

impl Default for FromEnv<SystemEnvReader> {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<E: EnvReader> CredentialProvider for FromEnv<E> {
    /// Provide credentials from environment variables
    ///
    /// This matches the JavaScript fromEnv implementation exactly:
    /// - Reads AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY (required)
    /// - Optionally reads AWS_SESSION_TOKEN, AWS_CREDENTIAL_EXPIRATION,
    ///   AWS_CREDENTIAL_SCOPE, and AWS_ACCOUNT_ID
    /// - Sets credential feature to "CREDENTIALS_ENV_VARS" with value "p"
    async fn provide_credentials(&self) -> Result<Credentials> {
        if let Some(ref logger) = self.logger {
            debug!("{} - fromEnv", logger);
        } else {
            debug!("@aws-sdk/credential-provider-env - fromEnv");
        }

        // Get required environment variables
        let access_key_id = self.env_reader.get_var(AWS_ACCESS_KEY_ID);
        let secret_access_key = self.env_reader.get_var(AWS_SECRET_ACCESS_KEY);

        if let (Some(access_key_id), Some(secret_access_key)) = (access_key_id, secret_access_key) {
            if !access_key_id.is_empty() && !secret_access_key.is_empty() {
                // Get optional environment variables
                let session_token = self.env_reader.get_var(AWS_SESSION_TOKEN)
                    .filter(|s| !s.is_empty());

                let expiration = self.env_reader.get_var(AWS_CREDENTIAL_EXPIRATION)
                    .and_then(|s| parse_credential_expiration(&s));

                let credential_scope = self.env_reader.get_var(AWS_CREDENTIAL_SCOPE)
                    .filter(|s| !s.is_empty());

                let account_id = self.env_reader.get_var(AWS_ACCOUNT_ID)
                    .filter(|s| !s.is_empty());

                // Build credentials object matching JavaScript structure
                let mut credentials = Credentials {
                    access_key_id,
                    secret_access_key,
                    session_token,
                    expiration,
                    credential_scope,
                    account_id,
                    credential_provider: None,
                    credential_provider_value: None,
                };

                // Set credential feature for tracking (JavaScript line 827)
                credentials = credentials.set_credential_feature("CREDENTIALS_ENV_VARS", "p");

                if let Some(ref logger) = self.logger {
                    debug!("{} - fromEnv::process.env", logger);
                } else {
                    debug!("@aws-sdk/credential-provider-env - fromEnv::process.env");
                }

                return Ok(credentials);
            }
        }

        // Throw error if credentials not found (JavaScript lines 830-835)
        let error = CredentialsProviderError::new("Unable to find environment variable credentials.")
            .with_logger(self.logger.clone().unwrap_or_else(|| "fromEnv".to_string()));

        Err(error.into())
    }
}

/// Create a new environment variable credential provider
///
/// This is a convenience function that matches the JavaScript export pattern.
pub fn from_env() -> FromEnv<SystemEnvReader> {
    FromEnv::new()
}

/// Create a new environment variable credential provider with options
///
/// This matches the JavaScript function signature with options parameter.
pub fn from_env_with_options(logger: Option<String>) -> FromEnv<SystemEnvReader> {
    match logger {
        Some(logger) => FromEnv::with_logger(logger),
        None => FromEnv::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_from_env_success() {
        let env_reader = MockEnvReader::new()
            .with_var(AWS_ACCESS_KEY_ID, "test_access_key")
            .with_var(AWS_SECRET_ACCESS_KEY, "test_secret_key")
            .with_var(AWS_SESSION_TOKEN, "test_session_token");

        let provider = FromEnv::with_env_reader(env_reader);
        let credentials = provider.provide_credentials().await.unwrap();

        assert_eq!(credentials.access_key_id, "test_access_key");
        assert_eq!(credentials.secret_access_key, "test_secret_key");
        assert_eq!(credentials.session_token, Some("test_session_token".to_string()));
        assert_eq!(credentials.credential_provider, Some("CREDENTIALS_ENV_VARS".to_string()));
        assert_eq!(credentials.credential_provider_value, Some("p".to_string()));
    }

    #[tokio::test]
    async fn test_from_env_missing_access_key() {
        let env_reader = MockEnvReader::new()
            .with_var(AWS_SECRET_ACCESS_KEY, "test_secret_key");
            // AWS_ACCESS_KEY_ID is not set

        let provider = FromEnv::with_env_reader(env_reader);
        let result = provider.provide_credentials().await;

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Unable to find environment variable credentials"));
    }

    #[tokio::test]
    async fn test_from_env_missing_secret_key() {
        let env_reader = MockEnvReader::new()
            .with_var(AWS_ACCESS_KEY_ID, "test_access_key");
            // AWS_SECRET_ACCESS_KEY is not set

        let provider = FromEnv::with_env_reader(env_reader);
        let result = provider.provide_credentials().await;

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Unable to find environment variable credentials"));
    }

    #[tokio::test]
    async fn test_from_env_empty_values() {
        let env_reader = MockEnvReader::new()
            .with_var(AWS_ACCESS_KEY_ID, "")
            .with_var(AWS_SECRET_ACCESS_KEY, "test_secret_key");

        let provider = FromEnv::with_env_reader(env_reader);
        let result = provider.provide_credentials().await;

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Unable to find environment variable credentials"));
    }

    #[tokio::test]
    async fn test_from_env_with_expiration() {
        let env_reader = MockEnvReader::new()
            .with_var(AWS_ACCESS_KEY_ID, "test_access_key")
            .with_var(AWS_SECRET_ACCESS_KEY, "test_secret_key")
            .with_var(AWS_CREDENTIAL_EXPIRATION, "2024-12-31T23:59:59Z");

        let provider = FromEnv::with_env_reader(env_reader);
        let credentials = provider.provide_credentials().await.unwrap();

        assert!(credentials.expiration.is_some());
    }

    #[tokio::test]
    async fn test_from_env_with_all_optional_fields() {
        let env_reader = MockEnvReader::new()
            .with_var(AWS_ACCESS_KEY_ID, "test_access_key")
            .with_var(AWS_SECRET_ACCESS_KEY, "test_secret_key")
            .with_var(AWS_SESSION_TOKEN, "test_session_token")
            .with_var(AWS_CREDENTIAL_EXPIRATION, "2024-12-31T23:59:59Z")
            .with_var(AWS_CREDENTIAL_SCOPE, "us-east-1")
            .with_var(AWS_ACCOUNT_ID, "123456789012");

        let provider = FromEnv::with_env_reader(env_reader);
        let credentials = provider.provide_credentials().await.unwrap();

        assert_eq!(credentials.access_key_id, "test_access_key");
        assert_eq!(credentials.secret_access_key, "test_secret_key");
        assert_eq!(credentials.session_token, Some("test_session_token".to_string()));
        assert!(credentials.expiration.is_some());
        assert_eq!(credentials.credential_scope, Some("us-east-1".to_string()));
        assert_eq!(credentials.account_id, Some("123456789012".to_string()));
    }

    #[tokio::test]
    async fn test_from_env_with_logger() {
        let env_reader = MockEnvReader::new()
            .with_var(AWS_ACCESS_KEY_ID, "test_access_key")
            .with_var(AWS_SECRET_ACCESS_KEY, "test_secret_key");

        let provider = FromEnv::with_env_reader_and_logger(env_reader, "test-logger");
        let credentials = provider.provide_credentials().await.unwrap();

        assert_eq!(credentials.access_key_id, "test_access_key");
        assert_eq!(credentials.secret_access_key, "test_secret_key");
    }

    #[tokio::test]
    async fn test_from_env_filters_empty_optional_values() {
        let env_reader = MockEnvReader::new()
            .with_var(AWS_ACCESS_KEY_ID, "test_access_key")
            .with_var(AWS_SECRET_ACCESS_KEY, "test_secret_key")
            .with_var(AWS_SESSION_TOKEN, "")
            .with_var(AWS_CREDENTIAL_SCOPE, "");

        let provider = FromEnv::with_env_reader(env_reader);
        let credentials = provider.provide_credentials().await.unwrap();

        assert_eq!(credentials.session_token, None);
        assert_eq!(credentials.credential_scope, None);
    }
}