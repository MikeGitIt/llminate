use super::{Credentials, CredentialProvider, CredentialsProviderError, memoize, MemoizedProvider};
use super::http::{FromHttp, from_http};
use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use std::env;
use std::time::Duration;
use tracing::debug;

// Environment variables for EC2 Instance Metadata Service
pub const AWS_EC2_METADATA_DISABLED: &str = "AWS_EC2_METADATA_DISABLED";
pub const AWS_EC2_METADATA_V1_DISABLED: &str = "AWS_EC2_METADATA_V1_DISABLED";

// Default timeouts and retry settings
const DEFAULT_TIMEOUT_MS: u64 = 1000;
const DEFAULT_MAX_RETRIES: u32 = 3;

// IMDSv2 endpoints
const IMDS_TOKEN_ENDPOINT: &str = "http://169.254.169.254/latest/api/token";
const IMDS_CREDENTIALS_ENDPOINT: &str = "http://169.254.169.254/latest/meta-data/iam/security-credentials/";

/// Instance metadata credentials response
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
struct InstanceMetadataCredentials {
    #[serde(rename = "AccessKeyId")]
    pub access_key_id: String,
    #[serde(rename = "SecretAccessKey")]
    pub secret_access_key: String,
    #[serde(rename = "Token")]
    pub token: String,
    #[serde(rename = "Expiration")]
    pub expiration: String,
    #[serde(rename = "Code")]
    pub code: String,
    #[serde(rename = "LastUpdated")]
    pub last_updated: String,
    #[serde(rename = "Type")]
    pub r#type: String,
}

/// Configuration for instance metadata provider (JavaScript value5062)
#[derive(Debug, Clone)]
pub struct InstanceMetadataConfig {
    pub timeout: Duration,
    pub max_retries: u32,
    pub ec2_metadata_v1_disabled: bool,
    pub ec2_metadata_v2_disable_session_token: bool,
    pub logger: Option<String>,
}

impl Default for InstanceMetadataConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_millis(DEFAULT_TIMEOUT_MS),
            max_retries: DEFAULT_MAX_RETRIES,
            ec2_metadata_v1_disabled: env::var(AWS_EC2_METADATA_V1_DISABLED)
                .map(|v| v == "true")
                .unwrap_or(false),
            ec2_metadata_v2_disable_session_token: false,
            logger: None,
        }
    }
}

/// Container metadata credential provider
///
/// This matches the JavaScript fromContainerMetadata implementation at lines 896-911.
pub struct FromContainerMetadata {
    http_provider: FromHttp,
    logger: Option<String>,
}

impl FromContainerMetadata {
    /// Create a new container metadata credential provider
    pub fn new() -> Self {
        Self {
            http_provider: from_http(),
            logger: None,
        }
    }

    /// Create with custom HTTP provider
    pub fn with_http_provider(http_provider: FromHttp) -> Self {
        Self {
            http_provider,
            logger: None,
        }
    }

    /// Set the logger
    pub fn with_logger(mut self, logger: impl Into<String>) -> Self {
        self.logger = Some(logger.into());
        self
    }

    /// Check if the credentials response is valid IMDS credentials
    fn is_imds_credentials(&self, creds: &Credentials) -> bool {
        creds.is_valid() && creds.session_token.is_some()
    }
}

impl Default for FromContainerMetadata {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CredentialProvider for FromContainerMetadata {
    /// Provide credentials from container metadata service
    ///
    /// This matches the JavaScript fromContainerMetadata implementation at lines 896-911.
    async fn provide_credentials(&self) -> Result<Credentials> {
        if let Some(ref logger) = self.logger {
            debug!("{} - fromContainerMetadata", logger);
        } else {
            debug!("@aws-sdk/credential-provider-imds - fromContainerMetadata");
        }

        // Get credentials from HTTP provider
        let credentials = self.http_provider.provide_credentials().await?;

        // Validate that response is valid IMDS credentials
        if !self.is_imds_credentials(&credentials) {
            return Err(CredentialsProviderError::new(
                "Invalid response from container metadata service."
            ).with_logger(self.logger.clone().unwrap_or_else(|| "fromContainerMetadata".to_string())).into());
        }

        // Set the credential feature for container metadata
        let credentials = credentials.set_credential_feature("CREDENTIALS_CONTAINER_METADATA", "c");

        Ok(credentials)
    }
}

/// Instance metadata credential provider
///
/// This matches the JavaScript fromInstanceMetadata implementation at lines 917-940.
pub struct FromInstanceMetadata {
    config: InstanceMetadataConfig,
    client: Client,
}

impl FromInstanceMetadata {
    /// Create a new instance metadata credential provider
    pub fn new() -> Self {
        let config = InstanceMetadataConfig::default();
        let client = Client::builder()
            .timeout(config.timeout)
            .build()
            .expect("Failed to create HTTP client for instance metadata");

        Self { config, client }
    }

