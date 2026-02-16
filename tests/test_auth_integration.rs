use anyhow::Result;
use llminate::auth::{AuthManager, ClaudeAiOauth, AuthMethod};
use std::env;
use tempfile::TempDir;
use std::process::Command;

/// Test get_oauth_token with environment variable containing JSON OAuth object
#[tokio::test]
async fn test_get_oauth_token_from_env_json() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let mut auth_manager = AuthManager::new_with_config_dir(temp_dir.path().to_path_buf())?;
    
    // Set environment variable with JSON OAuth object
    let oauth_json = r#"{
        "accessToken": "test-access-token-123",
        "refreshToken": "test-refresh-token-456",
        "expiresAt": 1735689600,
        "scopes": ["user:inference"]
    }"#;
    
    env::set_var("CLAUDE_CODE_OAUTH_TOKEN", oauth_json);
    
    let result = auth_manager.get_oauth_token().await?;
    
    env::remove_var("CLAUDE_CODE_OAUTH_TOKEN");
    
    assert!(result.is_some());
    let oauth = result.unwrap();
    assert_eq!(oauth.access_token, "test-access-token-123");
    assert_eq!(oauth.refresh_token, "test-refresh-token-456");
    assert_eq!(oauth.expires_at, Some(1735689600));
    assert_eq!(oauth.scopes, vec!["user:inference"]);
    
    Ok(())
}

/// Test get_oauth_token with environment variable containing plain token string
#[tokio::test]
async fn test_get_oauth_token_from_env_plain() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let mut auth_manager = AuthManager::new_with_config_dir(temp_dir.path().to_path_buf())?;
    
    // Set environment variable with plain token
    env::set_var("CLAUDE_CODE_OAUTH_TOKEN", "plain-token-789");
    
    let result = auth_manager.get_oauth_token().await?;
    
    env::remove_var("CLAUDE_CODE_OAUTH_TOKEN");
    
    assert!(result.is_some());
    let oauth = result.unwrap();
    assert_eq!(oauth.access_token, "plain-token-789");
    assert!(oauth.refresh_token.is_empty());
    assert!(oauth.expires_at.is_none());
    assert_eq!(oauth.scopes, vec!["user:inference"]);
    
    Ok(())
}

/// Test get_oauth_token with no environment variable (reads from storage)
#[tokio::test]
async fn test_get_oauth_token_from_storage() -> Result<()> {
    let temp_dir = TempDir::new()?;
    
    // Create credentials file with OAuth data
    let credentials_path = temp_dir.path().join(".credentials.json");
    let credentials_json = r#"{
        "claudeAiOauth": {
            "accessToken": "storage-access-token",
            "refreshToken": "storage-refresh-token",
            "expiresAt": 1735689600,
            "scopes": ["user:inference", "user:read"]
        }
    }"#;
    std::fs::write(&credentials_path, credentials_json)?;
    
    let mut auth_manager = AuthManager::new_with_config_dir(temp_dir.path().to_path_buf())?;
    
    // Ensure env var is not set
    env::remove_var("CLAUDE_CODE_OAUTH_TOKEN");
    
    let result = auth_manager.get_oauth_token().await?;
    
    assert!(result.is_some());
    let oauth = result.unwrap();
    assert_eq!(oauth.access_token, "storage-access-token");
    assert_eq!(oauth.refresh_token, "storage-refresh-token");
    assert_eq!(oauth.expires_at, Some(1735689600));
    assert!(oauth.scopes.contains(&"user:inference".to_string()));
    assert!(oauth.scopes.contains(&"user:read".to_string()));
    
    Ok(())
}

/// Test has_valid_scopes with valid token containing required scope
#[test]
fn test_has_valid_scopes_with_required_scope() {
    let oauth = Some(ClaudeAiOauth {
        access_token: "test-token".to_string(),
        refresh_token: String::new(),
        expires_at: None,
        scopes: vec!["user:inference".to_string(), "user:read".to_string()],
        subscription_type: None,
        account_uuid: None,
    });

    assert!(AuthManager::has_valid_scopes(&oauth));
}

/// Test has_valid_scopes with token missing required scope
#[test]
fn test_has_valid_scopes_without_required_scope() {
    let oauth = Some(ClaudeAiOauth {
        access_token: "test-token".to_string(),
        refresh_token: String::new(),
        expires_at: None,
        scopes: vec!["user:read".to_string(), "user:write".to_string()],
        subscription_type: None,
        account_uuid: None,
    });
    
    assert!(!AuthManager::has_valid_scopes(&oauth));
}

/// Test has_valid_scopes with None token
#[test]
fn test_has_valid_scopes_with_none() {
    assert!(!AuthManager::has_valid_scopes(&None));
}

