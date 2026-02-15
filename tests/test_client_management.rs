use llminate::auth::client::*;
use reqwest::header::{HeaderMap, HeaderValue, HeaderName};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use wiremock::{Mock, MockServer, ResponseTemplate};
use wiremock::matchers::{method, path, header};

#[tokio::test]
async fn test_client_creation_with_api_key() {
    let mut config = ClientConfig::default();
    config.api_key = Some("test-api-key".to_string());

    let client = AnthropicClient::new(config);
    assert!(client.is_ok());
}

#[tokio::test]
async fn test_client_creation_with_auth_token() {
    let mut config = ClientConfig::default();
    config.auth_token = Some("test-bearer-token".to_string());

    let client = AnthropicClient::new(config);
    assert!(client.is_ok());
}

#[tokio::test]
async fn test_browser_environment_detection() {
    // Test that browser environment is properly detected in WASM
    #[cfg(target_arch = "wasm32")]
    {
        let config = ClientConfig::default();
        let client = AnthropicClient::new(config);
        assert!(client.is_err()); // Should fail without dangerouslyAllowBrowser

        let mut config = ClientConfig::default();
        config.dangerously_allow_browser = true;
        let client = AnthropicClient::new(config);
        assert!(client.is_ok()); // Should succeed with flag
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        // Not in browser, should always succeed
        let config = ClientConfig::default();
        let client = AnthropicClient::new(config);
        assert!(client.is_ok());
    }
}

#[tokio::test]
async fn test_validate_headers_with_api_key() {
    let mut config = ClientConfig::default();
    config.api_key = Some("test-key".to_string());

    let client = AnthropicClient::new(config).unwrap();

    // Build headers should succeed with API key
    let options = RequestOptions::default();
    let result = client.build_headers(
        &reqwest::Method::POST,
        HeaderMap::new(),
        0,
        &options
    );

    assert!(result.is_ok());
    let headers = result.unwrap();
    assert!(headers.contains_key("x-api-key"));
}

#[tokio::test]
async fn test_validate_headers_with_bearer_token() {
    let mut config = ClientConfig::default();
    config.auth_token = Some("test-token".to_string());

    let client = AnthropicClient::new(config).unwrap();

    // Build headers should succeed with bearer token
    let options = RequestOptions::default();
    let result = client.build_headers(
        &reqwest::Method::POST,
        HeaderMap::new(),
        0,
        &options
    );

    assert!(result.is_ok());
    let headers = result.unwrap();
    assert!(headers.contains_key("authorization"));
}

#[tokio::test]
async fn test_validate_headers_fails_without_auth() {
    let config = ClientConfig::default();
    // No API key or auth token set

    let client = AnthropicClient::new(config).unwrap();

    // Build headers should fail without any auth
    let options = RequestOptions::default();
    let result = client.build_headers(
        &reqwest::Method::POST,
        HeaderMap::new(),
        0,
        &options
    );

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("Could not resolve authentication method"));
}

