/// Permission system for tool execution
/// Based on JavaScript implementation

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use serde::{Deserialize, Serialize};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

/// Permission modes (from JS line 351446)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    /// Default mode - asks for permission
    Default,
    /// Bypass all permissions (requires user opt-in)
    BypassPermissions,
    /// Accept all edits automatically
    AcceptEdits,
    /// Planning mode
    Plan,
}

/// Permission behavior for a specific action
#[derive(Debug, Clone, PartialEq)]
pub enum PermissionBehavior {
    /// Allow the action
    Allow,
    /// Deny the action
    Deny,
    /// Ask the user for permission
    Ask,
    /// Always allow this type of action
    AlwaysAllow,
    /// Never allow this type of action
    Never,
    /// Wait for user to provide feedback
    Wait,
}

/// Permission result after checking
#[derive(Debug, Clone)]
pub struct PermissionResultStruct {
    pub behavior: PermissionBehavior,
    pub message: Option<String>,
    pub allowed_tools: Vec<String>,
}

/// Simple permission result enum for streaming
#[derive(Debug, Clone)]
pub enum PermissionResult {
    Allow,
    Deny,
    NeedsApproval,
}

/// Permission request that needs user input
#[derive(Debug, Clone)]
pub struct PermissionRequest {
    pub id: String,
    pub tool_name: String,
    pub action: String,
    pub details: String,
    pub timestamp: std::time::Instant,
}

/// Permission context for tool execution
#[derive(Debug, Clone)]
pub struct PermissionContext {
    pub mode: PermissionMode,
    pub allowed_commands: Vec<String>,
    pub denied_commands: Vec<String>,
    pub allowed_directories: HashSet<PathBuf>,
    pub always_allow_rules: HashMap<String, Vec<String>>,
    pub always_deny_rules: HashMap<String, Vec<String>>,
    pub bypass_permissions_accepted: bool,
    pub pending_request: Option<PermissionRequest>,
    pub permission_history: Vec<(String, PermissionBehavior)>,
}

impl Default for PermissionContext {
    fn default() -> Self {
        // Get home directory and common development directories
        let mut allowed_directories = HashSet::new();
        
        // Add current working directory
        if let Ok(cwd) = std::env::current_dir() {
            allowed_directories.insert(cwd);
        }
        
        // Add temp directory
        allowed_directories.insert(PathBuf::from("/tmp"));
        
        Self {
            mode: PermissionMode::Default,
            allowed_commands: vec![
                // Safe read-only commands allowed by default
                "ls".to_string(),
                "pwd".to_string(),
                "echo".to_string(),
                "cat".to_string(),
                "grep".to_string(),
                "find".to_string(),
                "which".to_string(),
                "git status".to_string(),
                "git diff".to_string(),
                "git log".to_string(),
            ],
            denied_commands: vec![
                // Dangerous commands denied by default
                "rm -rf /".to_string(),
                "sudo rm".to_string(),
                "dd if=".to_string(),
                "mkfs".to_string(),
                "format".to_string(),
            ],
            allowed_directories,
            always_allow_rules: HashMap::new(),
            always_deny_rules: HashMap::new(),
            bypass_permissions_accepted: false,
            pending_request: None,
            permission_history: Vec::new(),
        }
    }
}

impl PermissionContext {
    /// Add a rule to always allow certain commands for a tool
    pub fn add_always_allow_rule(&mut self, tool_name: &str, pattern: &str) {
        self.always_allow_rules
            .entry(tool_name.to_string())
            .or_insert_with(Vec::new)
            .push(pattern.to_string());
    }
    
    /// Add a rule to always deny certain commands for a tool
    pub fn add_always_deny_rule(&mut self, tool_name: &str, pattern: &str) {
        self.always_deny_rules
            .entry(tool_name.to_string())
            .or_insert_with(Vec::new)
            .push(pattern.to_string());
    }
    
