use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

#[test]
fn test_hmac_sha256_basic() {
    // Test with known HMAC-SHA256 test vector
    // Key: "key"
    // Message: "The quick brown fox jumps over the lazy dog"
    // Expected: f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8

    let key = b"key";
    let msg = b"The quick brown fox jumps over the lazy dog";

    let mut mac = HmacSha256::new_from_slice(key).unwrap();
    mac.update(msg);
    let result = mac.finalize().into_bytes();

    let result_hex = hex::encode(&result);
    println!("Result: {}", result_hex);

    assert_eq!(
        result_hex,
        "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8",
        "HMAC-SHA256 should match test vector"
    );
}