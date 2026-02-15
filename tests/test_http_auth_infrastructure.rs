use llminate::auth::http::{
    HttpRequest, HttpSigner, HttpApiKeyAuthSigner, HttpBearerAuthSigner,
    NoAuthSigner, HttpApiKeyAuthLocation, DefaultIdentityProviderConfig,
    IdentityProvider, QueryValue, clone_query
};
use serde_json::{json, Value};
use std::collections::HashMap;
use anyhow::Result;
use async_trait::async_trait;

#[cfg(test)]
mod http_request_tests {
    use super::*;

    #[test]
    fn test_http_request_new() {
        // Test basic URL parsing
        let req = HttpRequest::new("GET".to_string(), "https://api.example.com/test").unwrap();
        assert_eq!(req.method, "GET");
        assert_eq!(req.protocol, "https");
        assert_eq!(req.hostname, "api.example.com");
        assert_eq!(req.port, None);
        assert_eq!(req.path, "/test");
        assert!(req.query.is_empty());

        // Test with port
        let req = HttpRequest::new("POST".to_string(), "http://localhost:8080/api").unwrap();
        assert_eq!(req.protocol, "http");
        assert_eq!(req.hostname, "localhost");
        assert_eq!(req.port, Some(8080));
        assert_eq!(req.path, "/api");

        // Test with query parameters
        let req = HttpRequest::new("GET".to_string(), "https://api.example.com/search?q=test&limit=10").unwrap();
        assert!(matches!(req.query.get("q"), Some(QueryValue::Single(v)) if v == "test"));
        assert!(matches!(req.query.get("limit"), Some(QueryValue::Single(v)) if v == "10"));

        // Test with multiple same-name query parameters
        let req = HttpRequest::new("GET".to_string(), "https://api.example.com/test?tag=a&tag=b&tag=c").unwrap();
        assert!(matches!(req.query.get("tag"), Some(QueryValue::Multiple(v)) if v.len() == 3));
    }

    #[test]
    fn test_http_request_clone() {
        let mut original = HttpRequest::new("POST".to_string(), "https://api.example.com/test?key=value").unwrap();
        original.headers.insert("X-Custom-Header".to_string(), "test-value".to_string());
        original.headers.insert("Authorization".to_string(), "Bearer token".to_string());
        original.body = Some(b"test body content".to_vec());

        // Test clone method
        let cloned = original.clone_request();
        assert_eq!(cloned.method, original.method);
        assert_eq!(cloned.protocol, original.protocol);
        assert_eq!(cloned.hostname, original.hostname);
        assert_eq!(cloned.port, original.port);
        assert_eq!(cloned.path, original.path);
        assert_eq!(cloned.headers.len(), original.headers.len());
        assert_eq!(cloned.headers.get("X-Custom-Header"), Some(&"test-value".to_string()));
        assert_eq!(cloned.body, original.body);

        // Test static clone method
        let static_cloned = HttpRequest::clone(&original);
        assert_eq!(static_cloned.method, original.method);
        assert_eq!(static_cloned.headers.len(), original.headers.len());
    }

    #[test]
    fn test_is_instance() {
        // Valid HttpRequest-like object
        let valid = json!({
            "method": "GET",
            "protocol": "https",
            "hostname": "api.example.com",
            "path": "/v1/test",
            "query": {},
            "headers": {
                "Content-Type": "application/json"
            }
        });
        assert!(HttpRequest::is_instance(&valid));

        // Missing required fields
        let invalid_missing_method = json!({
            "protocol": "https",
            "hostname": "api.example.com",
            "path": "/v1/test",
            "query": {},
            "headers": {}
        });
        assert!(!HttpRequest::is_instance(&invalid_missing_method));

        // Null value
        assert!(!HttpRequest::is_instance(&Value::Null));

        // Non-object value
        assert!(!HttpRequest::is_instance(&json!("string")));
        assert!(!HttpRequest::is_instance(&json!(123)));
    }