    /// Check if a command is allowed to run
    pub fn check_command(&mut self, command: &str, tool_name: &str) -> PermissionResultStruct {
        
        // In bypass mode, everything is allowed
        if self.mode == PermissionMode::BypassPermissions && self.bypass_permissions_accepted {
            self.permission_history.push((command.to_string(), PermissionBehavior::Allow));
            return PermissionResultStruct {
                behavior: PermissionBehavior::Allow,
                message: None,
                allowed_tools: vec!["all".to_string()],
            };
        }

        // Check always deny rules first
        if let Some(deny_patterns) = self.always_deny_rules.get(tool_name) {
            for pattern in deny_patterns {
                if command.contains(pattern) || pattern == "*" {
                    self.permission_history.push((command.to_string(), PermissionBehavior::Deny));
                    return PermissionResultStruct {
                        behavior: PermissionBehavior::Deny,
                        message: Some(format!("Permission to run '{}' has been permanently denied.", command)),
                        allowed_tools: Vec::new(),
                    };
                }
            }
        }

        // Check if command is explicitly denied
        for denied_prefix in &self.denied_commands {
            if command.starts_with(denied_prefix) {
                self.permission_history.push((command.to_string(), PermissionBehavior::Deny));
                return PermissionResultStruct {
                    behavior: PermissionBehavior::Deny,
                    message: Some(format!("Permission to run '{}' has been denied.", command)),
                    allowed_tools: Vec::new(),
                };
            }
        }

        // Check always allow rules
        if let Some(allow_patterns) = self.always_allow_rules.get(tool_name) {
            for pattern in allow_patterns {
                if command.starts_with(pattern) || pattern == "*" {
                    self.permission_history.push((command.to_string(), PermissionBehavior::Allow));
                    return PermissionResultStruct {
                        behavior: PermissionBehavior::Allow,
                        message: None,
                        allowed_tools: vec![tool_name.to_string()],
                    };
                }
            }
        }

        // Check if command is explicitly allowed
        for allowed_prefix in &self.allowed_commands {
            if command.starts_with(allowed_prefix) {
                self.permission_history.push((command.to_string(), PermissionBehavior::Allow));
                return PermissionResultStruct {
                    behavior: PermissionBehavior::Allow,
                    message: None,
                    allowed_tools: vec![tool_name.to_string()],
                };
            }
        }

        // Check for sandbox mode (read-only commands)
        if is_safe_readonly_command(command) {
            self.permission_history.push((command.to_string(), PermissionBehavior::Allow));
            return PermissionResultStruct {
                behavior: PermissionBehavior::Allow,
                message: None,
                allowed_tools: vec![tool_name.to_string()],
            };
        }

        // Default: ask for permission
        self.pending_request = Some(PermissionRequest {
            id: uuid::Uuid::new_v4().to_string(),
            tool_name: tool_name.to_string(),
            action: "execute command".to_string(),
            details: command.to_string(),
            timestamp: std::time::Instant::now(),
        });

        PermissionResultStruct {
            behavior: PermissionBehavior::Ask,
            message: Some(format!(
                "Claude requested permission to run: {}", 
                command
            )),
            allowed_tools: Vec::new(),
        }
    }

    /// Check if a file operation is allowed
    pub fn check_file_operation(&mut self, path: &Path, operation: FileOperation, tool_name: &str) -> PermissionResultStruct {
        tracing::debug!("DEBUG: Permission check for {} operation on {} by tool {}", 
            operation.as_str(), path.display(), tool_name);
        tracing::debug!("DEBUG: Permission mode: {:?}, allowed directories: {:?}", 
            self.mode, self.allowed_directories);
            
        // In bypass mode, everything is allowed
        if self.mode == PermissionMode::BypassPermissions && self.bypass_permissions_accepted {
            tracing::debug!("DEBUG: Permission granted - bypass mode enabled");
            self.permission_history.push((format!("{:?} {}", operation, path.display()), PermissionBehavior::Allow));
            return PermissionResultStruct {
                behavior: PermissionBehavior::Allow,
                message: None,
                allowed_tools: vec!["all".to_string()],
            };
        }

        // For edit operations in AcceptEdits mode
        if self.mode == PermissionMode::AcceptEdits && operation == FileOperation::Edit {
            tracing::debug!("DEBUG: Permission granted - accept edits mode for edit operation");
            self.permission_history.push((format!("Edit {}", path.display()), PermissionBehavior::Allow));
            return PermissionResultStruct {
                behavior: PermissionBehavior::Allow,
                message: None,
                allowed_tools: vec!["Edit".to_string(), "MultiEdit".to_string()],
            };
        }

        // Check if path is in allowed directories
        for allowed_dir in &self.allowed_directories {
            if path.starts_with(allowed_dir) || path == allowed_dir {
                tracing::debug!("DEBUG: Permission granted - path {} is within allowed directory {}", 
                    path.display(), allowed_dir.display());
                self.permission_history.push((format!("{:?} {}", operation, path.display()), PermissionBehavior::Allow));
                return PermissionResultStruct {
                    behavior: PermissionBehavior::Allow,
                    message: None,
                    allowed_tools: vec![operation.tool_name()],
                };
            }
        }

        // Check if it's a read operation on a safe file
        if operation == FileOperation::Read && is_safe_file_to_read(path) {
            tracing::debug!("DEBUG: Permission granted - safe file read for {}", path.display());
            self.permission_history.push((format!("Read {}", path.display()), PermissionBehavior::Allow));
            return PermissionResultStruct {
                behavior: PermissionBehavior::Allow,
                message: None,
                allowed_tools: vec!["Read".to_string()],
            };
        }

        // Default: ask for permission
        let op_str = match operation {
            FileOperation::Read => "read from",
            FileOperation::Write => "write to",
            FileOperation::Edit => "edit",
            FileOperation::Delete => "delete",
        };

        self.pending_request = Some(PermissionRequest {
            id: uuid::Uuid::new_v4().to_string(),
            tool_name: tool_name.to_string(),
            action: format!("{} file", op_str),
            details: path.display().to_string(),
            timestamp: std::time::Instant::now(),
        });

        PermissionResultStruct {
            behavior: PermissionBehavior::Ask,
            message: Some(format!(
                "Claude requested permission to {} {}", 
                op_str,
                path.display()
            )),
            allowed_tools: Vec::new(),
        }
    }