    /// Create with custom configuration
    pub fn with_config(config: InstanceMetadataConfig) -> Self {
        let client = Client::builder()
            .timeout(config.timeout)
            .build()
            .expect("Failed to create HTTP client for instance metadata");

        Self { config, client }
    }

    /// Set timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.config.timeout = timeout;
        self.client = Client::builder()
            .timeout(timeout)
            .build()
            .expect("Failed to create HTTP client for instance metadata");
        self
    }

    /// Set max retries
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.config.max_retries = max_retries;
        self
    }

    /// Set EC2 metadata v1 disabled flag
    pub fn with_ec2_metadata_v1_disabled(mut self, disabled: bool) -> Self {
        self.config.ec2_metadata_v1_disabled = disabled;
        self
    }

    /// Set logger
    pub fn with_logger(mut self, logger: impl Into<String>) -> Self {
        self.config.logger = Some(logger.into());
        self
    }

    /// Get IMDSv2 session token
    async fn get_imds_v2_token(&self) -> Result<Option<String>> {
        if self.config.ec2_metadata_v2_disable_session_token {
            debug!("IMDSv2 session token disabled by configuration");
            return Ok(None);
        }

        let mut attempt = 0;
        while attempt <= self.config.max_retries {
            let response = self.client
                .put(IMDS_TOKEN_ENDPOINT)
                .header("X-aws-ec2-metadata-token-ttl-seconds", "21600") // 6 hours
                .send()
                .await;

            match response {
                Ok(resp) if resp.status().is_success() => {
                    let token = resp.text().await.context("Failed to read IMDSv2 token response")?;
                    debug!("Successfully obtained IMDSv2 session token");
                    return Ok(Some(token));
                }
                Ok(resp) => {
                    debug!("IMDSv2 token request failed with status: {}", resp.status());
                    if resp.status().as_u16() == 403 {
                        // IMDSv2 required but not available
                        return Ok(None);
                    }
                }
                Err(e) => {
                    debug!("IMDSv2 token request failed: {}", e);
                }
            }

            attempt += 1;
            if attempt <= self.config.max_retries {
                let delay = Duration::from_millis(100 * 2_u64.pow(attempt - 1)); // Exponential backoff
                tokio::time::sleep(delay).await;
            }
        }

        Ok(None)
    }

    /// Get available IAM roles
    async fn get_available_roles(&self, token: Option<&str>) -> Result<Vec<String>> {
        let mut request_builder = self.client.get(IMDS_CREDENTIALS_ENDPOINT);

        if let Some(token) = token {
            request_builder = request_builder.header("X-aws-ec2-metadata-token", token);
        } else if self.config.ec2_metadata_v1_disabled {
            return Err(CredentialsProviderError::new(
                "EC2 Instance Metadata Service v1 is disabled and v2 is not available"
            ).into());
        }

        let response = request_builder
            .send()
            .await
            .context("Failed to get available IAM roles")?;

        if !response.status().is_success() {
            return Err(CredentialsProviderError::new(
                format!("Failed to get IAM roles: HTTP {}", response.status())
            ).into());
        }

        let roles_text = response.text().await.context("Failed to read roles response")?;
        let roles: Vec<String> = roles_text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| line.trim().to_string())
            .collect();

        if roles.is_empty() {
            return Err(CredentialsProviderError::new(
                "No IAM roles available from instance metadata"
            ).into());
        }

        Ok(roles)
    }

    /// Get credentials for a specific IAM role
    async fn get_role_credentials(&self, role_name: &str, token: Option<&str>) -> Result<InstanceMetadataCredentials> {
        let credentials_url = format!("{}{}", IMDS_CREDENTIALS_ENDPOINT, role_name);
        let mut request_builder = self.client.get(&credentials_url);

        if let Some(token) = token {
            request_builder = request_builder.header("X-aws-ec2-metadata-token", token);
        }

        let response = request_builder
            .send()
            .await
            .context("Failed to get role credentials")?;

        if !response.status().is_success() {
            return Err(CredentialsProviderError::new(
                format!("Failed to get credentials for role {}: HTTP {}", role_name, response.status())
            ).into());
        }

        let credentials: InstanceMetadataCredentials = response
            .json()
            .await
            .context("Failed to parse credentials response")?;

        // Validate that credentials are successful
        if credentials.code != "Success" {
            return Err(CredentialsProviderError::new(
                format!("Credentials request failed with code: {}", credentials.code)
            ).into());
        }

        Ok(credentials)
    }
}