/// Test has_oauth_access with valid OAuth token
#[tokio::test]
async fn test_has_oauth_access_with_valid_token() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let mut auth_manager = AuthManager::new_with_config_dir(temp_dir.path().to_path_buf())?;
    
    // Set environment variable with valid OAuth token
    let oauth_json = r#"{
        "accessToken": "test-access-token",
        "refreshToken": "test-refresh-token",
        "scopes": ["user:inference"]
    }"#;
    
    env::set_var("CLAUDE_CODE_OAUTH_TOKEN", oauth_json);
    
    let has_access = auth_manager.has_oauth_access().await;
    
    env::remove_var("CLAUDE_CODE_OAUTH_TOKEN");
    
    assert!(has_access);
    
    Ok(())
}

/// Test has_oauth_access with invalid scopes
#[tokio::test]
async fn test_has_oauth_access_with_invalid_scopes() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let mut auth_manager = AuthManager::new_with_config_dir(temp_dir.path().to_path_buf())?;
    
    // Set environment variable with OAuth token but wrong scopes
    let oauth_json = r#"{
        "accessToken": "test-access-token",
        "refreshToken": "test-refresh-token",
        "scopes": ["user:read", "user:write"]
    }"#;
    
    env::set_var("CLAUDE_CODE_OAUTH_TOKEN", oauth_json);
    
    let has_access = auth_manager.has_oauth_access().await;
    
    env::remove_var("CLAUDE_CODE_OAUTH_TOKEN");
    
    assert!(!has_access);
    
    Ok(())
}

/// Test has_oauth_access with no token available
#[tokio::test]
async fn test_has_oauth_access_with_no_token() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let mut auth_manager = AuthManager::new_with_config_dir(temp_dir.path().to_path_buf())?;
    
    // Ensure env var is not set
    env::remove_var("CLAUDE_CODE_OAUTH_TOKEN");
    
    let has_access = auth_manager.has_oauth_access().await;
    
    assert!(!has_access);
    
    Ok(())
}

/// Integration test for determine_auth_method with real keychain on macOS
#[tokio::test]
#[cfg(target_os = "macos")]
async fn test_macos_determine_auth_with_existing_keychain() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let mut auth_manager = AuthManager::new_with_config_dir(temp_dir.path().to_path_buf())?;
    
    // Clear all environment variables to force keychain lookup
    env::remove_var("ANTHROPIC_API_KEY");
    env::remove_var("ANTHROPIC_AUTH_TOKEN");
    env::remove_var("CLAUDE_CODE_OAUTH_TOKEN");
    
    // Try to determine auth method - this will check real keychain
    let result = auth_manager.determine_auth_method().await;
    
    // The result depends on what's actually in the user's keychain
    match result {
        Ok(AuthMethod::ApiKey(key)) => {
            println!("Found API key in keychain: {}", &key[..10]);
            assert!(key.starts_with("sk-ant-"));
        },
        Ok(AuthMethod::ClaudeAiOauth(oauth)) => {
            println!("Found OAuth token in keychain");
            assert!(!oauth.access_token.is_empty());
        },
        Err(e) => {
            println!("No auth found in keychain (expected if not configured): {}", e);
        }
    }
    
    Ok(())
}

/// Test OAuth token expiration checking
#[tokio::test]
async fn test_oauth_token_expiration_check() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let mut auth_manager = AuthManager::new_with_config_dir(temp_dir.path().to_path_buf())?;
    
    // Get current time
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64;
    
    // Create credentials with token expiring in 3 minutes (within 5 minute buffer)
    let expires_soon = now + 180; // 3 minutes from now
    
    let credentials_json = format!(r#"{{
        "claudeAiOauth": {{
            "accessToken": "soon-to-expire-token",
            "refreshToken": "valid-refresh-token",
            "expiresAt": {},
            "scopes": ["user:inference"]
        }}
    }}"#, expires_soon);
    
    let credentials_path = temp_dir.path().join(".credentials.json");
    std::fs::write(&credentials_path, credentials_json)?;
    
    // This should trigger refresh attempt (which will fail without mock server)
    let result = auth_manager.get_oauth_token().await;
    
    // Even if refresh fails, function should not panic
    assert!(result.is_ok());
    
    Ok(())
}

