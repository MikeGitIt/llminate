//! Plugin system implementation matching JavaScript Claude Code
//!
//! This module provides full plugin support including:
//! - Plugin manifest parsing and validation
//! - Marketplace management
//! - Plugin installation/uninstallation
//! - Plugin enable/disable tracking
//!
//! File locations:
//! - Plugin manifests: `.claude-plugin/plugin.json`
//! - Marketplace manifests: `.claude-plugin/marketplace.json`
//! - Installed plugins: `~/.claude/plugins/installed_plugins.json`
//! - Marketplaces: `~/.claude/marketplaces.json`

use crate::config::{get_global_config_dir, load_settings, save_settings, SettingsSource};
use crate::error::{Error, Result};
use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

// ============================================================================
// Plugin Manifest Schema (matches JavaScript variable6320)
// ============================================================================

/// Plugin manifest matching JavaScript schema
/// Located at `.claude-plugin/plugin.json` or `plugin.json`
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    /// Unique plugin name (required)
    pub name: String,

    /// Human-readable description
    #[serde(default)]
    pub description: String,

    /// Plugin version (semver)
    #[serde(default)]
    pub version: Option<String>,

    /// Author name
    #[serde(default)]
    pub author: Option<String>,

    /// Repository URL
    #[serde(default)]
    pub repository: Option<String>,

    /// Homepage URL
    #[serde(default)]
    pub homepage: Option<String>,

    /// License identifier
    #[serde(default)]
    pub license: Option<String>,

    /// Keywords for discovery
    #[serde(default)]
    pub keywords: Vec<String>,

    /// Slash commands provided by this plugin
    #[serde(default)]
    pub commands: Option<PluginCommands>,

    /// Agents provided by this plugin
    #[serde(default)]
    pub agents: Option<Value>,

    /// Skills provided by this plugin
    #[serde(default)]
    pub skills: Option<Value>,

    /// Hooks configuration
    #[serde(default)]
    pub hooks: Option<Value>,

    /// MCP servers provided by this plugin
    #[serde(default)]
    pub mcp_servers: Option<Value>,

    /// LSP servers provided by this plugin
    #[serde(default)]
    pub lsp_servers: Option<Value>,

    /// Output styles configuration
    #[serde(default)]
    pub output_styles: Option<Value>,

    /// Additional fields
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// Commands configuration in plugin manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PluginCommands {
    /// Single command file path
    Path(String),
    /// Map of command names to definitions
    Map(HashMap<String, CommandDefinition>),
    /// Array of command paths or definitions
    Array(Vec<Value>),
}

/// Individual command definition
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandDefinition {
    /// Path to command file (relative to plugin root)
    #[serde(default)]
    pub source: Option<String>,

    /// Inline content for the command
    #[serde(default)]
    pub content: Option<String>,

    /// Command description
    #[serde(default)]
    pub description: Option<String>,

    /// When to suggest this command
    #[serde(default)]
    pub when_to_use: Option<String>,

    /// Allowed tools for this command
    #[serde(default)]
    pub allowed_tools: Option<Vec<String>>,
}

// ============================================================================
// Marketplace Manifest Schema (matches JavaScript variable13195)
// ============================================================================

/// Marketplace manifest matching JavaScript schema
/// Located at `.claude-plugin/marketplace.json`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceManifest {
    /// Unique marketplace name (required)
    pub name: String,

    /// Human-readable description
    #[serde(default)]
    pub description: Option<String>,

    /// List of plugins in this marketplace
    #[serde(default)]
    pub plugins: Vec<MarketplacePluginEntry>,

    /// Additional fields
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// Plugin entry in a marketplace manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplacePluginEntry {
    /// Plugin name (required, must be unique)
    pub name: String,

    /// Plugin source (where to fetch it)
    #[serde(default)]
    pub source: Option<PluginSource>,

    /// Plugin description override
    #[serde(default)]
    pub description: Option<String>,

    /// Category for organizing
    #[serde(default)]
    pub category: Option<String>,

    /// Tags for searchability
    #[serde(default)]
    pub tags: Option<Vec<String>>,

    /// Version constraint
    #[serde(default)]
    pub version: Option<String>,

    /// Require plugin.json in plugin folder
    #[serde(default = "default_true")]
    pub strict: bool,

    /// Commands override (when strict=false)
    #[serde(default)]
    pub commands: Option<Value>,

    /// Agents override (when strict=false)
    #[serde(default)]
    pub agents: Option<Value>,

    /// Skills override (when strict=false)
    #[serde(default)]
    pub skills: Option<Value>,

    /// Hooks override (when strict=false)
    #[serde(default)]
    pub hooks: Option<Value>,

    /// MCP servers override (when strict=false)
    #[serde(default)]
    pub mcp_servers: Option<Value>,

    /// Additional fields
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

