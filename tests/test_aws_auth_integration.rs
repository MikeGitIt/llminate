// Integration tests for AWS authentication
// These tests require actual AWS credentials to run
// Run with: AWS_ACCESS_KEY_ID=xxx AWS_SECRET_ACCESS_KEY=yyy cargo test test_aws_auth_integration -- --ignored

use llminate::auth::aws::{
    AwsCredentials, SignatureV4, CredentialProvider,
    EnvCredentialProvider, DefaultCredentialProvider,
    InstanceMetadataProvider, ContainerMetadataProvider
};
use reqwest::header::HeaderMap;
use std::env;

#[tokio::test]
#[ignore] // Ignored by default since it requires real AWS credentials
async fn test_sigv4_with_real_aws_sts() {
    // This test will make a real request to AWS STS GetCallerIdentity
    // which is the standard way to verify AWS credentials are working

    let access_key = env::var("AWS_ACCESS_KEY_ID")
        .expect("AWS_ACCESS_KEY_ID must be set for integration tests");
    let secret_key = env::var("AWS_SECRET_ACCESS_KEY")
        .expect("AWS_SECRET_ACCESS_KEY must be set for integration tests");
    let session_token = env::var("AWS_SESSION_TOKEN").ok();

    let credentials = AwsCredentials {
        access_key_id: access_key,
        secret_access_key: secret_key,
        session_token,
        expiration: None,
        credential_scope: None,
        account_id: None,
        source: None,
    };

    // Create SigV4 signer for STS
    let signer = SignatureV4::new("us-east-1".to_string(), "sts".to_string());

    // Prepare request to GetCallerIdentity
    let mut headers = HeaderMap::new();
    headers.insert("Host", "sts.amazonaws.com".parse().unwrap());
    headers.insert("Content-Type", "application/x-www-form-urlencoded".parse().unwrap());

    let body = b"Action=GetCallerIdentity&Version=2011-06-15";

    // Sign the request
    signer.sign(
        "POST",
        "/",
        &mut headers,
        body,
        &credentials
    ).await.expect("Signing should succeed");

    // Make the actual request to AWS
    let client = reqwest::Client::new();
    let response = client
        .post("https://sts.amazonaws.com/")
        .headers(headers)
        .body(body.to_vec())
        .send()
        .await
        .expect("Request should succeed");

    // Check response
    assert_eq!(response.status(), 200, "AWS should accept our signature");

    let response_text = response.text().await.unwrap();

    // Response should contain our account ID and ARN
    assert!(response_text.contains("<GetCallerIdentityResponse"));
    assert!(response_text.contains("<Account>"));
    assert!(response_text.contains("<Arn>"));

    println!("Successfully authenticated with AWS STS!");
    println!("Response: {}", response_text);
}

#[tokio::test]
#[ignore] // Requires real AWS environment
async fn test_default_credential_chain_with_real_aws() {
    // This test verifies the credential chain works with real AWS credentials
    // It will try each provider in order until it finds valid credentials

    let provider = DefaultCredentialProvider::new();

    let result = provider.get_credentials().await;

    if result.is_err() {
        println!("No AWS credentials found in environment. This is expected in CI.");
        println!("Error: {}", result.unwrap_err());
        return;
    }

    let credentials = result.unwrap();

    // Verify we got credentials
    assert!(!credentials.access_key_id.is_empty());
    assert!(!credentials.secret_access_key.is_empty());

    // Now verify these credentials actually work with AWS
    let signer = SignatureV4::new("us-east-1".to_string(), "sts".to_string());

    let mut headers = HeaderMap::new();
    headers.insert("Host", "sts.amazonaws.com".parse().unwrap());
    headers.insert("Content-Type", "application/x-www-form-urlencoded".parse().unwrap());

    let body = b"Action=GetCallerIdentity&Version=2011-06-15";

    signer.sign("POST", "/", &mut headers, body, &credentials)
        .await
        .expect("Signing should succeed");

    let client = reqwest::Client::new();
    let response = client
        .post("https://sts.amazonaws.com/")
        .headers(headers)
        .body(body.to_vec())
        .send()
        .await
        .expect("Request should succeed");

    assert_eq!(response.status(), 200, "Credentials from chain should work with AWS");
}

#[tokio::test]
#[ignore] // Requires EC2 instance
async fn test_instance_metadata_provider_on_ec2() {
    // This test only works when running on an actual EC2 instance

    // First check if we're on EC2
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .unwrap();

    let token_result = client
        .put("http://169.254.169.254/latest/api/token")
        .header("X-aws-ec2-metadata-token-ttl-seconds", "21600")
        .send()
        .await;

    if token_result.is_err() {
        println!("Not running on EC2, skipping instance metadata test");
        return;
    }

    // We're on EC2, test the provider
    let provider = InstanceMetadataProvider::new();
    let credentials = provider.get_credentials().await
        .expect("Should get credentials from instance metadata");

    // Verify credentials
    assert!(!credentials.access_key_id.is_empty());
    assert!(!credentials.secret_access_key.is_empty());
    assert!(credentials.session_token.is_some());

    // Test that these credentials work
    verify_credentials_with_sts(&credentials).await;
}

