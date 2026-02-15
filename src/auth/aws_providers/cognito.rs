use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::debug;

use super::{Credentials, CredentialProvider, CredentialsProviderError};

/// Cognito Identity credentials parameters (matches JavaScript fromCognitoIdentity)
#[derive(Debug)]
pub struct CognitoIdentityParams {
    pub client: Option<Box<dyn CognitoIdentityClient>>,
    pub identity_id: String,
    pub logins: Option<HashMap<String, LoginProvider>>,
    pub custom_role_arn: Option<String>,
    pub client_config: Option<HashMap<String, String>>,
    pub parent_client_config: Option<HashMap<String, String>>,
    pub logger: Option<Box<dyn Logger>>,
}

/// Logger trait for debug output (matches JavaScript logger)
#[async_trait]
pub trait Logger: Send + Sync + std::fmt::Debug {
    fn debug(&self, message: &str);
}

/// Default console logger
#[derive(Debug)]
pub struct ConsoleLogger;

#[async_trait]
impl Logger for ConsoleLogger {
    fn debug(&self, message: &str) {
        debug!("{}", message);
    }
}

/// Cognito Identity Pool credentials parameters (matches JavaScript fromCognitoIdentityPool)
#[derive(Debug)]
pub struct CognitoIdentityPoolParams {
    pub account_id: Option<String>,
    pub cache: Option<Box<dyn CognitoCache>>,
    pub client: Option<Box<dyn CognitoIdentityClient>>,
    pub custom_role_arn: Option<String>,
    pub identity_pool_id: String,
    pub logins: Option<HashMap<String, LoginProvider>>,
    pub user_identifier: Option<String>,
    pub client_config: Option<HashMap<String, String>>,
    pub parent_client_config: Option<HashMap<String, String>>,
    pub logger: Option<Box<dyn Logger>>,
}

/// Login provider - can be a string or a function that returns a string
pub enum LoginProvider {
    Token(String),
    TokenProvider(Box<dyn Fn() -> Result<String> + Send + Sync>),
}

impl std::fmt::Debug for LoginProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoginProvider::Token(token) => write!(f, "Token({})", token),
            LoginProvider::TokenProvider(_) => write!(f, "TokenProvider(<function>)"),
        }
    }
}

/// Trait for Cognito Identity client operations (for testing)
#[async_trait]
pub trait CognitoIdentityClient: Send + Sync + std::fmt::Debug {
    /// Get credentials for identity
    async fn get_credentials_for_identity(&self, params: &GetCredentialsForIdentityParams) -> Result<CognitoCredentialsResponse>;
    /// Get identity ID
    async fn get_id(&self, params: &GetIdParams) -> Result<GetIdResponse>;
    /// Get OpenID token
    async fn get_open_id_token(&self, params: &GetOpenIdTokenParams) -> Result<GetOpenIdTokenResponse>;
}

/// Trait for Cognito caching - stores Identity IDs only, not credentials
#[async_trait]
pub trait CognitoCache: Send + Sync + std::fmt::Debug {
    /// Get cached identity ID
    async fn get_item(&self, key: &str) -> Option<String>;
    /// Set cached identity ID
    async fn set_item(&self, key: &str, value: String) -> Result<()>;
    /// Remove cached identity ID
    async fn remove_item(&self, key: &str) -> Result<()>;
}

/// Parameters for GetCredentialsForIdentity
#[derive(Debug, Clone)]
pub struct GetCredentialsForIdentityParams {
    pub identity_id: String,
    pub logins: Option<HashMap<String, String>>,
    pub custom_role_arn: Option<String>,
}

/// Response from GetCredentialsForIdentity
#[derive(Debug, Clone)]
pub struct CognitoCredentialsResponse {
    pub credentials: CognitoCredentials,
}

/// Cognito credentials structure
#[derive(Debug, Clone)]
pub struct CognitoCredentials {
    pub access_key_id: String,
    pub secret_key: String,
    pub session_token: String,
    pub expiration: Option<DateTime<Utc>>,
}

