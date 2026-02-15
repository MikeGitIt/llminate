use anyhow::{Context, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// HTTP API Key authentication location constants
pub struct HttpApiKeyAuthLocation;

impl HttpApiKeyAuthLocation {
    pub const QUERY: &'static str = "query";
    pub const HEADER: &'static str = "header";
}

/// HTTP Request representation matching JavaScript HttpRequest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpRequest {
    pub method: String,
    pub protocol: String,
    pub hostname: String,
    pub port: Option<u16>,
    pub path: String,
    pub query: HashMap<String, QueryValue>,
    pub headers: HashMap<String, String>,
    pub body: Option<Vec<u8>>,
}

/// Represents query parameter values (can be single or multiple)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum QueryValue {
    Single(String),
    Multiple(Vec<String>),
}

impl HttpRequest {
    /// Creates a new HttpRequest
    pub fn new(method: String, url: &str) -> Result<Self> {
        let parsed = url::Url::parse(url).context("Failed to parse URL")?;

        let protocol = parsed.scheme().to_string();
        let hostname = parsed.host_str()
            .ok_or_else(|| anyhow::anyhow!("URL missing hostname"))?
            .to_string();
        let port = parsed.port();
        let path = parsed.path().to_string();

        // Parse query parameters
        let mut query = HashMap::new();
        for (key, value) in parsed.query_pairs() {
            query.entry(key.to_string())
                .and_modify(|e| match e {
                    QueryValue::Single(v) => *e = QueryValue::Multiple(vec![v.clone(), value.to_string()]),
                    QueryValue::Multiple(v) => v.push(value.to_string()),
                })
                .or_insert(QueryValue::Single(value.to_string()));
        }

        Ok(Self {
            method,
            protocol,
            hostname,
            port,
            path,
            query,
            headers: HashMap::new(),
            body: None,
        })
    }

    /// Clones the request (matches JavaScript HttpRequest.clone)
    pub fn clone_request(&self) -> Self {
        let mut cloned = Self {
            method: self.method.clone(),
            protocol: self.protocol.clone(),
            hostname: self.hostname.clone(),
            port: self.port,
            path: self.path.clone(),
            query: clone_query(&self.query),
            headers: self.headers.clone(),
            body: self.body.clone(),
        };

        cloned
    }

    /// Static clone method matching JavaScript HttpRequest.clone(request)
    pub fn clone(request: &HttpRequest) -> HttpRequest {
        request.clone_request()
    }

    /// Check if an object is an instance of HttpRequest (matches JavaScript isInstance)
    pub fn is_instance(obj: &Value) -> bool {
        if obj.is_null() {
            return false;
        }

        obj.get("method").is_some() &&
        obj.get("protocol").is_some() &&
        obj.get("hostname").is_some() &&
        obj.get("path").is_some() &&
        obj.get("query").is_some() &&
        obj.get("headers").is_some()
    }

    /// Build URL from request components
    pub fn build_url(&self) -> String {
        let mut url = format!("{}://{}", self.protocol, self.hostname);

        if let Some(port) = self.port {
            // Don't add default ports
            if !(self.protocol == "https" && port == 443) && !(self.protocol == "http" && port == 80) {
                url.push_str(&format!(":{}", port));
            }
        }

        url.push_str(&self.path);

        // Add query parameters
        if !self.query.is_empty() {
            let mut params = Vec::new();
            for (key, value) in &self.query {
                match value {
                    QueryValue::Single(v) => {
                        params.push(format!("{}={}",
                            urlencoding::encode(key),
                            urlencoding::encode(v)
                        ));
                    }
                    QueryValue::Multiple(values) => {
                        for v in values {
                            params.push(format!("{}={}",
                                urlencoding::encode(key),
                                urlencoding::encode(v)
                            ));
                        }
                    }
                }
            }
            if !params.is_empty() {
                url.push('?');
                url.push_str(&params.join("&"));
            }
        }

        url
    }
}

/// Deep clones query parameters (matches JavaScript cloneQuery)
pub fn clone_query(query: &HashMap<String, QueryValue>) -> HashMap<String, QueryValue> {
    let mut result = HashMap::new();

    for (key, value) in query {
        result.insert(key.clone(), value.clone());
    }

    result
}

