//! Hook System Implementation
//!
//! Hooks allow plugins and configuration to run commands at various points
//! in the Claude Code lifecycle. This matches the JavaScript implementation.
//!
//! Hook Types (12 total):
//! - SessionStart: When a new session is started
//! - SessionEnd: When a session is ending
//! - PreToolUse: Before a tool is called
//! - PostToolUse: After a tool completes successfully
//! - PostToolUseFailure: When a tool fails
//! - PreCompact: Before conversation compaction
//! - UserPromptSubmit: When user submits a prompt
//! - Notification: For notifications
//! - Stop: Stop signal hooks
//! - SubagentStart: When a sub-agent starts
//! - SubagentStop: When a sub-agent stops
//! - PermissionRequest: When permission is requested

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::process::Command;
use tokio::time::{timeout, Duration};
use once_cell::sync::Lazy;

/// Global hook registry
pub static HOOK_REGISTRY: Lazy<Arc<RwLock<HookRegistry>>> =
    Lazy::new(|| Arc::new(RwLock::new(HookRegistry::default())));

/// Hook event types matching JavaScript implementation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum HookType {
    /// When a new session is started
    SessionStart,
    /// When a session is ending
    SessionEnd,
    /// Before a tool is called
    PreToolUse,
    /// After a tool completes successfully
    PostToolUse,
    /// When a tool fails
    PostToolUseFailure,
    /// Before conversation compaction (/compact)
    PreCompact,
    /// When user submits a prompt
    UserPromptSubmit,
    /// For notifications
    Notification,
    /// Stop signal hooks
    Stop,
    /// When a sub-agent starts
    SubagentStart,
    /// When a sub-agent stops
    SubagentStop,
    /// When permission is requested
    PermissionRequest,
}

impl HookType {
    /// Get all hook types
    pub fn all() -> Vec<HookType> {
        vec![
            HookType::SessionStart,
            HookType::SessionEnd,
            HookType::PreToolUse,
            HookType::PostToolUse,
            HookType::PostToolUseFailure,
            HookType::PreCompact,
            HookType::UserPromptSubmit,
            HookType::Notification,
            HookType::Stop,
            HookType::SubagentStart,
            HookType::SubagentStop,
            HookType::PermissionRequest,
        ]
    }
}

/// Hook matcher - determines when a hook should run
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HookMatcher {
    /// Tool names that trigger this hook (for tool-related hooks)
    #[serde(default)]
    pub tools: Option<Vec<String>>,

    /// Additional properties to match
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl HookMatcher {
    /// Check if this matcher matches the given tool name
    pub fn matches_tool(&self, tool_name: &str) -> bool {
        match &self.tools {
            Some(tools) => tools.iter().any(|t| t == tool_name || t == "*"),
            None => true, // No filter means match all
        }
    }
}

/// A single hook command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookCommand {
    /// Hook type (usually "command")
    #[serde(rename = "type")]
    pub hook_type: String,

    /// Command to execute
    pub command: String,

    /// Timeout in milliseconds (default: 30000)
    #[serde(default = "default_timeout")]
    pub timeout: u64,
}

fn default_timeout() -> u64 {
    30000
}

/// A hook entry with matcher and commands
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookEntry {
    /// Matcher to determine when this hook runs
    #[serde(default)]
    pub matcher: HookMatcher,

    /// Commands to execute
    pub hooks: Vec<HookCommand>,

    /// Plugin name this hook belongs to
    #[serde(default)]
    pub plugin_name: Option<String>,
}

/// Result of hook execution
#[derive(Debug, Clone, Default)]
pub struct HookResult {
    /// Whether to suppress tool output
    pub suppress_output: bool,

    /// Whether to stop further hook execution
    pub stop_execution: bool,

    /// System message to display to user
    pub system_message: Option<String>,

    /// Modified tool input (for PreToolUse)
    pub updated_input: Option<serde_json::Value>,

    /// Additional context to add
    pub additional_contexts: Vec<String>,

    /// Reason execution was stopped
    pub stop_reason: Option<String>,

    /// Stdout from hook command
    pub stdout: String,

    /// Stderr from hook command
    pub stderr: String,

    /// Exit code
    pub exit_code: Option<i32>,
}

/// Hook registry - stores all registered hooks by type
#[derive(Debug, Clone, Default)]
pub struct HookRegistry {
    /// Hooks organized by type
    hooks: HashMap<HookType, Vec<HookEntry>>,

    /// Whether hooks are globally disabled
    pub disabled: bool,
}