    #[test]
    fn test_build_url() {
        // Basic URL
        let req = HttpRequest::new("GET".to_string(), "https://api.example.com/test").unwrap();
        assert_eq!(req.build_url(), "https://api.example.com/test");

        // URL with custom port
        let req = HttpRequest::new("GET".to_string(), "http://localhost:3000/api").unwrap();
        assert_eq!(req.build_url(), "http://localhost:3000/api");

        // URL with default ports (should be omitted)
        let mut req = HttpRequest::new("GET".to_string(), "https://api.example.com/test").unwrap();
        req.port = Some(443);
        assert_eq!(req.build_url(), "https://api.example.com/test");

        let mut req = HttpRequest::new("GET".to_string(), "http://api.example.com/test").unwrap();
        req.port = Some(80);
        assert_eq!(req.build_url(), "http://api.example.com/test");

        // URL with query parameters
        let mut req = HttpRequest::new("GET".to_string(), "https://api.example.com/search").unwrap();
        req.query.insert("q".to_string(), QueryValue::Single("test query".to_string()));
        req.query.insert("limit".to_string(), QueryValue::Single("10".to_string()));
        let url = req.build_url();
        assert!(url.starts_with("https://api.example.com/search?"));
        assert!(url.contains("q=test%20query"));
        assert!(url.contains("limit=10"));

        // URL with multiple same-name parameters
        let mut req = HttpRequest::new("GET".to_string(), "https://api.example.com/filter").unwrap();
        req.query.insert("tag".to_string(), QueryValue::Multiple(vec![
            "rust".to_string(),
            "async".to_string(),
            "web".to_string()
        ]));
        let url = req.build_url();
        assert!(url.contains("tag=rust"));
        assert!(url.contains("tag=async"));
        assert!(url.contains("tag=web"));
    }

    #[test]
    fn test_clone_query() {
        let mut original = HashMap::new();
        original.insert("simple".to_string(), QueryValue::Single("value".to_string()));
        original.insert("array".to_string(), QueryValue::Multiple(vec![
            "first".to_string(),
            "second".to_string(),
            "third".to_string()
        ]));

        let cloned = clone_query(&original);

        // Verify the clone has the same content
        assert_eq!(cloned.len(), original.len());
        assert!(matches!(cloned.get("simple"), Some(QueryValue::Single(v)) if v == "value"));
        assert!(matches!(cloned.get("array"), Some(QueryValue::Multiple(v)) if v.len() == 3));

        // Verify it's a deep clone (modifying clone doesn't affect original)
        drop(cloned);
        assert_eq!(original.len(), 2); // Original unchanged
    }
}

#[cfg(test)]
mod signer_tests {
    use super::*;

    #[tokio::test]
    async fn test_no_auth_signer() {
        let signer = NoAuthSigner;
        let request = HttpRequest::new("GET".to_string(), "https://api.example.com/test").unwrap();
        let identity = json!({});

        let signed = signer.sign(&request, &identity, None).await.unwrap();

        // NoAuthSigner should return request unchanged
        assert_eq!(signed.method, request.method);
        assert_eq!(signed.hostname, request.hostname);
        assert_eq!(signed.headers.len(), request.headers.len());
    }

    #[tokio::test]
    async fn test_bearer_auth_signer_success() {
        let signer = HttpBearerAuthSigner;
        let request = HttpRequest::new("GET".to_string(), "https://api.example.com/protected").unwrap();
        let identity = json!({
            "token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.test"
        });

        let signed = signer.sign(&request, &identity, None).await.unwrap();

        assert_eq!(
            signed.headers.get("Authorization"),
            Some(&"Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.test".to_string())
        );
    }