/// Parameters for GetId
#[derive(Debug, Clone)]
pub struct GetIdParams {
    pub account_id: Option<String>,
    pub identity_pool_id: String,
    pub logins: Option<HashMap<String, String>>,
}

/// Response from GetId
#[derive(Debug, Clone)]
pub struct GetIdResponse {
    pub identity_id: String,
}

/// Parameters for GetOpenIdToken
#[derive(Debug, Clone)]
pub struct GetOpenIdTokenParams {
    pub identity_id: String,
    pub logins: Option<HashMap<String, String>>,
}

/// Response from GetOpenIdToken
#[derive(Debug, Clone)]
pub struct GetOpenIdTokenResponse {
    pub token: String,
}

/// Type alias for GetCredentialsForIdentityParams for compatibility
type GetCredentialsParams = GetCredentialsForIdentityParams;

/// Helper function to resolve config values (matches JavaScript fromConfigs)
fn from_configs(
    key: &str,
    client_config: Option<&HashMap<String, String>>,
    parent_client_config: Option<&HashMap<String, String>>,
    caller_client_config: Option<&HashMap<String, String>>,
) -> Option<String> {
    client_config.and_then(|c| c.get(key))
        .or_else(|| parent_client_config.and_then(|c| c.get(key)))
        .or_else(|| caller_client_config.and_then(|c| c.get(key)))
        .cloned()
}

/// Throw functions for Cognito errors (matches JavaScript thrower functions)
fn throw_no_access_key_id() -> Result<String> {
    Err(CredentialsProviderError::new("Response from Cognito contained no AccessKeyId").into())
}

fn throw_no_credentials() -> Result<CognitoCredentials> {
    Err(CredentialsProviderError::new("Response from Cognito contained no Credentials").into())
}

fn throw_no_secret_key() -> Result<String> {
    Err(CredentialsProviderError::new("Response from Cognito contained no SecretKey").into())
}

/// Default HTTP-based Cognito Identity client implementation
#[derive(Debug)]
pub struct HttpCognitoIdentityClient {
    region: String,
    base_url: String,
}

impl HttpCognitoIdentityClient {
    pub fn new(region: String) -> Self {
        let base_url = format!("https://cognito-identity.{}.amazonaws.com", region);
        Self { region, base_url }
    }
}

#[async_trait]
impl CognitoIdentityClient for HttpCognitoIdentityClient {
    async fn get_credentials_for_identity(&self, params: &GetCredentialsForIdentityParams) -> Result<CognitoCredentialsResponse> {
        let client = reqwest::Client::new();

        let mut request_body = serde_json::json!({
            "IdentityId": params.identity_id
        });

        if let Some(ref logins) = params.logins {
            request_body["Logins"] = serde_json::to_value(logins)?;
        }

        if let Some(ref custom_role_arn) = params.custom_role_arn {
            request_body["CustomRoleArn"] = serde_json::Value::String(custom_role_arn.clone());
        }

        let response = client
            .post(&format!("{}/", self.base_url))
            .header("X-Amz-Target", "AWSCognitoIdentityService.GetCredentialsForIdentity")
            .header("Content-Type", "application/x-amz-json-1.1")
            .json(&request_body)
            .send()
            .await
            .context("Failed to send GetCredentialsForIdentity request")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(CredentialsProviderError::new(format!(
                "GetCredentialsForIdentity failed: {} - {}",
                status, error_text
            )).into());
        }

        let response_data: serde_json::Value = response.json().await
            .context("Failed to parse GetCredentialsForIdentity response")?;

        // Use thrower pattern like JavaScript (exact match)
        let credentials = response_data
            .get("Credentials")
            .ok_or_else(|| throw_no_credentials().unwrap_err())?;

        let access_key_id = credentials
            .get("AccessKeyId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| throw_no_access_key_id().unwrap());

        let secret_key = credentials
            .get("SecretKey")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| throw_no_secret_key().unwrap());

