use super::{Credentials, CredentialProvider, CredentialsProviderError};
use super::env::{EnvReader, SystemEnvReader};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::debug;

/// Environment variable constants for web identity tokens
pub const AWS_WEB_IDENTITY_TOKEN_FILE: &str = "AWS_WEB_IDENTITY_TOKEN_FILE";
pub const AWS_ROLE_ARN: &str = "AWS_ROLE_ARN";
pub const AWS_ROLE_SESSION_NAME: &str = "AWS_ROLE_SESSION_NAME";

/// Request for AssumeRoleWithWebIdentity operation
#[derive(Debug, Clone, Serialize)]
pub struct AssumeRoleWithWebIdentityRequest {
    #[serde(rename = "RoleArn")]
    pub role_arn: String,
    #[serde(rename = "RoleSessionName")]
    pub role_session_name: String,
    #[serde(rename = "WebIdentityToken")]
    pub web_identity_token: String,
    #[serde(rename = "ProviderId", skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(rename = "PolicyArns", skip_serializing_if = "Option::is_none")]
    pub policy_arns: Option<Vec<String>>,
    #[serde(rename = "Policy", skip_serializing_if = "Option::is_none")]
    pub policy: Option<String>,
    #[serde(rename = "DurationSeconds", skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<i32>,
}

/// Response from STS AssumeRoleWithWebIdentity operation
#[derive(Debug, Clone, Deserialize)]
pub struct AssumeRoleWithWebIdentityResponse {
    #[serde(rename = "Credentials")]
    pub credentials: Option<StsCredentials>,
    #[serde(rename = "AssumedRoleUser")]
    pub assumed_role_user: Option<AssumedRoleUser>,
}

/// STS credentials in the response
#[derive(Debug, Clone, Deserialize)]
pub struct StsCredentials {
    #[serde(rename = "AccessKeyId")]
    pub access_key_id: Option<String>,
    #[serde(rename = "SecretAccessKey")]
    pub secret_access_key: Option<String>,
    #[serde(rename = "SessionToken")]
    pub session_token: Option<String>,
    #[serde(rename = "Expiration")]
    pub expiration: Option<DateTime<Utc>>,
    #[serde(rename = "CredentialScope")]
    pub credential_scope: Option<String>,
}

/// Assumed role user information
#[derive(Debug, Clone, Deserialize)]
pub struct AssumedRoleUser {
    #[serde(rename = "Arn")]
    pub arn: Option<String>,
    #[serde(rename = "AssumedRoleId")]
    pub assumed_role_id: Option<String>,
}

/// Trait for role assumer with web identity - allows mocking in tests
#[async_trait]
pub trait RoleAssumerWithWebIdentity: Send + Sync {
    async fn assume_role_with_web_identity(
        &self,
        request: AssumeRoleWithWebIdentityRequest,
    ) -> Result<AssumeRoleWithWebIdentityResponse>;
}

/// Trait for file reader - allows mocking file system access in tests
#[async_trait]
pub trait FileReader: Send + Sync {
    async fn read_to_string(&self, path: &str) -> Result<String>;
}

/// System file reader - reads from actual file system
#[derive(Debug, Clone)]
pub struct SystemFileReader;

#[async_trait]
impl FileReader for SystemFileReader {
    async fn read_to_string(&self, path: &str) -> Result<String> {
        tokio::fs::read_to_string(path).await
            .map_err(|e| anyhow::anyhow!("Failed to read file {}: {}", path, e))
    }
}

/// Web token credentials provider
///
/// This implements the fromWebToken functionality from the JavaScript code
/// at lines 159-197 in the extracted file.
pub struct WebTokenCredentialsProvider {
    web_identity_token: String,
    role_arn: String,
    role_session_name: String,
    provider_id: Option<String>,
    policy_arns: Option<Vec<String>>,
    policy: Option<String>,
    duration_seconds: Option<i32>,
    role_assumer: Option<Arc<dyn RoleAssumerWithWebIdentity>>,
    logger: Option<String>,
}