    #[tokio::test]
    async fn test_bearer_auth_signer_missing_token() {
        let signer = HttpBearerAuthSigner;
        let request = HttpRequest::new("GET".to_string(), "https://api.example.com/protected").unwrap();
        let identity = json!({}); // No token

        let result = signer.sign(&request, &identity, None).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("token"));
        assert!(err.to_string().contains("not defined"));
    }

    #[tokio::test]
    async fn test_api_key_signer_header_simple() {
        let signer = HttpApiKeyAuthSigner;
        let request = HttpRequest::new("GET".to_string(), "https://api.example.com/data").unwrap();
        let identity = json!({
            "apiKey": "sk-test123456789"
        });
        let props = json!({
            "name": "X-API-Key",
            "in": "header"
        });

        let signed = signer.sign(&request, &identity, Some(&props)).await.unwrap();

        assert_eq!(signed.headers.get("X-API-Key"), Some(&"sk-test123456789".to_string()));
    }

    #[tokio::test]
    async fn test_api_key_signer_header_with_scheme() {
        let signer = HttpApiKeyAuthSigner;
        let request = HttpRequest::new("GET".to_string(), "https://api.example.com/data").unwrap();
        let identity = json!({
            "apiKey": "test-key-123"
        });
        let props = json!({
            "name": "Authorization",
            "in": "header",
            "scheme": "ApiKey"
        });

        let signed = signer.sign(&request, &identity, Some(&props)).await.unwrap();

        assert_eq!(signed.headers.get("Authorization"), Some(&"ApiKey test-key-123".to_string()));
    }

    #[tokio::test]
    async fn test_api_key_signer_query_parameter() {
        let signer = HttpApiKeyAuthSigner;
        let request = HttpRequest::new("GET".to_string(), "https://api.example.com/data").unwrap();
        let identity = json!({
            "apiKey": "public-key-xyz"
        });
        let props = json!({
            "name": "api_key",
            "in": "query"
        });

        let signed = signer.sign(&request, &identity, Some(&props)).await.unwrap();

        assert!(matches!(
            signed.query.get("api_key"),
            Some(QueryValue::Single(v)) if v == "public-key-xyz"
        ));
    }

    #[tokio::test]
    async fn test_api_key_signer_invalid_location() {
        let signer = HttpApiKeyAuthSigner;
        let request = HttpRequest::new("GET".to_string(), "https://api.example.com/data").unwrap();
        let identity = json!({
            "apiKey": "test-key"
        });
        let props = json!({
            "name": "api_key",
            "in": "cookie" // Invalid location
        });

        let result = signer.sign(&request, &identity, Some(&props)).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("cookie"));
        assert!(err.to_string().contains("can only be signed with"));
    }

    #[tokio::test]
    async fn test_api_key_signer_missing_properties() {
        let signer = HttpApiKeyAuthSigner;
        let request = HttpRequest::new("GET".to_string(), "https://api.example.com/data").unwrap();
        let identity = json!({
            "apiKey": "test-key"
        });

        // No signing properties provided
        let result = signer.sign(&request, &identity, None).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("signer properties are missing"));
    }

    #[tokio::test]
    async fn test_api_key_signer_missing_name() {
        let signer = HttpApiKeyAuthSigner;
        let request = HttpRequest::new("GET".to_string(), "https://api.example.com/data").unwrap();
        let identity = json!({
            "apiKey": "test-key"
        });
        let props = json!({
            "in": "header" // Missing "name"
        });

        let result = signer.sign(&request, &identity, Some(&props)).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("`name` signer property is missing"));
    }

    #[tokio::test]
    async fn test_api_key_signer_missing_location() {
        let signer = HttpApiKeyAuthSigner;
        let request = HttpRequest::new("GET".to_string(), "https://api.example.com/data").unwrap();
        let identity = json!({
            "apiKey": "test-key"
        });
        let props = json!({
            "name": "X-API-Key" // Missing "in"
        });

        let result = signer.sign(&request, &identity, Some(&props)).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("`in` signer property is missing"));
    }

    #[tokio::test]
    async fn test_api_key_signer_missing_api_key() {
        let signer = HttpApiKeyAuthSigner;
        let request = HttpRequest::new("GET".to_string(), "https://api.example.com/data").unwrap();
        let identity = json!({}); // No apiKey
        let props = json!({
            "name": "X-API-Key",
            "in": "header"
        });

        let result = signer.sign(&request, &identity, Some(&props)).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("`apiKey` is not defined"));
    }

    #[tokio::test]
    async fn test_request_cloning_preserves_original() {
        let signer = HttpApiKeyAuthSigner;
        let mut original = HttpRequest::new("GET".to_string(), "https://api.example.com/data").unwrap();
        original.headers.insert("X-Original".to_string(), "should-remain".to_string());

        let identity = json!({
            "apiKey": "test-key"
        });
        let props = json!({
            "name": "X-API-Key",
            "in": "header"
        });

        let signed = signer.sign(&original, &identity, Some(&props)).await.unwrap();

        // Original should be unchanged
        assert!(!original.headers.contains_key("X-API-Key"));
        assert_eq!(original.headers.get("X-Original"), Some(&"should-remain".to_string()));

        // Signed should have both headers
        assert_eq!(signed.headers.get("X-API-Key"), Some(&"test-key".to_string()));
        assert_eq!(signed.headers.get("X-Original"), Some(&"should-remain".to_string()));
    }
}

#[cfg(test)]
mod identity_provider_tests {
    use super::*;

    struct MockIdentityProvider {
        identity: Value,
    }

    #[async_trait]
    impl IdentityProvider for MockIdentityProvider {
        async fn get_identity(&self) -> Result<Value> {
            Ok(self.identity.clone())
        }
    }

