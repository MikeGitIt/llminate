use super::{Credentials, CredentialProvider, CredentialsProviderError, parse_credential_expiration};
use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use std::env;
use std::time::Duration;
use tokio::fs;
use tracing::debug;

// Environment variable constants from JavaScript (lines 844-848)
pub const AWS_CONTAINER_CREDENTIALS_RELATIVE_URI: &str = "AWS_CONTAINER_CREDENTIALS_RELATIVE_URI";
pub const AWS_CONTAINER_CREDENTIALS_FULL_URI: &str = "AWS_CONTAINER_CREDENTIALS_FULL_URI";
pub const AWS_CONTAINER_AUTHORIZATION_TOKEN_FILE: &str = "AWS_CONTAINER_AUTHORIZATION_TOKEN_FILE";
pub const AWS_CONTAINER_AUTHORIZATION_TOKEN: &str = "AWS_CONTAINER_AUTHORIZATION_TOKEN";

// Default container metadata endpoint
pub const CONTAINER_METADATA_ENDPOINT: &str = "http://169.254.170.2";

/// HTTP response structure for container metadata credentials
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
struct ContainerMetadataCredentials {
    #[serde(rename = "AccessKeyId")]
    pub access_key_id: String,
    #[serde(rename = "SecretAccessKey")]
    pub secret_access_key: String,
    #[serde(rename = "Token")]
    pub token: Option<String>,
    #[serde(rename = "Expiration")]
    pub expiration: Option<String>,
    #[serde(rename = "RoleArn")]
    pub role_arn: Option<String>,
}

/// HTTP credential provider for container metadata
///
/// This matches the JavaScript fromHttp implementation at lines 850-890.
pub struct FromHttp {
    /// Container credentials relative URI
    pub aws_container_credentials_relative_uri: Option<String>,
    /// Container credentials full URI
    pub aws_container_credentials_full_uri: Option<String>,
    /// Authorization token
    pub aws_container_authorization_token: Option<String>,
    /// Authorization token file path
    pub aws_container_authorization_token_file: Option<String>,
    /// HTTP timeout in milliseconds
    pub timeout: Duration,
    /// Logger name
    pub logger: Option<String>,
}

impl FromHttp {
    /// Create a new HTTP credential provider with default settings
    pub fn new() -> Self {
        Self {
            aws_container_credentials_relative_uri: None,
            aws_container_credentials_full_uri: None,
            aws_container_authorization_token: None,
            aws_container_authorization_token_file: None,
            timeout: Duration::from_millis(1000), // JavaScript default
            logger: None,
        }
    }

    /// Create HTTP credential provider from environment variables
    pub fn from_env() -> Self {
        Self {
            aws_container_credentials_relative_uri: env::var(AWS_CONTAINER_CREDENTIALS_RELATIVE_URI).ok(),
            aws_container_credentials_full_uri: env::var(AWS_CONTAINER_CREDENTIALS_FULL_URI).ok(),
            aws_container_authorization_token: env::var(AWS_CONTAINER_AUTHORIZATION_TOKEN).ok(),
            aws_container_authorization_token_file: env::var(AWS_CONTAINER_AUTHORIZATION_TOKEN_FILE).ok(),
            timeout: Duration::from_millis(1000),
            logger: None,
        }
    }

    /// Set the relative URI
    pub fn with_relative_uri(mut self, uri: impl Into<String>) -> Self {
        self.aws_container_credentials_relative_uri = Some(uri.into());
        self
    }

    /// Set the full URI
    pub fn with_full_uri(mut self, uri: impl Into<String>) -> Self {
        self.aws_container_credentials_full_uri = Some(uri.into());
        self
    }

    /// Set the authorization token
    pub fn with_auth_token(mut self, token: impl Into<String>) -> Self {
        self.aws_container_authorization_token = Some(token.into());
        self
    }

    /// Set the authorization token file
    pub fn with_auth_token_file(mut self, file: impl Into<String>) -> Self {
        self.aws_container_authorization_token_file = Some(file.into());
        self
    }

    /// Set the timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set the logger
    pub fn with_logger(mut self, logger: impl Into<String>) -> Self {
        self.logger = Some(logger.into());
        self
    }

    /// Determine the URL to use for the request (JavaScript lines 861-871)
    fn get_credentials_url(&self) -> Result<Url> {
        if let Some(ref full_uri) = self.aws_container_credentials_full_uri {
            return Ok(Url::parse(full_uri).context("Invalid full URI")?);
        }

        if let Some(ref relative_uri) = self.aws_container_credentials_relative_uri {
            let base_url = Url::parse(CONTAINER_METADATA_ENDPOINT)
                .context("Invalid container metadata endpoint")?;
            return Ok(base_url.join(relative_uri).context("Invalid relative URI")?);
        }

        Err(CredentialsProviderError::new(
            "The AWS_CONTAINER_CREDENTIALS_RELATIVE_URI or AWS_CONTAINER_CREDENTIALS_FULL_URI environment variable must be set to use fromHttp credential provider."
        ).into())
    }