    /// Process user's permission decision
    pub fn process_permission_decision(&mut self, decision: PermissionBehavior) -> PermissionResultStruct {
        if let Some(request) = &self.pending_request {
            // Record the decision
            self.permission_history.push((request.details.clone(), decision.clone()));

            // Handle "always" and "never" decisions
            match decision {
                PermissionBehavior::AlwaysAllow => {
                    // Add to always allow rules
                    self.always_allow_rules
                        .entry(request.tool_name.clone())
                        .or_insert_with(Vec::new)
                        .push(extract_pattern(&request.details));
                    
                    return PermissionResultStruct {
                        behavior: PermissionBehavior::Allow,
                        message: Some(format!("Always allowing {} for {}", request.action, request.tool_name)),
                        allowed_tools: vec![request.tool_name.clone()],
                    };
                }
                PermissionBehavior::Never => {
                    // Add to always deny rules
                    self.always_deny_rules
                        .entry(request.tool_name.clone())
                        .or_insert_with(Vec::new)
                        .push(extract_pattern(&request.details));
                    
                    return PermissionResultStruct {
                        behavior: PermissionBehavior::Deny,
                        message: Some(format!("Never allowing {} for {}", request.action, request.tool_name)),
                        allowed_tools: Vec::new(),
                    };
                }
                PermissionBehavior::Allow => {
                    return PermissionResultStruct {
                        behavior: PermissionBehavior::Allow,
                        message: None,
                        allowed_tools: vec![request.tool_name.clone()],
                    };
                }
                PermissionBehavior::Deny => {
                    return PermissionResultStruct {
                        behavior: PermissionBehavior::Deny,
                        message: Some("Permission denied".to_string()),
                        allowed_tools: Vec::new(),
                    };
                }
                _ => {}
            }
        }

        // No pending request
        PermissionResultStruct {
            behavior: PermissionBehavior::Deny,
            message: Some("No pending permission request".to_string()),
            allowed_tools: Vec::new(),
        }
    }

    /// Add an allowed command prefix
    pub fn allow_command(&mut self, command_prefix: String) {
        self.allowed_commands.push(command_prefix);
    }

    /// Add a denied command prefix
    pub fn deny_command(&mut self, command_prefix: String) {
        self.denied_commands.push(command_prefix);
    }

    /// Add an allowed directory
    pub fn allow_directory(&mut self, path: PathBuf) {
        self.allowed_directories.insert(path);
    }

    /// Set permission mode
    pub fn set_mode(&mut self, mode: PermissionMode) {
        self.mode = mode;
    }

    /// Clear pending request
    pub fn clear_pending_request(&mut self) {
        self.pending_request = None;
    }
}

/// File operation types
#[derive(Debug, Clone, PartialEq)]
pub enum FileOperation {
    Read,
    Write,
    Edit,
    Delete,
}