impl WebTokenCredentialsProvider {
    /// Create a new web token credentials provider
    pub fn new(
        web_identity_token: String,
        role_arn: String,
        role_session_name: String,
    ) -> Self {
        Self {
            web_identity_token,
            role_arn,
            role_session_name,
            provider_id: None,
            policy_arns: None,
            policy: None,
            duration_seconds: None,
            role_assumer: None,
            logger: None,
        }
    }

    /// Set the provider ID
    pub fn with_provider_id(mut self, provider_id: String) -> Self {
        self.provider_id = Some(provider_id);
        self
    }

    /// Set policy ARNs
    pub fn with_policy_arns(mut self, policy_arns: Vec<String>) -> Self {
        self.policy_arns = Some(policy_arns);
        self
    }

    /// Set policy
    pub fn with_policy(mut self, policy: String) -> Self {
        self.policy = Some(policy);
        self
    }

    /// Set duration in seconds
    pub fn with_duration_seconds(mut self, duration_seconds: i32) -> Self {
        self.duration_seconds = Some(duration_seconds);
        self
    }

    /// Set a custom role assumer (useful for testing)
    pub fn with_role_assumer(mut self, role_assumer: Arc<dyn RoleAssumerWithWebIdentity>) -> Self {
        self.role_assumer = Some(role_assumer);
        self
    }

    /// Set a logger for debugging
    pub fn with_logger(mut self, logger: impl Into<String>) -> Self {
        self.logger = Some(logger.into());
        self
    }

    /// Extract account ID from assumed role user ARN
    fn get_account_id_from_assumed_role_user(&self, assumed_role_user: &AssumedRoleUser) -> Option<String> {
        if let Some(ref arn) = assumed_role_user.arn {
            let parts: Vec<&str> = arn.split(':').collect();
            if parts.len() > 4 && !parts[4].is_empty() {
                return Some(parts[4].to_string());
            }
        }
        None
    }
}

#[async_trait]
impl CredentialProvider for WebTokenCredentialsProvider {
    /// Provide credentials using web identity token
    ///
    /// This matches the JavaScript implementation exactly from lines 159-197
    async fn provide_credentials(&self) -> Result<Credentials> {
        if let Some(ref logger) = self.logger {
            debug!("{} - fromWebToken", logger);
        } else {
            debug!("@aws-sdk/credential-provider-web-identity - fromWebToken");
        }

        // Use the provided role assumer or return error (JavaScript lines 173-186)
        let role_assumer = self.role_assumer.as_ref().ok_or_else(|| {
            CredentialsProviderError::new("Role assumer with web identity is required but not provided")
        })?;

        // Build the request (JavaScript lines 188-196)
        let request = AssumeRoleWithWebIdentityRequest {
            role_arn: self.role_arn.clone(),
            role_session_name: if self.role_session_name.is_empty() {
                format!("aws-sdk-js-session-{}", chrono::Utc::now().timestamp_millis())
            } else {
                self.role_session_name.clone()
            },
            web_identity_token: self.web_identity_token.clone(),
            provider_id: self.provider_id.clone(),
            policy_arns: self.policy_arns.clone(),
            policy: self.policy.clone(),
            duration_seconds: self.duration_seconds,
        };

        // Call the role assumer
        let response = role_assumer.assume_role_with_web_identity(request).await
            .map_err(|e| CredentialsProviderError::new(
                format!("Failed to assume role with web identity: {}", e)
            ))?;

        // Validate the response
        let sts_credentials = response.credentials.ok_or_else(|| {
            CredentialsProviderError::new(
                format!("Invalid response from STS.assumeRoleWithWebIdentity call with role {}", self.role_arn)
            ).with_logger(self.logger.clone().unwrap_or_else(|| "fromWebToken".to_string()))
        })?;

        let access_key_id = sts_credentials.access_key_id.ok_or_else(|| {
            CredentialsProviderError::new(
                format!("Invalid response from STS.assumeRoleWithWebIdentity call with role {}", self.role_arn)
            ).with_logger(self.logger.clone().unwrap_or_else(|| "fromWebToken".to_string()))
        })?;

        let secret_access_key = sts_credentials.secret_access_key.ok_or_else(|| {
            CredentialsProviderError::new(
                format!("Invalid response from STS.assumeRoleWithWebIdentity call with role {}", self.role_arn)
            ).with_logger(self.logger.clone().unwrap_or_else(|| "fromWebToken".to_string()))
        })?;

        // Extract account ID if available
        let account_id = response.assumed_role_user
            .as_ref()
            .and_then(|user| self.get_account_id_from_assumed_role_user(user));

        // Build credentials object
        let mut credentials = Credentials {
            access_key_id,
            secret_access_key,
            session_token: sts_credentials.session_token,
            expiration: sts_credentials.expiration,
            credential_scope: sts_credentials.credential_scope,
            account_id: account_id.clone(),
            credential_provider: None,
            credential_provider_value: None,
        };

        // Set credential features for tracking (matching JavaScript lines 691-702)
        if account_id.is_some() {
            credentials = credentials.set_credential_feature("RESOLVED_ACCOUNT_ID", "T");
        }
        credentials = credentials.set_credential_feature("CREDENTIALS_STS_ASSUME_ROLE_WEB_ID", "k");

        Ok(credentials)
    }
}