fn default_true() -> bool {
    true
}

/// Plugin source types matching JavaScript
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "lowercase")]
pub enum PluginSource {
    /// Relative path within marketplace
    #[serde(rename = "path")]
    Path { path: String },

    /// GitHub repository
    #[serde(rename = "github")]
    GitHub {
        repo: String,
        #[serde(rename = "ref")]
        git_ref: Option<String>,
    },

    /// Git repository URL
    #[serde(rename = "git")]
    Git {
        url: String,
        #[serde(rename = "ref")]
        git_ref: Option<String>,
    },

    /// NPM package
    #[serde(rename = "npm")]
    Npm { package: String },

    /// Direct URL
    #[serde(rename = "url")]
    Url { url: String },
}

// ============================================================================
// Marketplace Source (for adding marketplaces)
// ============================================================================

/// Source configuration for adding marketplaces
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "lowercase")]
pub enum MarketplaceSource {
    /// Direct URL to marketplace.json
    Url {
        url: String,
        #[serde(default)]
        headers: Option<HashMap<String, String>>,
    },

    /// GitHub repository
    #[serde(rename = "github")]
    GitHub {
        repo: String,
        #[serde(rename = "ref")]
        git_ref: Option<String>,
        #[serde(default)]
        path: Option<String>,
    },

    /// Git repository URL
    Git {
        url: String,
        #[serde(rename = "ref")]
        git_ref: Option<String>,
        #[serde(default)]
        path: Option<String>,
    },

    /// NPM package
    Npm { package: String },

    /// Local file path
    File { path: String },

    /// Local directory
    Directory { path: String },
}

// ============================================================================
// Installed Plugins Storage (V2 format)
// ============================================================================

/// V2 format for installed_plugins.json
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InstalledPluginsV2 {
    /// Version marker (always 2)
    pub version: u32,

    /// Map of plugin IDs to installation info
    pub plugins: HashMap<String, InstalledPluginInfo>,
}

/// Information about an installed plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledPluginInfo {
    /// Plugin source identifier
    pub source: String,

    /// Installation path on disk
    pub install_path: Option<String>,

    /// Version installed
    pub version: Option<String>,

    /// Installation timestamp
    pub installed_at: String,

    /// Last update timestamp
    pub updated_at: Option<String>,

    /// Marketplace this plugin came from
    pub marketplace: Option<String>,

    /// Git commit SHA if from git source
    pub commit_sha: Option<String>,

    /// Scope where this plugin was enabled
    pub scope: String,

    /// Additional fields
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

// ============================================================================
// Installed Marketplaces Storage
// ============================================================================

/// Marketplaces configuration file
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MarketplacesConfig {
    /// Map of marketplace names to their info
    #[serde(flatten)]
    pub marketplaces: HashMap<String, MarketplaceInfo>,
}

/// Information about an installed marketplace
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceInfo {
    /// Source configuration
    pub source: MarketplaceSource,

    /// Cache location on disk
    pub install_location: String,

    /// Last update timestamp
    pub last_updated: String,

    /// Auto-update enabled
    #[serde(default)]
    pub auto_update: Option<bool>,
}

// ============================================================================
// Plugin Command Types (for /plugin slash command)
// ============================================================================

/// Parsed /plugin command
#[derive(Debug, Clone)]
pub enum PluginCommand {
    /// Show main plugin menu
    Menu,

    /// Show help
    Help,

    /// Install a plugin
    Install {
        plugin: Option<String>,
        marketplace: Option<String>,
    },

    /// Uninstall a plugin
    Uninstall { plugin: Option<String> },

    /// Enable a plugin
    Enable { plugin: Option<String> },