#[tokio::test]
async fn test_validate_headers_with_explicit_null() {
    let mut config = ClientConfig::default();
    config.api_key = Some("test-key".to_string());

    let client = AnthropicClient::new(config).unwrap();

    // Create options that explicitly null the x-api-key header
    let mut options = RequestOptions::default();
    let mut headers = HeaderMap::new();
    headers.insert(
        HeaderName::from_static("x-api-key"),
        HeaderValue::from_static(""), // Empty value means null
    );
    options.headers = Some(headers);

    // This should succeed because the header was explicitly nulled
    let result = client.build_headers(
        &reqwest::Method::POST,
        HeaderMap::new(),
        0,
        &options
    );

    // The validation should pass because nulling is explicit
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_bedrock_client_empty_validation() {
    // Bedrock client should have empty validateHeaders
    let config = ClientConfig::default();
    let client = BedrockClient::new(
        Some("us-west-2".to_string()),
        None,
        None,
        None,
        config,
    ).unwrap();

    // Even without auth, Bedrock should not fail validation
    // (It uses AWS SigV4 instead)
    assert_eq!(client.aws_region, "us-west-2");
}

#[tokio::test]
async fn test_vertex_client_empty_validation() {
    // Vertex client should have empty validateHeaders
    let config = ClientConfig::default();
    let client = VertexClient::new(
        Some("us-central1".to_string()),
        None,
        config,
    ).unwrap();

    // Even without auth, Vertex should not fail validation
    // (It uses Google Cloud auth instead)
    assert_eq!(client.region, "us-central1");
}

#[tokio::test]
async fn test_header_merging() {
    let mut headers1 = HeaderMap::new();
    headers1.insert("x-custom-1", HeaderValue::from_static("value1"));

    let mut headers2 = HeaderMap::new();
    headers2.insert("x-custom-2", HeaderValue::from_static("value2"));
    headers2.insert("x-custom-1", HeaderValue::from_static("override"));

    let mut headers3 = HeaderMap::new();
    headers3.insert("x-custom-3", HeaderValue::from_static(""));  // Null header

    let merged = merge_headers(vec![
        Some(headers1),
        Some(headers2),
        Some(headers3),
    ]);

    // Check merged values
    assert!(merged.values.contains_key("x-custom-2"));
    assert_eq!(merged.values.get("x-custom-1").unwrap(), "override");
    assert!(!merged.values.contains_key("x-custom-3"));

    // Check nulls set
    assert!(merged.nulls.contains("x-custom-3"));
}

#[tokio::test]
async fn test_client_manager() {
    let manager = ClientManager::new();

    // Initially no client
    assert!(manager.get_client().await.is_none());

    // Set a client
    let mut config = ClientConfig::default();
    config.api_key = Some("test-key".to_string());
    let client = Arc::new(AnthropicClient::new(config).unwrap());
    manager.set_client(client.clone()).await;

    // Should retrieve the same client
    let retrieved = manager.get_client().await;
    assert!(retrieved.is_some());
}

#[tokio::test]
async fn test_client_hub_management() {
    let hub = ClientManagerHub::new();

    // Initially no client
    assert!(hub.get_client().await.is_none());

    // Bind a client
    let mut config = ClientConfig::default();
    config.api_key = Some("test-key".to_string());
    let client = Arc::new(AnthropicClient::new(config).unwrap());
    hub.bind_client(client.clone()).await;

    // Should retrieve the client
    let retrieved = hub.get_client().await;
    assert!(retrieved.is_some());
}

#[tokio::test]
async fn test_idempotency_key_generation() {
    let config = ClientConfig::default();
    let client = AnthropicClient::new(config).unwrap();

    let key1 = client.default_idempotency_key();
    let key2 = client.default_idempotency_key();

    // Keys should be unique
    assert!(key1.starts_with("key_"));
    assert!(key2.starts_with("key_"));
    assert_ne!(key1, key2);
}

#[tokio::test]
async fn test_request_with_retry() {
    let mock_server = MockServer::start().await;

    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    let request_count = Arc::new(AtomicUsize::new(0));
    let request_count_clone = request_count.clone();

    // Mock that fails first time, succeeds second time
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(move |_req: &wiremock::Request| {
            let count = request_count_clone.fetch_add(1, Ordering::SeqCst);
            if count == 0 {
                // First request fails
                ResponseTemplate::new(500).set_body_json(json!({
                    "message": "Internal server error"
                }))
            } else {
                // Second request succeeds
                ResponseTemplate::new(200).set_body_json(json!({
                    "result": "success"
                }))
            }
        })
        .mount(&mock_server)
        .await;

    let mut config = ClientConfig::default();
    config.base_url = mock_server.uri();
    config.api_key = Some("test-key".to_string());
    config.max_retries = 1;

    let client = AnthropicClient::new(config).unwrap();

    let mut options = RequestOptions::default();
    options.body = Some(json!({"test": "data"}));

    let result = client.post("/v1/messages", Some(options)).await;

    // Should succeed after retry
    if let Err(e) = &result {
        eprintln!("Request failed: {:?}", e);
    }
    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response["result"], "success");
}