impl Default for FromInstanceMetadata {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CredentialProvider for FromInstanceMetadata {
    /// Provide credentials from EC2 instance metadata service
    ///
    /// This matches the JavaScript fromInstanceMetadata implementation at lines 917-940.
    async fn provide_credentials(&self) -> Result<Credentials> {
        if let Some(ref logger) = self.config.logger {
            debug!("{} - fromInstanceMetadata", logger);
        } else {
            debug!("@aws-sdk/credential-provider-imds - fromInstanceMetadata");
        }

        // Check if EC2 metadata is disabled
        if env::var(AWS_EC2_METADATA_DISABLED).map(|v| v == "true").unwrap_or(false) {
            return Err(CredentialsProviderError::new(
                "EC2 Instance Metadata Service access disabled"
            ).with_logger(self.config.logger.clone().unwrap_or_else(|| "fromInstanceMetadata".to_string())).into());
        }

        // Get IMDSv2 session token
        let token = self.get_imds_v2_token().await?;

        // Get available IAM roles
        let roles = self.get_available_roles(token.as_deref()).await?;

        // Use the first available role
        let role_name = &roles[0];
        debug!("Using IAM role: {}", role_name);

        // Get credentials for the role
        let metadata_creds = self.get_role_credentials(role_name, token.as_deref()).await?;

        // Parse expiration
        let expiration = super::parse_credential_expiration(&metadata_creds.expiration);

        // Build credentials object
        let credentials = Credentials {
            access_key_id: metadata_creds.access_key_id,
            secret_access_key: metadata_creds.secret_access_key,
            session_token: Some(metadata_creds.token),
            expiration,
            credential_scope: None,
            account_id: None,
            credential_provider: None,
            credential_provider_value: None,
        };

        // Set credential feature for instance metadata
        let credentials = credentials.set_credential_feature("CREDENTIALS_INSTANCE_METADATA", "i");

        Ok(credentials)
    }
}

/// Create a new container metadata credential provider with memoization
///
/// This matches the JavaScript fromContainerMetadata export pattern.
pub fn from_container_metadata() -> MemoizedProvider<FromContainerMetadata> {
    memoize(FromContainerMetadata::new())
}

/// Create a new container metadata credential provider with options
pub fn from_container_metadata_with_options(
    http_provider: Option<FromHttp>,
    logger: Option<String>,
) -> MemoizedProvider<FromContainerMetadata> {
    let mut provider = FromContainerMetadata::new();

    if let Some(http) = http_provider {
        provider = FromContainerMetadata::with_http_provider(http);
    }
    if let Some(logger) = logger {
        provider = provider.with_logger(logger);
    }

    memoize(provider)
}

/// Create a new instance metadata credential provider with memoization
///
/// This matches the JavaScript fromInstanceMetadata export pattern.
pub fn from_instance_metadata() -> MemoizedProvider<FromInstanceMetadata> {
    memoize(FromInstanceMetadata::new())
}

/// Create a new instance metadata credential provider with options
pub fn from_instance_metadata_with_options(
    config: Option<InstanceMetadataConfig>,
) -> MemoizedProvider<FromInstanceMetadata> {
    let provider = match config {
        Some(config) => FromInstanceMetadata::with_config(config),
        None => FromInstanceMetadata::new(),
    };

    memoize(provider)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wiremock::matchers::{method, path, header};

    // Helper to set and clean up environment variables
    struct EnvVar {
        key: String,
        original: Option<String>,
    }

    impl EnvVar {
        fn set(key: &str, value: &str) -> Self {
            let original = env::var(key).ok();
            env::set_var(key, value);
            Self {
                key: key.to_string(),
                original,
            }
        }
    }

    impl Drop for EnvVar {
        fn drop(&mut self) {
            match &self.original {
                Some(value) => env::set_var(&self.key, value),
                None => env::remove_var(&self.key),
            }
        }
    }

    #[tokio::test]
    async fn test_from_container_metadata_success() {
        let mock_server = MockServer::start().await;

        let credentials_response = serde_json::json!({
            "AccessKeyId": "container_access_key",
            "SecretAccessKey": "container_secret_key",
            "Token": "container_session_token",
            "Expiration": "2024-12-31T23:59:59Z"
        });

        Mock::given(method("GET"))
            .and(path("/v2/credentials/test-task"))
            .respond_with(ResponseTemplate::new(200).set_body_json(credentials_response))
            .mount(&mock_server)
            .await;

        let http_provider = FromHttp::new()
            .with_full_uri(format!("{}/v2/credentials/test-task", mock_server.uri()))
            .with_timeout(Duration::from_secs(5));

        let provider = FromContainerMetadata::with_http_provider(http_provider);
        let credentials = provider.provide_credentials().await.unwrap();

        assert_eq!(credentials.access_key_id, "container_access_key");
        assert_eq!(credentials.secret_access_key, "container_secret_key");
        assert_eq!(credentials.session_token, Some("container_session_token".to_string()));
        assert_eq!(credentials.credential_provider, Some("CREDENTIALS_CONTAINER_METADATA".to_string()));
    }

    #[tokio::test]
    async fn test_from_container_metadata_invalid_response() {
        let mock_server = MockServer::start().await;

        let invalid_response = serde_json::json!({
            "AccessKeyId": "container_access_key",
            "SecretAccessKey": "container_secret_key"
            // Missing Token - not valid IMDS credentials
        });

        Mock::given(method("GET"))
            .and(path("/v2/credentials/test-task"))
            .respond_with(ResponseTemplate::new(200).set_body_json(invalid_response))
            .mount(&mock_server)
            .await;

        let http_provider = FromHttp::new()
            .with_full_uri(format!("{}/v2/credentials/test-task", mock_server.uri()))
            .with_timeout(Duration::from_secs(5));

        let provider = FromContainerMetadata::with_http_provider(http_provider);
        let result = provider.provide_credentials().await;

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Invalid response from container metadata service"));
    }

    #[tokio::test]
    async fn test_instance_metadata_config_default() {
        let config = InstanceMetadataConfig::default();
        assert_eq!(config.timeout, Duration::from_millis(DEFAULT_TIMEOUT_MS));
        assert_eq!(config.max_retries, DEFAULT_MAX_RETRIES);
    }

    #[tokio::test]
    async fn test_instance_metadata_config_from_env() {
        let _v1_disabled = EnvVar::set(AWS_EC2_METADATA_V1_DISABLED, "true");

        let config = InstanceMetadataConfig::default();
        assert!(config.ec2_metadata_v1_disabled);
    }

    #[tokio::test]
    async fn test_from_instance_metadata_disabled() {
        let _metadata_disabled = EnvVar::set(AWS_EC2_METADATA_DISABLED, "true");

        let provider = FromInstanceMetadata::new();
        let result = provider.provide_credentials().await;

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("EC2 Instance Metadata Service access disabled"));
    }

    #[tokio::test]
    async fn test_from_instance_metadata_builder_pattern() {
        let provider = FromInstanceMetadata::new()
            .with_timeout(Duration::from_secs(5))
            .with_max_retries(5)
            .with_ec2_metadata_v1_disabled(true)
            .with_logger("test-logger");

        assert_eq!(provider.config.timeout, Duration::from_secs(5));
        assert_eq!(provider.config.max_retries, 5);
        assert!(provider.config.ec2_metadata_v1_disabled);
        assert_eq!(provider.config.logger, Some("test-logger".to_string()));
    }

    #[tokio::test]
    async fn test_memoized_providers() {
        // Test that the convenience functions return memoized providers
        let _container_provider = from_container_metadata();
        let _instance_provider = from_instance_metadata();

        // These should compile and not panic
        assert!(true);
    }
}