/// Identity provider trait
#[async_trait]
pub trait IdentityProvider: Send + Sync {
    async fn get_identity(&self) -> Result<Value>;
}

/// Default Identity Provider Configuration (matches JavaScript DefaultIdentityProviderConfig)
pub struct DefaultIdentityProviderConfig {
    auth_schemes: HashMap<String, Box<dyn IdentityProvider>>,
}

impl DefaultIdentityProviderConfig {
    /// Creates a new configuration with the given auth schemes
    pub fn new(auth_schemes: HashMap<String, Box<dyn IdentityProvider>>) -> Self {
        Self { auth_schemes }
    }

    /// Gets an identity provider by scheme ID
    pub fn get_identity_provider(&self, scheme_id: &str) -> Option<&Box<dyn IdentityProvider>> {
        self.auth_schemes.get(scheme_id)
    }

    /// Adds an identity provider for a scheme
    pub fn add_provider(&mut self, scheme_id: String, provider: Box<dyn IdentityProvider>) {
        self.auth_schemes.insert(scheme_id, provider);
    }

    /// Removes an identity provider
    pub fn remove_provider(&mut self, scheme_id: &str) -> Option<Box<dyn IdentityProvider>> {
        self.auth_schemes.remove(scheme_id)
    }

    /// Checks if a scheme has a provider
    pub fn has_provider(&self, scheme_id: &str) -> bool {
        self.auth_schemes.contains_key(scheme_id)
    }

    /// Gets all scheme IDs
    pub fn get_scheme_ids(&self) -> Vec<String> {
        self.auth_schemes.keys().cloned().collect()
    }
}

/// Trait for HTTP request signers
#[async_trait]
pub trait HttpSigner: Send + Sync {
    async fn sign(
        &self,
        request: &HttpRequest,
        identity: &Value,
        signing_properties: Option<&Value>
    ) -> Result<HttpRequest>;
}

/// HTTP API Key Authentication Signer (matches JavaScript HttpApiKeyAuthSigner)
pub struct HttpApiKeyAuthSigner;

#[async_trait]
impl HttpSigner for HttpApiKeyAuthSigner {
    async fn sign(
        &self,
        request: &HttpRequest,
        identity: &Value,
        signing_properties: Option<&Value>
    ) -> Result<HttpRequest> {
        let props = signing_properties
            .ok_or_else(|| anyhow::anyhow!(
                "request could not be signed with `apiKey` since the `name` and `in` signer properties are missing"
            ))?;

        let name = props.get("name")
            .and_then(|n| n.as_str())
            .ok_or_else(|| anyhow::anyhow!(
                "request could not be signed with `apiKey` since the `name` signer property is missing"
            ))?;

        let location = props.get("in")
            .and_then(|l| l.as_str())
            .ok_or_else(|| anyhow::anyhow!(
                "request could not be signed with `apiKey` since the `in` signer property is missing"
            ))?;

        let api_key = identity.get("apiKey")
            .and_then(|k| k.as_str())
            .ok_or_else(|| anyhow::anyhow!(
                "request could not be signed with `apiKey` since the `apiKey` is not defined"
            ))?;

        let mut cloned_request = HttpRequest::clone(request);

        if location == HttpApiKeyAuthLocation::QUERY {
            // Add API key to query parameters
            cloned_request.query.insert(
                name.to_string(),
                QueryValue::Single(api_key.to_string())
            );
        } else if location == HttpApiKeyAuthLocation::HEADER {
            // Add API key to headers
            let value = if let Some(scheme) = props.get("scheme").and_then(|s| s.as_str()) {
                format!("{} {}", scheme, api_key)
            } else {
                api_key.to_string()
            };
            cloned_request.headers.insert(name.to_string(), value);
        } else {
            return Err(anyhow::anyhow!(
                "request can only be signed with `apiKey` locations `query` or `header`, but found: `{}`",
                location
            ));
        }

        Ok(cloned_request)
    }
}

/// HTTP Bearer Token Authentication Signer (matches JavaScript HttpBearerAuthSigner)
pub struct HttpBearerAuthSigner;

