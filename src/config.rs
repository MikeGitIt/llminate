use crate::error::{Error, Result};
use anyhow::Context;
use dirs;
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

static CONFIG_CACHE: Lazy<Arc<RwLock<ConfigCache>>> = Lazy::new(|| {
    Arc::new(RwLock::new(ConfigCache::default()))
});

#[derive(Debug, Default)]
struct ConfigCache {
    global: Option<Config>,
    local: Option<Config>,
    project: Option<Config>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum ConfigScope {
    Local,
    User,
    Project,
}

impl std::fmt::Display for ConfigScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigScope::Local => write!(f, "local"),
            ConfigScope::User => write!(f, "user"),
            ConfigScope::Project => write!(f, "project"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    // Core settings
    pub theme: Option<String>,
    pub model: Option<String>,
    pub verbose: Option<bool>,
    pub api_key_helper: Option<String>,
    
    // Features
    pub todo_feature_enabled: Option<bool>,
    pub memory_usage_count: Option<u32>,
    pub prompt_queue_use_count: Option<u32>,
    
    // Installation status
    pub has_completed_onboarding: Option<bool>,
    pub last_onboarding_version: Option<String>,
    pub auto_updater_status: Option<String>,
    pub num_startups: Option<u32>,
    
    // MCP servers
    pub mcp_servers: Option<HashMap<String, McpServerConfig>>,
    pub enabled_mcpjson_servers: Option<Vec<String>>,
    pub disabled_mcpjson_servers: Option<Vec<String>>,
    pub enable_all_project_mcp_servers: Option<bool>,
    
    // Terminal setup
    pub shift_enter_key_binding_installed: Option<bool>,
    pub option_as_meta_key_installed: Option<bool>,
    
    // GitHub integration
    pub github_action_setup_count: Option<u32>,
    
    // Session data
    pub last_cost: Option<f64>,
    pub last_duration: Option<u64>,
    pub last_api_duration: Option<u64>,
    pub last_lines_added: Option<u32>,
    pub last_lines_removed: Option<u32>,
    pub last_total_input_tokens: Option<u64>,
    pub last_total_output_tokens: Option<u64>,
    pub last_total_cache_creation_input_tokens: Option<u64>,
    pub last_total_cache_read_input_tokens: Option<u64>,
    pub last_session_id: Option<String>,
    
    // AI configuration
    pub ai_config: Option<crate::ai::AIConfig>,
    
    // Logging configuration
    pub logging_config: Option<LoggingConfig>,
    
    // Task tool configuration
    pub parallel_tasks_count: Option<usize>,
    
    // Authentication
    pub oauth_account: Option<serde_json::Value>, // Complex object, using Value for now
    
    // API key management
    pub custom_api_key_responses: Option<serde_json::Value>,
    
    // Environment variables
    pub env: Option<HashMap<String, String>>,
    
    // Editor settings
    pub editor_mode: Option<String>,
    
    // Auto compact
    pub auto_compact_enabled: Option<bool>,
    
    // Diff tool
    pub diff_tool: Option<String>,
    
    // Data sharing
    pub initial_data_sharing_message_seen: Option<bool>,
    pub is_qualified_for_data_sharing: Option<bool>,
    
    // Fallback settings
    pub fallback_available_warning_threshold: Option<f64>,
    
    // Subscription
    pub recommended_subscription: Option<String>,
    
    // Cost acknowledgement
    pub has_acknowledged_cost_threshold: Option<bool>,
    
    // Tips and hints
    pub tips_history: Option<HashMap<String, u32>>,
    pub has_seen_tasks_hint: Option<bool>,
    pub queued_command_up_hint_count: Option<u32>,
    
    // Notification settings
    pub message_idle_notif_threshold_ms: Option<u64>,
    
    // Bypass permissions
    pub bypass_permissions_mode_accepted: Option<bool>,
    
    // Cached data
    pub cached_changelog: Option<String>,
    
    // Terminal settings
    pub has_used_backslash_return: Option<bool>,
    pub iterm2_backup_path: Option<String>,
    
    // Dynamic fields
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoggingConfig {
    // Core control
    pub default_level: Option<String>,                      // "debug", "info", "warn", "error"
    pub module_levels: Option<HashMap<String, String>>,     // Module-specific overrides
    
    // Output targets
    pub enable_file_logging: Option<bool>,
    pub enable_stderr_logging: Option<bool>, 
    pub enable_json_logging: Option<bool>,
    
    // Formatting
    pub format_style: Option<String>,                       // "compact", "full", "pretty", "json"
    pub include_timestamps: Option<bool>,
    pub include_thread_info: Option<bool>,
    pub include_source_location: Option<bool>,
    
    // File management
    pub log_file_path: Option<String>,
    pub max_file_size_mb: Option<u64>,
    pub enable_rotation: Option<bool>,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        LoggingConfig {
            default_level: Some("info".to_string()),
            module_levels: Some(HashMap::new()),
            enable_file_logging: Some(false),
            enable_stderr_logging: Some(true),
            enable_json_logging: Some(false),
            format_style: Some("compact".to_string()),
            include_timestamps: Some(true),
            include_thread_info: Some(false),
            include_source_location: Some(false),
            log_file_path: Some("claude.log".to_string()),
            max_file_size_mb: Some(10),
            enable_rotation: Some(true),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    #[serde(rename = "type")]
    pub transport_type: Option<String>,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub url: Option<String>,
    pub headers: Option<HashMap<String, String>>,
    pub env: Option<HashMap<String, String>>,
}

/// Permissions configuration matching JavaScript settings.json schema
/// This stores allowed directories and permission rules
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionsConfig {
    /// Additional directories allowed for file tools (additionalDirectories in JS)
    #[serde(default)]
    pub additional_directories: Vec<String>,

    /// Allow rules for tools
    #[serde(default)]
    pub allow: Vec<String>,

    /// Deny rules for tools
    #[serde(default)]
    pub deny: Vec<String>,
}

/// Settings file structure matching JavaScript settings.json schema
/// This is separate from Config to match the JavaScript structure exactly
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Permissions configuration
    #[serde(default)]
    pub permissions: PermissionsConfig,

    /// Dynamic fields for extensibility
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// Settings source types matching JavaScript
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsSource {
    /// User settings: ~/.claude/settings.json
    User,
    /// Project settings: .claude/settings.json (shared, committed to git)
    Project,
    /// Local settings: .claude/settings.local.json (gitignored)
    Local,
    /// Session: runtime only, not persisted
    Session,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionMode {
    Default,
    Strict,
    Relaxed,
    BypassAll,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            theme: Some("dark".to_string()),
            model: Some("claude-opus-4-1-20250805".to_string()),
            verbose: Some(false),
            api_key_helper: Some("claude-api-key".to_string()),
            todo_feature_enabled: Some(true),
            memory_usage_count: Some(0),
            prompt_queue_use_count: Some(0),
            has_completed_onboarding: Some(false),
            last_onboarding_version: Some("0.0.0".to_string()),
            auto_updater_status: Some("enabled".to_string()),
            num_startups: Some(0),
            mcp_servers: Some(HashMap::new()),
            enabled_mcpjson_servers: Some(Vec::new()),
            disabled_mcpjson_servers: Some(Vec::new()),
            enable_all_project_mcp_servers: Some(false),
            shift_enter_key_binding_installed: Some(false),
            option_as_meta_key_installed: Some(false),
            github_action_setup_count: Some(0),
            last_cost: Some(0.0),
            last_duration: Some(0),
            last_api_duration: Some(0),
            last_lines_added: Some(0),
            last_lines_removed: Some(0),
            last_total_input_tokens: Some(0),
            last_total_output_tokens: Some(0),
            last_total_cache_creation_input_tokens: Some(0),
            last_total_cache_read_input_tokens: Some(0),
            last_session_id: Some(String::new()),
            ai_config: Some(crate::ai::AIConfig::default()),
            logging_config: None,
            parallel_tasks_count: Some(1),
            oauth_account: None,
            custom_api_key_responses: None,
            env: Some(HashMap::new()),
            editor_mode: None,
            auto_compact_enabled: Some(true),
            diff_tool: Some("auto".to_string()),
            initial_data_sharing_message_seen: Some(false),
            is_qualified_for_data_sharing: Some(false),
            fallback_available_warning_threshold: None,
            recommended_subscription: None,
            has_acknowledged_cost_threshold: Some(false),
            tips_history: Some(HashMap::new()),
            has_seen_tasks_hint: Some(false),
            queued_command_up_hint_count: Some(0),
            message_idle_notif_threshold_ms: Some(60000),
            bypass_permissions_mode_accepted: Some(false),
            cached_changelog: None,
            has_used_backslash_return: Some(false),
            iterm2_backup_path: None,
            extra: HashMap::new(),
        }
    }
}

/// Get the global config directory (~/.claude)
pub fn get_global_config_dir() -> PathBuf {
    dirs::home_dir()
        .expect("Could not determine home directory")
        .join(".claude")
}

/// Get the local config directory (current working directory)
pub fn get_local_config_dir() -> PathBuf {
    std::env::current_dir().expect("Could not get current directory")
}

/// Get the project config directory (traverse up to find .git)
pub fn get_project_config_dir() -> Option<PathBuf> {
    let mut current = std::env::current_dir().ok()?;
    
    loop {
        if current.join(".git").exists() {
            return Some(current);
        }
        
        if !current.pop() {
            break;
        }
    }
    
    None
}

/// Get the config file path for a given scope
pub fn get_config_path(scope: ConfigScope) -> Result<PathBuf> {
    match scope {
        ConfigScope::User => Ok(get_global_config_dir().join("config.json")),
        ConfigScope::Local => Ok(get_local_config_dir().join(".claude").join("config.json")),
        ConfigScope::Project => {
            if let Some(dir) = get_project_config_dir() {
                Ok(dir.join(".claude").join("config.json"))
            } else {
                Err(Error::Config("No project root found (no .git directory)".to_string()))
            }
        }
    }
}

/// Load config from a specific scope
pub fn load_config(scope: ConfigScope) -> Result<Config> {
    let path = get_config_path(scope)?;
    
    if !path.exists() {
        return Ok(Config::default());
    }
    
    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read config from {}", path.display()))?;
    
    let config: Config = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse config from {}", path.display()))?;
    
    Ok(config)
}

/// Save config to a specific scope
pub fn save_config(scope: ConfigScope, config: &Config) -> Result<()> {
    let path = get_config_path(scope)?;
    
    // Create parent directory if it doesn't exist
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create config directory: {}", parent.display()))?;
    }
    
    let content = serde_json::to_string_pretty(config)
        .context("Failed to serialize config")?;
    
    fs::write(&path, content)
        .with_context(|| format!("Failed to write config to {}", path.display()))?;
    
    // Invalidate cache
    let mut cache = CONFIG_CACHE.write();
    match scope {
        ConfigScope::User => cache.global = None,
        ConfigScope::Local => cache.local = None,
        ConfigScope::Project => cache.project = None,
    }
    
    Ok(())
}

/// Get merged config (project -> local -> global)
pub fn get_merged_config() -> Result<Config> {
    let global = load_config(ConfigScope::User)?;
    let local = load_config(ConfigScope::Local)?;
    let project = if get_project_config_dir().is_some() {
        Some(load_config(ConfigScope::Project)?)
    } else {
        None
    };
    
    // Merge configs (project overrides local overrides global)
    let mut merged = global;
    merge_config(&mut merged, &local);
    if let Some(proj) = project {
        merge_config(&mut merged, &proj);
    }
    
    Ok(merged)
}

/// Merge source config into target
fn merge_config(target: &mut Config, source: &Config) {
    // Use serde_json to handle the merge
    let mut target_value = serde_json::to_value(&*target).unwrap();
    let source_value = serde_json::to_value(source).unwrap();
    
    merge_json(&mut target_value, &source_value);
    
    *target = serde_json::from_value(target_value).unwrap();
}

/// Recursively merge JSON values
fn merge_json(target: &mut Value, source: &Value) {
    match (target, source) {
        (Value::Object(target_map), Value::Object(source_map)) => {
            for (key, value) in source_map {
                match target_map.get_mut(key) {
                    Some(target_value) => merge_json(target_value, value),
                    None => {
                        target_map.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (target, source) => {
            *target = source.clone();
        }
    }
}

/// Get a config value
pub fn get(key: &str, global: bool) -> Result<Value> {
    let scope = if global { ConfigScope::User } else { ConfigScope::Local };
    let config = load_config(scope)?;
    let value = serde_json::to_value(config)?;
    
    let parts: Vec<&str> = key.split('.').collect();
    let mut current = &value;
    
    for part in parts {
        match current.get(part) {
            Some(v) => current = v,
            None => return Ok(Value::Null),
        }
    }
    
    Ok(current.clone())
}

/// Set a config value
pub fn set(key: &str, value: &str, global: bool) -> Result<()> {
    let scope = if global { ConfigScope::User } else { ConfigScope::Local };
    let mut config = load_config(scope)?;
    
    // Parse value as JSON if possible, otherwise as string
    let json_value = serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_string()));
    
    // Convert config to JSON value
    let mut config_value = serde_json::to_value(&config)?;
    
    // Navigate to the key and set the value
    let parts: Vec<&str> = key.split('.').collect();
    let mut current = &mut config_value;
    
    for (i, part) in parts.iter().enumerate() {
        if i == parts.len() - 1 {
            // Last part - set the value
            if let Value::Object(map) = current {
                map.insert(part.to_string(), json_value.clone());
            }
        } else {
            // Navigate deeper
            if let Value::Object(map) = current {
                let entry = map.entry(part.to_string()).or_insert(Value::Object(serde_json::Map::new()));
                current = entry;
            }
        }
    }
    
    // Convert back to config
    config = serde_json::from_value(config_value)?;
    save_config(scope, &config)?;
    
    Ok(())
}

/// Remove a config value
pub fn remove(key: &str, global: bool) -> Result<()> {
    let scope = if global { ConfigScope::User } else { ConfigScope::Local };
    let mut config = load_config(scope)?;
    
    // Convert config to JSON value
    let mut config_value = serde_json::to_value(&config)?;
    
    // Navigate to the key and remove it
    let parts: Vec<&str> = key.split('.').collect();
    
    if parts.len() == 1 {
        // Top-level key
        if let Value::Object(map) = &mut config_value {
            map.remove(key);
        }
    } else {
        // Nested key
        let mut current = &mut config_value;
        
        for (i, part) in parts.iter().enumerate() {
            if i == parts.len() - 1 {
                // Last part - remove the key
                if let Value::Object(map) = current {
                    map.remove(*part);
                }
            } else {
                // Navigate deeper
                if let Value::Object(map) = current {
                    if let Some(next) = map.get_mut(*part) {
                        current = next;
                    } else {
                        break;
                    }
                }
            }
        }
    }
    
    // Convert back to config
    config = serde_json::from_value(config_value)?;
    save_config(scope, &config)?;
    
    Ok(())
}

/// Add values to a config array
pub fn add_to_array(key: &str, values: &[String], global: bool) -> Result<()> {
    let scope = if global { ConfigScope::User } else { ConfigScope::Local };
    let mut config = load_config(scope)?;
    
    // Convert config to JSON value
    let mut config_value = serde_json::to_value(&config)?;
    
    // Navigate to the key
    let parts: Vec<&str> = key.split('.').collect();
    let mut current = &mut config_value;
    
    for (i, part) in parts.iter().enumerate() {
        if i == parts.len() - 1 {
            // Last part - add to array
            if let Value::Object(map) = current {
                let array = map.entry(part.to_string()).or_insert(Value::Array(vec![]));
                if let Value::Array(arr) = array {
                    for value in values {
                        if !arr.iter().any(|v| v.as_str() == Some(value)) {
                            arr.push(Value::String(value.clone()));
                        }
                    }
                }
            }
        } else {
            // Navigate deeper
            if let Value::Object(map) = current {
                let entry = map.entry(part.to_string()).or_insert(Value::Object(serde_json::Map::new()));
                current = entry;
            }
        }
    }
    
    // Convert back to config
    config = serde_json::from_value(config_value)?;
    save_config(scope, &config)?;
    
    Ok(())
}

/// Remove values from a config array
pub fn remove_from_array(key: &str, values: &[String], global: bool) -> Result<()> {
    let scope = if global { ConfigScope::User } else { ConfigScope::Local };
    let mut config = load_config(scope)?;
    
    // Convert config to JSON value
    let mut config_value = serde_json::to_value(&config)?;
    
    // Navigate to the key
    let parts: Vec<&str> = key.split('.').collect();
    let mut current = &mut config_value;
    
    for (i, part) in parts.iter().enumerate() {
        if i == parts.len() - 1 {
            // Last part - remove from array
            if let Value::Object(map) = current {
                if let Some(Value::Array(arr)) = map.get_mut(*part) {
                    arr.retain(|v| !values.iter().any(|val| v.as_str() == Some(val)));
                }
            }
        } else {
            // Navigate deeper
            if let Value::Object(map) = current {
                if let Some(next) = map.get_mut(*part) {
                    current = next;
                } else {
                    break;
                }
            }
        }
    }
    
    // Convert back to config
    config = serde_json::from_value(config_value)?;
    save_config(scope, &config)?;
    
    Ok(())
}

/// Check if a key is an array in config
pub fn is_array_key(key: &str, global: bool) -> bool {
    match get(key, global) {
        Ok(Value::Array(_)) => true,
        _ => false,
    }
}

/// List all config values
pub fn list(global: bool) -> Result<Value> {
    let scope = if global { ConfigScope::User } else { ConfigScope::Local };
    let config = load_config(scope)?;
    Ok(serde_json::to_value(config)?)
}

/// Get a config value
pub fn get_config_value(key: &str, scope: ConfigScope) -> Result<Value> {
    let config = load_config(scope)?;
    let value = match key {
        "theme" => config.theme.map(Value::String),
        "model" => config.model.map(Value::String),
        "verbose" => config.verbose.map(Value::Bool),
        "api_key_helper" => config.api_key_helper.map(Value::String),
        "todo_feature_enabled" => config.todo_feature_enabled.map(Value::Bool),
        "memory_usage_count" => config.memory_usage_count.map(|v| Value::Number(v.into())),
        "prompt_queue_use_count" => config.prompt_queue_use_count.map(|v| Value::Number(v.into())),
        "has_completed_onboarding" => config.has_completed_onboarding.map(Value::Bool),
        "mcp_servers" => config.mcp_servers.map(|v| serde_json::to_value(v).unwrap_or(Value::Null)),
        "logging_config" => config.logging_config.map(|v| serde_json::to_value(v).unwrap_or(Value::Null)),
        _ => config.extra.get(key).cloned(),
    };
    
    value.ok_or_else(|| Error::Config(format!("Key '{}' not found", key)))
}

/// Set a config value
pub fn set_config_value(key: &str, value: &str, scope: ConfigScope) -> Result<()> {
    let mut config = load_config(scope)?;
    
    // Parse value
    let parsed_value: Value = if let Ok(v) = value.parse::<bool>() {
        Value::Bool(v)
    } else if let Ok(v) = value.parse::<u32>() {
        Value::Number(v.into())
    } else if let Ok(v) = value.parse::<f64>() {
        Value::Number(serde_json::Number::from_f64(v).unwrap_or(0.into()))
    } else {
        Value::String(value.to_string())
    };
    
    match key {
        "theme" => config.theme = parsed_value.as_str().map(String::from),
        "model" => config.model = parsed_value.as_str().map(String::from),
        "verbose" => config.verbose = parsed_value.as_bool(),
        "api_key_helper" => config.api_key_helper = parsed_value.as_str().map(String::from),
        "todo_feature_enabled" => config.todo_feature_enabled = parsed_value.as_bool(),
        "memory_usage_count" => config.memory_usage_count = parsed_value.as_u64().map(|v| v as u32),
        "prompt_queue_use_count" => config.prompt_queue_use_count = parsed_value.as_u64().map(|v| v as u32),
        "has_completed_onboarding" => config.has_completed_onboarding = parsed_value.as_bool(),
        "logging_config" => {
            config.logging_config = serde_json::from_value(parsed_value).ok();
        },
        _ => {
            config.extra.insert(key.to_string(), parsed_value);
        }
    }
    
    save_config(scope, &config)?;
    Ok(())
}

/// Remove a config value
pub fn remove_config_value(key: &str, scope: ConfigScope) -> Result<()> {
    let mut config = load_config(scope)?;
    
    match key {
        "theme" => config.theme = None,
        "model" => config.model = None,
        "verbose" => config.verbose = None,
        "api_key_helper" => config.api_key_helper = None,
        "todo_feature_enabled" => config.todo_feature_enabled = None,
        "memory_usage_count" => config.memory_usage_count = None,
        "prompt_queue_use_count" => config.prompt_queue_use_count = None,
        "has_completed_onboarding" => config.has_completed_onboarding = None,
        "mcp_servers" => config.mcp_servers = None,
        "logging_config" => config.logging_config = None,
        _ => {
            config.extra.remove(key);
        }
    }
    
    save_config(scope, &config)?;
    Ok(())
}

/// Get permission mode from config
pub fn get_permission_mode() -> Result<PermissionMode> {
    let config = get_merged_config()?;
    
    if let Some(mode) = config.extra.get("permissionMode") {
        if let Some(mode_str) = mode.as_str() {
            match mode_str {
                "strict" => return Ok(PermissionMode::Strict),
                "relaxed" => return Ok(PermissionMode::Relaxed),
                "bypass" => return Ok(PermissionMode::BypassAll),
                _ => {}
            }
        }
    }
    
    Ok(PermissionMode::Default)
}

/// Get all MCP servers from all scopes
pub fn get_all_mcp_servers() -> Result<HashMap<String, McpServerConfig>> {
    let mut servers = HashMap::new();
    
    // Load from all scopes (global -> local -> project)
    for scope in [ConfigScope::User, ConfigScope::Local, ConfigScope::Project] {
        if let Ok(config) = load_config(scope) {
            if let Some(mcp_servers) = config.mcp_servers {
                servers.extend(mcp_servers);
            }
        }
    }
    
    Ok(servers)
}

// ============================================================================
// Settings file functions (matching JavaScript settings.json schema)
// ============================================================================

/// Get the path for user settings: ~/.claude/settings.json
pub fn get_user_settings_path() -> PathBuf {
    get_global_config_dir().join("settings.json")
}

/// Get the path for project settings: .claude/settings.json
/// This is shared settings committed to git
pub fn get_project_settings_path() -> Option<PathBuf> {
    get_project_config_dir().map(|p| p.join(".claude").join("settings.json"))
}

/// Get the path for local settings: .claude/settings.local.json
/// This is gitignored local settings
pub fn get_local_settings_path() -> Option<PathBuf> {
    get_project_config_dir().map(|p| p.join(".claude").join("settings.local.json"))
}

/// Get the settings file path for a given source
pub fn get_settings_path(source: SettingsSource) -> Option<PathBuf> {
    match source {
        SettingsSource::User => Some(get_user_settings_path()),
        SettingsSource::Project => get_project_settings_path(),
        SettingsSource::Local => get_local_settings_path(),
        SettingsSource::Session => None, // Session settings are not persisted
    }
}

/// Load settings from a specific source
pub fn load_settings(source: SettingsSource) -> Result<Settings> {
    let path = match get_settings_path(source) {
        Some(p) => p,
        None => return Ok(Settings::default()), // Session source returns empty settings
    };

    if !path.exists() {
        return Ok(Settings::default());
    }

    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read settings from {}", path.display()))?;

    let settings: Settings = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse settings from {}", path.display()))?;

    Ok(settings)
}

/// Save settings to a specific source
pub fn save_settings(source: SettingsSource, settings: &Settings) -> Result<()> {
    let path = match get_settings_path(source) {
        Some(p) => p,
        None => return Err(Error::Config("Cannot save session settings".to_string())),
    };

    // Create parent directory if it doesn't exist
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create settings directory: {}", parent.display()))?;
    }

    let content = serde_json::to_string_pretty(settings)
        .context("Failed to serialize settings")?;

    fs::write(&path, content)
        .with_context(|| format!("Failed to write settings to {}", path.display()))?;

    Ok(())
}

/// Add a directory to the additionalDirectories list in settings
/// Returns Ok(true) if the directory was added, Ok(false) if it was already present
pub fn add_directory_to_settings(source: SettingsSource, directory: &Path) -> Result<bool> {
    let mut settings = load_settings(source)?;

    let dir_str = directory.to_string_lossy().to_string();

    // Check if already present
    if settings.permissions.additional_directories.contains(&dir_str) {
        return Ok(false);
    }

    // Add the directory
    settings.permissions.additional_directories.push(dir_str);

    // Save back
    save_settings(source, &settings)?;

    Ok(true)
}

/// Remove a directory from the additionalDirectories list in settings
/// Returns Ok(true) if the directory was removed, Ok(false) if it wasn't present
pub fn remove_directory_from_settings(source: SettingsSource, directory: &Path) -> Result<bool> {
    let mut settings = load_settings(source)?;

    let dir_str = directory.to_string_lossy().to_string();

    // Find and remove
    let original_len = settings.permissions.additional_directories.len();
    settings.permissions.additional_directories.retain(|d| d != &dir_str);

    if settings.permissions.additional_directories.len() == original_len {
        return Ok(false); // Wasn't present
    }

    // Save back
    save_settings(source, &settings)?;

    Ok(true)
}

/// Get all additional directories from all settings sources
pub fn get_all_additional_directories() -> Result<Vec<(String, SettingsSource)>> {
    let mut directories = Vec::new();

    // Load from each source in order (user -> project -> local)
    for source in [SettingsSource::User, SettingsSource::Project, SettingsSource::Local] {
        if let Ok(settings) = load_settings(source) {
            for dir in settings.permissions.additional_directories {
                directories.push((dir, source));
            }
        }
    }

    Ok(directories)
}

/// Get a friendly name for a settings source
pub fn get_settings_source_name(source: SettingsSource) -> &'static str {
    match source {
        SettingsSource::User => "user settings",
        SettingsSource::Project => "shared project settings",
        SettingsSource::Local => "project local settings",
        SettingsSource::Session => "current session",
    }
}

/// Get a short name for a settings source (for display)
pub fn get_settings_source_short_name(source: SettingsSource) -> &'static str {
    match source {
        SettingsSource::User => "user",
        SettingsSource::Project => "project",
        SettingsSource::Local => "local",
        SettingsSource::Session => "session",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_logging_config_default() {
        let config = LoggingConfig::default();
        
        assert_eq!(config.default_level, Some("info".to_string()));
        assert!(config.module_levels.is_some());
        assert_eq!(config.enable_file_logging, Some(false));
        assert_eq!(config.enable_stderr_logging, Some(true));
        assert_eq!(config.enable_json_logging, Some(false));
        assert_eq!(config.format_style, Some("compact".to_string()));
        assert_eq!(config.include_timestamps, Some(true));
        assert_eq!(config.include_thread_info, Some(false));
        assert_eq!(config.include_source_location, Some(false));
        assert_eq!(config.log_file_path, Some("claude.log".to_string()));
        assert_eq!(config.max_file_size_mb, Some(10));
        assert_eq!(config.enable_rotation, Some(true));
    }
    
    #[test]
    fn test_logging_config_serialization() {
        let config = LoggingConfig::default();
        
        // Test serialization
        let json = serde_json::to_string(&config).expect("Should serialize");
        assert!(json.contains("defaultLevel"));
        assert!(json.contains("moduleLevel"));
        assert!(json.contains("enableFileLogging"));
        assert!(json.contains("enableStderrLogging"));
        assert!(json.contains("formatStyle"));
        
        // Test deserialization
        let parsed: LoggingConfig = serde_json::from_str(&json).expect("Should deserialize");
        assert_eq!(parsed.default_level, config.default_level);
        assert_eq!(parsed.enable_file_logging, config.enable_file_logging);
    }
    
    #[test]
    fn test_config_with_logging_config() {
        let mut config = Config::default();
        config.logging_config = Some(LoggingConfig::default());
        
        // Test serialization
        let json = serde_json::to_string(&config).expect("Should serialize");
        assert!(json.contains("loggingConfig"));
        
        // Test deserialization
        let parsed: Config = serde_json::from_str(&json).expect("Should deserialize");
        assert!(parsed.logging_config.is_some());
        
        let logging_config = parsed.logging_config.unwrap();
        assert_eq!(logging_config.default_level, Some("info".to_string()));
    }
}