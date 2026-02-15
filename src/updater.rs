use crate::error::{Error, Result};
use anyhow::Context;
use colored::Colorize;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

static UPDATE_LOCK: Lazy<Arc<Mutex<()>>> = Lazy::new(|| Arc::new(Mutex::new(())));

#[derive(Debug, Clone, PartialEq)]
pub enum UpdateResult {
    Updated(String),
    AlreadyLatest,
    UpdateAvailable(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateStatus {
    pub latest_version: String,
    pub current_version: String,
    pub update_available: bool,
    pub update_channel: String,
}

/// Check if running from local installation
pub fn is_running_from_local() -> bool {
    // Check if executable is in ~/.claude/local
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(path_str) = exe_path.to_str() {
            return path_str.contains(".claude/local");
        }
    }
    false
}

/// Check if local installation exists
pub fn has_local_installation() -> bool {
    let local_path = dirs::home_dir()
        .map(|h| h.join(".claude").join("local"))
        .unwrap_or_default();
    
    local_path.exists()
}

/// Get local installation path
pub fn get_local_installation_path() -> PathBuf {
    dirs::home_dir()
        .expect("Could not determine home directory")
        .join(".claude")
        .join("local")
}

/// Check for updates
pub async fn check_for_updates() -> Result<UpdateStatus> {
    // In a real implementation, this would check a remote endpoint
    // For now, we'll simulate the check
    
    let current_version = crate::VERSION.to_string();
    
    // Check npm registry or GitHub releases
    let latest_version = fetch_latest_version().await?;
    
    Ok(UpdateStatus {
        latest_version: latest_version.clone(),
        current_version: current_version.clone(),
        update_available: latest_version != current_version,
        update_channel: if is_running_from_local() { "local".to_string() } else { "global".to_string() },
    })
}

/// Check and perform update if available
pub async fn check_and_update() -> Result<UpdateResult> {
    let _lock = UPDATE_LOCK.lock();
    
    let status = check_for_updates().await?;
    
    if !status.update_available {
        return Ok(UpdateResult::AlreadyLatest);
    }
    
    // Check if we have permissions to update
    if !can_update()? {
        return Ok(UpdateResult::UpdateAvailable(status.latest_version));
    }
    
    // Perform the update
    install_update(&status.latest_version).await?;
    
    Ok(UpdateResult::Updated(status.latest_version))
}

/// Install a specific version
pub async fn install_update(version: &str) -> Result<()> {
    let _lock = UPDATE_LOCK.lock();
    
    if is_running_from_local() {
        install_local_update(version).await
    } else {
        install_global_update(version).await
    }
}

/// Install update to local installation
async fn install_local_update(version: &str) -> Result<()> {
    let local_path = get_local_installation_path();
    
    println!("Updating local installation to version {}...", version);
    
    // Run npm update in the local directory
    let output = Command::new("npm")
        .args(&["update", &format!("{}@{}", crate::PACKAGE_NAME, version)])
        .current_dir(&local_path)
        .output()
        .context("Failed to run npm update")?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::Update(format!("npm update failed: {}", stderr)));
    }
    
    Ok(())
}

/// Install update globally
async fn install_global_update(version: &str) -> Result<()> {
    println!("Updating global installation to version {}...", version);
    
    // Run npm update globally
    let output = Command::new("npm")
        .args(&[
            "install",
            "-g",
            &format!("{}@{}", crate::PACKAGE_NAME, version),
        ])
        .output()
        .context("Failed to run npm install")?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::Update(format!("npm install failed: {}", stderr)));
    }
    
    Ok(())
}

/// Check if we have permissions to update
pub fn can_update() -> Result<bool> {
    if is_running_from_local() {
        // Check write permissions to local installation
        let local_path = get_local_installation_path();
        check_write_permissions(&local_path)
    } else {
        // Check global npm permissions
        check_npm_permissions()
    }
}

/// Check write permissions to a directory
fn check_write_permissions(path: &Path) -> Result<bool> {
    let test_file = path.join(".update-test");
    
    match fs::write(&test_file, "test") {
        Ok(_) => {
            let _ = fs::remove_file(test_file);
            Ok(true)
        }
        Err(_) => Ok(false),
    }
}

/// Check npm global permissions
fn check_npm_permissions() -> Result<bool> {
    // Get npm prefix
    let output = Command::new("npm")
        .args(&["config", "get", "prefix"])
        .output()
        .context("Failed to get npm prefix")?;
    
    if !output.status.success() {
        return Ok(false);
    }
    
    let prefix = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let npm_path = PathBuf::from(prefix);
    
    check_write_permissions(&npm_path)
}