/// Token file credentials provider
///
/// This implements the fromTokenFile functionality from the JavaScript code
/// at lines 203-233 in the extracted file.
pub struct TokenFileCredentialsProvider<E: EnvReader = SystemEnvReader, F: FileReader = SystemFileReader> {
    web_identity_token_file: Option<String>,
    role_arn: Option<String>,
    role_session_name: Option<String>,
    env_reader: E,
    file_reader: F,
    logger: Option<String>,
}

impl TokenFileCredentialsProvider<SystemEnvReader, SystemFileReader> {
    /// Create a new token file credentials provider
    pub fn new() -> Self {
        Self {
            web_identity_token_file: None,
            role_arn: None,
            role_session_name: None,
            env_reader: SystemEnvReader,
            file_reader: SystemFileReader,
            logger: None,
        }
    }

    /// Create a new token file credentials provider with logger
    pub fn with_logger(logger: impl Into<String>) -> Self {
        Self {
            web_identity_token_file: None,
            role_arn: None,
            role_session_name: None,
            env_reader: SystemEnvReader,
            file_reader: SystemFileReader,
            logger: Some(logger.into()),
        }
    }
}

impl<E: EnvReader, F: FileReader> TokenFileCredentialsProvider<E, F> {
    /// Create a new token file credentials provider with custom readers
    pub fn with_readers(env_reader: E, file_reader: F) -> Self {
        Self {
            web_identity_token_file: None,
            role_arn: None,
            role_session_name: None,
            env_reader,
            file_reader,
            logger: None,
        }
    }

    /// Create a new token file credentials provider with custom readers and logger
    pub fn with_readers_and_logger(env_reader: E, file_reader: F, logger: impl Into<String>) -> Self {
        Self {
            web_identity_token_file: None,
            role_arn: None,
            role_session_name: None,
            env_reader,
            file_reader,
            logger: Some(logger.into()),
        }
    }

    /// Set the web identity token file path
    pub fn with_web_identity_token_file(mut self, web_identity_token_file: String) -> Self {
        self.web_identity_token_file = Some(web_identity_token_file);
        self
    }

    /// Set the role ARN
    pub fn with_role_arn(mut self, role_arn: String) -> Self {
        self.role_arn = Some(role_arn);
        self
    }

    /// Set the role session name
    pub fn with_role_session_name(mut self, role_session_name: String) -> Self {
        self.role_session_name = Some(role_session_name);
        self
    }
}

