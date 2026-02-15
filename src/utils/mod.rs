pub mod error;
pub mod ripgrep;

use crate::error::{Error, Result};
use colored::Colorize;
use std::fs;
use std::path::{Path, PathBuf};

/// Ensure all config directories exist
pub fn ensure_config_dirs() -> Result<()> {
    let global_dir = crate::config::get_global_config_dir();
    fs::create_dir_all(&global_dir)
        .map_err(|e| Error::Io(e))?;
    
    let local_dir = crate::config::get_local_config_dir().join(".claude");
    if !local_dir.exists() {
        fs::create_dir_all(&local_dir)
            .map_err(|e| Error::Io(e))?;
    }
    
    Ok(())
}

/// Get version information string
pub fn get_version_info() -> String {
    format!(
        "{} v{}\n{}: {}\n{}: {}",
        crate::PACKAGE_NAME.bold(),
        crate::VERSION,
        "Issues".dimmed(),
        crate::ISSUES_URL,
        "Documentation".dimmed(),
        crate::README_URL
    )
}

/// Check if a path exists
pub fn path_exists(path: impl AsRef<Path>) -> bool {
    path.as_ref().exists()
}

/// Get the current working directory
pub fn get_cwd() -> Result<PathBuf> {
    std::env::current_dir().map_err(|e| Error::Io(e))
}

/// Create a directory with all parent directories
pub fn create_dir_all(path: impl AsRef<Path>) -> Result<()> {
    fs::create_dir_all(path.as_ref())
        .map_err(|e| Error::Io(e))
}

/// Read a file to string
pub fn read_file(path: impl AsRef<Path>) -> Result<String> {
    fs::read_to_string(path.as_ref())
        .map_err(|e| Error::Io(e))
}

/// Write string to file
pub fn write_file(path: impl AsRef<Path>, contents: &str) -> Result<()> {
    fs::write(path.as_ref(), contents)
        .map_err(|e| Error::Io(e))
}

/// Get home directory
pub fn get_home_dir() -> Result<PathBuf> {
    dirs::home_dir()
        .ok_or_else(|| Error::Config("Could not determine home directory".to_string()))
}

/// Format bytes as human readable
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;
    
    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }
    
    if unit_index == 0 {
        format!("{} {}", size as u64, UNITS[unit_index])
    } else {
        format!("{:.2} {}", size, UNITS[unit_index])
    }
}

/// Format duration as human readable
pub fn format_duration(millis: u64) -> String {
    if millis < 1000 {
        format!("{}ms", millis)
    } else if millis < 60_000 {
        format!("{:.1}s", millis as f64 / 1000.0)
    } else if millis < 3_600_000 {
        let minutes = millis / 60_000;
        let seconds = (millis % 60_000) / 1000;
        format!("{}m {}s", minutes, seconds)
    } else {
        let hours = millis / 3_600_000;
        let minutes = (millis % 3_600_000) / 60_000;
        format!("{}h {}m", hours, minutes)
    }
}

/// Check if running in CI environment
pub fn is_ci() -> bool {
    std::env::var("CI").is_ok() || std::env::var("CONTINUOUS_INTEGRATION").is_ok()
}

/// Check if running in TTY
pub fn is_tty() -> bool {
    atty::is(atty::Stream::Stdout) && atty::is(atty::Stream::Stderr)
}

/// Get terminal width
pub fn terminal_width() -> u16 {
    terminal_size::terminal_size()
        .map(|(w, _)| w.0)
        .unwrap_or(80)
}

/// Get terminal height
pub fn terminal_height() -> u16 {
    terminal_size::terminal_size()
        .map(|(_, h)| h.0)
        .unwrap_or(24)
}

/// Truncate string to fit terminal width
pub fn truncate_to_terminal(s: &str, prefix_len: usize) -> String {
    let width = terminal_width() as usize;
    if prefix_len + s.len() <= width {
        s.to_string()
    } else {
        let available = width.saturating_sub(prefix_len + 3); // 3 for "..."
        if available > 0 {
            format!("{}...", &s[..available])
        } else {
            "...".to_string()
        }
    }
}

/// Generate a random session ID
pub fn generate_session_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Get timestamp in milliseconds
pub fn timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}