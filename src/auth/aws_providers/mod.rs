pub mod env;
pub mod http;
pub mod container;
pub mod sts;
pub mod web_identity;
pub mod sso;
pub mod cognito;

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// AWS Credentials structure representing AWS credentials with optional metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    #[serde(rename = "accessKeyId")]
    pub access_key_id: String,
    #[serde(rename = "secretAccessKey")]
    pub secret_access_key: String,
    #[serde(rename = "sessionToken", skip_serializing_if = "Option::is_none")]
    pub session_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiration: Option<DateTime<Utc>>,
    #[serde(rename = "credentialScope", skip_serializing_if = "Option::is_none")]
    pub credential_scope: Option<String>,
    #[serde(rename = "accountId", skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    /// Internal tracking for credential source and feature flags
    #[serde(skip)]
    pub credential_provider: Option<String>,
    #[serde(skip)]
    pub credential_provider_value: Option<String>,
}

impl Credentials {
    /// Create new credentials with minimal required fields
    pub fn new(access_key_id: String, secret_access_key: String) -> Self {
        Self {
            access_key_id,
            secret_access_key,
            session_token: None,
            expiration: None,
            credential_scope: None,
            account_id: None,
            credential_provider: None,
            credential_provider_value: None,
        }
    }

    /// Set credential feature for tracking (equivalent to JavaScript setCredentialFeature)
    pub fn set_credential_feature(mut self, feature: impl Into<String>, value: impl Into<String>) -> Self {
        self.credential_provider = Some(feature.into());
        self.credential_provider_value = Some(value.into());
        self
    }

    /// Check if credentials are expired or will expire within the given buffer
    pub fn is_expired(&self, buffer_seconds: i64) -> bool {
        if let Some(expiration) = &self.expiration {
            let now = Utc::now();
            let buffer = chrono::Duration::seconds(buffer_seconds);
            return now + buffer >= *expiration;
        }
        false
    }

    /// Check if credentials are valid (have required fields)
    pub fn is_valid(&self) -> bool {
        !self.access_key_id.is_empty() && !self.secret_access_key.is_empty()
    }
}

/// Trait for AWS credential providers
#[async_trait]
pub trait CredentialProvider: Send + Sync {
    /// Provide AWS credentials
    async fn provide_credentials(&self) -> Result<Credentials>;
}

/// Error type for credential provider errors with JavaScript compatibility
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct CredentialsProviderError {
    pub message: String,
    pub try_next_link: bool,
    pub logger: Option<String>,
}

impl CredentialsProviderError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            try_next_link: false,
            logger: None,
        }
    }

    pub fn with_try_next_link(mut self, try_next_link: bool) -> Self {
        self.try_next_link = try_next_link;
        self
    }

    pub fn with_logger(mut self, logger: impl Into<String>) -> Self {
        self.logger = Some(logger.into());
        self
    }
}

/// Memoization wrapper for credential providers (JavaScript style)
pub struct MemoizedProvider<T>
where
    T: CredentialProvider,
{
    provider: T,
    cached_credentials: tokio::sync::Mutex<Option<(Credentials, DateTime<Utc>)>>,
    cache_duration: chrono::Duration,
}

impl<T> MemoizedProvider<T>
where
    T: CredentialProvider,
{
    pub fn new(provider: T) -> Self {
        Self {
            provider,
            cached_credentials: tokio::sync::Mutex::new(None),
            cache_duration: chrono::Duration::minutes(15), // Default 15 minutes
        }
    }

    pub fn with_cache_duration(mut self, duration: chrono::Duration) -> Self {
        self.cache_duration = duration;
        self
    }
}

#[async_trait]
impl<T> CredentialProvider for MemoizedProvider<T>
where
    T: CredentialProvider,
{
    async fn provide_credentials(&self) -> Result<Credentials> {
        let mut cache = self.cached_credentials.lock().await;

        // Check if we have valid cached credentials
        if let Some((ref credentials, cached_at)) = *cache {
            let now = Utc::now();
            if now - cached_at < self.cache_duration && !credentials.is_expired(300) {
                tracing::debug!("Using cached credentials");
                return Ok(credentials.clone());
            }
        }

        // Get fresh credentials
        tracing::debug!("Fetching fresh credentials");
        let credentials = self.provider.provide_credentials().await?;

        // Cache the credentials
        *cache = Some((credentials.clone(), Utc::now()));

        Ok(credentials)
    }
}

/// Helper function to create a memoized provider
pub fn memoize<T>(provider: T) -> MemoizedProvider<T>
where
    T: CredentialProvider,
{
    MemoizedProvider::new(provider)
}

// Re-export convenience functions for easy access
pub use sts::{from_temporary_credentials, AssumeRoleParams, TemporaryCredentialsProvider};
pub use web_identity::{from_web_token, from_token_file, WebTokenCredentialsProvider, TokenFileCredentialsProvider};
pub use sso::{from_sso, SsoCredentialsProvider, SsoCredentialsParams, is_sso_profile, validate_sso_profile};
pub use cognito::{from_cognito_identity, from_cognito_identity_pool, CognitoIdentityParams, CognitoIdentityPoolParams};

/// Helper function to parse credential expiration from string
pub fn parse_credential_expiration(expiration_str: &str) -> Option<DateTime<Utc>> {
    // Try different date formats
    if let Ok(dt) = DateTime::parse_from_rfc3339(expiration_str) {
        return Some(dt.with_timezone(&Utc));
    }

    if let Ok(dt) = DateTime::parse_from_str(expiration_str, "%Y-%m-%dT%H:%M:%S%.fZ") {
        return Some(dt.with_timezone(&Utc));
    }

    if let Ok(dt) = DateTime::parse_from_str(expiration_str, "%Y-%m-%dT%H:%M:%SZ") {
        return Some(dt.with_timezone(&Utc));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_credentials_creation() {
        let creds = Credentials::new("access_key".to_string(), "secret_key".to_string());
        assert_eq!(creds.access_key_id, "access_key");
        assert_eq!(creds.secret_access_key, "secret_key");
        assert!(creds.is_valid());
    }

    #[test]
    fn test_credentials_expiration() {
        let mut creds = Credentials::new("access_key".to_string(), "secret_key".to_string());

        // Not expired if no expiration
        assert!(!creds.is_expired(300));

        // Set expiration in the past
        creds.expiration = Some(Utc::now() - chrono::Duration::minutes(1));
        assert!(creds.is_expired(0));

        // Set expiration in the future
        creds.expiration = Some(Utc::now() + chrono::Duration::hours(1));
        assert!(!creds.is_expired(300));
    }

    #[test]
    fn test_credential_feature() {
        let creds = Credentials::new("access_key".to_string(), "secret_key".to_string())
            .set_credential_feature("CREDENTIALS_ENV_VARS", "p");

        assert_eq!(creds.credential_provider, Some("CREDENTIALS_ENV_VARS".to_string()));
        assert_eq!(creds.credential_provider_value, Some("p".to_string()));
    }

    #[test]
    fn test_parse_credential_expiration() {
        // Test RFC3339 format
        let exp1 = parse_credential_expiration("2024-01-01T12:00:00Z");
        assert!(exp1.is_some());

        // Test with milliseconds
        let exp2 = parse_credential_expiration("2024-01-01T12:00:00.123Z");
        assert!(exp2.is_some());

        // Test invalid format
        let exp3 = parse_credential_expiration("invalid");
        assert!(exp3.is_none());
    }
}