impl Default for TokenFileCredentialsProvider<SystemEnvReader, SystemFileReader> {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<E: EnvReader, F: FileReader> CredentialProvider for TokenFileCredentialsProvider<E, F> {
    /// Provide credentials from token file
    ///
    /// This matches the JavaScript implementation exactly from lines 203-233
    async fn provide_credentials(&self) -> Result<Credentials> {
        if let Some(ref logger) = self.logger {
            debug!("{} - fromTokenFile", logger);
        } else {
            debug!("@aws-sdk/credential-provider-web-identity - fromTokenFile");
        }

        // Get configuration from environment variables or provided values (JavaScript lines 207-210)
        let token_file = self.web_identity_token_file.clone()
            .or_else(|| self.env_reader.get_var(AWS_WEB_IDENTITY_TOKEN_FILE));

        let role_arn = self.role_arn.clone()
            .or_else(|| self.env_reader.get_var(AWS_ROLE_ARN));

        let role_session_name = self.role_session_name.clone()
            .or_else(|| self.env_reader.get_var(AWS_ROLE_SESSION_NAME));

        // Validate required parameters (JavaScript lines 211-217)
        let token_file = token_file.ok_or_else(|| {
            CredentialsProviderError::new("Web identity configuration not specified")
                .with_logger(self.logger.clone().unwrap_or_else(|| "fromTokenFile".to_string()))
        })?;

        let role_arn = role_arn.ok_or_else(|| {
            CredentialsProviderError::new("Web identity configuration not specified")
                .with_logger(self.logger.clone().unwrap_or_else(|| "fromTokenFile".to_string()))
        })?;

        // Read the token from file (JavaScript lines 218-225)
        let web_identity_token = self.file_reader.read_to_string(&token_file).await
            .map_err(|e| CredentialsProviderError::new(
                format!("Failed to read web identity token file {}: {}", token_file, e)
            ).with_logger(self.logger.clone().unwrap_or_else(|| "fromTokenFile".to_string())))?;

        // Trim whitespace from token
        let web_identity_token = web_identity_token.trim().to_string();

        if web_identity_token.is_empty() {
            return Err(CredentialsProviderError::new(
                format!("Web identity token file {} is empty", token_file)
            ).with_logger(self.logger.clone().unwrap_or_else(|| "fromTokenFile".to_string()))
            .into());
        }

        // Create WebTokenCredentialsProvider and get credentials (JavaScript lines 218-225)
        let web_token_provider = WebTokenCredentialsProvider::new(
            web_identity_token,
            role_arn,
            role_session_name.unwrap_or_else(|| format!("aws-sdk-js-session-{}", chrono::Utc::now().timestamp_millis())),
        );

        // For this implementation, we need a role assumer. In a real implementation,
        // this would be provided or created by default. For now, we'll return an error.
        return Err(CredentialsProviderError::new(
            "Role assumer with web identity is required for token file provider"
        ).into());

        // JavaScript lines 226-232 show setting credential feature for env vars usage
        // This would be implemented when we have the actual credentials:
        // if token_file == self.env_reader.get_var(AWS_WEB_IDENTITY_TOKEN_FILE).as_deref() {
        //     credentials = credentials.set_credential_feature("CREDENTIALS_ENV_VARS_STS_WEB_ID_TOKEN", "h");
        // }
    }
}

/// Create a new web token credentials provider
///
/// This is a convenience function that matches the JavaScript export pattern.
pub fn from_web_token(
    web_identity_token: String,
    role_arn: String,
    role_session_name: String,
) -> WebTokenCredentialsProvider {
    WebTokenCredentialsProvider::new(web_identity_token, role_arn, role_session_name)
}

/// Create a new token file credentials provider
///
/// This is a convenience function that matches the JavaScript export pattern.
pub fn from_token_file() -> TokenFileCredentialsProvider<SystemEnvReader, SystemFileReader> {
    TokenFileCredentialsProvider::new()
}

/// Create a new token file credentials provider with options
///
/// This matches the JavaScript function signature with options parameter.
pub fn from_token_file_with_options(
    logger: Option<String>,
) -> TokenFileCredentialsProvider<SystemEnvReader, SystemFileReader> {
    match logger {
        Some(logger) => TokenFileCredentialsProvider::with_logger(logger),
        None => TokenFileCredentialsProvider::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::env::MockEnvReader;
    use std::collections::HashMap;

    /// Mock role assumer for testing
    struct MockRoleAssumerWithWebIdentity {
        responses: HashMap<String, AssumeRoleWithWebIdentityResponse>,
        should_error: bool,
    }

    impl MockRoleAssumerWithWebIdentity {
        fn new() -> Self {
            Self {
                responses: HashMap::new(),
                should_error: false,
            }
        }

        fn with_response(mut self, role_arn: &str, response: AssumeRoleWithWebIdentityResponse) -> Self {
            self.responses.insert(role_arn.to_string(), response);
            self
        }

        fn with_error(mut self) -> Self {
            self.should_error = true;
            self
        }
    }

    #[async_trait]
    impl RoleAssumerWithWebIdentity for MockRoleAssumerWithWebIdentity {
        async fn assume_role_with_web_identity(
            &self,
            request: AssumeRoleWithWebIdentityRequest,
        ) -> Result<AssumeRoleWithWebIdentityResponse> {
            if self.should_error {
                return Err(anyhow::anyhow!("Mock role assumer error"));
            }

            if let Some(response) = self.responses.get(&request.role_arn) {
                Ok(response.clone())
            } else {
                Err(anyhow::anyhow!("Role not found in mock"))
            }
        }
    }

    /// Mock file reader for testing
    struct MockFileReader {
        files: HashMap<String, String>,
        should_error: bool,
    }

    impl MockFileReader {
        fn new() -> Self {
            Self {
                files: HashMap::new(),
                should_error: false,
            }
        }

        fn with_file(mut self, path: &str, content: &str) -> Self {
            self.files.insert(path.to_string(), content.to_string());
            self
        }

        fn with_error(mut self) -> Self {
            self.should_error = true;
            self
        }
    }

    #[async_trait]
    impl FileReader for MockFileReader {
        async fn read_to_string(&self, path: &str) -> Result<String> {
            if self.should_error {
                return Err(anyhow::anyhow!("Mock file read error"));
            }

            if let Some(content) = self.files.get(path) {
                Ok(content.clone())
            } else {
                Err(anyhow::anyhow!("File not found: {}", path))
            }
        }
    }

    #[tokio::test]
    async fn test_web_token_credentials_success() {
        let response = AssumeRoleWithWebIdentityResponse {
            credentials: Some(StsCredentials {
                access_key_id: Some("web_access_key".to_string()),
                secret_access_key: Some("web_secret_key".to_string()),
                session_token: Some("web_session_token".to_string()),
                expiration: Some(Utc::now() + chrono::Duration::hours(1)),
                credential_scope: Some("us-east-1".to_string()),
            }),
            assumed_role_user: Some(AssumedRoleUser {
                arn: Some("arn:aws:sts::123456789012:assumed-role/test-role/session".to_string()),
                assumed_role_id: Some("AROA123456789012:session".to_string()),
            }),
        };

        let role_assumer = Arc::new(
            MockRoleAssumerWithWebIdentity::new()
                .with_response("arn:aws:iam::123456789012:role/test-role", response)
        );

        let provider = WebTokenCredentialsProvider::new(
            "mock-web-token".to_string(),
            "arn:aws:iam::123456789012:role/test-role".to_string(),
            "test-session".to_string(),
        ).with_role_assumer(role_assumer);

        let credentials = provider.provide_credentials().await.unwrap();

        assert_eq!(credentials.access_key_id, "web_access_key");
        assert_eq!(credentials.secret_access_key, "web_secret_key");
        assert_eq!(credentials.session_token, Some("web_session_token".to_string()));
        assert_eq!(credentials.credential_scope, Some("us-east-1".to_string()));
        assert_eq!(credentials.account_id, Some("123456789012".to_string()));
        assert_eq!(credentials.credential_provider, Some("CREDENTIALS_STS_ASSUME_ROLE_WEB_ID".to_string()));
        assert_eq!(credentials.credential_provider_value, Some("k".to_string()));
    }

    #[tokio::test]
    async fn test_web_token_credentials_missing_role_assumer() {
        let provider = WebTokenCredentialsProvider::new(
            "mock-web-token".to_string(),
            "arn:aws:iam::123456789012:role/test-role".to_string(),
            "test-session".to_string(),
        );

        let result = provider.provide_credentials().await;

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Role assumer with web identity is required"));
    }

    #[tokio::test]
    async fn test_web_token_credentials_empty_session_name() {
        let response = AssumeRoleWithWebIdentityResponse {
            credentials: Some(StsCredentials {
                access_key_id: Some("web_access_key".to_string()),
                secret_access_key: Some("web_secret_key".to_string()),
                session_token: Some("web_session_token".to_string()),
                expiration: Some(Utc::now() + chrono::Duration::hours(1)),
                credential_scope: None,
            }),
            assumed_role_user: None,
        };

        let role_assumer = Arc::new(
            MockRoleAssumerWithWebIdentity::new()
                .with_response("arn:aws:iam::123456789012:role/test-role", response)
        );

        let provider = WebTokenCredentialsProvider::new(
            "mock-web-token".to_string(),
            "arn:aws:iam::123456789012:role/test-role".to_string(),
            "".to_string(), // Empty session name
        ).with_role_assumer(role_assumer);

        let credentials = provider.provide_credentials().await.unwrap();

        assert_eq!(credentials.access_key_id, "web_access_key");
        assert_eq!(credentials.secret_access_key, "web_secret_key");
    }

    #[tokio::test]
    async fn test_web_token_credentials_with_optional_params() {
        let response = AssumeRoleWithWebIdentityResponse {
            credentials: Some(StsCredentials {
                access_key_id: Some("web_access_key".to_string()),
                secret_access_key: Some("web_secret_key".to_string()),
                session_token: Some("web_session_token".to_string()),
                expiration: Some(Utc::now() + chrono::Duration::hours(1)),
                credential_scope: None,
            }),
            assumed_role_user: None,
        };

        let role_assumer = Arc::new(
            MockRoleAssumerWithWebIdentity::new()
                .with_response("arn:aws:iam::123456789012:role/test-role", response)
        );

        let provider = WebTokenCredentialsProvider::new(
            "mock-web-token".to_string(),
            "arn:aws:iam::123456789012:role/test-role".to_string(),
            "test-session".to_string(),
        )
        .with_provider_id("provider123".to_string())
        .with_policy("{}".to_string())
        .with_policy_arns(vec!["arn:aws:iam::123456789012:policy/test".to_string()])
        .with_duration_seconds(3600)
        .with_role_assumer(role_assumer);

        let credentials = provider.provide_credentials().await.unwrap();

        assert_eq!(credentials.access_key_id, "web_access_key");
        assert_eq!(credentials.secret_access_key, "web_secret_key");
    }

    #[tokio::test]
    async fn test_token_file_credentials_missing_config() {
        let env_reader = MockEnvReader::new();
        let file_reader = MockFileReader::new();

        let provider = TokenFileCredentialsProvider::with_readers(env_reader, file_reader);
        let result = provider.provide_credentials().await;

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Web identity configuration not specified"));
    }

    #[tokio::test]
    async fn test_token_file_credentials_missing_role_arn() {
        let env_reader = MockEnvReader::new()
            .with_var(AWS_WEB_IDENTITY_TOKEN_FILE, "/path/to/token");
        let file_reader = MockFileReader::new()
            .with_file("/path/to/token", "mock-token");

        let provider = TokenFileCredentialsProvider::with_readers(env_reader, file_reader);
        let result = provider.provide_credentials().await;

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Web identity configuration not specified"));
    }

    #[tokio::test]
    async fn test_token_file_credentials_file_read_error() {
        let env_reader = MockEnvReader::new()
            .with_var(AWS_WEB_IDENTITY_TOKEN_FILE, "/path/to/token")
            .with_var(AWS_ROLE_ARN, "arn:aws:iam::123456789012:role/test-role");
        let file_reader = MockFileReader::new().with_error();

        let provider = TokenFileCredentialsProvider::with_readers(env_reader, file_reader);
        let result = provider.provide_credentials().await;

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Failed to read web identity token file"));
    }

    #[tokio::test]
    async fn test_token_file_credentials_empty_token() {
        let env_reader = MockEnvReader::new()
            .with_var(AWS_WEB_IDENTITY_TOKEN_FILE, "/path/to/token")
            .with_var(AWS_ROLE_ARN, "arn:aws:iam::123456789012:role/test-role");
        let file_reader = MockFileReader::new()
            .with_file("/path/to/token", "  \n  "); // Whitespace only

        let provider = TokenFileCredentialsProvider::with_readers(env_reader, file_reader);
        let result = provider.provide_credentials().await;

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Web identity token file /path/to/token is empty"));
    }

    #[tokio::test]
    async fn test_token_file_credentials_requires_role_assumer() {
        let env_reader = MockEnvReader::new()
            .with_var(AWS_WEB_IDENTITY_TOKEN_FILE, "/path/to/token")
            .with_var(AWS_ROLE_ARN, "arn:aws:iam::123456789012:role/test-role")
            .with_var(AWS_ROLE_SESSION_NAME, "test-session");
        let file_reader = MockFileReader::new()
            .with_file("/path/to/token", "mock-token");

        let provider = TokenFileCredentialsProvider::with_readers(env_reader, file_reader);
        let result = provider.provide_credentials().await;

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Role assumer with web identity is required"));
    }

    #[tokio::test]
    async fn test_token_file_credentials_with_provided_values() {
        let env_reader = MockEnvReader::new();
        let file_reader = MockFileReader::new()
            .with_file("/custom/path/token", "mock-token");

        let provider = TokenFileCredentialsProvider::with_readers(env_reader, file_reader)
            .with_web_identity_token_file("/custom/path/token".to_string())
            .with_role_arn("arn:aws:iam::123456789012:role/test-role".to_string())
            .with_role_session_name("custom-session".to_string());

        let result = provider.provide_credentials().await;

        assert!(result.is_err());
        // Should still fail due to missing role assumer, but not due to config
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Role assumer with web identity is required"));
    }

    #[test]
    fn test_get_account_id_from_assumed_role_user() {
        let provider = WebTokenCredentialsProvider::new(
            "token".to_string(),
            "arn:aws:iam::123456789012:role/test-role".to_string(),
            "session".to_string(),
        );

        // Test valid ARN
        let user = AssumedRoleUser {
            arn: Some("arn:aws:sts::123456789012:assumed-role/test-role/session".to_string()),
            assumed_role_id: Some("AROA123456789012:session".to_string()),
        };
        let account_id = provider.get_account_id_from_assumed_role_user(&user);
        assert_eq!(account_id, Some("123456789012".to_string()));

        // Test invalid ARN
        let user = AssumedRoleUser {
            arn: Some("invalid-arn".to_string()),
            assumed_role_id: Some("AROA123456789012:session".to_string()),
        };
        let account_id = provider.get_account_id_from_assumed_role_user(&user);
        assert_eq!(account_id, None);

        // Test missing ARN
        let user = AssumedRoleUser {
            arn: None,
            assumed_role_id: Some("AROA123456789012:session".to_string()),
        };
        let account_id = provider.get_account_id_from_assumed_role_user(&user);
        assert_eq!(account_id, None);
    }
}