#[async_trait]
impl HttpSigner for HttpBearerAuthSigner {
    async fn sign(
        &self,
        request: &HttpRequest,
        identity: &Value,
        _signing_properties: Option<&Value>
    ) -> Result<HttpRequest> {
        let mut cloned_request = HttpRequest::clone(request);

        let token = identity.get("token")
            .and_then(|t| t.as_str())
            .ok_or_else(|| anyhow::anyhow!(
                "request could not be signed with `token` since the `token` is not defined"
            ))?;

        cloned_request.headers.insert(
            "Authorization".to_string(),
            format!("Bearer {}", token)
        );

        Ok(cloned_request)
    }
}

/// No Authentication Signer (pass-through) - matches JavaScript NoAuthSigner
pub struct NoAuthSigner;

#[async_trait]
impl HttpSigner for NoAuthSigner {
    async fn sign(
        &self,
        request: &HttpRequest,
        _identity: &Value,
        _signing_properties: Option<&Value>
    ) -> Result<HttpRequest> {
        // Pass-through, return request unmodified
        Ok(request.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_http_request_creation() {
        let request = HttpRequest::new(
            "GET".to_string(),
            "https://api.example.com:8080/path?key=value&arr=1&arr=2"
        ).unwrap();

        assert_eq!(request.method, "GET");
        assert_eq!(request.protocol, "https");
        assert_eq!(request.hostname, "api.example.com");
        assert_eq!(request.port, Some(8080));
        assert_eq!(request.path, "/path");

        // Check query parsing
        assert!(matches!(request.query.get("key"), Some(QueryValue::Single(v)) if v == "value"));
        assert!(matches!(request.query.get("arr"), Some(QueryValue::Multiple(v)) if v.len() == 2));
    }

    #[test]
    fn test_http_request_clone() {
        let mut request = HttpRequest::new("POST".to_string(), "https://api.example.com/test").unwrap();
        request.headers.insert("X-Custom".to_string(), "value".to_string());
        request.body = Some(b"test body".to_vec());

        let cloned = HttpRequest::clone(&request);

        assert_eq!(cloned.method, request.method);
        assert_eq!(cloned.hostname, request.hostname);
        assert_eq!(cloned.headers, request.headers);
        assert_eq!(cloned.body, request.body);

        // Ensure it's a deep clone
        assert_eq!(cloned.headers.get("X-Custom"), Some(&"value".to_string()));
    }

    #[test]
    fn test_query_cloning() {
        let mut query = HashMap::new();
        query.insert("single".to_string(), QueryValue::Single("value".to_string()));
        query.insert("multi".to_string(), QueryValue::Multiple(vec!["a".to_string(), "b".to_string()]));

        let cloned = clone_query(&query);

        assert_eq!(cloned.len(), 2);
        assert!(matches!(cloned.get("single"), Some(QueryValue::Single(v)) if v == "value"));
        assert!(matches!(cloned.get("multi"), Some(QueryValue::Multiple(v)) if v.len() == 2));
    }

    #[test]
    fn test_is_instance() {
        let valid = json!({
            "method": "GET",
            "protocol": "https",
            "hostname": "example.com",
            "path": "/",
            "query": {},
            "headers": {}
        });

        assert!(HttpRequest::is_instance(&valid));

        let invalid = json!({
            "method": "GET",
            "hostname": "example.com"
        });

        assert!(!HttpRequest::is_instance(&invalid));
        assert!(!HttpRequest::is_instance(&Value::Null));
    }

    #[tokio::test]
    async fn test_no_auth_signer() {
        let request = HttpRequest::new("GET".to_string(), "https://api.example.com/test").unwrap();
        let signer = NoAuthSigner;

        let signed = signer.sign(&request, &json!({}), None).await.unwrap();

        // Should return request unchanged
        assert_eq!(signed.method, request.method);
        assert_eq!(signed.headers.len(), 0);
    }

    #[tokio::test]
    async fn test_bearer_auth_signer() {
        let request = HttpRequest::new("GET".to_string(), "https://api.example.com/test").unwrap();
        let signer = HttpBearerAuthSigner;

        let identity = json!({
            "token": "test-token-123"
        });

        let signed = signer.sign(&request, &identity, None).await.unwrap();

        assert_eq!(signed.headers.get("Authorization"), Some(&"Bearer test-token-123".to_string()));
    }

    #[tokio::test]
    async fn test_bearer_auth_signer_missing_token() {
        let request = HttpRequest::new("GET".to_string(), "https://api.example.com/test").unwrap();
        let signer = HttpBearerAuthSigner;

        let identity = json!({});

        let result = signer.sign(&request, &identity, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("token"));
    }

    #[tokio::test]
    async fn test_api_key_auth_signer_header() {
        let request = HttpRequest::new("GET".to_string(), "https://api.example.com/test").unwrap();
        let signer = HttpApiKeyAuthSigner;

        let identity = json!({
            "apiKey": "my-api-key"
        });

        let props = json!({
            "name": "X-API-Key",
            "in": "header"
        });

        let signed = signer.sign(&request, &identity, Some(&props)).await.unwrap();

        assert_eq!(signed.headers.get("X-API-Key"), Some(&"my-api-key".to_string()));
    }

    #[tokio::test]
    async fn test_api_key_auth_signer_header_with_scheme() {
        let request = HttpRequest::new("GET".to_string(), "https://api.example.com/test").unwrap();
        let signer = HttpApiKeyAuthSigner;

        let identity = json!({
            "apiKey": "my-api-key"
        });

        let props = json!({
            "name": "Authorization",
            "in": "header",
            "scheme": "ApiKey"
        });

        let signed = signer.sign(&request, &identity, Some(&props)).await.unwrap();

        assert_eq!(signed.headers.get("Authorization"), Some(&"ApiKey my-api-key".to_string()));
    }

    #[tokio::test]
    async fn test_api_key_auth_signer_query() {
        let request = HttpRequest::new("GET".to_string(), "https://api.example.com/test").unwrap();
        let signer = HttpApiKeyAuthSigner;

        let identity = json!({
            "apiKey": "my-api-key"
        });

        let props = json!({
            "name": "api_key",
            "in": "query"
        });

        let signed = signer.sign(&request, &identity, Some(&props)).await.unwrap();

        assert!(matches!(signed.query.get("api_key"), Some(QueryValue::Single(v)) if v == "my-api-key"));
    }

    #[tokio::test]
    async fn test_api_key_auth_signer_invalid_location() {
        let request = HttpRequest::new("GET".to_string(), "https://api.example.com/test").unwrap();
        let signer = HttpApiKeyAuthSigner;

        let identity = json!({
            "apiKey": "my-api-key"
        });

        let props = json!({
            "name": "api_key",
            "in": "cookie"  // Invalid location
        });

        let result = signer.sign(&request, &identity, Some(&props)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cookie"));
    }

    #[tokio::test]
    async fn test_api_key_auth_signer_missing_props() {
        let request = HttpRequest::new("GET".to_string(), "https://api.example.com/test").unwrap();
        let signer = HttpApiKeyAuthSigner;

        let identity = json!({
            "apiKey": "my-api-key"
        });

        let result = signer.sign(&request, &identity, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("signer properties are missing"));
    }

    #[test]
    fn test_build_url() {
        let mut request = HttpRequest::new("GET".to_string(), "https://api.example.com/test").unwrap();
        request.query.insert("key".to_string(), QueryValue::Single("value".to_string()));
        request.query.insert("multi".to_string(), QueryValue::Multiple(vec!["a".to_string(), "b".to_string()]));

        let url = request.build_url();
        assert!(url.starts_with("https://api.example.com/test?"));
        assert!(url.contains("key=value"));
        assert!(url.contains("multi=a"));
        assert!(url.contains("multi=b"));
    }

    #[test]
    fn test_default_identity_provider_config() {
        let mut config = DefaultIdentityProviderConfig::new(HashMap::new());

        // Initially empty
        assert!(!config.has_provider("test"));
        assert_eq!(config.get_scheme_ids().len(), 0);

        // Add a mock provider
        struct MockProvider;
        #[async_trait]
        impl IdentityProvider for MockProvider {
            async fn get_identity(&self) -> Result<Value> {
                Ok(json!({"test": true}))
            }
        }

        config.add_provider("test".to_string(), Box::new(MockProvider));

        assert!(config.has_provider("test"));
        assert!(config.get_identity_provider("test").is_some());
        assert_eq!(config.get_scheme_ids().len(), 1);

        // Remove provider
        let removed = config.remove_provider("test");
        assert!(removed.is_some());
        assert!(!config.has_provider("test"));
    }
}