    /// Disable a plugin
    Disable { plugin: Option<String> },

    /// Validate a plugin manifest
    Validate { path: Option<String> },

    /// Manage plugins (interactive)
    Manage,

    /// Marketplace subcommands
    Marketplace(MarketplaceCommand),
}

/// Marketplace subcommands
#[derive(Debug, Clone)]
pub enum MarketplaceCommand {
    /// Show marketplace menu
    Menu,

    /// Add a marketplace
    Add { target: Option<String> },

    /// Remove a marketplace
    Remove { target: Option<String> },

    /// Update a marketplace
    Update { target: Option<String> },

    /// List marketplaces
    List,
}

// ============================================================================
// File Path Functions
// ============================================================================

/// Get the plugins directory: ~/.claude/plugins/
pub fn get_plugins_dir() -> PathBuf {
    get_global_config_dir().join("plugins")
}

/// Get the installed plugins file: ~/.claude/plugins/installed_plugins.json
pub fn get_installed_plugins_path() -> PathBuf {
    get_plugins_dir().join("installed_plugins.json")
}

/// Get the marketplaces config file: ~/.claude/marketplaces.json
pub fn get_marketplaces_path() -> PathBuf {
    get_global_config_dir().join("marketplaces.json")
}

/// Get the plugin cache directory: ~/.claude/plugins/cache/
pub fn get_plugin_cache_dir() -> PathBuf {
    get_plugins_dir().join("cache")
}

/// Get the marketplace cache directory: ~/.claude/marketplaces/
pub fn get_marketplace_cache_dir() -> PathBuf {
    get_global_config_dir().join("marketplaces")
}

// ============================================================================
// Installed Plugins Management
// ============================================================================

/// Load installed plugins from disk
pub fn load_installed_plugins() -> Result<InstalledPluginsV2> {
    let path = get_installed_plugins_path();

    if !path.exists() {
        return Ok(InstalledPluginsV2 {
            version: 2,
            plugins: HashMap::new(),
        });
    }

    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read installed plugins from {}", path.display()))?;

    // Try to parse as V2 first
    let data: Value = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse installed plugins from {}", path.display()))?;

    // Check version
    let version = data.get("version").and_then(|v| v.as_u64()).unwrap_or(1) as u32;

    if version == 2 {
        let plugins: InstalledPluginsV2 = serde_json::from_value(data)
            .context("Failed to deserialize V2 installed plugins")?;
        return Ok(plugins);
    }

    // V1 format - convert to V2
    // V1 format: { plugins: { [id]: { source, ... } } }
    let v1_plugins = data.get("plugins").cloned().unwrap_or(Value::Object(serde_json::Map::new()));

    let plugins: HashMap<String, InstalledPluginInfo> = if let Value::Object(map) = v1_plugins {
        map.into_iter()
            .filter_map(|(k, v)| {
                serde_json::from_value(v).ok().map(|info| (k, info))
            })
            .collect()
    } else {
        HashMap::new()
    };

    Ok(InstalledPluginsV2 { version: 2, plugins })
}

/// Save installed plugins to disk
pub fn save_installed_plugins(plugins: &InstalledPluginsV2) -> Result<()> {
    let path = get_installed_plugins_path();

    // Create parent directory if needed
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create plugins directory: {}", parent.display()))?;
    }

    let content = serde_json::to_string_pretty(plugins)
        .context("Failed to serialize installed plugins")?;

    fs::write(&path, content)
        .with_context(|| format!("Failed to write installed plugins to {}", path.display()))?;

    Ok(())
}

/// Get a specific installed plugin
pub fn get_installed_plugin(plugin_id: &str) -> Result<Option<InstalledPluginInfo>> {
    let plugins = load_installed_plugins()?;
    Ok(plugins.plugins.get(plugin_id).cloned())
}

/// Check if a plugin is installed
pub fn is_plugin_installed(plugin_id: &str) -> Result<bool> {
    let plugins = load_installed_plugins()?;
    Ok(plugins.plugins.contains_key(plugin_id))
}

/// Add an installed plugin
pub fn add_installed_plugin(plugin_id: &str, info: InstalledPluginInfo) -> Result<()> {
    let mut plugins = load_installed_plugins()?;
    plugins.plugins.insert(plugin_id.to_string(), info);
    save_installed_plugins(&plugins)
}