    /// Get authorization headers (JavaScript lines 872-884)
    async fn get_authorization_header(&self) -> Result<Option<String>> {
        if let Some(ref token) = self.aws_container_authorization_token {
            return Ok(Some(token.clone()));
        }

        if let Some(ref token_file) = self.aws_container_authorization_token_file {
            match fs::read_to_string(token_file).await {
                Ok(token) => Ok(Some(token.trim().to_string())),
                Err(e) => {
                    let error = CredentialsProviderError::new(
                        format!("Unable to read authorization token from file {}: {}", token_file, e)
                    ).with_logger(self.logger.clone().unwrap_or_else(|| "fromHttp".to_string()));
                    Err(error.into())
                }
            }
        } else {
            Ok(None)
        }
    }

    /// Make HTTP request for credentials (JavaScript httpRequest call at line 885)
    async fn http_request(&self, url: Url, auth_header: Option<String>) -> Result<ContainerMetadataCredentials> {
        let client = Client::builder()
            .timeout(self.timeout)
            .build()
            .context("Failed to create HTTP client")?;

        let mut request_builder = client.get(url);

        if let Some(auth_token) = auth_header {
            request_builder = request_builder.header("Authorization", auth_token);
        }

        let response = request_builder
            .send()
            .await
            .context("Failed to send HTTP request for credentials")?;

        if !response.status().is_success() {
            return Err(CredentialsProviderError::new(
                format!("HTTP request failed with status: {}", response.status())
            ).into());
        }

        let credentials: ContainerMetadataCredentials = response
            .json()
            .await
            .context("Failed to parse credentials response as JSON")?;

        Ok(credentials)
    }

    /// Validate that the response contains valid credentials
    fn is_valid_credentials_response(&self, creds: &ContainerMetadataCredentials) -> bool {
        !creds.access_key_id.is_empty() && !creds.secret_access_key.is_empty()
    }
}

impl Default for FromHttp {
    fn default() -> Self {
        Self::from_env()
    }
}

#[async_trait]
impl CredentialProvider for FromHttp {
    /// Provide credentials from HTTP container metadata service
    ///
    /// This matches the JavaScript fromHttp implementation exactly at lines 850-890.
    async fn provide_credentials(&self) -> Result<Credentials> {
        if let Some(ref logger) = self.logger {
            debug!("{} - fromHttp", logger);
        } else {
            debug!("@aws-sdk/credential-provider-http - fromHttp");
        }

        // Get the URL to request credentials from
        let url = self.get_credentials_url()?;

        // Get authorization header if needed
        let auth_header = self.get_authorization_header().await?;

        // Make the HTTP request
        let metadata_creds = self.http_request(url, auth_header).await?;

        // Validate the response
        if !self.is_valid_credentials_response(&metadata_creds) {
            return Err(CredentialsProviderError::new(
                "Invalid response from container metadata service."
            ).with_logger(self.logger.clone().unwrap_or_else(|| "fromHttp".to_string())).into());
        }

        // Parse expiration if present
        let expiration = metadata_creds.expiration
            .as_ref()
            .and_then(|exp| parse_credential_expiration(exp));

        // Build credentials object
        let credentials = Credentials {
            access_key_id: metadata_creds.access_key_id,
            secret_access_key: metadata_creds.secret_access_key,
            session_token: metadata_creds.token,
            expiration,
            credential_scope: None,
            account_id: None,
            credential_provider: None,
            credential_provider_value: None,
        };

        // Set credential feature for tracking
        let credentials = credentials.set_credential_feature("CREDENTIALS_HTTP", "h");

        Ok(credentials)
    }
}

/// Create a new HTTP credential provider with default settings
///
/// This matches the JavaScript export pattern.
pub fn from_http() -> FromHttp {
    FromHttp::from_env()
}

