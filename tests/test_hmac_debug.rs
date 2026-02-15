use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

#[test]
fn test_aws_hmac_signing_key() {
    // Test vector from AWS documentation
    // From: https://docs.aws.amazon.com/general/latest/gr/sigv4-calculate-signature.html
    let secret = "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY";
    let date = "20150830";
    let region = "us-east-1";
    let service = "iam";

    // Step 1: Create the initial key
    let k_secret = format!("AWS4{}", secret);
    println!("k_secret: {}", k_secret);

    // Step 2: HMAC with date
    let k_date = hmac_sign(k_secret.as_bytes(), date.as_bytes());
    println!("k_date: {}", hex::encode(&k_date));

    // Step 3: HMAC with region
    let k_region = hmac_sign(&k_date, region.as_bytes());
    println!("k_region: {}", hex::encode(&k_region));

    // Step 4: HMAC with service
    let k_service = hmac_sign(&k_region, service.as_bytes());
    println!("k_service: {}", hex::encode(&k_service));

    // Step 5: HMAC with "aws4_request"
    let k_signing = hmac_sign(&k_service, b"aws4_request");
    println!("k_signing: {}", hex::encode(&k_signing));

    println!("\nExpected: f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41");

    assert_eq!(
        hex::encode(&k_signing),
        "f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41",
        "Signing key should match AWS test vector"
    );
}

fn hmac_sign(key: &[u8], msg: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC can take key of any size");
    mac.update(msg);
    mac.finalize().into_bytes().to_vec()
}