/// Remove an installed plugin
pub fn remove_installed_plugin(plugin_id: &str) -> Result<bool> {
    let mut plugins = load_installed_plugins()?;
    let existed = plugins.plugins.remove(plugin_id).is_some();
    if existed {
        save_installed_plugins(&plugins)?;
    }
    Ok(existed)
}

// ============================================================================
// Enabled Plugins Management (via settings)
// ============================================================================

/// Get enabled plugins from settings
/// Returns a map of plugin_id -> enabled status (true/false or scopes array)
pub fn get_enabled_plugins(source: SettingsSource) -> Result<HashMap<String, Value>> {
    let settings = load_settings(source)?;

    if let Some(Value::Object(enabled)) = settings.extra.get("enabledPlugins") {
        let result: HashMap<String, Value> = enabled
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        return Ok(result);
    }

    Ok(HashMap::new())
}

/// Check if a plugin is enabled
pub fn is_plugin_enabled(plugin_id: &str) -> Result<bool> {
    // Check all settings sources
    for source in [SettingsSource::User, SettingsSource::Project, SettingsSource::Local] {
        let enabled = get_enabled_plugins(source)?;
        if let Some(value) = enabled.get(plugin_id) {
            // Could be bool or array of scopes
            if let Some(b) = value.as_bool() {
                if b {
                    return Ok(true);
                }
            } else if value.is_array() {
                // Non-empty array means enabled in those scopes
                return Ok(true);
            }
        }
    }

    Ok(false)
}

/// Enable a plugin in settings
pub fn enable_plugin(plugin_id: &str, source: SettingsSource) -> Result<()> {
    let mut settings = load_settings(source)?;

    let enabled_plugins = settings
        .extra
        .entry("enabledPlugins".to_string())
        .or_insert(Value::Object(serde_json::Map::new()));

    if let Value::Object(map) = enabled_plugins {
        map.insert(plugin_id.to_string(), Value::Bool(true));
    }

    save_settings(source, &settings)?;
    Ok(())
}

/// Disable a plugin in settings
pub fn disable_plugin(plugin_id: &str, source: SettingsSource) -> Result<()> {
    let mut settings = load_settings(source)?;

    if let Some(Value::Object(map)) = settings.extra.get_mut("enabledPlugins") {
        map.insert(plugin_id.to_string(), Value::Bool(false));
    }

    save_settings(source, &settings)?;
    Ok(())
}

// ============================================================================
// Marketplace Management
// ============================================================================

/// Load marketplaces config
pub fn load_marketplaces() -> Result<MarketplacesConfig> {
    let path = get_marketplaces_path();

    if !path.exists() {
        return Ok(MarketplacesConfig::default());
    }

    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read marketplaces from {}", path.display()))?;

    let config: MarketplacesConfig = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse marketplaces from {}", path.display()))?;

    Ok(config)
}

/// Save marketplaces config
pub fn save_marketplaces(config: &MarketplacesConfig) -> Result<()> {
    let path = get_marketplaces_path();

    // Create parent directory if needed
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create config directory: {}", parent.display()))?;
    }

    let content = serde_json::to_string_pretty(config)
        .context("Failed to serialize marketplaces")?;

    fs::write(&path, content)
        .with_context(|| format!("Failed to write marketplaces to {}", path.display()))?;

    Ok(())
}

/// Check if a marketplace is installed
pub fn is_marketplace_installed(name: &str) -> Result<bool> {
    let config = load_marketplaces()?;
    Ok(config.marketplaces.contains_key(name))
}

/// Get a marketplace by name
pub fn get_marketplace(name: &str) -> Result<Option<MarketplaceInfo>> {
    let config = load_marketplaces()?;
    Ok(config.marketplaces.get(name).cloned())
}

/// Add a marketplace
pub fn add_marketplace(name: &str, info: MarketplaceInfo) -> Result<()> {
    let mut config = load_marketplaces()?;

    if config.marketplaces.contains_key(name) {
        return Err(Error::Config(format!(
            "Marketplace '{}' is already installed. Please remove it first using '/plugin marketplace remove {}'",
            name, name
        )));
    }

    config.marketplaces.insert(name.to_string(), info);
    save_marketplaces(&config)
}