impl FileOperation {
    fn tool_name(&self) -> String {
        match self {
            FileOperation::Read => "Read".to_string(),
            FileOperation::Write => "Write".to_string(),
            FileOperation::Edit => "Edit".to_string(),
            FileOperation::Delete => "Delete".to_string(),
        }
    }
    
    fn as_str(&self) -> &'static str {
        match self {
            FileOperation::Read => "read",
            FileOperation::Write => "write", 
            FileOperation::Edit => "edit",
            FileOperation::Delete => "delete",
        }
    }
}

/// Check if a command is safe to run without permission
fn is_safe_readonly_command(command: &str) -> bool {
    let safe_commands = [
        "ls", "pwd", "echo", "date", "whoami", "hostname", 
        "uname", "which", "type", "env", "printenv", "locale",
        "id", "groups", "ps", "top", "df", "du", "free",
        "uptime", "w", "who", "last", "history", "help"
    ];
    
    // Check if command starts with any safe command
    let cmd_parts: Vec<&str> = command.split_whitespace().collect();
    if let Some(base_cmd) = cmd_parts.first() {
        // Remove any path prefix
        let cmd_name = base_cmd.split('/').last().unwrap_or(base_cmd);
        return safe_commands.contains(&cmd_name);
    }
    
    false
}

/// Check if a file is safe to read without permission
fn is_safe_file_to_read(path: &Path) -> bool {
    // Allow reading from current directory and subdirectories
    if let Ok(cwd) = std::env::current_dir() {
        if path.starts_with(&cwd) {
            // But not sensitive files even in current directory
            if let Some(filename) = path.file_name() {
                let name = filename.to_string_lossy();
                if name.starts_with(".env") || 
                   name.contains("secret") || 
                   name.contains("password") ||
                   name.contains("key") ||
                   name == ".git" {
                    return false;
                }
            }
            return true;
        }
    }
    
    // Allow reading common documentation files
    if let Some(ext) = path.extension() {
        let ext_str = ext.to_string_lossy();
        if ext_str == "md" || ext_str == "txt" || ext_str == "json" || 
           ext_str == "toml" || ext_str == "yaml" || ext_str == "yml" {
            return true;
        }
    }
    
    false
}

/// Extract a pattern from a command or path for always/never rules
fn extract_pattern(details: &str) -> String {
    // For commands, use the base command
    if !details.contains('/') || details.contains(' ') {
        let parts: Vec<&str> = details.split_whitespace().collect();
        if let Some(cmd) = parts.first() {
            return cmd.to_string();
        }
    }
    
    // For paths, use the directory
    if let Some(dir) = Path::new(details).parent() {
        return dir.display().to_string();
    }
    
    details.to_string()
}

/// Permission dialog UI component
#[derive(Debug)]
pub struct PermissionDialog {
    pub visible: bool,
    pub request: Option<PermissionRequest>,
    pub selected_option: usize,
    pub options: Vec<PermissionOption>,
}

/// A single permission option
#[derive(Debug, Clone)]
pub struct PermissionOption {
    pub label: String,
    pub value: PermissionBehavior,
    pub key_hint: Option<String>,
}

impl PermissionDialog {
    pub fn new() -> Self {
        Self {
            visible: false,
            request: None,
            selected_option: 0,
            options: Vec::new(),
        }
    }

    /// Show a permission request with context-specific options
    pub fn show(&mut self, request: PermissionRequest) {
        // Generate context-specific options based on the request
        self.options = self.generate_options(&request);
        self.request = Some(request);
        self.visible = true;
        self.selected_option = 0;
    }

    /// Generate context-specific options based on the request (like JavaScript AF function)
    fn generate_options(&self, request: &PermissionRequest) -> Vec<PermissionOption> {
        let mut options = Vec::new();
        
        // Option 1: Yes (allow once)
        options.push(PermissionOption {
            label: "Yes".to_string(),
            value: PermissionBehavior::Allow,
            key_hint: Some("1".to_string()),
        });
        
        // Option 2: Context-specific "don't ask again" option
        let dont_ask_label = match request.tool_name.as_str() {
            "Bash" => {
                // For Bash commands, extract the base command
                let base_cmd = request.details.split_whitespace()
                    .next()
                    .unwrap_or(&request.details);
                format!("Yes, allow all '{}' commands this session", base_cmd)
            }
            "Edit" | "MultiEdit" | "Write" => {
                // For file operations, use directory
                if let Some(dir) = std::path::Path::new(&request.details).parent() {
                    format!("Yes, allow all edits in {} this session", dir.display())
                } else {
                    "Yes, allow all edits this session".to_string()
                }
            }
            "Read" => {
                if let Some(dir) = std::path::Path::new(&request.details).parent() {
                    format!("Yes, allow reading all files in {} this session", dir.display())
                } else {
                    "Yes, allow all file reads this session".to_string()
                }
            }
            _ => format!("Yes, and don't ask again this session"),
        };
        
        options.push(PermissionOption {
            label: dont_ask_label,
            value: PermissionBehavior::AlwaysAllow,
            key_hint: Some("2 or shift+tab".to_string()),
        });
        
        // Option 3: No, and provide feedback
        options.push(PermissionOption {
            label: "No, and tell Claude what to do differently".to_string(),
            value: PermissionBehavior::Wait,
            key_hint: Some("3 or esc".to_string()),
        });
        
        options
    }

