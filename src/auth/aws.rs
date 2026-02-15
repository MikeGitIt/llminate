use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use reqwest::header::HeaderMap;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::env;
use tokio::fs;
use tracing::{debug, warn};

type HmacSha256 = Hmac<Sha256>;

// AWS Credentials structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AwsCredentials {
    pub access_key_id: String,
    pub secret_access_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiration: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<HashMap<String, String>>,
}

// AWS Credential Provider trait
#[async_trait::async_trait]
pub trait CredentialProvider: Send + Sync {
    async fn get_credentials(&self) -> Result<AwsCredentials>;
}

// Environment Variable Credential Provider
pub struct EnvCredentialProvider;

#[async_trait::async_trait]
impl CredentialProvider for EnvCredentialProvider {
    async fn get_credentials(&self) -> Result<AwsCredentials> {
        debug!("@aws-sdk/credential-provider-env - fromEnv");

        let access_key_id = env::var("AWS_ACCESS_KEY_ID")
            .context("AWS_ACCESS_KEY_ID not found")?;
        let secret_access_key = env::var("AWS_SECRET_ACCESS_KEY")
            .context("AWS_SECRET_ACCESS_KEY not found")?;

        let session_token = env::var("AWS_SESSION_TOKEN").ok();
        let expiration = env::var("AWS_CREDENTIAL_EXPIRATION")
            .ok()
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&Utc));
        let credential_scope = env::var("AWS_CREDENTIAL_SCOPE").ok();
        let account_id = env::var("AWS_ACCOUNT_ID").ok();

        let mut source = HashMap::new();
        source.insert("CREDENTIALS_ENV_VARS".to_string(), "g".to_string());

        Ok(AwsCredentials {
            access_key_id,
            secret_access_key,
            session_token,
            expiration,
            credential_scope,
            account_id,
            source: Some(source),
        })
    }
}

// Container Metadata Credential Provider (ECS)
pub struct ContainerMetadataProvider {
    timeout: std::time::Duration,
    max_retries: u32,
}

impl ContainerMetadataProvider {
    pub fn new() -> Self {
        Self {
            timeout: std::time::Duration::from_millis(1000),
            max_retries: 0,
        }
    }

    async fn get_cmds_uri(&self) -> Result<String> {
        if let Ok(relative_uri) = env::var("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI") {
            return Ok(format!("http://169.254.170.2{}", relative_uri));
        }

        if let Ok(full_uri) = env::var("AWS_CONTAINER_CREDENTIALS_FULL_URI") {
            let url = reqwest::Url::parse(&full_uri)?;

            // Validate host
            let valid_hosts = ["localhost", "127.0.0.1"];
            let host = url.host_str().ok_or_else(|| {
                anyhow::anyhow!("Invalid container metadata service URL")
            })?;

            if !valid_hosts.contains(&host) {
                return Err(anyhow::anyhow!(
                    "{} is not a valid container metadata service hostname",
                    host
                ));
            }

            // Validate protocol
            if url.scheme() != "http" && url.scheme() != "https" {
                return Err(anyhow::anyhow!(
                    "{} is not a valid container metadata service protocol",
                    url.scheme()
                ));
            }

            return Ok(full_uri);
        }

        Err(anyhow::anyhow!(
            "The container metadata credential provider cannot be used unless \
             AWS_CONTAINER_CREDENTIALS_RELATIVE_URI or AWS_CONTAINER_CREDENTIALS_FULL_URI is set"
        ))
    }

    async fn request_from_ecs_imds(&self, uri: &str) -> Result<AwsCredentials> {
        let client = reqwest::Client::builder()
            .timeout(self.timeout)
            .build()?;

        let mut request = client.get(uri);

        // Add authorization token if present
        if let Ok(token) = env::var("AWS_CONTAINER_AUTHORIZATION_TOKEN") {
            request = request.header("Authorization", token);
        }

        let response = request.send().await?;
        let credentials: AwsCredentials = response.json().await?;

        Ok(credentials)
    }
}