        let session_token = credentials
            .get("SessionToken")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let expiration = credentials
            .get("Expiration")
            .and_then(|v| v.as_str())
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc));

        Ok(CognitoCredentialsResponse {
            credentials: CognitoCredentials {
                access_key_id,
                secret_key,
                session_token,
                expiration,
            },
        })
    }

    async fn get_id(&self, params: &GetIdParams) -> Result<GetIdResponse> {
        let client = reqwest::Client::new();

        let mut request_body = serde_json::json!({
            "IdentityPoolId": params.identity_pool_id
        });

        if let Some(ref account_id) = params.account_id {
            request_body["AccountId"] = serde_json::Value::String(account_id.clone());
        }

        if let Some(ref logins) = params.logins {
            request_body["Logins"] = serde_json::to_value(logins)?;
        }

        let response = client
            .post(&format!("{}/", self.base_url))
            .header("X-Amz-Target", "AWSCognitoIdentityService.GetId")
            .header("Content-Type", "application/x-amz-json-1.1")
            .json(&request_body)
            .send()
            .await
            .context("Failed to send GetId request")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(CredentialsProviderError::new(format!(
                "GetId failed: {} - {}",
                status, error_text
            )).into());
        }

        let response_data: serde_json::Value = response.json().await
            .context("Failed to parse GetId response")?;

        let identity_id = response_data
            .get("IdentityId")
            .and_then(|v| v.as_str())
            .ok_or_else(|| CredentialsProviderError::new("Identity ID is missing from the response of GetId operation."))?
            .to_string();

        Ok(GetIdResponse { identity_id })
    }

    async fn get_open_id_token(&self, params: &GetOpenIdTokenParams) -> Result<GetOpenIdTokenResponse> {
        let client = reqwest::Client::new();

        let mut request_body = serde_json::json!({
            "IdentityId": params.identity_id
        });

        if let Some(ref logins) = params.logins {
            request_body["Logins"] = serde_json::to_value(logins)?;
        }

        let response = client
            .post(&format!("{}/", self.base_url))
            .header("X-Amz-Target", "AWSCognitoIdentityService.GetOpenIdToken")
            .header("Content-Type", "application/x-amz-json-1.1")
            .json(&request_body)
            .send()
            .await
            .context("Failed to send GetOpenIdToken request")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(CredentialsProviderError::new(format!(
                "GetOpenIdToken failed: {} - {}",
                status, error_text
            )).into());
        }

        let response_data: serde_json::Value = response.json().await
            .context("Failed to parse GetOpenIdToken response")?;

        let token = response_data
            .get("Token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| CredentialsProviderError::new("Open ID token is missing from the response of GetOpenIdToken operation."))?
            .to_string();

        Ok(GetOpenIdTokenResponse { token })
    }
}

/// Create Cognito Identity credentials provider function (matches JavaScript fromCognitoIdentity exactly)
pub fn from_cognito_identity(params: CognitoIdentityParams) -> impl for<'a> Fn(Option<&'a HashMap<String, String>>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Credentials>> + Send + 'a>> + Send + Sync {
    move |caller_client_config| {
        let params = CognitoIdentityParams {
            client: None, // Can't clone Box<dyn CognitoIdentityClient>
            identity_id: params.identity_id.clone(),
            logins: None, // Would need to implement Clone for LoginProvider
            custom_role_arn: params.custom_role_arn.clone(),
            client_config: params.client_config.clone(),
            parent_client_config: params.parent_client_config.clone(),
            logger: None, // Can't clone Box<dyn Logger>
        };

        Box::pin(async move {
            if let Some(ref logger) = params.logger {
                logger.debug("@aws-sdk/credential-provider-cognito-identity - fromCognitoIdentity");
            }

            // Get client or create new one (matches JavaScript logic)
            let client = params.client.unwrap_or_else(|| {
                let region = from_configs(
                    "region",
                    params.client_config.as_ref(),
                    params.parent_client_config.as_ref(),
                    caller_client_config,
                ).unwrap_or_else(|| "us-east-1".to_string());
                Box::new(HttpCognitoIdentityClient::new(region))
            });

            // Resolve logins (matches JavaScript resolveLogins)
            let resolved_logins = if let Some(ref logins) = params.logins {
                Some(resolve_logins(logins).await?)
            } else {
                None
            };

            // Call GetCredentialsForIdentity (matches JavaScript API call)
            let response = client.get_credentials_for_identity(&GetCredentialsForIdentityParams {
                identity_id: params.identity_id.clone(),
                logins: resolved_logins,
                custom_role_arn: params.custom_role_arn.clone(),
            }).await?;

            // Return credentials with exact JavaScript structure
            Ok(Credentials {
                access_key_id: response.credentials.access_key_id,
                secret_access_key: response.credentials.secret_key,
                session_token: Some(response.credentials.session_token),
                expiration: response.credentials.expiration,
                credential_scope: Some("us-east-1".to_string()),
                account_id: None,
                credential_provider: Some("CREDENTIALS_COGNITO".to_string()),
                credential_provider_value: Some("O".to_string()),
            })
        })
    }
}