    #[test]
    fn test_default_identity_provider_config() {
        let mut config = DefaultIdentityProviderConfig::new(HashMap::new());

        // Initially empty
        assert_eq!(config.get_scheme_ids().len(), 0);
        assert!(!config.has_provider("oauth"));
        assert!(config.get_identity_provider("oauth").is_none());

        // Add providers
        let oauth_provider = Box::new(MockIdentityProvider {
            identity: json!({"token": "oauth-token"}),
        });
        config.add_provider("oauth".to_string(), oauth_provider);

        let api_key_provider = Box::new(MockIdentityProvider {
            identity: json!({"apiKey": "test-key"}),
        });
        config.add_provider("apiKey".to_string(), api_key_provider);

        // Verify providers added
        assert_eq!(config.get_scheme_ids().len(), 2);
        assert!(config.has_provider("oauth"));
        assert!(config.has_provider("apiKey"));
        assert!(config.get_identity_provider("oauth").is_some());
        assert!(config.get_identity_provider("apiKey").is_some());

        // Remove a provider
        let removed = config.remove_provider("oauth");
        assert!(removed.is_some());
        assert!(!config.has_provider("oauth"));
        assert_eq!(config.get_scheme_ids().len(), 1);
    }

    #[tokio::test]
    async fn test_identity_provider_with_signers() {
        // Create config with identity providers
        let mut config = DefaultIdentityProviderConfig::new(HashMap::new());

        config.add_provider("bearer".to_string(), Box::new(MockIdentityProvider {
            identity: json!({"token": "test-bearer-token"}),
        }));

        config.add_provider("apiKey".to_string(), Box::new(MockIdentityProvider {
            identity: json!({"apiKey": "test-api-key"}),
        }));

        // Test with bearer signer
        if let Some(provider) = config.get_identity_provider("bearer") {
            let identity = provider.get_identity().await.unwrap();
            let request = HttpRequest::new("GET".to_string(), "https://api.example.com/test").unwrap();
            let signer = HttpBearerAuthSigner;
            let signed = signer.sign(&request, &identity, None).await.unwrap();
            assert_eq!(signed.headers.get("Authorization"), Some(&"Bearer test-bearer-token".to_string()));
        }

        // Test with API key signer
        if let Some(provider) = config.get_identity_provider("apiKey") {
            let identity = provider.get_identity().await.unwrap();
            let request = HttpRequest::new("GET".to_string(), "https://api.example.com/test").unwrap();
            let signer = HttpApiKeyAuthSigner;
            let props = json!({"name": "X-API-Key", "in": "header"});
            let signed = signer.sign(&request, &identity, Some(&props)).await.unwrap();
            assert_eq!(signed.headers.get("X-API-Key"), Some(&"test-api-key".to_string()));
        }
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_full_auth_flow_with_bearer() {
        // Create request
        let mut request = HttpRequest::new("POST".to_string(), "https://api.anthropic.com/v1/messages").unwrap();
        request.headers.insert("Content-Type".to_string(), "application/json".to_string());
        request.body = Some(br#"{"model":"claude-3","messages":[]}"#.to_vec());

        // Setup identity
        let identity = json!({
            "token": "sk-ant-api03-test-token"
        });

        // Sign request
        let signer = HttpBearerAuthSigner;
        let signed = signer.sign(&request, &identity, None).await.unwrap();

        // Verify
        assert_eq!(signed.headers.get("Authorization"), Some(&"Bearer sk-ant-api03-test-token".to_string()));
        assert_eq!(signed.headers.get("Content-Type"), Some(&"application/json".to_string()));
        assert_eq!(signed.body, request.body);
    }

    #[tokio::test]
    async fn test_full_auth_flow_with_api_key() {
        // Create request
        let request = HttpRequest::new("GET".to_string(), "https://api.example.com/v2/data?format=json").unwrap();

        // Setup identity
        let identity = json!({
            "apiKey": "pk_live_51234567890"
        });

        // Sign request with query parameter
        let signer = HttpApiKeyAuthSigner;
        let props = json!({
            "name": "api_key",
            "in": "query"
        });
        let signed = signer.sign(&request, &identity, Some(&props)).await.unwrap();

        // Build final URL
        let final_url = signed.build_url();
        assert!(final_url.contains("api_key=pk_live_51234567890"));
        assert!(final_url.contains("format=json"));
    }

    #[tokio::test]
    async fn test_multiple_signers_on_same_request() {
        let original = HttpRequest::new("GET".to_string(), "https://api.example.com/test").unwrap();

        // First signer - API key in header
        let api_signer = HttpApiKeyAuthSigner;
        let api_identity = json!({"apiKey": "test-api-key"});
        let api_props = json!({"name": "X-API-Key", "in": "header"});
        let signed1 = api_signer.sign(&original, &api_identity, Some(&api_props)).await.unwrap();

        // Second signer - Bearer token (on already signed request)
        let bearer_signer = HttpBearerAuthSigner;
        let bearer_identity = json!({"token": "bearer-token"});
        let signed2 = bearer_signer.sign(&signed1, &bearer_identity, None).await.unwrap();

        // Should have both authentication headers
        assert_eq!(signed2.headers.get("X-API-Key"), Some(&"test-api-key".to_string()));
        assert_eq!(signed2.headers.get("Authorization"), Some(&"Bearer bearer-token".to_string()));

        // Original should be unchanged
        assert_eq!(original.headers.len(), 0);
    }
}

#[cfg(test)]
mod real_world_integration_tests {
    use super::*;
    use wiremock::{MockServer, Mock, ResponseTemplate};
    use wiremock::matchers::{header, query_param, method, path};

    #[tokio::test]
    async fn test_api_key_header_with_mock_server() {
        // Start mock server
        let mock_server = MockServer::start().await;

        // Setup mock endpoint
        Mock::given(method("GET"))
            .and(path("/api/v1/users"))
            .and(header("X-API-Key", "test-api-key-123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "users": ["alice", "bob"]
            })))
            .mount(&mock_server)
            .await;

        // Create request
        let request = HttpRequest::new("GET".to_string(), &format!("{}/api/v1/users", mock_server.uri())).unwrap();

        // Sign with API key
        let signer = HttpApiKeyAuthSigner;
        let identity = json!({"apiKey": "test-api-key-123"});
        let props = json!({"name": "X-API-Key", "in": "header"});
        let signed = signer.sign(&request, &identity, Some(&props)).await.unwrap();

        // Convert to reqwest and execute
        let client = reqwest::Client::new();
        let mut req_builder = client.get(&signed.build_url());
        for (key, value) in &signed.headers {
            req_builder = req_builder.header(key, value);
        }

        let response = req_builder.send().await.unwrap();
        assert_eq!(response.status(), 200);

        let body: Value = response.json().await.unwrap();
        assert_eq!(body["users"][0], "alice");
    }