    /// Hide the dialog
    pub fn hide(&mut self) {
        self.visible = false;
        self.request = None;
        self.options.clear();
    }

    /// Handle key input (matching JavaScript behavior)
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<PermissionBehavior> {
        if !self.visible || self.options.is_empty() {
            return None;
        }

        match key.code {
            // Number keys for quick selection
            KeyCode::Char('1') => {
                if self.options.len() > 0 {
                    Some(self.options[0].value.clone())
                } else {
                    None
                }
            }
            KeyCode::Char('2') => {
                if self.options.len() > 1 {
                    Some(self.options[1].value.clone())
                } else {
                    None
                }
            }
            KeyCode::Char('3') => {
                if self.options.len() > 2 {
                    Some(self.options[2].value.clone())
                } else {
                    None
                }
            }
            // Tab with shift for "don't ask again"
            KeyCode::BackTab | KeyCode::Tab if key.modifiers.contains(KeyModifiers::SHIFT) => {
                if self.options.len() > 1 {
                    Some(self.options[1].value.clone())
                } else {
                    None
                }
            }
            // Escape for deny
            KeyCode::Esc => {
                // Don't hide here - let the caller do it
                if self.options.len() > 2 {
                    return Some(self.options[2].value.clone());
                }
                return Some(PermissionBehavior::Deny);
            }
            // Arrow keys for selection
            KeyCode::Left => {
                if self.selected_option > 0 {
                    self.selected_option -= 1;
                }
                None
            }
            KeyCode::Right => {
                if self.selected_option < self.options.len() - 1 {
                    self.selected_option += 1;
                }
                None
            }
            // Enter to confirm selection
            KeyCode::Enter => {
                if self.selected_option < self.options.len() {
                    // Clone the value but DON'T hide here - let the caller do it
                    let selected_value = self.options[self.selected_option].value.clone();
                    Some(selected_value)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Render the permission dialog
    pub fn render(&self, f: &mut Frame, area: Rect) {
        if !self.visible || self.request.is_none() {
            return;
        }

        let request = self.request.as_ref().unwrap();

        // Create a centered popup
        let popup_area = centered_rect(60, 40, area);

        // Clear the background
        f.render_widget(Clear, popup_area);

        // Create the dialog content
        let title = format!(" 🔒 Permission Request - {} ", request.tool_name);
        
        let mut lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::raw("Action: "),
                Span::styled(&request.action, Style::default().fg(Color::Yellow)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::raw("Details: "),
            ]),
            Line::from(vec![
                Span::styled(&request.details, Style::default().fg(Color::Cyan)),
            ]),
            Line::from(""),
            Line::from("─".repeat(50)),
            Line::from(""),
        ];

        // Add the actual generated options with highlighting
        for (idx, option) in self.options.iter().enumerate() {
            let style = if idx == self.selected_option {
                Style::default().fg(Color::Black).bg(Color::White)
            } else {
                Style::default()
            };
            
            // Include key hint if available
            let label_text = if let Some(ref hint) = option.key_hint {
                format!("[{}] {}", hint, option.label)
            } else {
                format!("[{}] {}", idx + 1, option.label)
            };
            
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(label_text, style),
            ]));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::raw("Use "),
            Span::styled("←→", Style::default().fg(Color::Yellow)),
            Span::raw(" to select, "),
            Span::styled("Enter", Style::default().fg(Color::Yellow)),
            Span::raw(" to confirm, "),
            Span::styled("Esc", Style::default().fg(Color::Yellow)),
            Span::raw(" to deny"),
        ]));

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow));

        let paragraph = Paragraph::new(lines)
            .block(block)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });

        f.render_widget(paragraph, popup_area);
    }
}