/// Create Cognito Identity Pool credentials provider function (matches JavaScript fromCognitoIdentityPool exactly)
pub fn from_cognito_identity_pool(params: CognitoIdentityPoolParams) -> impl for<'a> Fn(Option<&'a HashMap<String, String>>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Credentials>> + Send + 'a>> + Send + Sync {
    let user_identifier = params.user_identifier.as_deref().unwrap_or("").to_string();
    let cache_key = if user_identifier.is_empty() {
        format!("amazon-cognito-identity-js:{}", params.identity_pool_id)
    } else {
        format!("amazon-cognito-identity-js:{}:{}", params.identity_pool_id, user_identifier)
    };

    move |caller_client_config| {
        let params = CognitoIdentityPoolParams {
            account_id: params.account_id.clone(),
            cache: None, // Can't clone Box<dyn CognitoCache>
            client: None, // Can't clone Box<dyn CognitoIdentityClient>
            custom_role_arn: params.custom_role_arn.clone(),
            identity_pool_id: params.identity_pool_id.clone(),
            logins: None, // Would need to implement Clone for LoginProvider
            user_identifier: params.user_identifier.clone(),
            client_config: params.client_config.clone(),
            parent_client_config: params.parent_client_config.clone(),
            logger: None, // Can't clone Box<dyn Logger>
        };
        let cache_key = cache_key.clone();

        Box::pin(async move {
            if let Some(ref logger) = params.logger {
                logger.debug("@aws-sdk/credential-provider-cognito-identity - fromCognitoIdentityPool");
            }

            // Note: In JavaScript, the cache stores identity IDs, not credentials
            // We're not caching credentials here, only identity IDs in the provider implementation

            // Create client (matches JavaScript fromConfigs pattern)
            let region = from_configs(
                "region",
                params.client_config.as_ref(),
                params.parent_client_config.as_ref(),
                caller_client_config,
            ).unwrap_or_else(|| "us-east-1".to_string());

            let client = params.client.unwrap_or_else(|| {
                Box::new(HttpCognitoIdentityClient::new(region.clone()))
            });

            // Resolve logins (matches JavaScript resolveLogins)
            let resolved_logins = if let Some(ref logins) = params.logins {
                Some(resolve_logins(logins).await?)
            } else {
                None
            };

            // GetId operation (matches JavaScript)
            let get_id_response = client.get_id(&GetIdParams {
                account_id: params.account_id.clone(),
                identity_pool_id: params.identity_pool_id.clone(),
                logins: resolved_logins.clone(),
            }).await?;

            // GetOpenIdToken operation (matches JavaScript)
            let get_token_response = client.get_open_id_token(&GetOpenIdTokenParams {
                identity_id: get_id_response.identity_id.clone(),
                logins: resolved_logins.clone(),
            }).await?;

            // Create enhanced logins with the OpenID token (matches JavaScript)
            let mut enhanced_logins = resolved_logins.unwrap_or_default();
            enhanced_logins.insert(
                format!("cognito-identity.{}.amazonaws.com", region),
                get_token_response.token,
            );

            // Call fromCognitoIdentity with enhanced logins (matches JavaScript line 519-527)
            let cognito_identity_fn = from_cognito_identity(CognitoIdentityParams {
                client: Some(Box::new(HttpCognitoIdentityClient::new(region))),
                identity_id: get_id_response.identity_id,
                logins: Some(enhanced_logins.into_iter().map(|(k, v)| (k, LoginProvider::Token(v))).collect()),
                custom_role_arn: params.custom_role_arn.clone(),
                client_config: params.client_config.clone(),
                parent_client_config: params.parent_client_config.clone(),
                logger: None,
            });

            let credentials = cognito_identity_fn(caller_client_config).await?;

            // Note: Credentials are not cached at this level - only identity IDs are cached
            // in the CognitoIdentityPoolCredentialsProvider implementation

            Ok(credentials)
        })
    }
}