#[tokio::test]
#[ignore] // Requires ECS container
async fn test_container_metadata_provider_on_ecs() {
    // This test only works when running in an ECS container

    if env::var("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI").is_err()
        && env::var("AWS_CONTAINER_CREDENTIALS_FULL_URI").is_err() {
        println!("Not running in ECS, skipping container metadata test");
        return;
    }

    let provider = ContainerMetadataProvider::new();
    let credentials = provider.get_credentials().await
        .expect("Should get credentials from container metadata");

    // Verify credentials
    assert!(!credentials.access_key_id.is_empty());
    assert!(!credentials.secret_access_key.is_empty());

    // Test that these credentials work
    verify_credentials_with_sts(&credentials).await;
}

// Helper function to verify credentials work with AWS STS
async fn verify_credentials_with_sts(credentials: &AwsCredentials) {
    let signer = SignatureV4::new("us-east-1".to_string(), "sts".to_string());

    let mut headers = HeaderMap::new();
    headers.insert("Host", "sts.amazonaws.com".parse().unwrap());
    headers.insert("Content-Type", "application/x-www-form-urlencoded".parse().unwrap());

    let body = b"Action=GetCallerIdentity&Version=2011-06-15";

    signer.sign("POST", "/", &mut headers, body, credentials)
        .await
        .expect("Signing should succeed");

    let client = reqwest::Client::new();
    let response = client
        .post("https://sts.amazonaws.com/")
        .headers(headers)
        .body(body.to_vec())
        .send()
        .await
        .expect("Request to AWS should succeed");

    assert_eq!(
        response.status(),
        200,
        "AWS should accept our signature. Response: {}",
        response.text().await.unwrap_or_default()
    );
}

#[tokio::test]
async fn test_sigv4_signature_against_aws_test_suite() {
    // AWS provides a test suite with known good signatures
    // This test verifies our implementation produces the same signatures
    // Test vector from: https://docs.aws.amazon.com/AmazonS3/latest/API/sig-v4-header-based-auth.html


    let signer = SignatureV4::new("us-east-1".to_string(), "s3".to_string());

    // Known test case from AWS documentation
    // From: https://docs.aws.amazon.com/general/latest/gr/sigv4-calculate-signature.html
    let test_secret = "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY";
    let test_date = "20150830";  // The AWS test uses this date
    let test_region = "us-east-1";
    let test_service = "iam";  // The AWS test uses IAM service

    let signing_key = signer.get_signing_key(test_secret, test_date, test_region, test_service);

    // The expected signing key (from AWS test suite)
    let expected_key_hex = "f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41";
    let actual_key_hex = hex::encode(&signing_key);

    assert_eq!(
        actual_key_hex,
        expected_key_hex,
        "Signing key should match AWS test suite"
    );

    // Test canonical request generation
    let mut headers = HeaderMap::new();
    headers.insert("Host", "examplebucket.s3.amazonaws.com".parse().unwrap());
    headers.insert("Range", "bytes=0-9".parse().unwrap());
    headers.insert("x-amz-content-sha256", "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".parse().unwrap());
    headers.insert("x-amz-date", "20130524T000000Z".parse().unwrap());

    let canonical_headers = signer.get_canonical_headers(&headers);
    let expected_canonical_headers = "host:examplebucket.s3.amazonaws.com\nrange:bytes=0-9\nx-amz-content-sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\nx-amz-date:20130524T000000Z";

    assert_eq!(
        canonical_headers,
        expected_canonical_headers,
        "Canonical headers should match AWS test suite"
    );
}

#[tokio::test]
async fn test_aws_signature_error_handling() {
    // Test that we get meaningful errors when signatures are wrong

    let bad_credentials = AwsCredentials {
        access_key_id: "INVALID_KEY".to_string(),
        secret_access_key: "INVALID_SECRET".to_string(),
        session_token: None,
        expiration: None,
        credential_scope: None,
        account_id: None,
        source: None,
    };

    let signer = SignatureV4::new("us-east-1".to_string(), "sts".to_string());

    let mut headers = HeaderMap::new();
    headers.insert("Host", "sts.amazonaws.com".parse().unwrap());
    headers.insert("Content-Type", "application/x-www-form-urlencoded".parse().unwrap());

    let body = b"Action=GetCallerIdentity&Version=2011-06-15";

    // This should succeed (signing always works with any credentials)
    signer.sign("POST", "/", &mut headers, body, &bad_credentials)
        .await
        .expect("Signing should succeed even with bad credentials");

    // But the request to AWS should fail
    let client = reqwest::Client::new();
    let response = client
        .post("https://sts.amazonaws.com/")
        .headers(headers)
        .body(body.to_vec())
        .send()
        .await
        .expect("Request should be sent");

    // AWS should reject our bad credentials
    assert_eq!(response.status(), 403, "AWS should reject invalid credentials");

    let error_response = response.text().await.unwrap();
    assert!(
        error_response.contains("InvalidClientTokenId") ||
        error_response.contains("SignatureDoesNotMatch"),
        "AWS should return a signature error"
    );
}