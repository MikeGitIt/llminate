pub mod ai;
pub mod auth;
pub mod cli;
pub mod config;
pub mod error;
pub mod hooks;
pub mod mcp;
pub mod oauth;
pub mod permissions;
pub mod plugin;
pub mod progress;
pub mod telemetry;
pub mod tui;
pub mod updater;
pub mod utils;

// Re-export commonly used types
pub use error::{Error, Result};

// Version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const PACKAGE_NAME: &str = env!("CARGO_PKG_NAME");
pub const PACKAGE_TITLE: &str = "Claude Code";
pub const ISSUES_URL: &str = "https://github.com/anthropics/claude-code/issues";
pub const README_URL: &str = "https://docs.anthropic.com/en/docs/claude-code";