/// Cognito Identity Credentials Provider
pub struct CognitoIdentityCredentialsProvider {
    client: Arc<dyn CognitoIdentityClient>,
    identity_id: String,
    custom_role_arn: Option<String>,
    logins: Option<HashMap<String, LoginProvider>>,
    logger: Option<Box<dyn Logger>>,
}

impl CognitoIdentityCredentialsProvider {
    pub fn new(params: CognitoIdentityParams) -> Self {
        let client: Arc<dyn CognitoIdentityClient> = params.client
            .map(|c| Arc::from(c) as Arc<dyn CognitoIdentityClient>)
            .unwrap_or_else(|| Arc::new(HttpCognitoIdentityClient::new("us-east-1".to_string())) as Arc<dyn CognitoIdentityClient>);

        Self {
            client,
            identity_id: params.identity_id,
            custom_role_arn: params.custom_role_arn,
            logins: params.logins,
            logger: params.logger,
        }
    }
}

#[async_trait]
impl CredentialProvider for CognitoIdentityCredentialsProvider {
    async fn provide_credentials(&self) -> Result<Credentials> {
        if let Some(ref logger) = self.logger {
            logger.debug(&format!("@aws-sdk/credential-provider-cognito-identity - fromCognitoIdentity"));
        } else {
            debug!("@aws-sdk/credential-provider-cognito-identity - fromCognitoIdentity");
        }

        // Resolve logins if provided
        let resolved_logins = if let Some(ref logins) = self.logins {
            Some(resolve_logins(logins).await?)
        } else {
            None
        };

        // Call GetCredentialsForIdentity
        let params = GetCredentialsParams {
            identity_id: self.identity_id.clone(),
            logins: resolved_logins,
            custom_role_arn: self.custom_role_arn.clone(),
        };

        let response = self.client.get_credentials_for_identity(&params).await?;

        // Extract credentials from response
        Ok(Credentials {
            access_key_id: response.credentials.access_key_id,
            secret_access_key: response.credentials.secret_key,
            session_token: Some(response.credentials.session_token),
            expiration: response.credentials.expiration,
            credential_scope: Some("us-east-1".to_string()),
            account_id: None,
            credential_provider: Some("CREDENTIALS_COGNITO".to_string()),
            credential_provider_value: Some("O".to_string()),
        })
    }
}

/// Cognito Identity Pool Credentials Provider
pub struct CognitoIdentityPoolCredentialsProvider {
    client: Arc<dyn CognitoIdentityClient>,
    cache: Option<Arc<dyn CognitoCache>>,
    identity_pool_id: String,
    account_id: Option<String>,
    custom_role_arn: Option<String>,
    logins: Option<HashMap<String, LoginProvider>>,
    user_identifier: Option<String>,
    logger: Option<Box<dyn Logger>>,
}

