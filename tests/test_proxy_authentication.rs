use llminate::auth::proxy::*;
use llminate::auth::client::*;
use reqwest::header::{HeaderMap, HeaderValue, PROXY_AUTHORIZATION};
use std::env;
use wiremock::{Mock, MockServer, ResponseTemplate};
use wiremock::matchers::{method, path, header};

#[test]
fn test_basic_auth_header_generation() {
    let mut headers = HeaderMap::new();
    let config = ProxyConfig {
        username: Some("testuser".to_string()),
        password: Some("testpass".to_string()),
        ..Default::default()
    };

    config.add_proxy_auth(&mut headers).unwrap();

    let auth_header = headers.get(PROXY_AUTHORIZATION).unwrap();
    // "testuser:testpass" in base64 is "dGVzdHVzZXI6dGVzdHBhc3M="
    assert_eq!(auth_header, "Basic dGVzdHVzZXI6dGVzdHBhc3M=");
}

#[test]
fn test_bearer_token_priority() {
    let mut headers = HeaderMap::new();
    let config = ProxyConfig {
        username: Some("user".to_string()),
        password: Some("pass".to_string()),
        auth_token: Some("bearer-token-123".to_string()),
        ..Default::default()
    };

    config.add_proxy_auth(&mut headers).unwrap();

    let auth_header = headers.get(PROXY_AUTHORIZATION).unwrap();
    // Bearer token should take priority over Basic auth
    assert_eq!(auth_header, "Bearer bearer-token-123");
}

#[test]
fn test_proxy_url_parsing_with_credentials() {
    let (clean_url, auth) = parse_proxy_url("http://john:secret@proxy.example.com:8080").unwrap();

    assert_eq!(clean_url, "http://proxy.example.com:8080/");

    if let Some(ProxyAuth::Basic { username, password }) = auth {
        assert_eq!(username, "john");
        assert_eq!(password, "secret");
    } else {
        panic!("Expected Basic auth from proxy URL");
    }
}

#[test]
fn test_url_encoded_credentials_in_proxy_url() {
    // Test special characters in username and password
    let (_, auth) = parse_proxy_url("http://user%40company.com:pass%23word%21@proxy.com:3128").unwrap();

    if let Some(ProxyAuth::Basic { username, password }) = auth {
        assert_eq!(username, "user@company.com");
        assert_eq!(password, "pass#word!");
    } else {
        panic!("Expected decoded credentials");
    }
}

#[test]
fn test_proxy_url_without_credentials() {
    let (clean_url, auth) = parse_proxy_url("http://proxy.example.com:8080").unwrap();

    assert_eq!(clean_url, "http://proxy.example.com:8080/");
    assert!(auth.is_none());
}

#[test]
fn test_no_proxy_bypass_patterns() {
    let config = ProxyConfig {
        no_proxy: vec![
            "localhost".to_string(),
            "127.0.0.1".to_string(),
            ".internal.com".to_string(),
            "192.168.0.0/16".to_string(),
            "*.local".to_string(),
        ],
        ..Default::default()
    };

    // Should bypass
    assert!(config.should_bypass("localhost"));
    assert!(config.should_bypass("127.0.0.1"));
    assert!(config.should_bypass("api.internal.com"));
    assert!(config.should_bypass("internal.com"));
    assert!(config.should_bypass("service.local"));

    // Should NOT bypass
    assert!(!config.should_bypass("example.com"));
    assert!(!config.should_bypass("google.com"));
    assert!(!config.should_bypass("internal-api.com")); // Not a suffix match
}

#[test]
fn test_wildcard_no_proxy() {
    let config = ProxyConfig {
        no_proxy: vec!["*".to_string()],
        ..Default::default()
    };

    // Wildcard should bypass everything
    assert!(config.should_bypass("example.com"));
    assert!(config.should_bypass("localhost"));
    assert!(config.should_bypass("any.domain.whatsoever"));
}

#[test]
fn test_proxy_from_url_extraction() {
    let config = ProxyConfig {
        url: Some("http://alice:wonderland@corporate-proxy.com:8888".to_string()),
        ..Default::default()
    };

    let mut headers = HeaderMap::new();
    config.add_proxy_auth(&mut headers).unwrap();

    // Since username/password aren't set directly, they should be extracted from URL
    assert!(headers.contains_key(PROXY_AUTHORIZATION));
}

#[test]
fn test_grpc_proxy_auth() {
    let mut headers = HeaderMap::new();
    add_grpc_proxy_auth(&mut headers, "grpc:credentials").unwrap();

    let auth_header = headers.get(PROXY_AUTHORIZATION).unwrap();
    // "grpc:credentials" in base64 is "Z3JwYzpjcmVkZW50aWFscw=="
    assert_eq!(auth_header, "Basic Z3JwYzpjcmVkZW50aWFscw==");
}