/// Create a new HTTP credential provider with options
///
/// This provides a convenient way to create the provider with custom settings.
pub fn from_http_with_options(
    relative_uri: Option<String>,
    full_uri: Option<String>,
    auth_token: Option<String>,
    auth_token_file: Option<String>,
    timeout: Option<Duration>,
) -> FromHttp {
    let mut provider = FromHttp::new();

    if let Some(uri) = relative_uri {
        provider = provider.with_relative_uri(uri);
    }
    if let Some(uri) = full_uri {
        provider = provider.with_full_uri(uri);
    }
    if let Some(token) = auth_token {
        provider = provider.with_auth_token(token);
    }
    if let Some(file) = auth_token_file {
        provider = provider.with_auth_token_file(file);
    }
    if let Some(timeout) = timeout {
        provider = provider.with_timeout(timeout);
    }

    provider
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
    async fn test_from_http_with_relative_uri() {
        let mock_server = MockServer::start().await;

        let credentials_response = serde_json::json!({
            "AccessKeyId": "test_access_key",
            "SecretAccessKey": "test_secret_key",
            "Token": "test_session_token",
            "Expiration": "2024-12-31T23:59:59Z"
        });

        Mock::given(method("GET"))
            .and(path("/v2/credentials/test-task-id"))
            .respond_with(ResponseTemplate::new(200).set_body_json(credentials_response))
            .mount(&mock_server)
            .await;

        let provider = FromHttp::new()
            .with_full_uri(format!("{}/v2/credentials/test-task-id", mock_server.uri()))
            .with_timeout(Duration::from_secs(5));

        let credentials = provider.provide_credentials().await.unwrap();

        assert_eq!(credentials.access_key_id, "test_access_key");
        assert_eq!(credentials.secret_access_key, "test_secret_key");
        assert_eq!(credentials.session_token, Some("test_session_token".to_string()));
        assert!(credentials.expiration.is_some());
    }

    #[tokio::test]
    async fn test_from_http_with_auth_token() {
        let mock_server = MockServer::start().await;

        let credentials_response = serde_json::json!({
            "AccessKeyId": "test_access_key",
            "SecretAccessKey": "test_secret_key"
        });

        Mock::given(method("GET"))
            .and(path("/credentials"))
            .and(header("Authorization", "test-auth-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(credentials_response))
            .mount(&mock_server)
            .await;

        let provider = FromHttp::new()
            .with_full_uri(format!("{}/credentials", mock_server.uri()))
            .with_auth_token("test-auth-token")
            .with_timeout(Duration::from_secs(5));

        let credentials = provider.provide_credentials().await.unwrap();

        assert_eq!(credentials.access_key_id, "test_access_key");
        assert_eq!(credentials.secret_access_key, "test_secret_key");
    }

    #[tokio::test]
    async fn test_from_http_missing_uri() {
        let provider = FromHttp::new();
        let result = provider.provide_credentials().await;

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI or AWS_CONTAINER_CREDENTIALS_FULL_URI"));
    }

    #[tokio::test]
    async fn test_from_http_invalid_response() {
        let mock_server = MockServer::start().await;

        let invalid_response = serde_json::json!({
            "AccessKeyId": "",  // Empty access key
            "SecretAccessKey": "test_secret_key"
        });

        Mock::given(method("GET"))
            .and(path("/credentials"))
            .respond_with(ResponseTemplate::new(200).set_body_json(invalid_response))
            .mount(&mock_server)
            .await;

        let provider = FromHttp::new()
            .with_full_uri(format!("{}/credentials", mock_server.uri()))
            .with_timeout(Duration::from_secs(5));

        let result = provider.provide_credentials().await;

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Invalid response from container metadata service"));
    }

    #[tokio::test]
    async fn test_from_http_http_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/credentials"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let provider = FromHttp::new()
            .with_full_uri(format!("{}/credentials", mock_server.uri()))
            .with_timeout(Duration::from_secs(5));

        let result = provider.provide_credentials().await;

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("HTTP request failed with status: 404"));
    }

    #[tokio::test]
    async fn test_from_http_from_env() {
        let mock_server = MockServer::start().await;

        let credentials_response = serde_json::json!({
            "AccessKeyId": "env_access_key",
            "SecretAccessKey": "env_secret_key"
        });

        Mock::given(method("GET"))
            .and(path("/v2/credentials/task-id"))
            .respond_with(ResponseTemplate::new(200).set_body_json(credentials_response))
            .mount(&mock_server)
            .await;

        let _full_uri = EnvVar::set(
            AWS_CONTAINER_CREDENTIALS_FULL_URI,
            &format!("{}/v2/credentials/task-id", mock_server.uri())
        );

        let provider = from_http().with_timeout(Duration::from_secs(5));
        let credentials = provider.provide_credentials().await.unwrap();

        assert_eq!(credentials.access_key_id, "env_access_key");
        assert_eq!(credentials.secret_access_key, "env_secret_key");
    }

    #[tokio::test]
    async fn test_from_http_with_auth_token_file() {
        let mock_server = MockServer::start().await;

        let credentials_response = serde_json::json!({
            "AccessKeyId": "file_access_key",
            "SecretAccessKey": "file_secret_key"
        });

        Mock::given(method("GET"))
            .and(path("/credentials"))
            .and(header("Authorization", "file-token-content"))
            .respond_with(ResponseTemplate::new(200).set_body_json(credentials_response))
            .mount(&mock_server)
            .await;

        // Create a temporary file with auth token
        let temp_file = tempfile::NamedTempFile::new().unwrap();
        fs::write(&temp_file.path(), "file-token-content").await.unwrap();

        let provider = FromHttp::new()
            .with_full_uri(format!("{}/credentials", mock_server.uri()))
            .with_auth_token_file(temp_file.path().to_string_lossy().to_string())
            .with_timeout(Duration::from_secs(5));

        let credentials = provider.provide_credentials().await.unwrap();

        assert_eq!(credentials.access_key_id, "file_access_key");
        assert_eq!(credentials.secret_access_key, "file_secret_key");
    }

    #[tokio::test]
    async fn test_from_http_invalid_auth_token_file() {
        let provider = FromHttp::new()
            .with_full_uri("http://example.com/credentials")
            .with_auth_token_file("/nonexistent/token/file");

        let result = provider.provide_credentials().await;

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Unable to read authorization token from file"));
    }
}