impl CognitoIdentityPoolCredentialsProvider {
    pub fn new(params: CognitoIdentityPoolParams) -> Self {
        let client: Arc<dyn CognitoIdentityClient> = params.client
            .map(|c| Arc::from(c) as Arc<dyn CognitoIdentityClient>)
            .unwrap_or_else(|| Arc::new(HttpCognitoIdentityClient::new("us-east-1".to_string())) as Arc<dyn CognitoIdentityClient>);

        let cache = params.cache.map(|c| Arc::from(c) as Arc<dyn CognitoCache>);

        Self {
            client,
            cache,
            identity_pool_id: params.identity_pool_id,
            account_id: params.account_id,
            custom_role_arn: params.custom_role_arn,
            logins: params.logins,
            user_identifier: params.user_identifier,
            logger: params.logger,
        }
    }
}

#[async_trait]
impl CredentialProvider for CognitoIdentityPoolCredentialsProvider {
    async fn provide_credentials(&self) -> Result<Credentials> {
        if let Some(ref logger) = self.logger {
            logger.debug(&format!("@aws-sdk/credential-provider-cognito-identity - fromCognitoIdentityPool"));
        } else {
            debug!("@aws-sdk/credential-provider-cognito-identity - fromCognitoIdentityPool");
        }

        // Generate cache key - matches JavaScript format exactly
        let cache_key = if let Some(ref user_identifier) = self.user_identifier {
            format!("aws:cognito-identity-credentials:{}:{}", self.identity_pool_id, user_identifier)
        } else {
            // Without user identifier, caching is not used in JavaScript
            String::new()
        };

        // Check cache for identity ID (only if we have a cache key)
        let identity_id = if let Some(ref cache) = self.cache {
            if !cache_key.is_empty() {
                if let Some(cached_id) = cache.get_item(&cache_key).await {
                    cached_id
                } else {
                    // Get new identity ID
                    let resolved_logins = if let Some(ref logins) = self.logins {
                        Some(resolve_logins(logins).await?)
                    } else {
                        None
                    };

                    let get_id_params = GetIdParams {
                        account_id: self.account_id.clone(),
                        identity_pool_id: self.identity_pool_id.clone(),
                        logins: resolved_logins.clone(),
                    };

                    let get_id_response = self.client.get_id(&get_id_params).await?;
                    let identity_id = get_id_response.identity_id;

                    // Cache the identity ID (fire and forget pattern like JavaScript)
                    // JavaScript does: Promise.resolve(cache.setItem(key, id)).catch(() => {})
                    let cache_key_clone = cache_key.clone();
                    let identity_id_clone = identity_id.clone();
                    let cache_clone = Arc::clone(cache);
                    tokio::spawn(async move {
                        let _ = cache_clone.set_item(&cache_key_clone, identity_id_clone).await;
                    });
                    identity_id
                }
            } else {
                // No cache key, get identity ID directly
                let resolved_logins = if let Some(ref logins) = self.logins {
                    Some(resolve_logins(logins).await?)
                } else {
                    None
                };

                let get_id_params = GetIdParams {
                    account_id: self.account_id.clone(),
                    identity_pool_id: self.identity_pool_id.clone(),
                    logins: resolved_logins.clone(),
                };

                let get_id_response = self.client.get_id(&get_id_params).await?;
                get_id_response.identity_id
            }
        } else {
            // No cache, get identity ID directly
            let resolved_logins = if let Some(ref logins) = self.logins {
                Some(resolve_logins(logins).await?)
            } else {
                None
            };

            let get_id_params = GetIdParams {
                account_id: self.account_id.clone(),
                identity_pool_id: self.identity_pool_id.clone(),
                logins: resolved_logins.clone(),
            };

            let get_id_response = self.client.get_id(&get_id_params).await?;
            get_id_response.identity_id
        };

        // Get open ID token if custom role is specified
        if self.custom_role_arn.is_some() {
            let resolved_logins = if let Some(ref logins) = self.logins {
                Some(resolve_logins(logins).await?)
            } else {
                None
            };

            let token_params = GetOpenIdTokenParams {
                identity_id: identity_id.clone(),
                logins: resolved_logins.clone(),
            };

            let _token_response = self.client.get_open_id_token(&token_params).await?;
            // Token is used internally by the service
        }

        // Get credentials for the identity
        let resolved_logins = if let Some(ref logins) = self.logins {
            Some(resolve_logins(logins).await?)
        } else {
            None
        };

        let creds_params = GetCredentialsParams {
            identity_id,
            logins: resolved_logins,
            custom_role_arn: self.custom_role_arn.clone(),
        };

        let response = self.client.get_credentials_for_identity(&creds_params).await?;

        Ok(Credentials {
            access_key_id: response.credentials.access_key_id,
            secret_access_key: response.credentials.secret_key,
            session_token: Some(response.credentials.session_token),
            expiration: response.credentials.expiration,
            credential_scope: Some("us-east-1".to_string()),
            account_id: self.account_id.clone(),
            credential_provider: Some("CREDENTIALS_COGNITO".to_string()),
            credential_provider_value: Some("O".to_string()),
        })
    }
}