impl HookRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a hook entry for a specific type
    pub fn register(&mut self, hook_type: HookType, entry: HookEntry) {
        self.hooks.entry(hook_type).or_default().push(entry);
    }

    /// Get all hooks for a specific type
    pub fn get_hooks(&self, hook_type: HookType) -> &[HookEntry] {
        self.hooks.get(&hook_type).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Clear all hooks
    pub fn clear(&mut self) {
        self.hooks.clear();
    }

    /// Load hooks from plugin manifest
    pub fn load_from_plugin(&mut self, hooks_value: &serde_json::Value, plugin_name: &str) {
        if let Some(hooks_obj) = hooks_value.as_object() {
            for (hook_type_str, entries) in hooks_obj {
                // Parse hook type
                let hook_type = match hook_type_str.as_str() {
                    "SessionStart" => HookType::SessionStart,
                    "SessionEnd" => HookType::SessionEnd,
                    "PreToolUse" => HookType::PreToolUse,
                    "PostToolUse" => HookType::PostToolUse,
                    "PostToolUseFailure" => HookType::PostToolUseFailure,
                    "PreCompact" => HookType::PreCompact,
                    "UserPromptSubmit" => HookType::UserPromptSubmit,
                    "Notification" => HookType::Notification,
                    "Stop" => HookType::Stop,
                    "SubagentStart" => HookType::SubagentStart,
                    "SubagentStop" => HookType::SubagentStop,
                    "PermissionRequest" => HookType::PermissionRequest,
                    _ => continue, // Unknown hook type
                };

                // Parse entries array
                if let Some(entries_arr) = entries.as_array() {
                    for entry_value in entries_arr {
                        if let Ok(mut entry) = serde_json::from_value::<HookEntry>(entry_value.clone()) {
                            entry.plugin_name = Some(plugin_name.to_string());
                            self.register(hook_type, entry);
                        }
                    }
                }
            }
        }
    }

    /// Get hook count
    pub fn hook_count(&self) -> usize {
        self.hooks.values().map(|v| v.len()).sum()
    }
}

/// Execute a hook command
pub async fn execute_hook_command(
    command: &HookCommand,
    context: &HookContext,
) -> HookResult {
    let timeout_duration = Duration::from_millis(command.timeout);

    // Build environment variables for the hook
    let mut env_vars = std::env::vars().collect::<HashMap<String, String>>();

    // Add hook context as environment variables
    if let Some(tool_name) = &context.tool_name {
        env_vars.insert("CLAUDE_TOOL_NAME".to_string(), tool_name.clone());
    }
    if let Some(tool_input) = &context.tool_input {
        env_vars.insert("CLAUDE_TOOL_INPUT".to_string(), tool_input.to_string());
    }
    if let Some(tool_output) = &context.tool_output {
        env_vars.insert("CLAUDE_TOOL_OUTPUT".to_string(), tool_output.clone());
    }
    env_vars.insert("CLAUDE_SESSION_ID".to_string(), context.session_id.clone());
    env_vars.insert("CLAUDE_HOOK_TYPE".to_string(), format!("{:?}", context.hook_type));

    // Execute command
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());

    let result = timeout(
        timeout_duration,
        Command::new(&shell)
            .arg("-c")
            .arg(&command.command)
            .envs(env_vars)
            .output()
    ).await;

    match result {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();

            // Parse hook output for special directives
            let mut hook_result = HookResult {
                stdout: stdout.clone(),
                stderr,
                exit_code: output.status.code(),
                ..Default::default()
            };

            // Check for special output patterns
            for line in stdout.lines() {
                if line.starts_with("CLAUDE_STOP_EXECUTION=") {
                    hook_result.stop_execution = line.ends_with("true");
                } else if line.starts_with("CLAUDE_SUPPRESS_OUTPUT=") {
                    hook_result.suppress_output = line.ends_with("true");
                } else if line.starts_with("CLAUDE_SYSTEM_MESSAGE=") {
                    hook_result.system_message = Some(line.trim_start_matches("CLAUDE_SYSTEM_MESSAGE=").to_string());
                } else if line.starts_with("CLAUDE_STOP_REASON=") {
                    hook_result.stop_reason = Some(line.trim_start_matches("CLAUDE_STOP_REASON=").to_string());
                }
            }

            hook_result
        }
        Ok(Err(e)) => {
            HookResult {
                stderr: format!("Failed to execute hook: {}", e),
                exit_code: Some(-1),
                ..Default::default()
            }
        }
        Err(_) => {
            HookResult {
                stderr: format!("Hook timed out after {} ms", command.timeout),
                exit_code: Some(-1),
                stop_execution: false,
                ..Default::default()
            }
        }
    }
}

