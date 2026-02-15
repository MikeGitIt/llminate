use crate::error::{Error, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;
use tracing::{debug, error};

/// Storage backend type
#[derive(Debug, Clone)]
pub enum StorageBackend {
    Keychain,
    Plaintext,
    Combined,
}

/// Credentials structure matching JavaScript storage format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    #[serde(rename = "claudeAiOauth", skip_serializing_if = "Option::is_none")]
    pub claude_ai_oauth: Option<super::ClaudeAiOauth>,
}

/// Storage backend trait for credentials
#[async_trait]
pub trait CredentialsStorage: Send + Sync {
    /// Read credentials from storage
    async fn read(&self) -> Result<Option<Credentials>>;
    
    /// Update credentials in storage
    async fn update(&self, credentials: Credentials) -> Result<()>;
    
    /// Delete credentials from storage
    async fn delete(&self) -> Result<()>;
}

/// Plaintext file storage (JavaScript func154)
pub struct PlaintextStorage {
    file_path: PathBuf,
}

impl PlaintextStorage {
    pub fn new() -> Result<Self> {
        let config_dir = get_config_directory()?;
        let file_path = config_dir.join(".credentials.json");
        
        Ok(Self { file_path })
    }
    
    pub fn new_with_path(file_path: PathBuf) -> Self {
        Self { file_path }
    }
}

#[async_trait]
impl CredentialsStorage for PlaintextStorage {
    async fn read(&self) -> Result<Option<Credentials>> {
        if !self.file_path.exists() {
            debug!("Plaintext credentials file does not exist: {:?}", self.file_path);
            return Ok(None);
        }

        match fs::read_to_string(&self.file_path).await {
            Ok(content) => {
                match serde_json::from_str::<Credentials>(&content) {
                    Ok(creds) => {
                        debug!("Successfully read plaintext credentials");
                        Ok(Some(creds))
                    }
                    Err(e) => {
                        error!("Failed to parse credentials JSON: {}", e);
                        Ok(None)
                    }
                }
            }
            Err(e) => {
                error!("Failed to read credentials file: {}", e);
                Ok(None)
            }
        }
    }

    async fn update(&self, credentials: Credentials) -> Result<()> {
        // Ensure directory exists
        if let Some(parent) = self.file_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).await
                    .map_err(|e| Error::Config(format!("Failed to create config directory: {}", e)))?;
            }
        }

        let json = serde_json::to_string_pretty(&credentials)
            .map_err(|e| Error::Config(format!("Failed to serialize credentials: {}", e)))?;

        fs::write(&self.file_path, json).await
            .map_err(|e| Error::Config(format!("Failed to write credentials: {}", e)))?;

        // Set file permissions to 0600 (owner read/write only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = fs::metadata(&self.file_path).await
                .map_err(|e| Error::Config(format!("Failed to get file metadata: {}", e)))?;
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o600);
            fs::set_permissions(&self.file_path, permissions).await
                .map_err(|e| Error::Config(format!("Failed to set file permissions: {}", e)))?;
        }

        debug!("Successfully updated plaintext credentials");
        Ok(())
    }

    async fn delete(&self) -> Result<()> {
        if self.file_path.exists() {
            fs::remove_file(&self.file_path).await
                .map_err(|e| Error::Config(format!("Failed to delete credentials: {}", e)))?;
        }
        Ok(())
    }
}

/// macOS Keychain storage (JavaScript NvA function)
pub struct KeychainStorage {
    service_name: String,
}

impl KeychainStorage {
    pub fn new() -> Result<Self> {
        let service_name = get_keychain_service_name()?;
        Ok(Self { service_name })
    }
}

#[async_trait]
impl CredentialsStorage for KeychainStorage {
    async fn read(&self) -> Result<Option<Credentials>> {
        let username = std::env::var("USER").unwrap_or_else(|_| "unknown".to_string());
        
        let output = tokio::process::Command::new("security")
            .args(&[
                "find-generic-password",
                "-a", &username,
                "-w",
                "-s", &self.service_name
            ])
            .output()
            .await
            .map_err(|e| Error::Config(format!("Failed to execute security command: {}", e)))?;

        if !output.status.success() {
            debug!("No keychain entry found for service: {}", self.service_name);
            return Ok(None);
        }

        let keychain_data = String::from_utf8(output.stdout)
            .map_err(|e| Error::Config(format!("Invalid UTF-8 in keychain data: {}", e)))?;
        let keychain_data = keychain_data.trim();

        if keychain_data.is_empty() {
            debug!("Empty keychain data");
            return Ok(None);
        }

        match serde_json::from_str::<Credentials>(keychain_data) {
            Ok(creds) => {
                debug!("Successfully read keychain credentials");
                Ok(Some(creds))
            }
            Err(e) => {
                debug!("Failed to parse keychain JSON: {}", e);
                Ok(None)
            }
        }
    }