#[async_trait::async_trait]
impl CredentialProvider for ContainerMetadataProvider {
    async fn get_credentials(&self) -> Result<AwsCredentials> {
        let uri = self.get_cmds_uri().await?;

        // Retry logic
        let mut last_error = None;
        for _ in 0..=self.max_retries {
            match self.request_from_ecs_imds(&uri).await {
                Ok(creds) => return Ok(creds),
                Err(e) => last_error = Some(e),
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Failed to get container credentials")))
    }
}

// Instance Metadata Credential Provider (EC2)
pub struct InstanceMetadataProvider {
    timeout: std::time::Duration,
    max_retries: u32,
}

impl InstanceMetadataProvider {
    pub fn new() -> Self {
        Self {
            timeout: std::time::Duration::from_millis(1000),
            max_retries: 0,
        }
    }

    async fn get_instance_metadata_endpoint(&self) -> String {
        "http://169.254.169.254".to_string()
    }

    async fn get_credentials_from_imds(&self, endpoint: &str) -> Result<AwsCredentials> {
        let client = reqwest::Client::builder()
            .timeout(self.timeout)
            .build()?;

        // First get the token (IMDSv2)
        let token_response = client
            .put(format!("{}/latest/api/token", endpoint))
            .header("X-aws-ec2-metadata-token-ttl-seconds", "21600")
            .send()
            .await?;

        let token = token_response.text().await?;

        // Get the role name
        let role_response = client
            .get(format!("{}/latest/meta-data/iam/security-credentials/", endpoint))
            .header("X-aws-ec2-metadata-token", &token)
            .send()
            .await?;

        let role_name = role_response.text().await?;
        let role_name = role_name.trim();

        // Get the credentials
        let creds_response = client
            .get(format!("{}/latest/meta-data/iam/security-credentials/{}", endpoint, role_name))
            .header("X-aws-ec2-metadata-token", &token)
            .send()
            .await?;

        #[derive(Deserialize)]
        struct ImdsCredentials {
            #[serde(rename = "AccessKeyId")]
            access_key_id: String,
            #[serde(rename = "SecretAccessKey")]
            secret_access_key: String,
            #[serde(rename = "Token")]
            token: String,
            #[serde(rename = "Expiration")]
            expiration: String,
        }

        let imds_creds: ImdsCredentials = creds_response.json().await?;

        Ok(AwsCredentials {
            access_key_id: imds_creds.access_key_id,
            secret_access_key: imds_creds.secret_access_key,
            session_token: Some(imds_creds.token),
            expiration: DateTime::parse_from_rfc3339(&imds_creds.expiration)
                .ok()
                .map(|dt| dt.with_timezone(&Utc)),
            credential_scope: None,
            account_id: None,
            source: None,
        })
    }
}

#[async_trait::async_trait]
impl CredentialProvider for InstanceMetadataProvider {
    async fn get_credentials(&self) -> Result<AwsCredentials> {
        if env::var("AWS_EC2_METADATA_DISABLED").unwrap_or_default() == "true" {
            return Err(anyhow::anyhow!("EC2 Instance Metadata Service access disabled"));
        }

        let endpoint = self.get_instance_metadata_endpoint().await;

        // Retry logic
        let mut last_error = None;
        for _ in 0..=self.max_retries {
            match self.get_credentials_from_imds(&endpoint).await {
                Ok(creds) => return Ok(creds),
                Err(e) => last_error = Some(e),
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Failed to get instance metadata credentials")))
    }
}

// AWS SigV4 Signer
pub struct SignatureV4 {
    region: String,
    service: String,
    uri_escape_path: bool,
}

impl SignatureV4 {
    pub fn new(region: String, service: String) -> Self {
        Self {
            region,
            service,
            uri_escape_path: true,
        }
    }

    pub async fn sign(
        &self,
        method: &str,
        uri: &str,
        headers: &mut HeaderMap,
        body: &[u8],
        credentials: &AwsCredentials,
    ) -> Result<()> {
        let now = Utc::now();
        let date_stamp = now.format("%Y%m%d").to_string();
        let time_stamp = now.format("%Y%m%dT%H%M%SZ").to_string();

        // Add required headers
        headers.insert("x-amz-date", time_stamp.parse()?);

        if let Some(token) = &credentials.session_token {
            headers.insert("x-amz-security-token", token.parse()?);
        }

        // Create canonical request
        let canonical_uri = self.get_canonical_path(uri);
        let canonical_query_string = self.get_canonical_query_string(uri);
        let canonical_headers = self.get_canonical_headers(headers);
        let signed_headers = self.get_signed_headers(headers);

        let payload_hash = self.hash_payload(body);

        let canonical_request = format!(
            "{}\n{}\n{}\n{}\n\n{}\n{}",
            method,
            canonical_uri,
            canonical_query_string,
            canonical_headers,
            signed_headers,
            payload_hash
        );

        // Create string to sign
        let credential_scope = format!(
            "{}/{}/{}/aws4_request",
            date_stamp, self.region, self.service
        );

        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{}\n{}\n{}",
            time_stamp,
            credential_scope,
            self.hash_payload(canonical_request.as_bytes())
        );

        // Calculate signature
        let signing_key = self.get_signing_key(
            &credentials.secret_access_key,
            &date_stamp,
            &self.region,
            &self.service,
        );

        let signature = self.calculate_signature(&signing_key, &string_to_sign);

        // Add authorization header
        let authorization = format!(
            "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
            credentials.access_key_id,
            credential_scope,
            signed_headers,
            signature
        );

        headers.insert("Authorization", authorization.parse()?);

        Ok(())
    }

    pub fn get_canonical_path(&self, uri: &str) -> String {
        let url = reqwest::Url::parse(&format!("http://example.com{}", uri))
            .unwrap_or_else(|_| reqwest::Url::parse("http://example.com/").unwrap());

        let path = url.path();

        if !self.uri_escape_path {
            return path.to_string();
        }

        // Normalize path (matching JavaScript exactly)
        let mut segments = Vec::new();
        for segment in path.split('/') {
            if segment.is_empty() && !segments.is_empty() {
                // Skip empty segments except for leading slash
                continue;
            }
            if segment == "." {
                continue;
            }
            if segment == ".." {
                segments.pop();
            } else if !segment.is_empty() {
                segments.push(segment);
            }
        }

        // Build normalized path with proper leading/trailing slashes
        let mut normalized = String::new();
        if path.starts_with('/') {
            normalized.push('/');
        }
        normalized.push_str(&segments.join("/"));
        if !segments.is_empty() && path.ends_with('/') {
            normalized.push('/');
        }

        // URL encode matching JavaScript's escapeUri behavior
        // encodeURIComponent but then un-escape slashes
        normalized
            .split('/')
            .map(|segment| {
                // Encode the segment
                let encoded = urlencoding::encode(segment).to_string();
                // JavaScript's escapeUri replaces these back after encoding
                encoded
            })
            .collect::<Vec<_>>()
            .join("/")
    }

    pub fn get_canonical_query_string(&self, uri: &str) -> String {
        let url = reqwest::Url::parse(&format!("http://example.com{}", uri))
            .unwrap_or_else(|_| reqwest::Url::parse("http://example.com/").unwrap());

        let mut params: Vec<(String, String)> = url
            .query_pairs()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        params.sort_by(|a, b| a.0.cmp(&b.0));

        params
            .iter()
            .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
            .collect::<Vec<_>>()
            .join("&")
    }

    pub fn get_canonical_headers(&self, headers: &HeaderMap) -> String {
        let mut canonical: Vec<(String, String)> = headers
            .iter()
            .map(|(name, value)| {
                // Normalize header values: trim and replace multiple spaces with single space
                let normalized_value = value
                    .to_str()
                    .unwrap_or("")
                    .trim()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                (
                    name.as_str().to_lowercase(),
                    normalized_value,
                )
            })
            .collect();

        canonical.sort_by(|a, b| a.0.cmp(&b.0));

        canonical
            .iter()
            .map(|(k, v)| format!("{}:{}", k, v))
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn get_signed_headers(&self, headers: &HeaderMap) -> String {
        let mut names: Vec<String> = headers
            .keys()
            .map(|name| name.as_str().to_lowercase())
            .collect();

        names.sort();
        names.join(";")
    }

    pub fn hash_payload(&self, payload: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(payload);
        hex::encode(hasher.finalize())
    }

    pub fn get_signing_key(&self, secret: &str, date_stamp: &str, region: &str, service: &str) -> Vec<u8> {
        let k_secret = format!("AWS4{}", secret);
        let k_date = self.hmac_sign(k_secret.as_bytes(), date_stamp.as_bytes());
        let k_region = self.hmac_sign(&k_date, region.as_bytes());
        let k_service = self.hmac_sign(&k_region, service.as_bytes());
        self.hmac_sign(&k_service, b"aws4_request")
    }

    fn hmac_sign(&self, key: &[u8], msg: &[u8]) -> Vec<u8> {
        let mut mac = HmacSha256::new_from_slice(key).expect("HMAC can take key of any size");
        mac.update(msg);
        mac.finalize().into_bytes().to_vec()
    }

    fn calculate_signature(&self, signing_key: &[u8], string_to_sign: &str) -> String {
        hex::encode(self.hmac_sign(signing_key, string_to_sign.as_bytes()))
    }
}

// INI File Credential Provider
pub struct IniFileProvider {
    profile: Option<String>,
}

impl IniFileProvider {
    pub fn new() -> Self {
        Self {
            profile: env::var("AWS_PROFILE").ok(),
        }
    }

    async fn parse_known_files(&self) -> Result<HashMap<String, HashMap<String, String>>> {
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
        let credentials_path = home.join(".aws").join("credentials");
        let config_path = home.join(".aws").join("config");

        let mut profiles = HashMap::new();

        // Parse credentials file
        if credentials_path.exists() {
            let content = tokio::fs::read_to_string(&credentials_path).await?;
            self.parse_ini_file(&content, &mut profiles);
        }

        // Parse config file
        if config_path.exists() {
            let content = tokio::fs::read_to_string(&config_path).await?;
            self.parse_ini_file(&content, &mut profiles);
        }

        Ok(profiles)
    }

    fn parse_ini_file(&self, content: &str, profiles: &mut HashMap<String, HashMap<String, String>>) {
        let mut current_profile: Option<String> = None;

        for line in content.lines() {
            let trimmed = line.trim();

            // Skip comments and empty lines
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
                continue;
            }

            // Check for profile header
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                let profile_name = trimmed[1..trimmed.len() - 1].trim();
                // Remove "profile " prefix if present (common in config file)
                let profile_name = profile_name.strip_prefix("profile ").unwrap_or(profile_name);
                current_profile = Some(profile_name.to_string());
                profiles.entry(profile_name.to_string()).or_insert_with(HashMap::new);
                continue;
            }

            // Parse key-value pairs
            if let Some(ref profile) = current_profile {
                if let Some(eq_pos) = trimmed.find('=') {
                    let key = trimmed[..eq_pos].trim().to_string();
                    let value = trimmed[eq_pos + 1..].trim().to_string();
                    profiles.get_mut(profile).unwrap().insert(key, value);
                }
            }
        }
    }

    fn get_profile_name(&self) -> String {
        self.profile.clone()
            .or_else(|| env::var("AWS_PROFILE").ok())
            .unwrap_or_else(|| "default".to_string())
    }
}

#[async_trait::async_trait]
impl CredentialProvider for IniFileProvider {
    async fn get_credentials(&self) -> Result<AwsCredentials> {
        debug!("@aws-sdk/credential-provider-ini - fromIni");

        let profiles = self.parse_known_files().await?;
        let profile_name = self.get_profile_name();

        let profile = profiles.get(&profile_name)
            .ok_or_else(|| anyhow::anyhow!(
                "Profile {} could not be found in shared credentials file",
                profile_name
            ))?;

        // Check for static credentials
        if let (Some(access_key), Some(secret_key)) =
            (profile.get("aws_access_key_id"), profile.get("aws_secret_access_key")) {

            return Ok(AwsCredentials {
                access_key_id: access_key.clone(),
                secret_access_key: secret_key.clone(),
                session_token: profile.get("aws_session_token").cloned(),
                expiration: None,
                credential_scope: None,
                account_id: None,
                source: None,
            });
        }

        // TODO: Handle other credential sources (assume role, SSO, etc.)

        Err(anyhow::anyhow!(
            "Profile {} did not contain valid credential information",
            profile_name
        ))
    }
}

// AWS Credential Chain
pub struct DefaultCredentialProvider {
    providers: Vec<Box<dyn CredentialProvider>>,
}

impl DefaultCredentialProvider {
    pub fn new() -> Self {
        let mut providers: Vec<Box<dyn CredentialProvider>> = Vec::new();

        // Add providers in order of precedence

        // 1. Environment variables
        providers.push(Box::new(EnvCredentialProvider));

        // 2. INI file credentials (AWS CLI)
        providers.push(Box::new(IniFileProvider::new()));

        // 3. Container metadata (if available)
        if env::var("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI").is_ok()
            || env::var("AWS_CONTAINER_CREDENTIALS_FULL_URI").is_ok()
        {
            providers.push(Box::new(ContainerMetadataProvider::new()));
        }

        // 4. Instance metadata (if not disabled)
        if env::var("AWS_EC2_METADATA_DISABLED").unwrap_or_default() != "true" {
            providers.push(Box::new(InstanceMetadataProvider::new()));
        }

        // TODO: Add SSO, process, and web token providers

        Self { providers }
    }
}

#[async_trait::async_trait]
impl CredentialProvider for DefaultCredentialProvider {
    async fn get_credentials(&self) -> Result<AwsCredentials> {
        let mut last_error = None;

        for provider in &self.providers {
            match provider.get_credentials().await {
                Ok(creds) => {
                    debug!("Successfully obtained AWS credentials");
                    return Ok(creds);
                }
                Err(e) => {
                    debug!("Provider failed: {}", e);
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Could not load credentials from any providers")))
    }
}

// Memoized credential provider wrapper
pub struct MemoizedProvider {
    provider: Box<dyn CredentialProvider>,
    cached: tokio::sync::RwLock<Option<(AwsCredentials, std::time::Instant)>>,
}

impl MemoizedProvider {
    pub fn new(provider: Box<dyn CredentialProvider>) -> Self {
        Self {
            provider,
            cached: tokio::sync::RwLock::new(None),
        }
    }
}

#[async_trait::async_trait]
impl CredentialProvider for MemoizedProvider {
    async fn get_credentials(&self) -> Result<AwsCredentials> {
        // Check cache
        {
            let cache = self.cached.read().await;
            if let Some((ref creds, expiry)) = *cache {
                if expiry > std::time::Instant::now() {
                    return Ok(creds.clone());
                }
            }
        }

        // Get new credentials
        let creds = self.provider.get_credentials().await?;

        // Calculate expiry (default to 15 minutes)
        let expiry = if let Some(exp) = &creds.expiration {
            std::time::Instant::now() + std::time::Duration::from_secs(
                (exp.timestamp() - Utc::now().timestamp()).max(0) as u64
            )
        } else {
            std::time::Instant::now() + std::time::Duration::from_secs(15 * 60)
        };

        // Update cache
        let mut cache = self.cached.write().await;
        *cache = Some((creds.clone(), expiry));

        Ok(creds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_env_credential_provider() {
        // Set test environment variables
        env::set_var("AWS_ACCESS_KEY_ID", "test_access_key");
        env::set_var("AWS_SECRET_ACCESS_KEY", "test_secret_key");
        env::set_var("AWS_SESSION_TOKEN", "test_session_token");

        let provider = EnvCredentialProvider;
        let creds = provider.get_credentials().await.unwrap();

        assert_eq!(creds.access_key_id, "test_access_key");
        assert_eq!(creds.secret_access_key, "test_secret_key");
        assert_eq!(creds.session_token, Some("test_session_token".to_string()));

        // Clean up
        env::remove_var("AWS_ACCESS_KEY_ID");
        env::remove_var("AWS_SECRET_ACCESS_KEY");
        env::remove_var("AWS_SESSION_TOKEN");
    }

    #[test]
    fn test_sigv4_canonical_path() {
        let signer = SignatureV4::new("us-east-1".to_string(), "s3".to_string());

        assert_eq!(signer.get_canonical_path("/test/path"), "/test/path");
        assert_eq!(signer.get_canonical_path("/test/../path"), "/path");
        assert_eq!(signer.get_canonical_path("/test/./path"), "/test/path");
    }

    #[test]
    fn test_sigv4_canonical_query_string() {
        let signer = SignatureV4::new("us-east-1".to_string(), "s3".to_string());

        assert_eq!(signer.get_canonical_query_string("/path?b=2&a=1"), "a=1&b=2");
        assert_eq!(signer.get_canonical_query_string("/path?foo=bar"), "foo=bar");
        assert_eq!(signer.get_canonical_query_string("/path"), "");
    }

    #[test]
    fn test_sigv4_hash_payload() {
        let signer = SignatureV4::new("us-east-1".to_string(), "s3".to_string());

        let empty_hash = signer.hash_payload(b"");
        assert_eq!(empty_hash, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");

        let test_hash = signer.hash_payload(b"test");
        assert_eq!(test_hash, "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08");
    }
}