/// Context passed to hooks
#[derive(Debug, Clone)]
pub struct HookContext {
    pub hook_type: HookType,
    pub session_id: String,
    pub tool_name: Option<String>,
    pub tool_input: Option<serde_json::Value>,
    pub tool_output: Option<String>,
}

impl HookContext {
    pub fn new(hook_type: HookType, session_id: &str) -> Self {
        Self {
            hook_type,
            session_id: session_id.to_string(),
            tool_name: None,
            tool_input: None,
            tool_output: None,
        }
    }

    pub fn with_tool(mut self, name: &str, input: serde_json::Value) -> Self {
        self.tool_name = Some(name.to_string());
        self.tool_input = Some(input);
        self
    }

    pub fn with_output(mut self, output: &str) -> Self {
        self.tool_output = Some(output.to_string());
        self
    }
}

/// Execute all hooks for a given type and context
pub async fn execute_hooks(
    hook_type: HookType,
    context: &HookContext,
) -> Vec<HookResult> {
    let registry = HOOK_REGISTRY.read().await;

    if registry.disabled {
        return vec![];
    }

    let hooks = registry.get_hooks(hook_type);
    let mut results = Vec::new();

    for entry in hooks {
        // Check if matcher applies
        let should_run = match &context.tool_name {
            Some(tool) => entry.matcher.matches_tool(tool),
            None => true,
        };

        if !should_run {
            continue;
        }

        // Execute all commands in this entry
        for command in &entry.hooks {
            let result = execute_hook_command(command, context).await;

            let should_stop = result.stop_execution;
            results.push(result);

            if should_stop {
                return results;
            }
        }
    }

    results
}

/// Register hooks from plugin
pub async fn register_plugin_hooks(hooks_value: &serde_json::Value, plugin_name: &str) {
    let mut registry = HOOK_REGISTRY.write().await;
    registry.load_from_plugin(hooks_value, plugin_name);
}

/// Clear all hooks
pub async fn clear_hooks() {
    let mut registry = HOOK_REGISTRY.write().await;
    registry.clear();
}

/// Get hook count
pub async fn get_hook_count() -> usize {
    let registry = HOOK_REGISTRY.read().await;
    registry.hook_count()
}

/// Disable all hooks
pub async fn disable_hooks() {
    let mut registry = HOOK_REGISTRY.write().await;
    registry.disabled = true;
}

/// Enable all hooks
pub async fn enable_hooks() {
    let mut registry = HOOK_REGISTRY.write().await;
    registry.disabled = false;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_type_all() {
        let all_types = HookType::all();
        assert_eq!(all_types.len(), 12);
    }

    #[test]
    fn test_hook_matcher_matches_tool() {
        let matcher = HookMatcher {
            tools: Some(vec!["Bash".to_string(), "Write".to_string()]),
            extra: HashMap::new(),
        };

        assert!(matcher.matches_tool("Bash"));
        assert!(matcher.matches_tool("Write"));
        assert!(!matcher.matches_tool("Read"));
    }

    #[test]
    fn test_hook_matcher_wildcard() {
        let matcher = HookMatcher {
            tools: Some(vec!["*".to_string()]),
            extra: HashMap::new(),
        };

        assert!(matcher.matches_tool("Bash"));
        assert!(matcher.matches_tool("Read"));
        assert!(matcher.matches_tool("AnythingElse"));
    }

    #[test]
    fn test_hook_matcher_no_filter() {
        let matcher = HookMatcher::default();
        assert!(matcher.matches_tool("Bash"));
        assert!(matcher.matches_tool("Read"));
    }

    #[test]
    fn test_hook_registry() {
        let mut registry = HookRegistry::new();

        let entry = HookEntry {
            matcher: HookMatcher::default(),
            hooks: vec![HookCommand {
                hook_type: "command".to_string(),
                command: "echo test".to_string(),
                timeout: 5000,
            }],
            plugin_name: Some("test-plugin".to_string()),
        };

        registry.register(HookType::PreToolUse, entry);
        assert_eq!(registry.hook_count(), 1);

        let hooks = registry.get_hooks(HookType::PreToolUse);
        assert_eq!(hooks.len(), 1);

        let hooks = registry.get_hooks(HookType::PostToolUse);
        assert_eq!(hooks.len(), 0);
    }

    #[tokio::test]
    async fn test_execute_hook_command_success() {
        let command = HookCommand {
            hook_type: "command".to_string(),
            command: "echo 'hello world'".to_string(),
            timeout: 5000,
        };

        let context = HookContext::new(HookType::PreToolUse, "test-session");
        let result = execute_hook_command(&command, &context).await;

        assert!(result.stdout.contains("hello world"));
        assert_eq!(result.exit_code, Some(0));
    }
}