    async fn update(&self, credentials: Credentials) -> Result<()> {
        let username = std::env::var("USER").unwrap_or_else(|_| "unknown".to_string());
        let json = serde_json::to_string(&credentials)
            .map_err(|e| Error::Config(format!("Failed to serialize credentials: {}", e)))?;

        // Use -U flag to update existing or create new
        let output = tokio::process::Command::new("security")
            .args(&[
                "add-generic-password",
                "-U",
                "-a", &username,
                "-s", &self.service_name,
                "-w", &json
            ])
            .output()
            .await
            .map_err(|e| Error::Config(format!("Failed to execute security command: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Config(format!("Failed to update keychain: {}", stderr)));
        }

        debug!("Successfully updated keychain credentials");
        Ok(())
    }

    async fn delete(&self) -> Result<()> {
        let username = std::env::var("USER").unwrap_or_else(|_| "unknown".to_string());
        
        let output = tokio::process::Command::new("security")
            .args(&[
                "delete-generic-password",
                "-a", &username,
                "-s", &self.service_name
            ])
            .output()
            .await
            .map_err(|e| Error::Config(format!("Failed to execute security command: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Ignore "item not found" errors
            if !stderr.contains("SecKeychainSearchCopyNext") {
                return Err(Error::Config(format!("Failed to delete from keychain: {}", stderr)));
            }
        }

        Ok(())
    }
}

/// Combined storage that tries keychain first, then plaintext (JavaScript mK9 function)
pub struct CombinedStorage {
    keychain: Option<KeychainStorage>,
    plaintext: PlaintextStorage,
}

impl CombinedStorage {
    pub fn new() -> Result<Self> {
        let keychain = if cfg!(target_os = "macos") {
            Some(KeychainStorage::new()?)
        } else {
            None
        };
        
        let plaintext = PlaintextStorage::new()?;
        
        Ok(Self { keychain, plaintext })
    }
}

#[async_trait]
impl CredentialsStorage for CombinedStorage {
    async fn read(&self) -> Result<Option<Credentials>> {
        // Try keychain first if available
        if let Some(keychain) = &self.keychain {
            if let Some(creds) = keychain.read().await? {
                debug!("Read credentials from keychain");
                return Ok(Some(creds));
            }
        }

        // Fallback to plaintext
        if let Some(creds) = self.plaintext.read().await? {
            debug!("Read credentials from plaintext");
            return Ok(Some(creds));
        }

        debug!("No credentials found in any storage");
        Ok(None)
    }

    async fn update(&self, credentials: Credentials) -> Result<()> {
        // Try to update keychain first if available
        if let Some(keychain) = &self.keychain {
            match keychain.update(credentials.clone()).await {
                Ok(()) => {
                    debug!("Updated credentials in keychain");
                    return Ok(());
                }
                Err(e) => {
                    debug!("Failed to update keychain, falling back to plaintext: {}", e);
                }
            }
        }

        // Fallback to plaintext
        self.plaintext.update(credentials).await?;
        debug!("Updated credentials in plaintext");
        Ok(())
    }

    async fn delete(&self) -> Result<()> {
        let mut any_error = None;

        // Try to delete from keychain
        if let Some(keychain) = &self.keychain {
            if let Err(e) = keychain.delete().await {
                any_error = Some(e);
            }
        }

        // Try to delete from plaintext
        if let Err(e) = self.plaintext.delete().await {
            any_error = Some(e);
        }

        if let Some(e) = any_error {
            return Err(e);
        }

        Ok(())
    }
}

/// Get config directory (JavaScript checker64 function)
fn get_config_directory() -> Result<PathBuf> {
    if let Ok(claude_config_dir) = std::env::var("CLAUDE_CONFIG_DIR") {
        return Ok(PathBuf::from(claude_config_dir));
    }
    
    if let Ok(xdg_config_home) = std::env::var("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(xdg_config_home).join("claude"));
    }
    
    if let Some(home_dir) = dirs::home_dir() {
        return Ok(home_dir.join(".claude"));
    }
    
    Err(Error::Config("Cannot determine config directory".to_string()))
}

/// Generate keychain service name with optional hash (JavaScript ti/z41 functions)
pub fn get_keychain_service_name() -> Result<String> {
    let mut service_name = String::from("Claude Code-credentials");
    
    // If CLAUDE_CONFIG_DIR is set, append hash suffix
    if let Ok(config_dir) = std::env::var("CLAUDE_CONFIG_DIR") {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(config_dir.as_bytes());
        let hash = format!("{:x}", hasher.finalize());
        let suffix = &hash[..8.min(hash.len())];
        service_name.push('-');
        service_name.push_str(suffix);
    }
    
    Ok(service_name)
}

/// Get service name for API key storage (without -credentials suffix)
pub fn get_service_name_for_api_key() -> Result<String> {
    let mut service_name = String::from("Claude Code");
    
    // If CLAUDE_CONFIG_DIR is set, append hash suffix
    if let Ok(config_dir) = std::env::var("CLAUDE_CONFIG_DIR") {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(config_dir.as_bytes());
        let hash = format!("{:x}", hasher.finalize());
        let suffix = &hash[..8.min(hash.len())];
        service_name.push('-');
        service_name.push_str(suffix);
    }
    
    Ok(service_name)
}

/// Get the appropriate storage backend (JavaScript XJ function)
pub fn get_storage_backend() -> Result<Box<dyn CredentialsStorage>> {
    if cfg!(target_os = "macos") {
        // On macOS, use combined storage (keychain with plaintext fallback)
        debug!("Using combined storage (keychain + plaintext fallback)");
        Ok(Box::new(CombinedStorage::new()?))
    } else {
        // On other platforms, use plaintext only
        debug!("Using plaintext storage only");
        Ok(Box::new(PlaintextStorage::new()?))
    }
}