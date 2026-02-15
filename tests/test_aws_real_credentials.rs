use llminate::auth::aws::{DefaultCredentialProvider, CredentialProvider, SignatureV4};
use reqwest::header::HeaderMap;

#[tokio::test]
async fn test_read_aws_credentials_from_cli() {
    // This test will try to read your AWS CLI credentials
    let provider = DefaultCredentialProvider::new();

    match provider.get_credentials().await {
        Ok(creds) => {
            println!("Successfully loaded AWS credentials!");
            println!("Access Key ID: {}...", &creds.access_key_id[..10]);
            // Don't print the full secret key for security
            println!("Has Secret Key: {}", !creds.secret_access_key.is_empty());
            println!("Has Session Token: {}", creds.session_token.is_some());

            // Now test if these credentials actually work with AWS
            test_credentials_with_sts(&creds).await;
        }
        Err(e) => {
            println!("Failed to load AWS credentials: {}", e);
            println!("This might be because our INI file provider isn't implemented yet");
        }
    }
}

async fn test_credentials_with_sts(creds: &llminate::auth::aws::AwsCredentials) {
    println!("\nTesting credentials with AWS STS GetCallerIdentity...");

    let signer = SignatureV4::new("us-east-1".to_string(), "sts".to_string());

    let mut headers = HeaderMap::new();
    headers.insert("Host", "sts.amazonaws.com".parse().unwrap());
    headers.insert("Content-Type", "application/x-www-form-urlencoded".parse().unwrap());

    let body = b"Action=GetCallerIdentity&Version=2011-06-15";

    match signer.sign("POST", "/", &mut headers, body, creds).await {
        Ok(_) => {
            println!("Successfully signed request!");

            // Make the actual request to AWS
            let client = reqwest::Client::new();
            let response = client
                .post("https://sts.amazonaws.com/")
                .headers(headers)
                .body(body.to_vec())
                .send()
                .await;

            match response {
                Ok(resp) => {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();

                    if status == 200 {
                        println!("✅ SUCCESS! AWS accepted our signature!");
                        println!("Response: {}", body);

                        // Parse out the account ID and ARN
                        if let Some(account_start) = body.find("<Account>") {
                            if let Some(account_end) = body.find("</Account>") {
                                let account = &body[account_start + 9..account_end];
                                println!("AWS Account ID: {}", account);
                            }
                        }

                        if let Some(arn_start) = body.find("<Arn>") {
                            if let Some(arn_end) = body.find("</Arn>") {
                                let arn = &body[arn_start + 5..arn_end];
                                println!("AWS ARN: {}", arn);
                            }
                        }
                    } else {
                        println!("❌ AWS rejected our request!");
                        println!("Status: {}", status);
                        println!("Response: {}", body);
                    }
                }
                Err(e) => {
                    println!("Failed to make request to AWS: {}", e);
                }
            }
        }
        Err(e) => {
            println!("Failed to sign request: {}", e);
        }
    }
}