#[test]
fn test_proxy_auth_enum_variants() {
    let mut headers = HeaderMap::new();

    // Test Basic variant
    add_proxy_authentication(
        &mut headers,
        Some(ProxyAuth::Basic {
            username: "admin".to_string(),
            password: "admin123".to_string(),
        }),
    ).unwrap();
    assert_eq!(headers.get(PROXY_AUTHORIZATION).unwrap(), "Basic YWRtaW46YWRtaW4xMjM=");

    // Test Bearer variant
    headers.clear();
    add_proxy_authentication(
        &mut headers,
        Some(ProxyAuth::Bearer {
            token: "jwt-token-here".to_string(),
        }),
    ).unwrap();
    assert_eq!(headers.get(PROXY_AUTHORIZATION).unwrap(), "Bearer jwt-token-here");

    // Test Raw variant
    headers.clear();
    add_proxy_authentication(
        &mut headers,
        Some(ProxyAuth::Raw {
            value: "Custom auth-scheme-value".to_string(),
        }),
    ).unwrap();
    assert_eq!(headers.get(PROXY_AUTHORIZATION).unwrap(), "Custom auth-scheme-value");
}

#[test]
fn test_empty_credentials() {
    let config = ProxyConfig {
        username: Some("".to_string()),
        password: Some("".to_string()),
        ..Default::default()
    };

    let mut headers = HeaderMap::new();
    config.add_proxy_auth(&mut headers).unwrap();

    // Empty credentials should still create header
    let auth_header = headers.get(PROXY_AUTHORIZATION).unwrap();
    // ":" in base64 is "Og=="
    assert_eq!(auth_header, "Basic Og==");
}

#[tokio::test]
async fn test_reqwest_proxy_conversion() {
    let config = ProxyConfig {
        url: Some("http://proxy.example.com:3128".to_string()),
        username: Some("user".to_string()),
        password: Some("pass".to_string()),
        no_proxy: vec!["localhost".to_string()],
        ..Default::default()
    };

    let reqwest_proxy = config.to_reqwest_proxy().unwrap();
    assert!(reqwest_proxy.is_some());
}

#[tokio::test]
async fn test_client_with_proxy_config() {
    let mock_server = MockServer::start().await;
    let proxy_mock = MockServer::start().await;

    // Create client config with proxy
    let mut config = ClientConfig::default();
    config.base_url = mock_server.uri();
    config.api_key = Some("test-key".to_string());
    config.proxy = Some(ProxyConfig {
        url: Some(proxy_mock.uri()),
        username: Some("proxy_user".to_string()),
        password: Some("proxy_pass".to_string()),
        ..Default::default()
    });

    // Client should build successfully with proxy
    let client = AnthropicClient::new(config);
    assert!(client.is_ok());
}

#[tokio::test]
async fn test_proxy_headers_in_request() {
    let mut config = ClientConfig::default();
    config.api_key = Some("test-key".to_string());
    config.proxy = Some(ProxyConfig {
        username: Some("proxyuser".to_string()),
        password: Some("proxypass".to_string()),
        ..Default::default()
    });

    let client = AnthropicClient::new(config).unwrap();

    let options = RequestOptions::default();
    let headers = client.build_headers(
        &reqwest::Method::POST,
        HeaderMap::new(),
        0,
        &options,
    ).unwrap();

    // Check that proxy auth header is included
    assert!(headers.contains_key(PROXY_AUTHORIZATION));
    assert_eq!(
        headers.get(PROXY_AUTHORIZATION).unwrap(),
        "Basic cHJveHl1c2VyOnByb3h5cGFzcw=="  // base64("proxyuser:proxypass")
    );
}

#[tokio::test]
async fn test_no_proxy_bypass_in_client() {
    let mut config = ClientConfig::default();
    config.base_url = "http://localhost:8080".to_string();
    config.api_key = Some("test-key".to_string());
    config.proxy = Some(ProxyConfig {
        username: Some("user".to_string()),
        password: Some("pass".to_string()),
        no_proxy: vec!["localhost".to_string()],
        ..Default::default()
    });

    let client = AnthropicClient::new(config).unwrap();

    let options = RequestOptions {
        path: Some("/test".to_string()),
        ..Default::default()
    };

    let headers = client.build_headers(
        &reqwest::Method::GET,
        HeaderMap::new(),
        0,
        &options,
    ).unwrap();

    // Should NOT have proxy auth header for localhost
    assert!(!headers.contains_key(PROXY_AUTHORIZATION));
}

#[test]
fn test_environment_variable_priority() {
    // Test the priority order matching JavaScript:
    // HTTPS_PROXY > https_proxy > HTTP_PROXY > http_proxy

    // This test would need to set environment variables
    // which could affect other tests, so it's commented out
    // but shows the expected behavior

    /*
    env::set_var("HTTP_PROXY", "http://low-priority:8080");
    env::set_var("HTTPS_PROXY", "https://high-priority:8443");

    let config = ProxyConfig::from_env().unwrap();
    assert_eq!(config.url, Some("https://high-priority:8443".to_string()));

    env::remove_var("HTTPS_PROXY");
    env::remove_var("HTTP_PROXY");
    */
}

#[test]
fn test_anthropic_bearer_token_from_env() {
    // Test would set ANTHROPIC_AUTH_TOKEN env var
    // Commented out to avoid affecting other tests

    /*
    env::set_var("ANTHROPIC_AUTH_TOKEN", "test-bearer-token");

    let config = ProxyConfig::from_env().unwrap();
    assert_eq!(config.auth_token, Some("test-bearer-token".to_string()));

    let mut headers = HeaderMap::new();
    config.add_proxy_auth(&mut headers).unwrap();
    assert_eq!(headers.get(PROXY_AUTHORIZATION).unwrap(), "Bearer test-bearer-token");

    env::remove_var("ANTHROPIC_AUTH_TOKEN");
    */
}