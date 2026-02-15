use llminate::auth::aws::*;
use std::env;

#[tokio::test]
async fn test_env_credential_provider_success() {
    // Set test environment variables
    env::set_var("AWS_ACCESS_KEY_ID", "AKIAIOSFODNN7EXAMPLE");
    env::set_var("AWS_SECRET_ACCESS_KEY", "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY");
    env::set_var("AWS_SESSION_TOKEN", "test-session-token");
    env::set_var("AWS_CREDENTIAL_SCOPE", "20230101/us-east-1/s3/aws4_request");
    env::set_var("AWS_ACCOUNT_ID", "123456789012");

    let provider = EnvCredentialProvider;
    let result = provider.get_credentials().await;

    assert!(result.is_ok());
    let creds = result.unwrap();
    assert_eq!(creds.access_key_id, "AKIAIOSFODNN7EXAMPLE");
    assert_eq!(creds.secret_access_key, "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY");
    assert_eq!(creds.session_token, Some("test-session-token".to_string()));
    assert_eq!(creds.credential_scope, Some("20230101/us-east-1/s3/aws4_request".to_string()));
    assert_eq!(creds.account_id, Some("123456789012".to_string()));
    assert!(creds.source.is_some());
    assert_eq!(creds.source.as_ref().unwrap().get("CREDENTIALS_ENV_VARS"), Some(&"g".to_string()));

    // Clean up
    env::remove_var("AWS_ACCESS_KEY_ID");
    env::remove_var("AWS_SECRET_ACCESS_KEY");
    env::remove_var("AWS_SESSION_TOKEN");
    env::remove_var("AWS_CREDENTIAL_SCOPE");
    env::remove_var("AWS_ACCOUNT_ID");
}

#[tokio::test]
async fn test_env_credential_provider_missing_credentials() {
    // Ensure no AWS credentials are set
    env::remove_var("AWS_ACCESS_KEY_ID");
    env::remove_var("AWS_SECRET_ACCESS_KEY");

    let provider = EnvCredentialProvider;
    let result = provider.get_credentials().await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("AWS_ACCESS_KEY_ID not found"));
}

#[tokio::test]
async fn test_sigv4_canonical_path() {
    let signer = SignatureV4::new("us-east-1".to_string(), "s3".to_string());

    // Test normal path
    assert_eq!(signer.get_canonical_path("/test/path"), "%2Ftest%2Fpath");

    // Test path with parent directory reference
    assert_eq!(signer.get_canonical_path("/test/../path"), "%2Fpath");

    // Test path with current directory reference
    assert_eq!(signer.get_canonical_path("/test/./path"), "%2Ftest%2Fpath");

    // Test empty path
    assert_eq!(signer.get_canonical_path(""), "");

    // Test root path
    assert_eq!(signer.get_canonical_path("/"), "%2F");
}

#[tokio::test]
async fn test_sigv4_canonical_query_string() {
    let signer = SignatureV4::new("us-east-1".to_string(), "s3".to_string());

    // Test query parameters are sorted
    assert_eq!(signer.get_canonical_query_string("/path?b=2&a=1"), "a=1&b=2");

    // Test single parameter
    assert_eq!(signer.get_canonical_query_string("/path?foo=bar"), "foo=bar");

    // Test no query parameters
    assert_eq!(signer.get_canonical_query_string("/path"), "");

    // Test URL encoding
    assert_eq!(signer.get_canonical_query_string("/path?key=value with spaces"), "key=value%20with%20spaces");
}

#[tokio::test]
async fn test_sigv4_hash_payload() {
    let signer = SignatureV4::new("us-east-1".to_string(), "s3".to_string());

    // Test empty payload (SHA256 of empty string)
    let empty_hash = signer.hash_payload(b"");
    assert_eq!(empty_hash, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");

    // Test known payload
    let test_hash = signer.hash_payload(b"test");
    assert_eq!(test_hash, "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08");
}

#[tokio::test]
async fn test_sigv4_signing_key() {
    let signer = SignatureV4::new("us-east-1".to_string(), "service".to_string());

    // Test with known values (from AWS SigV4 test suite)
    let key = signer.get_signing_key(
        "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
        "20150830",
        "us-east-1",
        "service"
    );

    // The signing key should be deterministic for the same inputs
    assert_eq!(key.len(), 32); // HMAC-SHA256 produces 32 bytes
}

#[tokio::test]
async fn test_sigv4_canonical_headers() {
    use reqwest::header::{HeaderMap, HeaderValue};

    let signer = SignatureV4::new("us-east-1".to_string(), "s3".to_string());

    let mut headers = HeaderMap::new();
    headers.insert("Host", HeaderValue::from_static("example.amazonaws.com"));
    headers.insert("Content-Type", HeaderValue::from_static("text/plain"));
    headers.insert("X-Amz-Date", HeaderValue::from_static("20150830T123600Z"));

    let canonical = signer.get_canonical_headers(&headers);

    // Headers should be lowercase and sorted
    assert!(canonical.contains("content-type:text/plain"));
    assert!(canonical.contains("host:example.amazonaws.com"));
    assert!(canonical.contains("x-amz-date:20150830T123600Z"));

    // Check ordering
    let lines: Vec<&str> = canonical.split('\n').collect();
    assert_eq!(lines[0], "content-type:text/plain");
    assert_eq!(lines[1], "host:example.amazonaws.com");
    assert_eq!(lines[2], "x-amz-date:20150830T123600Z");
}

#[tokio::test]
async fn test_sigv4_signed_headers() {
    use reqwest::header::{HeaderMap, HeaderValue};

    let signer = SignatureV4::new("us-east-1".to_string(), "s3".to_string());

    let mut headers = HeaderMap::new();
    headers.insert("Host", HeaderValue::from_static("example.amazonaws.com"));
    headers.insert("Content-Type", HeaderValue::from_static("text/plain"));
    headers.insert("X-Amz-Date", HeaderValue::from_static("20150830T123600Z"));

    let signed = signer.get_signed_headers(&headers);

    // Should be semicolon-separated, lowercase, and sorted
    assert_eq!(signed, "content-type;host;x-amz-date");
}

#[tokio::test]
async fn test_container_metadata_provider_no_env() {
    // Ensure no container env vars are set
    env::remove_var("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI");
    env::remove_var("AWS_CONTAINER_CREDENTIALS_FULL_URI");

    let provider = ContainerMetadataProvider::new();
    let result = provider.get_credentials().await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("cannot be used unless"));
}