/// Remove a marketplace
pub fn remove_marketplace(name: &str) -> Result<bool> {
    let mut config = load_marketplaces()?;
    let existed = config.marketplaces.remove(name).is_some();

    if existed {
        save_marketplaces(&config)?;

        // Also clean up the cache directory
        let cache_dir = get_marketplace_cache_dir().join(name);
        if cache_dir.exists() {
            let _ = fs::remove_dir_all(&cache_dir);
        }
    }

    Ok(existed)
}

/// List all installed marketplaces
pub fn list_marketplaces() -> Result<Vec<(String, MarketplaceInfo)>> {
    let config = load_marketplaces()?;
    Ok(config.marketplaces.into_iter().collect())
}

// ============================================================================
// Manifest Loading and Validation
// ============================================================================

/// Load a plugin manifest from a directory
pub fn load_plugin_manifest(plugin_dir: &Path) -> Result<PluginManifest> {
    // Try .claude-plugin/plugin.json first
    let manifest_path = plugin_dir.join(".claude-plugin").join("plugin.json");

    if manifest_path.exists() {
        return load_manifest_from_path(&manifest_path);
    }

    // Try plugin.json in root (legacy)
    let legacy_path = plugin_dir.join("plugin.json");

    if legacy_path.exists() {
        return load_manifest_from_path(&legacy_path);
    }

    Err(Error::NotFound(format!(
        "No plugin manifest found in {}. Expected .claude-plugin/plugin.json or plugin.json",
        plugin_dir.display()
    )))
}

/// Load a plugin manifest from a specific file path
pub fn load_manifest_from_path(path: &Path) -> Result<PluginManifest> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read manifest from {}", path.display()))?;

    let manifest: PluginManifest = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse manifest from {}", path.display()))?;

    validate_plugin_manifest(&manifest)?;
    Ok(manifest)
}

/// Load a marketplace manifest from a directory
pub fn load_marketplace_manifest(marketplace_dir: &Path) -> Result<MarketplaceManifest> {
    let manifest_path = marketplace_dir.join(".claude-plugin").join("marketplace.json");

    if manifest_path.exists() {
        return load_marketplace_manifest_from_path(&manifest_path);
    }

    Err(Error::NotFound(format!(
        "No marketplace manifest found in {}. Expected .claude-plugin/marketplace.json",
        marketplace_dir.display()
    )))
}

/// Load a marketplace manifest from a specific file path
pub fn load_marketplace_manifest_from_path(path: &Path) -> Result<MarketplaceManifest> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read marketplace manifest from {}", path.display()))?;

    let manifest: MarketplaceManifest = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse marketplace manifest from {}", path.display()))?;

    validate_marketplace_manifest(&manifest)?;
    Ok(manifest)
}

/// Validate a plugin manifest
pub fn validate_plugin_manifest(manifest: &PluginManifest) -> Result<()> {
    // Name is required and cannot be empty
    if manifest.name.is_empty() {
        return Err(Error::InvalidInput("Plugin name cannot be empty".to_string()));
    }

    // Name cannot contain spaces
    if manifest.name.contains(' ') {
        return Err(Error::InvalidInput(
            format!("Plugin name cannot contain spaces. Use kebab-case (e.g., 'my-plugin'). Got: '{}'", manifest.name)
        ));
    }

    Ok(())
}

/// Validate a marketplace manifest
pub fn validate_marketplace_manifest(manifest: &MarketplaceManifest) -> Result<()> {
    // Name is required and cannot be empty
    if manifest.name.is_empty() {
        return Err(Error::InvalidInput("Marketplace name cannot be empty".to_string()));
    }

    // Name cannot contain spaces
    if manifest.name.contains(' ') {
        return Err(Error::InvalidInput(
            format!("Marketplace name cannot contain spaces. Use kebab-case. Got: '{}'", manifest.name)
        ));
    }

    // Validate each plugin entry
    for plugin in &manifest.plugins {
        if plugin.name.is_empty() {
            return Err(Error::InvalidInput("Plugin name cannot be empty".to_string()));
        }
        if plugin.name.contains(' ') {
            return Err(Error::InvalidInput(
                format!("Plugin name cannot contain spaces. Got: '{}'", plugin.name)
            ));
        }
    }

    Ok(())
}