/// Fetch the latest version from registry
async fn fetch_latest_version() -> Result<String> {
    // In a real implementation, this would query npm registry or GitHub API
    // For now, return the current version
    Ok(crate::VERSION.to_string())
}

/// Migrate from global to local installation
pub async fn migrate_to_local() -> Result<()> {
    if is_running_from_local() {
        return Err(Error::Update(
            "Already running from local installation".to_string(),
        ));
    }
    
    let local_path = get_local_installation_path();
    
    // Create local directory
    fs::create_dir_all(&local_path)
        .context("Failed to create local installation directory")?;
    
    // Initialize package.json
    let package_json = serde_json::json!({
        "name": "llminate-local",
        "version": "1.0.0",
        "private": true,
        "dependencies": {
            crate::PACKAGE_NAME: crate::VERSION
        }
    });
    
    fs::write(
        local_path.join("package.json"),
        serde_json::to_string_pretty(&package_json)?,
    )
    .context("Failed to write package.json")?;
    
    // Install the package
    println!("Installing {} locally...", crate::PACKAGE_NAME);
    
    let output = Command::new("npm")
        .arg("install")
        .current_dir(&local_path)
        .output()
        .context("Failed to run npm install")?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::Update(format!("npm install failed: {}", stderr)));
    }
    
    // Create wrapper script
    create_wrapper_script(&local_path)?;
    
    // Set up shell alias
    setup_shell_alias()?;
    
    // Mark migration as complete
    let mut config = crate::config::load_config(crate::config::ConfigScope::User)?;
    config.auto_updater_status = Some("migrated".to_string());
    crate::config::save_config(crate::config::ConfigScope::User, &config)?;
    
    Ok(())
}

/// Create wrapper script for local installation
fn create_wrapper_script(local_path: &Path) -> Result<()> {
    let wrapper_content = format!(
        r#"#!/usr/bin/env node
require('{}/node_modules/{}/bin/llminate');
"#,
        local_path.display(),
        crate::PACKAGE_NAME
    );
    
    let wrapper_path = local_path.join("llminate");
    fs::write(&wrapper_path, wrapper_content)?;
    
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&wrapper_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&wrapper_path, perms)?;
    }
    
    Ok(())
}

/// Set up shell alias for local installation
fn setup_shell_alias() -> Result<String> {
    let shell = detect_shell();
    let alias_line = format!(
        "alias llminate='{}/llminate'",
        get_local_installation_path().display()
    );
    
    let instructions = match shell.as_str() {
        "zsh" => {
            let rc_file = dirs::home_dir()
                .unwrap()
                .join(".zshrc");
            add_to_rc_file(&rc_file, &alias_line)?;
            format!("Added alias to ~/.zshrc. Run 'source ~/.zshrc' or restart your terminal.")
        }
        "bash" => {
            let rc_file = dirs::home_dir()
                .unwrap()
                .join(".bashrc");
            add_to_rc_file(&rc_file, &alias_line)?;
            format!("Added alias to ~/.bashrc. Run 'source ~/.bashrc' or restart your terminal.")
        }
        "fish" => {
            let config_dir = dirs::config_dir()
                .unwrap()
                .join("fish");
            fs::create_dir_all(&config_dir)?;
            let alias_file = config_dir.join("functions").join("llminate.fish");
            let fish_content = format!(
                "function llminate\n    {} $argv\nend",
                get_local_installation_path().join("llminate").display()
            );
            fs::write(alias_file, fish_content)?;
            format!("Added function to Fish config. Restart your terminal.")
        }
        _ => {
            format!(
                "Add this line to your shell configuration:\n{}",
                alias_line
            )
        }
    };
    
    Ok(instructions)
}

/// Detect the user's shell
fn detect_shell() -> String {
    std::env::var("SHELL")
        .ok()
        .and_then(|s| s.split('/').last().map(String::from))
        .unwrap_or_else(|| "bash".to_string())
}

/// Add line to RC file if not already present
fn add_to_rc_file(path: &Path, line: &str) -> Result<()> {
    let content = fs::read_to_string(path).unwrap_or_default();
    
    if !content.contains(line) {
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)?;
        writeln!(file, "\n# Added by llminate installer")?;
        writeln!(file, "{}", line)?;
    }
    
    Ok(())
}

/// Uninstall global npm package
pub async fn uninstall_global() -> Result<bool> {
    let output = Command::new("npm")
        .args(&["uninstall", "-g", "--force", crate::PACKAGE_NAME])
        .output()
        .context("Failed to run npm uninstall")?;
    
    Ok(output.status.success())
}