/// Create a centered rectangle
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

/// Global permission context (shared across the application)
pub static PERMISSION_CONTEXT: once_cell::sync::Lazy<Arc<Mutex<PermissionContext>>> = 
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(PermissionContext::default())));

/// Simple async function to check command permission for streaming
pub async fn check_command_permission(command: &str) -> PermissionResult {
    let mut ctx = PERMISSION_CONTEXT.lock().await;
    let result = ctx.check_command(command, "Bash");
    
    match result.behavior {
        PermissionBehavior::Allow | PermissionBehavior::AlwaysAllow => PermissionResult::Allow,
        PermissionBehavior::Deny | PermissionBehavior::Never => PermissionResult::Deny,
        PermissionBehavior::Ask => PermissionResult::NeedsApproval,
        PermissionBehavior::Wait => PermissionResult::NeedsApproval, // Wait requires user approval
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_permissions() {
        let mut ctx = PermissionContext::default();
        
        // Test safe command - should be allowed
        let result = ctx.check_command("ls -la", "Bash");
        assert_eq!(result.behavior, PermissionBehavior::Allow);
        
        // Test dangerous command - should be denied
        ctx.deny_command("rm -rf".to_string());
        let result = ctx.check_command("rm -rf /home", "Bash");
        assert_eq!(result.behavior, PermissionBehavior::Deny);
        
        // Test unknown command - should ask
        let result = ctx.check_command("some_unknown_command", "Bash");
        assert_eq!(result.behavior, PermissionBehavior::Ask);
    }

    #[test]
    fn test_bypass_mode() {
        let mut ctx = PermissionContext::default();
        ctx.mode = PermissionMode::BypassPermissions;
        ctx.bypass_permissions_accepted = true;
        
        // Everything should be allowed
        let result = ctx.check_command("rm -rf /", "Bash");
        assert_eq!(result.behavior, PermissionBehavior::Allow);
    }

    #[test]
    fn test_file_permissions() {
        let mut ctx = PermissionContext::default();
        
        // Test file in current directory - should be allowed for read
        if let Ok(cwd) = std::env::current_dir() {
            let test_file = cwd.join("test.txt");
            let result = ctx.check_file_operation(&test_file, FileOperation::Read, "Read");
            assert_eq!(result.behavior, PermissionBehavior::Allow);
        }
        
        // Test sensitive file - should ask even in current directory
        if let Ok(cwd) = std::env::current_dir() {
            let env_file = cwd.join(".env");
            let result = ctx.check_file_operation(&env_file, FileOperation::Read, "Read");
            assert_eq!(result.behavior, PermissionBehavior::Ask);
        }
        
        // Test system file - should ask
        let result = ctx.check_file_operation(
            &PathBuf::from("/etc/passwd"),
            FileOperation::Write,
            "Write"
        );
        assert_eq!(result.behavior, PermissionBehavior::Ask);
    }

    #[test]
    fn test_always_rules() {
        let mut ctx = PermissionContext::default();
        
        // Add always allow rule
        ctx.always_allow_rules.insert("Bash".to_string(), vec!["npm".to_string()]);
        let result = ctx.check_command("npm install", "Bash");
        assert_eq!(result.behavior, PermissionBehavior::Allow);
        
        // Add always deny rule
        ctx.always_deny_rules.insert("Bash".to_string(), vec!["curl".to_string()]);
        let result = ctx.check_command("curl http://example.com", "Bash");
        assert_eq!(result.behavior, PermissionBehavior::Deny);
    }

    #[test]
    fn test_safe_readonly_commands() {
        assert!(is_safe_readonly_command("ls"));
        assert!(is_safe_readonly_command("ls -la"));
        assert!(is_safe_readonly_command("pwd"));
        assert!(is_safe_readonly_command("/bin/ls"));
        assert!(!is_safe_readonly_command("rm"));
        assert!(!is_safe_readonly_command("curl"));
    }

    #[test]
    fn test_extract_pattern() {
        assert_eq!(extract_pattern("npm install"), "npm");
        assert_eq!(extract_pattern("git commit -m 'test'"), "git");
        assert_eq!(extract_pattern("/home/user/file.txt"), "/home/user");
        assert_eq!(extract_pattern("rm -rf /"), "rm");
    }
}