#[tokio::test]
async fn test_instance_metadata_provider_disabled() {
    // Set environment to disable IMDS
    env::set_var("AWS_EC2_METADATA_DISABLED", "true");

    let provider = InstanceMetadataProvider::new();
    let result = provider.get_credentials().await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("access disabled"));

    env::remove_var("AWS_EC2_METADATA_DISABLED");
}

#[tokio::test]
async fn test_default_credential_chain() {
    // Test that the chain tries providers in order
    // First remove all env vars to ensure clean state
    env::remove_var("AWS_ACCESS_KEY_ID");
    env::remove_var("AWS_SECRET_ACCESS_KEY");
    env::remove_var("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI");
    env::remove_var("AWS_CONTAINER_CREDENTIALS_FULL_URI");
    env::set_var("AWS_EC2_METADATA_DISABLED", "true");

    let provider = DefaultCredentialProvider::new();
    let result = provider.get_credentials().await;

    // Should fail since no credentials are available
    assert!(result.is_err());

    // Now set env credentials
    env::set_var("AWS_ACCESS_KEY_ID", "test-key");
    env::set_var("AWS_SECRET_ACCESS_KEY", "test-secret");

    let provider = DefaultCredentialProvider::new();
    let result = provider.get_credentials().await;

    // Should succeed with env credentials
    assert!(result.is_ok());
    let creds = result.unwrap();
    assert_eq!(creds.access_key_id, "test-key");
    assert_eq!(creds.secret_access_key, "test-secret");

    // Clean up
    env::remove_var("AWS_ACCESS_KEY_ID");
    env::remove_var("AWS_SECRET_ACCESS_KEY");
    env::remove_var("AWS_EC2_METADATA_DISABLED");
}

#[tokio::test]
async fn test_memoized_provider() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // Create a mock provider that counts how many times it's called
    struct CountingProvider {
        call_count: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl CredentialProvider for CountingProvider {
        async fn get_credentials(&self) -> anyhow::Result<AwsCredentials> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(AwsCredentials {
                access_key_id: "test-key".to_string(),
                secret_access_key: "test-secret".to_string(),
                session_token: None,
                expiration: None,
                credential_scope: None,
                account_id: None,
                source: None,
            })
        }
    }

    let call_count = Arc::new(AtomicUsize::new(0));
    let counting_provider = CountingProvider {
        call_count: call_count.clone(),
    };

    let memoized = MemoizedProvider::new(Box::new(counting_provider));

    // First call should fetch credentials
    let creds1 = memoized.get_credentials().await.unwrap();
    assert_eq!(creds1.access_key_id, "test-key");
    assert_eq!(call_count.load(Ordering::SeqCst), 1);

    // Second call should use cached credentials
    let creds2 = memoized.get_credentials().await.unwrap();
    assert_eq!(creds2.access_key_id, "test-key");
    assert_eq!(call_count.load(Ordering::SeqCst), 1); // Still 1, not fetched again

    // Credentials should be the same
    assert_eq!(creds1.access_key_id, creds2.access_key_id);
    assert_eq!(creds1.secret_access_key, creds2.secret_access_key);
}

#[tokio::test]
async fn test_sigv4_full_signing_flow() {
    use reqwest::header::{HeaderMap, HeaderValue};

    let signer = SignatureV4::new("us-east-1".to_string(), "service".to_string());

    let credentials = AwsCredentials {
        access_key_id: "AKIAIOSFODNN7EXAMPLE".to_string(),
        secret_access_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
        session_token: Some("test-token".to_string()),
        expiration: None,
        credential_scope: None,
        account_id: None,
        source: None,
    };

    let mut headers = HeaderMap::new();
    headers.insert("Host", HeaderValue::from_static("example.amazonaws.com"));
    headers.insert("Content-Type", HeaderValue::from_static("application/x-amz-json-1.0"));

    let result = signer.sign(
        "GET",
        "/",
        &mut headers,
        b"",
        &credentials
    ).await;

    assert!(result.is_ok());

    // Check that required headers were added
    assert!(headers.contains_key("x-amz-date"));
    assert!(headers.contains_key("x-amz-security-token"));
    assert!(headers.contains_key("Authorization"));

    // Check authorization header format
    let auth_header = headers.get("Authorization").unwrap().to_str().unwrap();
    assert!(auth_header.starts_with("AWS4-HMAC-SHA256"));
    assert!(auth_header.contains("Credential=AKIAIOSFODNN7EXAMPLE"));
    assert!(auth_header.contains("SignedHeaders="));
    assert!(auth_header.contains("Signature="));
}