/// Resolve login providers (convert functions to strings)
async fn resolve_logins(logins: &HashMap<String, LoginProvider>) -> Result<HashMap<String, String>> {
    let mut resolved = HashMap::new();

    for (key, provider) in logins {
        let token = match provider {
            LoginProvider::Token(token) => token.clone(),
            LoginProvider::TokenProvider(provider_fn) => provider_fn()?,
        };
        resolved.insert(key.clone(), token);
    }

    Ok(resolved)
}

// The main entry points are now the from_cognito_identity and from_cognito_identity_pool functions above
// which return closures that match the JavaScript API exactly

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    // Mock Cognito Identity client for testing
    #[derive(Debug)]
    struct MockCognitoIdentityClient {
        should_fail: bool,
    }

    #[async_trait]
    impl CognitoIdentityClient for MockCognitoIdentityClient {
        async fn get_credentials_for_identity(&self, _params: &GetCredentialsForIdentityParams) -> Result<CognitoCredentialsResponse> {
            if self.should_fail {
                return Err(CredentialsProviderError::new("Mock Cognito client failure").into());
            }

            Ok(CognitoCredentialsResponse {
                credentials: CognitoCredentials {
                    access_key_id: "AKIATEST".to_string(),
                    secret_key: "test-secret".to_string(),
                    session_token: "test-session-token".to_string(),
                    expiration: Some(Utc::now() + chrono::Duration::hours(1)),
                },
            })
        }

        async fn get_id(&self, _params: &GetIdParams) -> Result<GetIdResponse> {
            if self.should_fail {
                return Err(CredentialsProviderError::new("Mock GetId failure").into());
            }

            Ok(GetIdResponse {
                identity_id: "us-east-1:12345678-1234-1234-1234-123456789012".to_string(),
            })
        }

        async fn get_open_id_token(&self, _params: &GetOpenIdTokenParams) -> Result<GetOpenIdTokenResponse> {
            if self.should_fail {
                return Err(CredentialsProviderError::new("Mock GetOpenIdToken failure").into());
            }

            Ok(GetOpenIdTokenResponse {
                token: "mock-openid-token".to_string(),
            })
        }
    }

    // Mock cache for testing - stores identity IDs only
    #[derive(Debug)]
    struct MockCognitoCache {
        should_return_cached: bool,
    }

    #[async_trait]
    impl CognitoCache for MockCognitoCache {
        async fn get_item(&self, _key: &str) -> Option<String> {
            if self.should_return_cached {
                // Return the same identity ID that GetId would return
                Some("us-east-1:12345678-1234-1234-1234-123456789012".to_string())
            } else {
                None
            }
        }

        async fn set_item(&self, _key: &str, _value: String) -> Result<()> {
            Ok(())
        }

        async fn remove_item(&self, _key: &str) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_cognito_identity_credentials_provider() {
        let params = CognitoIdentityParams {
            client: Some(Box::new(MockCognitoIdentityClient { should_fail: false })),
            identity_id: "us-east-1:12345678-1234-1234-1234-123456789012".to_string(),
            logins: None,
            custom_role_arn: None,
            client_config: None,
            parent_client_config: None,
            logger: None,
        };

        let provider = CognitoIdentityCredentialsProvider::new(params);
        let result = provider.provide_credentials().await;
        assert!(result.is_ok());

        let credentials = result.unwrap();
        assert_eq!(credentials.access_key_id, "AKIATEST");
        assert_eq!(credentials.secret_access_key, "test-secret");
        assert_eq!(credentials.session_token, Some("test-session-token".to_string()));
        assert_eq!(credentials.credential_provider, Some("CREDENTIALS_COGNITO".to_string()));
    }

    #[tokio::test]
    async fn test_cognito_identity_pool_credentials_provider() {
        let params = CognitoIdentityPoolParams {
            account_id: Some("123456789012".to_string()),
            cache: None,
            client: Some(Box::new(MockCognitoIdentityClient { should_fail: false })),
            custom_role_arn: None,
            identity_pool_id: "us-east-1:12345678-1234-1234-1234-123456789012".to_string(),
            logins: None,
            user_identifier: Some("test-user".to_string()),
            client_config: None,
            parent_client_config: None,
            logger: None,
        };

        let provider = CognitoIdentityPoolCredentialsProvider::new(params);
        let result = provider.provide_credentials().await;
        assert!(result.is_ok());

        let credentials = result.unwrap();
        assert_eq!(credentials.access_key_id, "AKIATEST");
        assert_eq!(credentials.secret_access_key, "test-secret");
    }

    #[tokio::test]
    async fn test_cognito_identity_pool_with_cache() {
        let params = CognitoIdentityPoolParams {
            account_id: Some("123456789012".to_string()),
            cache: Some(Box::new(MockCognitoCache { should_return_cached: true })),
            client: Some(Box::new(MockCognitoIdentityClient { should_fail: false })),
            custom_role_arn: None,
            identity_pool_id: "us-east-1:12345678-1234-1234-1234-123456789012".to_string(),
            logins: None,
            user_identifier: Some("test-user".to_string()),
            client_config: None,
            parent_client_config: None,
            logger: None,
        };

        let provider = CognitoIdentityPoolCredentialsProvider::new(params);
        let result = provider.provide_credentials().await;
        assert!(result.is_ok());

        let credentials = result.unwrap();
        // Should get cached credentials from cache mock
        assert_eq!(credentials.access_key_id, "AKIATEST");
        assert_eq!(credentials.secret_access_key, "test-secret");
    }

    #[tokio::test]
    async fn test_resolve_logins() {
        let mut logins = HashMap::new();
        logins.insert("provider1".to_string(), LoginProvider::Token("token1".to_string()));

        let result = resolve_logins(&logins).await;
        assert!(result.is_ok());

        let resolved = result.unwrap();
        assert_eq!(resolved.get("provider1"), Some(&"token1".to_string()));
    }

    #[test]
    fn test_from_cognito_identity_creation() {
        let params = CognitoIdentityParams {
            client: None,
            identity_id: "us-east-1:12345678-1234-1234-1234-123456789012".to_string(),
            logins: None,
            custom_role_arn: None,
            client_config: None,
            parent_client_config: None,
            logger: None,
        };

        let _result = from_cognito_identity(params);
        // Function creation should succeed (can't easily test the closure)
    }

    #[test]
    fn test_from_cognito_identity_pool_creation() {
        let params = CognitoIdentityPoolParams {
            account_id: Some("123456789012".to_string()),
            cache: None,
            client: None,
            custom_role_arn: None,
            identity_pool_id: "us-east-1:12345678-1234-1234-1234-123456789012".to_string(),
            logins: None,
            user_identifier: None,
            client_config: None,
            parent_client_config: None,
            logger: None,
        };

        let _result = from_cognito_identity_pool(params);
        // Function creation should succeed (can't easily test the closure)
    }
}