// ============================================================================
// Plugin Command Parsing
// ============================================================================

/// Parse a /plugin command string
pub fn parse_plugin_command(input: &str) -> PluginCommand {
    let parts: Vec<&str> = input.split_whitespace().collect();

    // Get the first argument after /plugin
    let subcommand = parts.get(0).map(|s| s.to_lowercase());

    match subcommand.as_deref() {
        None | Some("") => PluginCommand::Menu,

        Some("help") | Some("-h") | Some("--help") => PluginCommand::Help,

        Some("install") | Some("i") => {
            let target = parts.get(1).map(|s| s.to_string());

            match target {
                None => PluginCommand::Install {
                    plugin: None,
                    marketplace: None,
                },
                Some(t) if t.contains('@') => {
                    let mut split = t.splitn(2, '@');
                    let plugin = split.next().map(|s| s.to_string());
                    let marketplace = split.next().map(|s| s.to_string());
                    PluginCommand::Install { plugin, marketplace }
                }
                Some(t) if t.starts_with("http://") || t.starts_with("https://") || t.starts_with("file://") || t.contains('/') || t.contains('\\') => {
                    // This is a marketplace path, not a plugin name
                    PluginCommand::Install {
                        plugin: None,
                        marketplace: Some(t),
                    }
                }
                Some(t) => PluginCommand::Install {
                    plugin: Some(t),
                    marketplace: None,
                },
            }
        }

        Some("manage") => PluginCommand::Manage,

        Some("uninstall") => PluginCommand::Uninstall {
            plugin: parts.get(1).map(|s| s.to_string()),
        },

        Some("enable") => PluginCommand::Enable {
            plugin: parts.get(1).map(|s| s.to_string()),
        },

        Some("disable") => PluginCommand::Disable {
            plugin: parts.get(1).map(|s| s.to_string()),
        },

        Some("validate") => PluginCommand::Validate {
            path: if parts.len() > 1 {
                Some(parts[1..].join(" "))
            } else {
                None
            },
        },

        Some("marketplace") | Some("market") => {
            let action = parts.get(1).map(|s| s.to_lowercase());
            let target = if parts.len() > 2 {
                Some(parts[2..].join(" "))
            } else {
                None
            };

            match action.as_deref() {
                Some("add") => PluginCommand::Marketplace(MarketplaceCommand::Add { target }),
                Some("remove") | Some("rm") => PluginCommand::Marketplace(MarketplaceCommand::Remove { target }),
                Some("update") => PluginCommand::Marketplace(MarketplaceCommand::Update { target }),
                Some("list") => PluginCommand::Marketplace(MarketplaceCommand::List),
                _ => PluginCommand::Marketplace(MarketplaceCommand::Menu),
            }
        }

        _ => PluginCommand::Menu,
    }
}

// ============================================================================
// Reserved Marketplace Names
// ============================================================================

/// Reserved marketplace names that can only be used by Anthropic
pub const RESERVED_MARKETPLACE_NAMES: &[&str] = &[
    "claude-code-marketplace",
    "claude-code-plugins",
    "claude-plugins-official",
    "anthropic-plugins",
    "official-plugins",
    "claude-official",
];

/// Check if a marketplace name is reserved
pub fn is_reserved_marketplace_name(name: &str) -> bool {
    RESERVED_MARKETPLACE_NAMES.iter().any(|reserved| *reserved == name.to_lowercase())
}