#[tokio::test]
async fn test_request_timeout() {
    let mock_server = MockServer::start().await;

    // Server delays response
    Mock::given(method("GET"))
        .and(path("/test"))
        .respond_with(ResponseTemplate::new(200)
            .set_delay(Duration::from_secs(2))
            .set_body_json(json!({"result": "success"})))
        .mount(&mock_server)
        .await;

    let mut config = ClientConfig::default();
    config.base_url = mock_server.uri();
    config.api_key = Some("test-key".to_string());
    config.timeout = Duration::from_millis(100); // Very short timeout

    let client = AnthropicClient::new(config).unwrap();

    let result = client.get("/test", None).await;

    // Should timeout
    assert!(result.is_err());
}

#[tokio::test]
async fn test_anthropic_version_header() {
    let mut config = ClientConfig::default();
    config.api_key = Some("test-key".to_string());

    let client = AnthropicClient::new(config).unwrap();

    let options = RequestOptions::default();
    let result = client.build_headers(
        &reqwest::Method::GET,
        HeaderMap::new(),
        0,
        &options
    );

    assert!(result.is_ok());
    let headers = result.unwrap();
    assert_eq!(
        headers.get("anthropic-version").unwrap(),
        "2023-06-01"
    );
}

#[tokio::test]
async fn test_browser_access_header() {
    let mut config = ClientConfig::default();
    config.api_key = Some("test-key".to_string());
    config.dangerously_allow_browser = true;

    let client = AnthropicClient::new(config).unwrap();

    let options = RequestOptions::default();
    let result = client.build_headers(
        &reqwest::Method::GET,
        HeaderMap::new(),
        0,
        &options
    );

    assert!(result.is_ok());
    let headers = result.unwrap();
    assert_eq!(
        headers.get("anthropic-dangerous-direct-browser-access").unwrap(),
        "true"
    );
}

#[tokio::test]
async fn test_extended_client_services() {
    let mut config = ClientConfig::default();
    config.api_key = Some("test-key".to_string());

    let client = ExtendedAnthropicClient::new(config).unwrap();

    // Verify all services are available
    assert!(Arc::strong_count(&client.completions) > 0);
    assert!(Arc::strong_count(&client.messages) > 0);
    assert!(Arc::strong_count(&client.models) > 0);
    assert!(Arc::strong_count(&client.beta) > 0);
}

#[tokio::test]
async fn test_messages_service_deprecated_model_warning() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "result": "success"
        })))
        .mount(&mock_server)
        .await;

    let mut config = ClientConfig::default();
    config.base_url = mock_server.uri();
    config.api_key = Some("test-key".to_string());

    let client = Arc::new(AnthropicClient::new(config).unwrap());
    let messages = MessagesService::new(client);

    // Test with deprecated model (should log warning but still work)
    let params = json!({
        "model": "claude-instant-1",
        "messages": []
    });

    let result = messages.create(params, None).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_error_types() {
    use AnthropicError::*;

    // Test error creation from status codes
    let headers = HeaderMap::new();

    let err = AnthropicError::from_status(
        reqwest::StatusCode::BAD_REQUEST,
        json!({"message": "Bad request"}),
        &headers
    );
    assert!(matches!(err, BadRequest { .. }));

    let err = AnthropicError::from_status(
        reqwest::StatusCode::UNAUTHORIZED,
        json!({"message": "Unauthorized"}),
        &headers
    );
    assert!(matches!(err, Authentication { .. }));

    let err = AnthropicError::from_status(
        reqwest::StatusCode::TOO_MANY_REQUESTS,
        json!({"message": "Rate limited"}),
        &headers
    );
    assert!(matches!(err, RateLimit { .. }));

    let err = AnthropicError::from_status(
        reqwest::StatusCode::INTERNAL_SERVER_ERROR,
        json!({"message": "Server error"}),
        &headers
    );
    assert!(matches!(err, InternalServer { .. }));
}