    #[tokio::test]
    async fn test_api_key_query_with_mock_server() {
        // Start mock server
        let mock_server = MockServer::start().await;

        // Setup mock endpoint
        Mock::given(method("GET"))
            .and(path("/api/v1/data"))
            .and(query_param("api_key", "secret-key-456"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": "sensitive information"
            })))
            .mount(&mock_server)
            .await;

        // Create request
        let request = HttpRequest::new("GET".to_string(), &format!("{}/api/v1/data", mock_server.uri())).unwrap();

        // Sign with API key in query
        let signer = HttpApiKeyAuthSigner;
        let identity = json!({"apiKey": "secret-key-456"});
        let props = json!({"name": "api_key", "in": "query"});
        let signed = signer.sign(&request, &identity, Some(&props)).await.unwrap();

        // Execute request
        let client = reqwest::Client::new();
        let response = client.get(&signed.build_url()).send().await.unwrap();
        assert_eq!(response.status(), 200);

        let body: Value = response.json().await.unwrap();
        assert_eq!(body["data"], "sensitive information");
    }

    #[tokio::test]
    async fn test_bearer_token_with_mock_server() {
        // Start mock server
        let mock_server = MockServer::start().await;

        // Setup mock endpoint
        Mock::given(method("POST"))
            .and(path("/api/v1/messages"))
            .and(header("Authorization", "Bearer jwt-token-789"))
            .and(header("Content-Type", "application/json"))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "id": "msg-123",
                "status": "created"
            })))
            .mount(&mock_server)
            .await;

        // Create request
        let mut request = HttpRequest::new("POST".to_string(), &format!("{}/api/v1/messages", mock_server.uri())).unwrap();
        request.headers.insert("Content-Type".to_string(), "application/json".to_string());
        request.body = Some(br#"{"text":"Hello"}"#.to_vec());

        // Sign with Bearer token
        let signer = HttpBearerAuthSigner;
        let identity = json!({"token": "jwt-token-789"});
        let signed = signer.sign(&request, &identity, None).await.unwrap();

        // Execute request
        let client = reqwest::Client::new();
        let mut req_builder = client.post(&signed.build_url());
        for (key, value) in &signed.headers {
            req_builder = req_builder.header(key, value);
        }
        if let Some(body) = &signed.body {
            req_builder = req_builder.body(body.clone());
        }

        let response = req_builder.send().await.unwrap();
        assert_eq!(response.status(), 201);

        let body: Value = response.json().await.unwrap();
        assert_eq!(body["id"], "msg-123");
        assert_eq!(body["status"], "created");
    }

    #[tokio::test]
    async fn test_chained_authentication_with_mock_server() {
        // Start mock server
        let mock_server = MockServer::start().await;

        // Setup mock endpoint requiring both API key and Bearer token
        Mock::given(method("GET"))
            .and(path("/api/v1/secure"))
            .and(header("X-API-Key", "api-key"))
            .and(header("Authorization", "Bearer oauth-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access": "granted"
            })))
            .mount(&mock_server)
            .await;

        // Create request
        let request = HttpRequest::new("GET".to_string(), &format!("{}/api/v1/secure", mock_server.uri())).unwrap();

        // First sign with API key
        let api_signer = HttpApiKeyAuthSigner;
        let api_identity = json!({"apiKey": "api-key"});
        let api_props = json!({"name": "X-API-Key", "in": "header"});
        let signed_once = api_signer.sign(&request, &api_identity, Some(&api_props)).await.unwrap();

        // Then sign with Bearer token
        let bearer_signer = HttpBearerAuthSigner;
        let bearer_identity = json!({"token": "oauth-token"});
        let signed_twice = bearer_signer.sign(&signed_once, &bearer_identity, None).await.unwrap();

        // Execute request
        let client = reqwest::Client::new();
        let mut req_builder = client.get(&signed_twice.build_url());
        for (key, value) in &signed_twice.headers {
            req_builder = req_builder.header(key, value);
        }

        let response = req_builder.send().await.unwrap();
        assert_eq!(response.status(), 200);

        let body: Value = response.json().await.unwrap();
        assert_eq!(body["access"], "granted");
    }

    #[tokio::test]
    async fn test_no_auth_signer_with_mock_server() {
        // Start mock server
        let mock_server = MockServer::start().await;

        // Setup public endpoint (no auth required)
        Mock::given(method("GET"))
            .and(path("/api/v1/public"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "message": "public data"
            })))
            .mount(&mock_server)
            .await;

        // Create request
        let request = HttpRequest::new("GET".to_string(), &format!("{}/api/v1/public", mock_server.uri())).unwrap();

        // Use NoAuthSigner
        let signer = NoAuthSigner;
        let identity = json!({});
        let signed = signer.sign(&request, &identity, None).await.unwrap();

        // Should be unchanged
        assert_eq!(signed.headers.len(), 0);

        // Execute request
        let client = reqwest::Client::new();
        let response = client.get(&signed.build_url()).send().await.unwrap();
        assert_eq!(response.status(), 200);

        let body: Value = response.json().await.unwrap();
        assert_eq!(body["message"], "public data");
    }

    #[tokio::test]
    async fn test_identity_provider_integration() {
        // Create a real identity provider
        struct ApiKeyProvider {
            key: String,
        }

        #[async_trait]
        impl IdentityProvider for ApiKeyProvider {
            async fn get_identity(&self) -> Result<Value> {
                // Simulate fetching from secure storage
                Ok(json!({"apiKey": self.key.clone()}))
            }
        }

        // Setup provider config
        let mut config = DefaultIdentityProviderConfig::new(HashMap::new());
        config.add_provider("primary".to_string(), Box::new(ApiKeyProvider {
            key: "sk-production-key".to_string(),
        }));

        // Get provider and sign request
        let provider = config.get_identity_provider("primary").unwrap();
        let identity = provider.get_identity().await.unwrap();

        let request = HttpRequest::new("GET".to_string(), "https://api.example.com/test").unwrap();
        let signer = HttpApiKeyAuthSigner;
        let props = json!({"name": "X-API-Key", "in": "header"});
        let signed = signer.sign(&request, &identity, Some(&props)).await.unwrap();

        assert_eq!(signed.headers.get("X-API-Key"), Some(&"sk-production-key".to_string()));
    }
}