/// Check if a source is allowed to use a reserved name
pub fn can_use_reserved_name(name: &str, source: &MarketplaceSource) -> bool {
    if !is_reserved_marketplace_name(name) {
        return true;
    }

    // Only GitHub sources from anthropics/ organization can use reserved names
    match source {
        MarketplaceSource::GitHub { repo, .. } => {
            repo.to_lowercase().starts_with("anthropics/")
        }
        MarketplaceSource::Git { url, .. } => {
            let url_lower = url.to_lowercase();
            url_lower.contains("github.com/anthropics/") || url_lower.contains("git@github.com:anthropics/")
        }
        _ => false,
    }
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Generate a plugin ID from name and marketplace
/// Format: "plugin-name@marketplace-name" or just "plugin-name"
pub fn make_plugin_id(plugin_name: &str, marketplace: Option<&str>) -> String {
    match marketplace {
        Some(m) => format!("{}@{}", plugin_name, m),
        None => plugin_name.to_string(),
    }
}

/// Parse a plugin ID into (plugin_name, marketplace)
pub fn parse_plugin_id(plugin_id: &str) -> (String, Option<String>) {
    if plugin_id.contains('@') {
        let mut parts = plugin_id.splitn(2, '@');
        let name = parts.next().unwrap_or("").to_string();
        let marketplace = parts.next().map(|s| s.to_string());
        (name, marketplace)
    } else {
        (plugin_id.to_string(), None)
    }
}

/// Detect manifest type from path
pub fn detect_manifest_type(path: &Path) -> ManifestType {
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let parent_name = path.parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("");

    if file_name == "plugin.json" {
        ManifestType::Plugin
    } else if file_name == "marketplace.json" {
        ManifestType::Marketplace
    } else if parent_name == ".claude-plugin" {
        ManifestType::Plugin
    } else {
        ManifestType::Unknown
    }
}

/// Type of manifest file
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestType {
    Plugin,
    Marketplace,
    Unknown,
}

// ============================================================================
// Validation Report
// ============================================================================

/// Result of validating a manifest
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Whether validation passed
    pub is_valid: bool,

    /// Errors found
    pub errors: Vec<ValidationError>,

    /// Warnings found
    pub warnings: Vec<String>,

    /// Manifest type detected
    pub manifest_type: ManifestType,
}

/// Validation error details
#[derive(Debug, Clone)]
pub struct ValidationError {
    /// Path to the problematic field
    pub path: String,

    /// Error message
    pub message: String,
}

impl ValidationResult {
    /// Create a successful validation result
    pub fn success(manifest_type: ManifestType) -> Self {
        ValidationResult {
            is_valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
            manifest_type,
        }
    }

    /// Create a failed validation result
    pub fn failure(errors: Vec<ValidationError>, manifest_type: ManifestType) -> Self {
        ValidationResult {
            is_valid: false,
            errors,
            warnings: Vec::new(),
            manifest_type,
        }
    }

    /// Add a warning
    pub fn with_warning(mut self, warning: String) -> Self {
        self.warnings.push(warning);
        self
    }
}

/// Validate a manifest file at the given path
pub fn validate_manifest_file(path: &Path) -> ValidationResult {
    // Determine manifest type
    let manifest_type = detect_manifest_type(path);

    // Check if file exists
    if !path.exists() {
        return ValidationResult::failure(
            vec![ValidationError {
                path: path.display().to_string(),
                message: "File not found".to_string(),
            }],
            manifest_type,
        );
    }

    // Read file content
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            return ValidationResult::failure(
                vec![ValidationError {
                    path: path.display().to_string(),
                    message: format!("Failed to read file: {}", e),
                }],
                manifest_type,
            );
        }
    };

    // Parse JSON
    let json: Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            return ValidationResult::failure(
                vec![ValidationError {
                    path: path.display().to_string(),
                    message: format!("Invalid JSON: {}", e),
                }],
                manifest_type,
            );
        }
    };

    // Validate based on type
    match manifest_type {
        ManifestType::Plugin => validate_plugin_json(&json),
        ManifestType::Marketplace => validate_marketplace_json(&json),
        ManifestType::Unknown => {
            // Try to detect from content
            if json.get("plugins").is_some() {
                validate_marketplace_json(&json)
            } else {
                validate_plugin_json(&json)
            }
        }
    }
}

fn validate_plugin_json(json: &Value) -> ValidationResult {
    let mut errors = Vec::new();

    // Check required fields
    if json.get("name").and_then(|v| v.as_str()).is_none() {
        errors.push(ValidationError {
            path: "name".to_string(),
            message: "Required field 'name' is missing or not a string".to_string(),
        });
    } else {
        let name = json.get("name").and_then(|v| v.as_str()).unwrap_or("");
        if name.contains(' ') {
            errors.push(ValidationError {
                path: "name".to_string(),
                message: "Plugin name cannot contain spaces. Use kebab-case.".to_string(),
            });
        }
    }

    if errors.is_empty() {
        ValidationResult::success(ManifestType::Plugin)
    } else {
        ValidationResult::failure(errors, ManifestType::Plugin)
    }
}