/// Test has_oauth_access with real credential storage
#[tokio::test]
async fn test_has_oauth_access_with_storage() -> Result<()> {
    let temp_dir = TempDir::new()?;
    
    // Create credentials with valid OAuth
    let credentials_json = r#"{
        "claudeAiOauth": {
            "accessToken": "valid-token",
            "refreshToken": "refresh-token",
            "expiresAt": 9999999999,
            "scopes": ["user:inference"]
        }
    }"#;
    
    let credentials_path = temp_dir.path().join(".credentials.json");
    std::fs::write(&credentials_path, credentials_json)?;
    
    let mut auth_manager = AuthManager::new_with_config_dir(temp_dir.path().to_path_buf())?;
    
    // Clear any environment variables
    env::remove_var("CLAUDE_CODE_OAUTH_TOKEN");
    env::remove_var("ANTHROPIC_AUTH_TOKEN");
    
    let has_access = auth_manager.has_oauth_access().await;
    assert!(has_access);
    
    // Now test with invalid scopes
    let invalid_credentials_json = r#"{
        "claudeAiOauth": {
            "accessToken": "valid-token",
            "refreshToken": "refresh-token",
            "expiresAt": 9999999999,
            "scopes": ["user:read"]
        }
    }"#;
    
    std::fs::write(&credentials_path, invalid_credentials_json)?;
    
    // Create new manager to clear cache
    let mut auth_manager2 = AuthManager::new_with_config_dir(temp_dir.path().to_path_buf())?;
    let has_access2 = auth_manager2.has_oauth_access().await;
    assert!(!has_access2);
    
    Ok(())
}

/// Test API key helper script execution
#[tokio::test]
async fn test_api_key_helper_execution() -> Result<()> {
    let temp_dir = TempDir::new()?;
    
    // Create a helper script that outputs an API key
    let helper_script = temp_dir.path().join("api_key_helper.sh");
    let test_api_key = format!("sk-ant-helper-{}", uuid::Uuid::new_v4());
    
    #[cfg(unix)]
    {
        std::fs::write(&helper_script, format!("#!/bin/bash\necho '{}'", test_api_key))?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&helper_script, std::fs::Permissions::from_mode(0o755))?;
    }
    
    #[cfg(windows)]
    {
        let helper_script = temp_dir.path().join("api_key_helper.bat");
        std::fs::write(&helper_script, format!("@echo off\necho {}", test_api_key))?;
    }
    
    // Create config with api_key_helper
    let config_json = format!(r#"{{
        "apiKeyHelper": "{}"
    }}"#, helper_script.display());
    
    let config_path = temp_dir.path().join("config.json");
    std::fs::write(&config_path, config_json)?;
    
    let mut auth_manager = AuthManager::new_with_config_dir(temp_dir.path().to_path_buf())?;
    
    // Clear environment variables to force helper usage
    env::remove_var("ANTHROPIC_API_KEY");
    env::remove_var("ANTHROPIC_AUTH_TOKEN");
    env::remove_var("CLAUDE_CODE_OAUTH_TOKEN");
    
    // This should execute the helper script through determine_auth_method
    let result = auth_manager.determine_auth_method().await?;
    
    match result {
        AuthMethod::ApiKey(key) => {
            assert_eq!(key, test_api_key);
        },
        _ => panic!("Expected ApiKey from helper script"),
    }
    
    Ok(())
}

/// Test authentication priority chain with environment variables
#[tokio::test]
async fn test_auth_priority_chain_env_vars() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let mut auth_manager = AuthManager::new_with_config_dir(temp_dir.path().to_path_buf())?;
    
    // Test Priority 3: ANTHROPIC_API_KEY (lower priority)
    env::set_var("ANTHROPIC_API_KEY", "sk-ant-api-key-priority-3");
    let result = auth_manager.determine_auth_method().await?;
    match result {
        AuthMethod::ApiKey(key) => assert_eq!(key, "sk-ant-api-key-priority-3"),
        _ => panic!("Expected ApiKey"),
    }
    
    // Test Priority 2: CLAUDE_CODE_OAUTH_TOKEN (higher priority)
    env::set_var("CLAUDE_CODE_OAUTH_TOKEN", "oauth-token-priority-2");
    let result = auth_manager.determine_auth_method().await?;
    match result {
        AuthMethod::ClaudeAiOauth(oauth) => assert_eq!(oauth.access_token, "oauth-token-priority-2"),
        _ => panic!("Expected ClaudeAiOauth"),
    }
    
    // Test Priority 1: ANTHROPIC_AUTH_TOKEN (highest priority)
    env::set_var("ANTHROPIC_AUTH_TOKEN", "auth-token-priority-1");
    let result = auth_manager.determine_auth_method().await?;
    match result {
        AuthMethod::ClaudeAiOauth(oauth) => assert_eq!(oauth.access_token, "auth-token-priority-1"),
        _ => panic!("Expected ClaudeAiOauth"),
    }
    
    // Clean up
    env::remove_var("ANTHROPIC_API_KEY");
    env::remove_var("CLAUDE_CODE_OAUTH_TOKEN");
    env::remove_var("ANTHROPIC_AUTH_TOKEN");
    
    Ok(())
}