fn validate_marketplace_json(json: &Value) -> ValidationResult {
    let mut errors = Vec::new();

    // Check required fields
    if json.get("name").and_then(|v| v.as_str()).is_none() {
        errors.push(ValidationError {
            path: "name".to_string(),
            message: "Required field 'name' is missing or not a string".to_string(),
        });
    } else {
        let name = json.get("name").and_then(|v| v.as_str()).unwrap_or("");
        if name.contains(' ') {
            errors.push(ValidationError {
                path: "name".to_string(),
                message: "Marketplace name cannot contain spaces. Use kebab-case.".to_string(),
            });
        }
    }

    // Check plugins array
    if let Some(plugins) = json.get("plugins") {
        if !plugins.is_array() {
            errors.push(ValidationError {
                path: "plugins".to_string(),
                message: "Field 'plugins' must be an array".to_string(),
            });
        } else if let Some(arr) = plugins.as_array() {
            for (i, plugin) in arr.iter().enumerate() {
                if plugin.get("name").and_then(|v| v.as_str()).is_none() {
                    errors.push(ValidationError {
                        path: format!("plugins[{}].name", i),
                        message: "Plugin entry must have a 'name' field".to_string(),
                    });
                }
            }
        }
    }

    if errors.is_empty() {
        ValidationResult::success(ManifestType::Marketplace)
    } else {
        ValidationResult::failure(errors, ManifestType::Marketplace)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_plugin_command_menu() {
        assert!(matches!(parse_plugin_command(""), PluginCommand::Menu));
    }

    #[test]
    fn test_parse_plugin_command_help() {
        assert!(matches!(parse_plugin_command("help"), PluginCommand::Help));
        assert!(matches!(parse_plugin_command("-h"), PluginCommand::Help));
        assert!(matches!(parse_plugin_command("--help"), PluginCommand::Help));
    }

    #[test]
    fn test_parse_plugin_command_install() {
        match parse_plugin_command("install my-plugin@my-market") {
            PluginCommand::Install { plugin, marketplace } => {
                assert_eq!(plugin, Some("my-plugin".to_string()));
                assert_eq!(marketplace, Some("my-market".to_string()));
            }
            _ => panic!("Expected Install command"),
        }

        match parse_plugin_command("install my-plugin") {
            PluginCommand::Install { plugin, marketplace } => {
                assert_eq!(plugin, Some("my-plugin".to_string()));
                assert_eq!(marketplace, None);
            }
            _ => panic!("Expected Install command"),
        }

        match parse_plugin_command("install") {
            PluginCommand::Install { plugin, marketplace } => {
                assert_eq!(plugin, None);
                assert_eq!(marketplace, None);
            }
            _ => panic!("Expected Install command"),
        }
    }

    #[test]
    fn test_parse_plugin_command_marketplace() {
        match parse_plugin_command("marketplace add anthropics/claude-code") {
            PluginCommand::Marketplace(MarketplaceCommand::Add { target }) => {
                assert_eq!(target, Some("anthropics/claude-code".to_string()));
            }
            _ => panic!("Expected Marketplace Add command"),
        }

        match parse_plugin_command("marketplace list") {
            PluginCommand::Marketplace(MarketplaceCommand::List) => {}
            _ => panic!("Expected Marketplace List command"),
        }
    }

    #[test]
    fn test_make_plugin_id() {
        assert_eq!(make_plugin_id("my-plugin", Some("my-market")), "my-plugin@my-market");
        assert_eq!(make_plugin_id("my-plugin", None), "my-plugin");
    }

    #[test]
    fn test_parse_plugin_id() {
        assert_eq!(
            parse_plugin_id("my-plugin@my-market"),
            ("my-plugin".to_string(), Some("my-market".to_string()))
        );
        assert_eq!(
            parse_plugin_id("my-plugin"),
            ("my-plugin".to_string(), None)
        );
    }

    #[test]
    fn test_reserved_marketplace_names() {
        assert!(is_reserved_marketplace_name("claude-code-plugins"));
        assert!(is_reserved_marketplace_name("Claude-Code-Plugins"));
        assert!(!is_reserved_marketplace_name("my-marketplace"));
    }
}
