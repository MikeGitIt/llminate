use crate::ai::{ContentPart, Tool};
use crate::ai::agent_tool::AgentTool;
use crate::ai::todo_tool::{TodoWriteTool, TodoReadTool};
use crate::ai::web_tools::{WebFetchTool, WebSearchTool};
use crate::ai::notebook_tools::{NotebookReadTool, NotebookEditTool};
use crate::ai::exit_plan_mode_tool::ExitPlanModeTool;
use crate::error::{Error, Result};
use crate::tui::{TuiEvent, PermissionDecision};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tokio::fs as async_fs;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::time;
use regex::Regex;
use glob::glob;
use which::which;
use std::env;
use std::time::SystemTime;
use tokio::process::{Child, Command as TokioCommand};
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;
use std::sync::Arc;
use once_cell::sync::Lazy;
use rand::Rng;

/// Tool execution context (mirrors JavaScript's context with AbortController)
pub struct ToolContext {
    pub tool_use_id: String,
    pub event_tx: Option<mpsc::UnboundedSender<TuiEvent>>,
    pub cancellation_token: Option<CancellationToken>,  // Like JavaScript's AbortSignal
}

/// Shell session state management (like JavaScript implementation)
#[derive(Debug, Clone)]
pub struct ShellSessionState {
    working_dir: PathBuf,
    original_working_dir: PathBuf,
    shell_executable: String,
    is_sandboxed: bool,
    env_vars: HashMap<String, String>,
    advanced_persistence: bool, // If true, persist shell variables; if false, only persist working directory like JS
    additional_working_directories: HashSet<PathBuf>, // Set of allowed directories (like JS)
}

impl ShellSessionState {
    fn new(shell_executable: Option<String>, working_dir: Option<PathBuf>, is_sandboxed: bool) -> Self {
        let shell = shell_executable.unwrap_or_else(|| "/bin/bash".to_string());
        
        // Get the actual working directory from PWD environment variable (like JavaScript)
        // This is where the user is actually running commands from
        let pwd_from_env = std::env::var("PWD")
            .ok()
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));
        
        let original_cwd = pwd_from_env.clone();
        let cwd = working_dir.unwrap_or_else(|| original_cwd.clone());
        
        // Start with original working directory in the allowed set (like JS)
        let mut additional_dirs = HashSet::new();
        additional_dirs.insert(original_cwd.clone());
        
        Self {
            working_dir: cwd,
            original_working_dir: original_cwd,
            shell_executable: shell,
            is_sandboxed,
            env_vars: HashMap::new(),
            advanced_persistence: false, // Default to JS-compatible mode
            additional_working_directories: additional_dirs,
        }
    }
    
    /// Execute command with state persistence like JavaScript implementation
    async fn execute_command(&mut self, command: &str, timeout_ms: u64, additional_env: Option<&HashMap<String, String>>, cancellation_token: Option<CancellationToken>) -> Result<(String, String, i32)> {
        // Create temporary file for tracking working directory like JavaScript
        let temp_dir = std::env::temp_dir();
        let random_id = rand::thread_rng().gen::<u32>();
        let cwd_file = temp_dir.join(format!("claude-{}-cwd", random_id));
        
        // Build command chain like JavaScript implementation:
        // 1. Source shell config
        // 2. Set environment variables 
        // 3. Change to working directory
        // 4. Execute command
        // 5. Track new working directory
        let mut command_chain = Vec::new();
        
        // Source shell configuration files
        if self.shell_executable.contains("bash") {
            command_chain.push("source ~/.bashrc 2>/dev/null || true".to_string());
        } else if self.shell_executable.contains("zsh") {
            command_chain.push("source ~/.zshrc 2>/dev/null || true".to_string());
        }
        
        // Handle environment variables based on persistence mode
        if self.advanced_persistence {
            // Advanced mode: Persist shell variables between commands
            for (key, value) in &self.env_vars {
                // Use export and handle potential readonly variables with || true
                command_chain.push(format!("export {}={} 2>/dev/null || true", key, shell_words::quote(value)));
            }
        }
        
        // Always set additional environment variables from input
        if let Some(env) = additional_env {
            for (key, value) in env {
                command_chain.push(format!("export {}={}", key, shell_words::quote(value)));
            }
        }
        
        // Change to working directory - make failure explicit
        command_chain.push(format!("cd {} || {{ echo 'ERROR: Failed to change to directory: {}' >&2; exit 1; }}", 
            shell_words::quote(&self.working_dir.to_string_lossy()),
            self.working_dir.to_string_lossy()));
        
        let eval_command = format!("{} < /dev/null", command);
        let quoted_eval = shell_words::quote(&eval_command);
        command_chain.push(format!("eval {}", quoted_eval));
        
        // Always track working directory after command (both modes need this)
        command_chain.push(format!("pwd -P >| {}", shell_words::quote(&cwd_file.to_string_lossy())));
        
        // Only capture environment/shell variables in advanced persistence mode
        let env_file = if self.advanced_persistence {
            let env_file = temp_dir.join(format!("claude-{}-env", random_id));
            // Use 'set' to capture all shell variables, then filter for exports
            command_chain.push(format!("(set; echo '---EXPORTS---'; env) > {}", shell_words::quote(&env_file.to_string_lossy())));
            Some(env_file)
        } else {
            None
        };
        
        // Join commands with && like JavaScript
        let full_command = command_chain.join(" && ");
        
        // Execute using spawn like JavaScript
        let mut cmd = if self.is_sandboxed && cfg!(target_os = "macos") {
            self.create_sandboxed_command(&full_command).await?
        } else {
            let mut cmd = TokioCommand::new(&self.shell_executable);
            cmd.args(&["-c", "-l", &full_command]);
            cmd
        };
        
        // Set working directory and environment
        cmd.current_dir(&self.working_dir)
            .env("SHELL", &self.shell_executable)
            .env("GIT_EDITOR", "true")
            .env("CLAUDECODE", "1")
            // Disable color output and TTY detection to prevent terminal corruption
            .env("NO_COLOR", "1")
            .env("TERM", "dumb")
            .env("CARGO_TERM_COLOR", "never")
            .env("RUST_LOG_STYLE", "never")
            .env("CLICOLOR", "0")
            .env("CLICOLOR_FORCE", "0")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        
        // Execute with timeout and cancellation support
        let start = std::time::Instant::now();
        let timeout = tokio::time::Duration::from_millis(timeout_ms);
        
        let mut child = cmd.spawn()?;
        
        // Execute with cancellation support (like JavaScript's AbortSignal)
        let output = if let Some(token) = cancellation_token {
            // We need to handle cancellation properly
            // Get the child's stdout and stderr
            let stdout = child.stdout.take().ok_or_else(|| Error::Process("Failed to capture stdout".to_string()))?;
            let stderr = child.stderr.take().ok_or_else(|| Error::Process("Failed to capture stderr".to_string()))?;
            
            // Read output while monitoring for cancellation
            let output_future = async {
                use tokio::io::AsyncReadExt;
                let mut stdout_buf = Vec::new();
                let mut stderr_buf = Vec::new();
                
                let mut stdout_reader = tokio::io::BufReader::new(stdout);
                let mut stderr_reader = tokio::io::BufReader::new(stderr);
                
                // Read both stdout and stderr
                let (stdout_result, stderr_result) = tokio::join!(
                    stdout_reader.read_to_end(&mut stdout_buf),
                    stderr_reader.read_to_end(&mut stderr_buf)
                );
                
                stdout_result.map_err(|e| Error::Process(format!("Failed to read stdout: {}", e)))?;
                stderr_result.map_err(|e| Error::Process(format!("Failed to read stderr: {}", e)))?;
                
                // Wait for the child to exit
                let status = child.wait().await
                    .map_err(|e| Error::Process(format!("Failed to wait for child: {}", e)))?;
                
                Ok::<_, Error>(std::process::Output {
                    status,
                    stdout: stdout_buf,
                    stderr: stderr_buf,
                })
            };
            
            tokio::select! {
                result = tokio::time::timeout(timeout, output_future) => {
                    result.map_err(|_| Error::Timeout(format!("Command timed out after {}ms", timeout_ms)))??
                }
                _ = token.cancelled() => {
                    // Kill the child process when cancelled
                    child.kill().await.map_err(|e| Error::Process(format!("Failed to kill process: {}", e)))?;
                    return Err(Error::Cancelled("Operation cancelled by user".to_string()));
                }
            }
        } else {
            tokio::time::timeout(timeout, child.wait_with_output()).await
                .map_err(|_| Error::Timeout(format!("Command timed out after {}ms", timeout_ms)))?
                .map_err(|e| Error::Process(format!("Command execution failed: {}", e)))?
        };
        
        // Update working directory from temp file
        if let Ok(new_cwd) = std::fs::read_to_string(&cwd_file) {
            let new_cwd = new_cwd.trim();
            if !new_cwd.is_empty() {
                self.working_dir = PathBuf::from(new_cwd);
            }
        }
        
        // Update environment variables from temp file (only in advanced persistence mode)
        if let Some(env_file) = env_file {
            if let Ok(env_content) = std::fs::read_to_string(&env_file) {
                // Clear old env vars and parse new ones
                self.env_vars.clear();
                
                // Split content into shell variables (set) and environment variables (env)
                let parts: Vec<&str> = env_content.split("---EXPORTS---").collect();
                
                // Process shell variables from 'set' command
                if let Some(set_output) = parts.get(0) {
                    for line in set_output.lines() {
                        if let Some((key, value)) = line.split_once('=') {
                            // Skip functions, special variables, and system variables
                            if !key.starts_with('_') && 
                               !key.contains(' ') && 
                               !key.starts_with("BASH_") && 
                               !matches!(key, "PWD" | "OLDPWD" | "SHLVL" | "PS1" | "PS2" | "IFS" | "PATH" | "HOME" | "USER" | "SHELL" | "PPID" | "$" | "?" | "!" | "-" | "EUID" | "UID" | "BASHOPTS" | "SHELLOPTS") &&
                               key.chars().all(|c| c.is_alphanumeric() || c == '_') &&
                               !value.starts_with('(') { // Skip functions
                                self.env_vars.insert(key.to_string(), value.to_string());
                            }
                        }
                    }
                }
                
                // Also process environment variables from 'env' command for exported vars
                if let Some(env_output) = parts.get(1) {
                    for line in env_output.lines() {
                        if let Some((key, value)) = line.split_once('=') {
                            // Only include custom environment variables
                            if !matches!(key, "_" | "PWD" | "OLDPWD" | "SHLVL" | "PS1" | "PS2" | "IFS" | "PATH" | "HOME" | "USER" | "SHELL" | "TERM" | "LANG" | "LC_ALL" | "PPID" | "EUID" | "UID" | "BASHOPTS" | "SHELLOPTS") &&
                               key.chars().all(|c| c.is_alphanumeric() || c == '_') {
                                self.env_vars.insert(key.to_string(), value.to_string());
                            }
                        }
                    }
                }
            }
            
            // Cleanup temp file
            let _ = std::fs::remove_file(&env_file);
        }
        
        // Cleanup working directory temp file
        let _ = std::fs::remove_file(&cwd_file);
        
        // Convert output
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(-1);
        
        Ok((stdout, stderr, exit_code))
    }
    
    async fn create_sandboxed_command(&self, command: &str) -> Result<TokioCommand> {
        // Create sandbox profile like JavaScript Class18
        let random_hex: String = (0..16).map(|_| format!("{:x}", rand::thread_rng().gen_range(0..16))).collect();
        let profile_path = format!("/tmp/claude-sandbox-{}.sb", random_hex);
        
        // Create a sandbox profile that allows file operations in the working directory
        // and other necessary paths, but restricts access to sensitive areas
        let sandbox_profile = r#"(version 1)
(deny default)
(allow file-read*)
(allow file-write*)
(allow file-read-metadata)
(allow file-write-create)
(allow file-write-unlink)
(allow sysctl-read)
(allow mach-lookup)
(allow process-exec)
(allow process-fork)
(allow signal (target children))
(allow signal (target self))"#;
        
        std::fs::write(&profile_path, sandbox_profile)?;
        
        let mut cmd = TokioCommand::new("/usr/bin/sandbox-exec");
        cmd.args(&["-f", &profile_path, &self.shell_executable, "-c", command]);
        
        Ok(cmd)
    }
    
    fn set_env_var(&mut self, key: String, value: String) {
        self.env_vars.insert(key, value);
    }
    
    /// Get the current working directory (like JavaScript cA() function)
    fn get_working_dir(&self) -> &PathBuf {
        &self.working_dir
    }
    
    /// Get the original working directory (like JavaScript helperFunc1())
    fn get_original_working_dir(&self) -> &PathBuf {
        &self.original_working_dir
    }
    
    /// Set advanced persistence mode
    fn set_advanced_persistence(&mut self, enabled: bool) {
        self.advanced_persistence = enabled;
    }
    
    /// Add a directory to the additional working directories set
    fn add_working_directory(&mut self, path: PathBuf) {
        self.additional_working_directories.insert(path);
    }
    
    /// Remove a directory from the additional working directories set
    fn remove_working_directory(&mut self, path: &Path) -> bool {
        self.additional_working_directories.remove(path)
    }
    
    /// Get the additional working directories
    fn get_additional_working_directories(&self) -> &HashSet<PathBuf> {
        &self.additional_working_directories
    }
    
    /// Check if a path is within allowed working directories
    fn is_path_allowed(&self, path: &Path) -> bool {
        // Normalize path
        let normalized = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.working_dir.join(path)
        };
        
        // Check if path is within any allowed directory
        for allowed_dir in &self.additional_working_directories {
            if normalized.starts_with(allowed_dir) {
                return true;
            }
        }
        
        // Also check against original working directory
        normalized.starts_with(&self.original_working_dir)
    }
}

/// Permission context for tools (matches JavaScript lz() function)
#[derive(Debug, Clone)]
pub struct ToolPermissionContext {
    pub mode: String, // "default", "acceptEdits", or "bypassPermissions"
    pub additional_working_directories: HashSet<PathBuf>,
    pub always_allow_rules: HashMap<String, serde_json::Value>,
    pub always_deny_rules: HashMap<String, serde_json::Value>,
    pub is_bypass_permissions_mode_available: bool,
}

impl Default for ToolPermissionContext {
    fn default() -> Self {
        // Matches JavaScript lz() function exactly
        Self {
            mode: "default".to_string(),
            additional_working_directories: HashSet::new(),
            always_allow_rules: HashMap::new(),
            always_deny_rules: HashMap::new(),
            is_bypass_permissions_mode_available: false,
        }
    }
}

/// Global shell session state manager (like JavaScript)
static SHELL_SESSIONS: Lazy<Arc<Mutex<HashMap<String, ShellSessionState>>>> = 
    Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

/// Background shell structure (like JavaScript Class46)
#[derive(Debug, Clone)]
pub struct BackgroundShell {
    pub id: String,
    pub command: String,
    pub status: String, // "running", "completed", "failed", "killed"
    pub start_time: u64,
    pub stdout: Arc<Mutex<String>>,
    pub stderr: Arc<Mutex<String>>,
    pub exit_code: Option<i32>,
    pub process: Option<Arc<Mutex<tokio::process::Child>>>,
}

impl BackgroundShell {
    fn new(id: String, command: String) -> Self {
        Self {
            id,
            command,
            status: "running".to_string(),
            start_time: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            stdout: Arc::new(Mutex::new(String::new())),
            stderr: Arc::new(Mutex::new(String::new())),
            exit_code: None,
            process: None,
        }
    }
    
    async fn get_output(&self) -> (String, String) {
        let stdout = self.stdout.lock().await.clone();
        let stderr = self.stderr.lock().await.clone();
        // Clear the buffers after reading (like JavaScript getOutput())
        self.stdout.lock().await.clear();
        self.stderr.lock().await.clear();
        (stdout, stderr)
    }
    
    async fn has_new_output(&self) -> bool {
        !self.stdout.lock().await.is_empty()
    }
    
    async fn kill(&mut self) -> Result<()> {
        if let Some(process_arc) = &self.process {
            let mut process = process_arc.lock().await;
            process.kill().await?;
            self.status = "killed".to_string();
        }
        Ok(())
    }
}

/// Background shell manager (like JavaScript Ju class)
pub struct BackgroundShellManager {
    shells: Arc<Mutex<HashMap<String, BackgroundShell>>>,
    shell_counter: Arc<Mutex<u32>>,
}

impl BackgroundShellManager {
    pub fn new() -> Self {
        Self {
            shells: Arc::new(Mutex::new(HashMap::new())),
            shell_counter: Arc::new(Mutex::new(0)),
        }
    }
    
    pub async fn add_background_shell(&self, shell: BackgroundShell) -> String {
        let id = shell.id.clone();
        self.shells.lock().await.insert(id.clone(), shell);
        id
    }
    
    pub async fn get_shell(&self, id: &str) -> Option<BackgroundShell> {
        self.shells.lock().await.get(id).cloned()
    }
    
    pub async fn get_shell_output(&self, id: &str) -> serde_json::Value {
        if let Some(shell) = self.get_shell(id).await {
            let (stdout, stderr) = shell.get_output().await;
            json!({
                "shellId": id,
                "command": shell.command,
                "status": shell.status,
                "exitCode": shell.exit_code,
                "stdout": stdout.trim_end(),
                "stderr": stderr.trim_end()
            })
        } else {
            json!({
                "shellId": id,
                "command": "",
                "status": "failed",
                "exitCode": null,
                "stdout": "",
                "stderr": "Shell not found"
            })
        }
    }
    
    pub async fn get_active_shells(&self) -> Vec<BackgroundShell> {
        self.shells.lock().await
            .values()
            .cloned()
            .collect()
    }
    
    pub async fn kill_shell(&self, id: &str) -> Result<bool> {
        if let Some(mut shell) = self.shells.lock().await.get_mut(id) {
            shell.kill().await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
    
    pub async fn generate_shell_id(&self) -> String {
        let mut counter = self.shell_counter.lock().await;
        *counter += 1;
        format!("bash_{}", counter)
    }
}

/// Global background shell manager
pub static BACKGROUND_SHELLS: Lazy<BackgroundShellManager> = 
    Lazy::new(|| BackgroundShellManager::new());

/// Tool executor
pub struct ToolExecutor {
    tools: HashMap<String, Box<dyn ToolHandler>>,
    allowed_tools: Vec<String>,
    disallowed_tools: Vec<String>,
    permission_handler: Box<dyn PermissionHandler>,
}

impl ToolExecutor {
    /// Create a new tool executor
    pub fn new() -> Self {
        let mut tools: HashMap<String, Box<dyn ToolHandler>> = HashMap::new();
        
        // Register built-in tools with names matching JavaScript implementation
        tools.insert("Read".to_string(), Box::new(ReadFileTool));
        tools.insert("Write".to_string(), Box::new(WriteFileTool));
        tools.insert("Edit".to_string(), Box::new(EditFileTool));
        tools.insert("MultiEdit".to_string(), Box::new(FileMultiEditTool));
        tools.insert("LS".to_string(), Box::new(ListFilesTool));
        tools.insert("Search".to_string(), Box::new(SearchFilesTool));
        tools.insert("Grep".to_string(), Box::new(GrepTool));
        tools.insert("Glob".to_string(), Box::new(GlobTool));
        tools.insert("Bash".to_string(), Box::new(BashTool));
        tools.insert("HttpRequest".to_string(), Box::new(HttpRequestTool));
        tools.insert("Task".to_string(), Box::new(AgentTool));
        tools.insert("TodoWrite".to_string(), Box::new(TodoWriteTool));
        tools.insert("TodoRead".to_string(), Box::new(TodoReadTool));
        tools.insert("WebFetch".to_string(), Box::new(WebFetchTool));
        tools.insert("WebSearch".to_string(), Box::new(WebSearchTool));
        tools.insert("NotebookRead".to_string(), Box::new(NotebookReadTool));
        tools.insert("NotebookEdit".to_string(), Box::new(NotebookEditTool));
        tools.insert("ExitPlanMode".to_string(), Box::new(ExitPlanModeTool));
        tools.insert("BashOutput".to_string(), Box::new(BashOutputTool));
        tools.insert("KillBash".to_string(), Box::new(KillBashTool));
        
        Self {
            tools,
            allowed_tools: Vec::new(),
            disallowed_tools: Vec::new(),
            permission_handler: Box::new(DefaultPermissionHandler),
        }
    }
    
    /// Set allowed tools
    pub fn set_allowed_tools(&mut self, tools: Vec<String>) {
        self.allowed_tools = tools;
    }
    
    /// Set disallowed tools
    pub fn set_disallowed_tools(&mut self, tools: Vec<String>) {
        self.disallowed_tools = tools;
    }
    
    /// Set permission handler
    pub fn set_permission_handler(&mut self, handler: Box<dyn PermissionHandler>) {
        self.permission_handler = handler;
    }
    
    /// Register a custom tool
    pub fn register_tool(&mut self, name: String, handler: Box<dyn ToolHandler>) {
        self.tools.insert(name, handler);
    }
    
    // OLD FLOW REMOVED: execute_bash_with_suspension
    // Permissions are now handled entirely in the streaming flow in state.rs
    
    /// Get available tools
    pub fn get_available_tools(&self) -> Vec<Tool> {
        self.tools
            .iter()
            .filter(|(name, _)| self.is_tool_allowed(name))
            .map(|(name, handler)| Tool::Standard {
                name: name.clone(),
                description: handler.description(),
                input_schema: handler.input_schema(),
            })
            .collect()
    }
    
    /// Check if a tool is allowed
    pub fn is_tool_allowed(&self, name: &str) -> bool {
        // Check disallowed list first
        if self.disallowed_tools.contains(&name.to_string()) {
            return false;
        }
        
        // If allowed list is empty, all tools are allowed
        if self.allowed_tools.is_empty() {
            return true;
        }
        
        // Check allowed list
        self.allowed_tools.contains(&name.to_string())
    }
    
    /// Execute a tool
    pub async fn execute(&self, name: &str, input: serde_json::Value) -> Result<ContentPart> {
        // For backward compatibility, execute without context
        self.execute_with_context(name, input, None).await
    }
    
    pub async fn execute_with_context(&self, name: &str, input: serde_json::Value, context: Option<ToolContext>) -> Result<ContentPart> {
        // Check if tool exists
        let handler = self
            .tools
            .get(name)
            .ok_or_else(|| Error::ToolNotFound(name.to_string()))?;
        
        // Check if tool is allowed
        if !self.is_tool_allowed(name) {
            return Err(Error::ToolNotAllowed(name.to_string()));
        }
        
        // Permission handling for Bash is now done entirely in the streaming flow in state.rs
        // No special handling needed here - just execute the tool normally
        
        // Extract cancellation token from context (like JavaScript's AbortSignal)
        let cancellation_token = context.as_ref().and_then(|ctx| ctx.cancellation_token.clone());
        
        // Execute tool with cancellation support
        let result = handler.execute(input.clone(), cancellation_token).await?;
        
        // Special handling for TodoWrite - notify TUI to update TODO display
        if name == "TodoWrite" {
            if let Some(context) = &context {
                if let Some(event_tx) = &context.event_tx {
                    // Parse the todos from the input to send to TUI
                    if let Some(todos_array) = input["todos"].as_array() {
                        let mut todos: Vec<crate::ai::todo_tool::Todo> = Vec::new();
                        for todo_value in todos_array {
                            if let Ok(todo) = serde_json::from_value::<crate::ai::todo_tool::Todo>(todo_value.clone()) {
                                todos.push(todo);
                            }
                        }
                        let _ = event_tx.send(crate::tui::TuiEvent::TodosUpdated(todos));
                    }
                }
            }
        }
        
        Ok(ContentPart::ToolResult {
            tool_use_id: uuid::Uuid::new_v4().to_string(),
            content: result,
            is_error: None,
        })
    }
}

/// Tool handler trait
#[async_trait::async_trait]
pub trait ToolHandler: Send + Sync {
    /// Get tool description
    fn description(&self) -> String;
    
    /// Get input schema
    fn input_schema(&self) -> serde_json::Value;
    
    /// Get action description for permission check
    fn action_description(&self, input: &serde_json::Value) -> String;
    
    /// Get permission details
    fn permission_details(&self, input: &serde_json::Value) -> String;
    
    /// Execute the tool with cancellation support
    async fn execute(&self, input: serde_json::Value, cancellation_token: Option<CancellationToken>) -> Result<String>;
}

/// Permission handler trait
#[async_trait::async_trait]
pub trait PermissionHandler: Send + Sync {
    /// Check if permission is granted
    async fn check_permission(&self, context: &PermissionContext) -> Result<bool>;
}

/// Permission context
#[derive(Debug, Clone)]
pub struct PermissionContext {
    pub tool_name: String,
    pub action: String,
    pub details: String,
}

/// Default permission handler (always allow)
struct DefaultPermissionHandler;

#[async_trait::async_trait]
impl PermissionHandler for DefaultPermissionHandler {
    async fn check_permission(&self, _context: &PermissionContext) -> Result<bool> {
        Ok(true)
    }
}

/// Read file tool
struct ReadFileTool;

#[async_trait::async_trait]
impl ToolHandler for ReadFileTool {
    fn description(&self) -> String {
        "Reads a file from the local filesystem. You can access any file directly by using this tool.
Assume this tool is able to read all files on the machine. If the User provides a path to a file assume that path is valid. It is okay to read a file that does not exist; an error will be returned.

Usage:
- The file_path parameter must be an absolute path, not a relative path
- By default, it reads up to 2000 lines starting from the beginning of the file
- You can optionally specify a line offset and limit (especially handy for long files), but it's recommended to read the whole file by not providing these parameters
- Any lines longer than 2000 characters will be truncated
- Results are returned using cat -n format, with line numbers starting at 1
- This tool allows Claude Code to read images (eg PNG, JPG, etc). When reading an image file the contents are presented visually as Claude Code is a multimodal LLM.
- This tool can read PDF files (.pdf). PDFs are processed page by page, extracting both text and visual content for analysis.
- This tool can read Jupyter notebooks (.ipynb files) and returns all cells with their outputs, combining code, text, and visualizations.
- This tool can only read files, not directories. To read a directory, use an ls command via the Bash tool.
- You have the capability to call multiple tools in a single response. It is always better to speculatively read multiple files as a batch that are potentially useful. 
- You will regularly be asked to read screenshots. If the user provides a path to a screenshot ALWAYS use this tool to view the file at the path. This tool will work with all temporary file paths like /var/folders/123/abc/T/TemporaryItems/NSIRD_screencaptureui_ZfB1tD/Screenshot.png
- If you read a file that exists but has empty contents you will receive a system reminder warning in place of file contents.".to_string()
    }
    
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The absolute path to the file to read"
                },
                "offset": {
                    "type": "number",
                    "description": "The line number to start reading from. Only provide if the file is too large to read at once"
                },
                "limit": {
                    "type": "number",
                    "description": "The number of lines to read. Only provide if the file is too large to read at once."
                }
            },
            "required": ["file_path"]
        })
    }
    
    fn action_description(&self, input: &serde_json::Value) -> String {
        format!("Read file: {}", input["file_path"].as_str().unwrap_or("<unknown>"))
    }
    
    fn permission_details(&self, input: &serde_json::Value) -> String {
        format!("Path: {}", input["file_path"].as_str().unwrap_or("<unknown>"))
    }
    
    async fn execute(&self, input: serde_json::Value, cancellation_token: Option<CancellationToken>) -> Result<String> {
        use crate::permissions::{PERMISSION_CONTEXT, PermissionBehavior, FileOperation};
        
        tracing::debug!("ReadFileTool::execute called");
        
        let path = input["file_path"]
            .as_str()
            .ok_or_else(|| Error::InvalidInput("Missing 'file_path' field".to_string()))?;
        
        let offset = input["offset"].as_u64().map(|n| n as usize).unwrap_or(1);
        let limit = input["limit"].as_u64().map(|n| n as usize);
        
        tracing::debug!("DEBUG: Reading file: {}, offset: {}, limit: {:?}", path, offset, limit);
        
        let file_path = Path::new(path);
        
        // Check permissions before file access
        tracing::debug!("DEBUG: Checking permissions for read operation on: {}", path);
        {
            let mut ctx = PERMISSION_CONTEXT.lock().await;
            let perm_result = ctx.check_file_operation(file_path, FileOperation::Read, "Read");
            tracing::debug!("DEBUG: Permission check for {}: {:?}", path, perm_result.behavior);
            match perm_result.behavior {
                PermissionBehavior::Deny | PermissionBehavior::Never => {
                    tracing::warn!("DEBUG: Permission denied for file read: {}", path);
                    return Err(Error::PermissionDenied(format!("Permission denied to read file: {}", path)));
                },
                PermissionBehavior::Ask => {
                    tracing::debug!("DEBUG: Permission required for file read: {}", path);
                    return Err(Error::PermissionDenied(format!("Permission required to read file: {} (use /add-dir to allow directory access)", path)));
                },
                _ => {
                    tracing::debug!("DEBUG: Permission granted for file read: {}", path);
                }
            }
        }
        
        // Check if file exists (matching JavaScript validateInput behavior)
        if !file_path.exists() {
            tracing::warn!("DEBUG: File not found: {}", path);

            // Build error message like JavaScript (includes cwd if different, suggests similar files)
            let mut error_msg = "File does not exist.".to_string();

            // Add current working directory info if different from home
            if let Ok(cwd) = std::env::current_dir() {
                if let Some(home) = dirs::home_dir() {
                    if cwd != home {
                        error_msg.push_str(&format!(" Current working directory: {}", cwd.display()));
                    }
                }
            }

            // Suggest similar files (like JavaScript variable3138)
            if let Some(parent) = file_path.parent() {
                if parent.exists() {
                    if let Some(file_name) = file_path.file_name().and_then(|n| n.to_str()) {
                        let file_stem = file_path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                        if let Ok(entries) = std::fs::read_dir(parent) {
                            for entry in entries.filter_map(|e| e.ok()) {
                                let entry_name = entry.file_name();
                                if let Some(entry_str) = entry_name.to_str() {
                                    // Check if entry has same stem but different extension or similar name
                                    let entry_stem = Path::new(entry_str)
                                        .file_stem()
                                        .and_then(|s| s.to_str())
                                        .unwrap_or("");
                                    if entry_stem == file_stem && entry_str != file_name {
                                        error_msg.push_str(&format!(" Did you mean {}?", entry_str));
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            return Err(Error::NotFound(error_msg));
        }

        tracing::debug!("DEBUG: File exists: {}", path);

        // Check file size before reading (JavaScript num54 at line 381419)
        const MAX_FILE_SIZE: u64 = 262_144; // 256 KB
        let metadata = async_fs::metadata(path).await?;
        let file_size = metadata.len();

        if file_size > MAX_FILE_SIZE {
            let size_kb = (file_size as f64) / 1024.0;
            let max_kb = (MAX_FILE_SIZE as f64) / 1024.0;
            return Err(Error::InvalidInput(
                format!("File \"{}\" is too large: {:.0}KB (max: {:.0}KB)", path, size_kb, max_kb)
            ));
        }

        // Get file extension for type detection
        let extension = file_path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();

        // Image extensions - ENHANCED: Rust supports more formats than JavaScript (which only has png, jpg, jpeg, gif, webp)
        // Note: SVG is NOT included because it's text-based XML and should be read as text
        let image_extensions: HashSet<&str> = [
            // JavaScript-supported
            "png", "jpg", "jpeg", "gif", "webp",
            // Rust enhancements
            "bmp", "ico", "tiff", "tif", "heic", "heif", "avif",
        ].iter().cloned().collect();
        let is_image = image_extensions.contains(extension.as_str());

        // Binary extensions - reject these explicitly (matches JavaScript variable11447)
        let binary_extensions: HashSet<&str> = [
            // Audio
            "mp3", "wav", "flac", "ogg", "aac", "m4a", "wma", "aiff", "opus",
            // Video
            "mp4", "avi", "mov", "wmv", "flv", "mkv", "webm", "m4v", "mpeg", "mpg",
            // Archives
            "zip", "rar", "tar", "gz", "bz2", "7z", "xz", "z", "tgz", "iso",
            // Executables/Libraries
            "exe", "dll", "so", "dylib", "app", "msi", "deb", "rpm",
            // Binary data
            "bin", "dat", "db", "sqlite", "sqlite3",
            // Other binary formats
            "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx",
            "class", "pyc", "pyo", "o", "obj", "a", "lib",
            "ttf", "otf", "woff", "woff2", "eot",
            "cur", "icns",
            "psd", "ai", "eps",
            "dmg", "pkg", "apk", "ipa",
        ].iter().cloned().collect();

        // Check for binary file extension first (like JavaScript validateInput)
        if binary_extensions.contains(extension.as_str()) && extension != "pdf" {
            return Err(Error::InvalidInput(format!(
                "This tool cannot read binary files. The file appears to be a binary .{} file. Please use appropriate tools for binary file analysis.",
                extension
            )));
        }

        if is_image {
            // Check for empty image files (like JavaScript errorCode: 5)
            if file_size == 0 {
                return Err(Error::InvalidInput("Empty image files cannot be processed.".to_string()));
            }

            // For images, read as binary and encode as base64 for display
            let bytes = async_fs::read(path).await?;
            let base64_data = base64::encode(&bytes);

            // Determine MIME type - ENHANCED: Rust supports more image formats
            let mime_type = match extension.as_str() {
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                "gif" => "image/gif",
                "webp" => "image/webp",
                "bmp" => "image/bmp",
                "ico" => "image/x-icon",
                "tiff" | "tif" => "image/tiff",
                "heic" | "heif" => "image/heic",
                "avif" => "image/avif",
                _ => "image/unknown"
            };

            // Return base64 encoded image data with metadata
            return Ok(format!(
                "<image>\npath: {}\ntype: {}\nsize: {} bytes\ndata: data:{};base64,{}\n</image>",
                path, mime_type, bytes.len(), mime_type, base64_data
            ));
        }
        
        // Check if this is a binary file by trying to read as UTF-8
        let content = match async_fs::read_to_string(path).await {
            Ok(text) => text,
            Err(_) => {
                // If it fails to read as UTF-8, it's likely a binary file
                let bytes = async_fs::read(path).await?;
                return Ok(format!(
                    "[Binary file: {} ({} bytes)]\n\nThis appears to be a binary file that cannot be displayed as text.",
                    path, bytes.len()
                ));
            }
        };
        
        let all_lines: Vec<&str> = content.split('\n').collect();
        let total_lines = all_lines.len();

        // Convert 1-based offset to 0-based index (matching JavaScript: offset === 0 ? 0 : offset - 1)
        let start_index = if offset == 0 { 0 } else { offset - 1 };

        // Get the slice of lines based on offset and limit
        let selected_lines: Vec<&str> = if let Some(limit_val) = limit {
            if total_lines <= start_index {
                Vec::new()
            } else if total_lines - start_index > limit_val {
                all_lines[start_index..start_index + limit_val].to_vec()
            } else {
                all_lines[start_index..].to_vec()
            }
        } else {
            if start_index >= total_lines {
                Vec::new()
            } else {
                all_lines[start_index..].to_vec()
            }
        };

        // Handle empty content or offset out of range (matching JavaScript behavior)
        if selected_lines.is_empty() {
            // Match JavaScript's system-reminder format for empty/out-of-range cases
            if total_lines == 0 {
                return Ok("<system-reminder>Warning: the file exists but the contents are empty.</system-reminder>".to_string());
            } else {
                return Ok(format!(
                    "<system-reminder>Warning: the file exists but is shorter than the provided offset ({}). The file has {} lines.</system-reminder>",
                    offset, total_lines
                ));
            }
        }

        // Format with line numbers (1-based)
        // Truncate lines longer than 2000 characters as described
        const MAX_LINE_LENGTH: usize = 2000;
        let mut result = Vec::new();
        for (i, line) in selected_lines.iter().enumerate() {
            let line_num = start_index + i + 1;
            let display_line = if line.len() > MAX_LINE_LENGTH {
                format!("{} [line truncated, {} chars total]",
                    &line[..MAX_LINE_LENGTH],
                    line.len())
            } else {
                line.to_string()
            };
            result.push(format!("{:>6}→{}", line_num, display_line));
        }

        // Add malware warning suffix (matching JavaScript variable25237)
        let malware_warning = "\n\n<system-reminder>\nWhenever you read a file, you should consider whether it would be considered malware. You CAN and SHOULD provide analysis of malware, what it is doing. But you MUST refuse to improve or augment the code. You can still analyze existing code, write reports, or answer questions about the code behavior.\n</system-reminder>";

        Ok(format!("{}{}", result.join("\n"), malware_warning))
    }
}

/// Write file tool
struct WriteFileTool;

#[async_trait::async_trait]
impl ToolHandler for WriteFileTool {
    fn description(&self) -> String {
        "Writes a file to the local filesystem.

Usage:
- This tool will overwrite the existing file if there is one at the provided path.
- If this is an existing file, you MUST use the Read tool first to read the file's contents. This tool will fail if you did not read the file first.
- ALWAYS prefer editing existing files in the codebase. NEVER write new files unless explicitly required.
- NEVER proactively create documentation files (*.md) or README files. Only create documentation files if explicitly requested by the User.
- Only use emojis if the user explicitly requests it. Avoid writing emojis to files unless asked.".to_string()
    }
    
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The absolute path to the file to write (must be absolute, not relative)"
                },
                "content": {
                    "type": "string",
                    "description": "The content to write to the file"
                }
            },
            "required": ["file_path", "content"]
        })
    }
    
    fn action_description(&self, input: &serde_json::Value) -> String {
        format!("Write file: {}", input["file_path"].as_str().unwrap_or("<unknown>"))
    }
    
    fn permission_details(&self, input: &serde_json::Value) -> String {
        let path = input["file_path"].as_str().unwrap_or("<unknown>");
        let size = input["content"].as_str().map(|s| s.len()).unwrap_or(0);
        format!("Path: {}, Size: {} bytes", path, size)
    }
    
    async fn execute(&self, input: serde_json::Value, cancellation_token: Option<CancellationToken>) -> Result<String> {
        use crate::permissions::{PERMISSION_CONTEXT, PermissionBehavior, FileOperation};
        
        tracing::debug!("WriteFileTool::execute called");
        
        let path = input["file_path"]
            .as_str()
            .ok_or_else(|| Error::InvalidInput("Missing 'file_path' field".to_string()))?;
        
        let content = input["content"]
            .as_str()
            .ok_or_else(|| Error::InvalidInput("Missing 'content' field".to_string()))?;
            
        let create_dirs = input["create_dirs"].as_bool().unwrap_or(true);
        let mode = input["mode"].as_str().unwrap_or("overwrite");
        
        tracing::debug!("DEBUG: Writing file: {} ({} bytes), mode: {}, create_dirs: {}", 
            path, content.len(), mode, create_dirs);
        
        let path_obj = Path::new(path);
        
        // Check permissions before file access
        tracing::debug!("DEBUG: Checking permissions for write operation on: {}", path);
        {
            let mut ctx = PERMISSION_CONTEXT.lock().await;
            let perm_result = ctx.check_file_operation(path_obj, FileOperation::Write, "Write");
            tracing::debug!("DEBUG: Permission check for {}: {:?}", path, perm_result.behavior);
            match perm_result.behavior {
                PermissionBehavior::Deny | PermissionBehavior::Never => {
                    tracing::warn!("DEBUG: Permission denied for file write: {}", path);
                    return Err(Error::PermissionDenied(format!("Permission denied to write file: {}", path)));
                },
                PermissionBehavior::Ask => {
                    tracing::debug!("DEBUG: Permission required for file write: {}", path);
                    return Err(Error::PermissionDenied(format!("Permission required to write file: {} (use /add-dir to allow directory access)", path)));
                },
                _ => {
                    tracing::debug!("DEBUG: Permission granted for file write: {}", path);
                }
            }
        }
        
        // Validate mode
        if mode != "overwrite" && mode != "append" {
            tracing::error!("DEBUG: Invalid write mode specified: {}", mode);
            return Err(Error::InvalidInput(format!("Invalid mode: {}. Must be 'overwrite' or 'append'", mode)));
        }
        
        // Check if file exists for safety
        let exists = path_obj.exists();
        tracing::debug!("DEBUG: File exists check for {}: {}", path, exists);
        
        // Read existing content if overwriting for diff display
        let old_content = if exists && mode == "overwrite" {
            match async_fs::read_to_string(path).await {
                Ok(content) => {
                    tracing::debug!("DEBUG: Successfully read existing content from {} ({} bytes)", path, content.len());
                    Some(content)
                },
                Err(e) => {
                    tracing::debug!("DEBUG: Failed to read existing content from {}: {}", path, e);
                    None
                }
            }
        } else {
            None
        };
        
        // Create parent directory if needed
        if create_dirs {
            if let Some(parent) = path_obj.parent() {
                tracing::debug!("DEBUG: Creating parent directories for: {:?}", parent);
                match async_fs::create_dir_all(parent).await {
                    Ok(_) => tracing::debug!("DEBUG: Successfully created parent directories for: {:?}", parent),
                    Err(e) => {
                        tracing::error!("DEBUG: Failed to create parent directories for {:?}: {}", parent, e);
                        return Err(Error::from(e));
                    }
                }
            }
        }
        
        // Write or append to file
        if mode == "append" {
            tracing::debug!("DEBUG: Appending {} bytes to file: {}", content.len(), path);
            let mut file = async_fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .await
                .map_err(|e| {
                    tracing::error!("DEBUG: Failed to open file for append {}: {}", path, e);
                    e
                })?;
            file.write_all(content.as_bytes()).await
                .map_err(|e| {
                    tracing::error!("DEBUG: Failed to write content to file {}: {}", path, e);
                    e
                })?;
            let result = format!("Appended to file: {} ({} bytes)", path, content.len());
            tracing::info!("DEBUG: File operation append successful: {}", result);
            Ok(result)
        } else {
            tracing::debug!("DEBUG: Overwriting file: {} with {} bytes", path, content.len());
            match async_fs::write(path, &content).await {
                Ok(_) => tracing::info!("DEBUG: File write successful: {} ({} bytes)", path, content.len()),
                Err(e) => {
                    tracing::error!("DEBUG: File write failed for {}: {}", path, e);
                    return Err(Error::from(e));
                }
            }
            
            // Generate diff if we overwrote existing content
            if let Some(old) = old_content {
                tracing::debug!("DEBUG: Generating diff for overwritten file: {}", path);
                let diff = crate::ai::diff_display::DiffDisplay::new(
                    old,
                    content.to_string(),
                    path.to_string()
                );
                
                let summary = diff.summary();
                let inline_diff = diff.inline_diff();
                
                // Return summary with compact diff
                if !inline_diff.is_empty() && inline_diff.len() < 500 {
                    tracing::debug!("DEBUG: Including inline diff in response ({} chars)", inline_diff.len());
                    Ok(format!("{}\n\n{}", summary, inline_diff))
                } else {
                    tracing::debug!("DEBUG: Inline diff too large ({} chars), returning summary only", inline_diff.len());
                    Ok(summary)
                }
            } else {
                let result = format!("Created file: {} ({} bytes)", path, content.len());
                tracing::info!("DEBUG: File operation create successful: {}", result);
                Ok(result)
            }
        }
    }
}

/// List files tool
struct ListFilesTool;

#[async_trait::async_trait]
impl ToolHandler for ListFilesTool {
    fn description(&self) -> String {
        "List files in a directory".to_string()
    }
    
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "The absolute path to the directory to list (must be absolute, not relative)"
                },
                "ignore": {
                    "type": "array",
                    "items": {
                        "type": "string"
                    },
                    "description": "List of glob patterns to ignore"
                }
            },
            "required": ["path"]
        })
    }
    
    fn action_description(&self, input: &serde_json::Value) -> String {
        format!("List files in: {}", input["path"].as_str().unwrap_or("<unknown>"))
    }
    
    fn permission_details(&self, input: &serde_json::Value) -> String {
        let path = input["path"].as_str().unwrap_or("<unknown>");
        let ignore_count = input["ignore"].as_array().map(|a| a.len()).unwrap_or(0);
        format!("Path: {}, Ignore patterns: {}", path, ignore_count)
    }
    
    async fn execute(&self, input: serde_json::Value, cancellation_token: Option<CancellationToken>) -> Result<String> {
        use crate::permissions::{PERMISSION_CONTEXT, PermissionBehavior, FileOperation};
        
        let path = input["path"]
            .as_str()
            .ok_or_else(|| Error::InvalidInput("Missing 'path' field".to_string()))?;
        
        // Ensure path is absolute
        let path = Path::new(path);
        if !path.is_absolute() {
            return Err(Error::InvalidInput("Path must be absolute".to_string()));
        }
        
        // Check permissions before directory access
        {
            let mut ctx = PERMISSION_CONTEXT.lock().await;
            let perm_result = ctx.check_file_operation(path, FileOperation::Read, "LS");
            match perm_result.behavior {
                PermissionBehavior::Deny | PermissionBehavior::Never => {
                    return Err(Error::PermissionDenied(format!("Permission denied to list directory: {}", path.display())));
                },
                PermissionBehavior::Ask => {
                    return Err(Error::PermissionDenied(format!("Permission required to list directory: {} (use /add-dir to allow directory access)", path.display())));
                },
                _ => {} // Allow or AlwaysAllow - proceed
            }
        }
        
        // Check if directory exists
        if !path.exists() {
            return Err(Error::NotFound(format!("Directory not found: {}", path.display())));
        }
        
        if !path.is_dir() {
            return Err(Error::InvalidInput(format!("Path is not a directory: {}", path.display())));
        }
        
        // Get ignore patterns
        let ignore_patterns: Vec<String> = input["ignore"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default();
        
        // Create glob patterns for ignore
        let mut ignore_globs = Vec::new();
        for pattern in &ignore_patterns {
            if let Ok(glob_pattern) = glob::Pattern::new(pattern) {
                ignore_globs.push(glob_pattern);
            }
        }
        
        // List entries
        let mut entries = Vec::new();
        let read_dir = fs::read_dir(path)?;
        
        for entry in read_dir {
            let entry = entry?;
            let entry_path = entry.path();
            let file_name = entry.file_name().to_string_lossy().to_string();
            
            // Check if should ignore
            let should_ignore = ignore_globs.iter().any(|pattern| {
                pattern.matches(&file_name) || 
                pattern.matches(&entry_path.to_string_lossy())
            });
            
            if should_ignore {
                continue;
            }
            
            // Get file type and metadata
            let metadata = entry.metadata()?;
            let file_type = if metadata.is_dir() {
                "directory"
            } else if metadata.is_symlink() {
                "symlink"
            } else {
                "file"
            };
            
            // Format entry similar to JavaScript output
            let size = if metadata.is_file() {
                format!(", {} bytes", metadata.len())
            } else {
                String::new()
            };
            
            entries.push(format!("{} ({}{})", file_name, file_type, size));
        }
        
        // Sort entries: directories first, then files
        entries.sort_by(|a, b| {
            let a_is_dir = a.contains("(directory");
            let b_is_dir = b.contains("(directory");
            match (a_is_dir, b_is_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.cmp(b),
            }
        });
        
        if entries.is_empty() {
            Ok("No files or directories found".to_string())
        } else {
            Ok(entries.join("\n"))
        }
    }
}

/// Search files tool
struct SearchFilesTool;

#[async_trait::async_trait]
impl ToolHandler for SearchFilesTool {
    fn description(&self) -> String {
        "Search for files containing a pattern".to_string()
    }
    
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to search in"
                },
                "pattern": {
                    "type": "string",
                    "description": "Pattern to search for"
                },
                "file_pattern": {
                    "type": "string",
                    "description": "File name pattern (glob)",
                    "default": "*"
                }
            },
            "required": ["path", "pattern"]
        })
    }
    
    fn action_description(&self, input: &serde_json::Value) -> String {
        format!(
            "Search for '{}' in {}",
            input["pattern"].as_str().unwrap_or("<unknown>"),
            input["path"].as_str().unwrap_or("<unknown>")
        )
    }
    
    fn permission_details(&self, input: &serde_json::Value) -> String {
        format!(
            "Path: {}, Pattern: {}, File pattern: {}",
            input["path"].as_str().unwrap_or("<unknown>"),
            input["pattern"].as_str().unwrap_or("<unknown>"),
            input["file_pattern"].as_str().unwrap_or("*")
        )
    }
    
    async fn execute(&self, input: serde_json::Value, cancellation_token: Option<CancellationToken>) -> Result<String> {
        use crate::permissions::{PERMISSION_CONTEXT, PermissionBehavior, FileOperation};
        
        let path = input["path"]
            .as_str()
            .ok_or_else(|| Error::InvalidInput("Missing 'path' field".to_string()))?;
        
        let path_obj = Path::new(path);
        
        // Check permissions before directory access
        {
            let mut ctx = PERMISSION_CONTEXT.lock().await;
            let perm_result = ctx.check_file_operation(path_obj, FileOperation::Read, "Search");
            match perm_result.behavior {
                PermissionBehavior::Deny | PermissionBehavior::Never => {
                    return Err(Error::PermissionDenied(format!("Permission denied to search in: {}", path)));
                },
                PermissionBehavior::Ask => {
                    return Err(Error::PermissionDenied(format!("Permission required to search in: {} (use /add-dir to allow directory access)", path)));
                },
                _ => {} // Allow or AlwaysAllow - proceed
            }
        }
        
        let pattern = input["pattern"]
            .as_str()
            .ok_or_else(|| Error::InvalidInput("Missing 'pattern' field".to_string()))?;
        
        let file_pattern = input["file_pattern"].as_str().unwrap_or("*");
        
        // Use ripgrep if available
        let output = Command::new("rg")
            .args(&[
                "--files-with-matches",
                "--glob",
                file_pattern,
                pattern,
                path,
            ])
            .output();
        
        match output {
            Ok(output) => {
                if output.status.success() {
                    Ok(String::from_utf8_lossy(&output.stdout).to_string())
                } else {
                    Ok("No matches found".to_string())
                }
            }
            Err(_) => {
                // Fallback to basic search
                let mut matches = Vec::new();
                search_files_recursive(Path::new(path), pattern, file_pattern, &mut matches)?;
                
                if matches.is_empty() {
                    Ok("No matches found".to_string())
                } else {
                    Ok(matches.join("\n"))
                }
            }
        }
    }
}

/// Grep tool - powerful search built on ripgrep
pub struct GrepTool;

#[async_trait::async_trait]
impl ToolHandler for GrepTool {
    fn description(&self) -> String {
        "- Fast content search tool that works with any codebase size\n- Searches file contents using regular expressions\n- Supports full regex syntax (eg. \"log.*Error\", \"function\\s+\\w+\", etc.)\n- Filter files by pattern with the include parameter (eg. \"*.js\", \"*.{ts,tsx}\")\n- Returns file paths with at least one match sorted by modification time\n- Use this tool when you need to find files containing specific patterns\n- When you are doing an open ended search that may require multiple rounds of globbing and grepping, use the Agent tool instead\n".to_string()
    }
    
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "The regular expression pattern to search for"
                },
                "path": {
                    "type": "string",
                    "description": "The directory to search in. Defaults to the current working directory."
                },
                "include": {
                    "type": "string",
                    "description": "File pattern to include in the search (e.g. \"*.js\", \"*.{ts,tsx}\")"
                },
                "glob": {
                    "type": "string",
                    "description": "Glob pattern to filter files (e.g. '*.js', '**/*.tsx')"
                },
                "type": {
                    "type": "string",
                    "description": "File type to search (e.g. 'js', 'py', 'rust')"
                },
                "output_mode": {
                    "type": "string",
                    "enum": ["content", "files_with_matches", "count"],
                    "description": "Output mode (default: 'files_with_matches')"
                },
                "-i": {
                    "type": "boolean",
                    "description": "Case insensitive search"
                },
                "-n": {
                    "type": "boolean",
                    "description": "Show line numbers (only with output_mode: 'content')"
                },
                "-A": {
                    "type": "number",
                    "description": "Lines to show after match (only with output_mode: 'content')"
                },
                "-B": {
                    "type": "number",
                    "description": "Lines to show before match (only with output_mode: 'content')"
                },
                "-C": {
                    "type": "number",
                    "description": "Lines to show before and after match (only with output_mode: 'content')"
                },
                "multiline": {
                    "type": "boolean",
                    "description": "Enable multiline mode where . matches newlines"
                },
                "head_limit": {
                    "type": "number",
                    "description": "Limit output to first N lines/entries"
                }
            },
            "required": ["pattern"]
        })
    }
    
    fn action_description(&self, input: &serde_json::Value) -> String {
        let pattern = input["pattern"].as_str().unwrap_or("<unknown>");
        let path = input["path"].as_str().unwrap_or(".");
        format!("Search for '{}' in {}", pattern, path)
    }
    
    fn permission_details(&self, input: &serde_json::Value) -> String {
        let pattern = input["pattern"].as_str().unwrap_or("<unknown>");
        let path = input["path"].as_str().unwrap_or(".");
        let mode = input["output_mode"].as_str().unwrap_or("files_with_matches");
        format!("Pattern: {}, Path: {}, Mode: {}", pattern, path, mode)
    }
    
    async fn execute(&self, input: serde_json::Value, cancellation_token: Option<CancellationToken>) -> Result<String> {
        use crate::permissions::{PERMISSION_CONTEXT, PermissionBehavior, FileOperation};
        
        let pattern = input["pattern"]
            .as_str()
            .ok_or_else(|| Error::InvalidInput("Missing 'pattern' field".to_string()))?;
            
        let path = input["path"].as_str().unwrap_or(".");
        let path_obj = Path::new(path);
        
        // Check permissions before directory access
        {
            let mut ctx = PERMISSION_CONTEXT.lock().await;
            let perm_result = ctx.check_file_operation(path_obj, FileOperation::Read, "Grep");
            match perm_result.behavior {
                PermissionBehavior::Deny | PermissionBehavior::Never => {
                    return Err(Error::PermissionDenied(format!("Permission denied to search in: {}", path)));
                },
                PermissionBehavior::Ask => {
                    return Err(Error::PermissionDenied(format!("Permission required to search in: {} (use /add-dir to allow directory access)", path)));
                },
                _ => {} // Allow or AlwaysAllow - proceed
            }
        }
        
        let output_mode = input["output_mode"].as_str().unwrap_or("files_with_matches");
        
        // Check if ripgrep is available
        if which("rg").is_err() {
            return Err(Error::ToolExecution("ripgrep (rg) is not installed. Please install ripgrep to use the Grep tool.".to_string()));
        }
        
        let mut cmd = Command::new("rg");
        
        // JavaScript implementation uses these specific flags: -Uli --multiline-dotall
        cmd.arg("-l"); // files with matches (default behavior)
        cmd.arg("-i"); // case insensitive (JavaScript uses -i)
        cmd.arg("-U").arg("--multiline-dotall"); // multiline mode (JavaScript default)
        
        // Add pattern
        cmd.arg(pattern);
        
        // Handle include parameter (JavaScript's file pattern filter) - EXACT MATCH TO JS
        if let Some(include) = input["include"].as_str() {
            // JavaScript implementation splits by whitespace and handles patterns with {} specially
            let mut patterns: Vec<String> = Vec::new();
            let parts: Vec<&str> = include.split_whitespace().collect();
            
            for part in parts {
                if part.contains('{') && part.contains('}') {
                    // Pattern with braces like "*.{ts,tsx}" - keep as-is
                    patterns.push(part.to_string());
                } else {
                    // Split by comma if present
                    for p in part.split(',').filter(|s| !s.is_empty()) {
                        patterns.push(p.to_string());
                    }
                }
            }
            
            for pattern in patterns {
                cmd.arg("--glob").arg(pattern);
            }
        }
        
        // Handle enhanced features (keep these as improvements)
        
        // Handle output mode (enhancement)
        if output_mode != "files_with_matches" {
            match output_mode {
                "count" => {
                    cmd.arg("--count");
                }
                "content" => {
                    // Remove -l flag for content mode
                    cmd.args(&["-U", "--multiline-dotall", pattern]);
                    let mut new_cmd = Command::new("rg");
                    new_cmd.args(&["-U", "--multiline-dotall"]);
                    
                    // Reapply case insensitive if needed
                    if input["-i"].as_bool().unwrap_or(true) {
                        new_cmd.arg("-i");
                    }
                    
                    // Add pattern
                    new_cmd.arg(pattern);
                    
                    cmd = new_cmd;
                }
                _ => {
                    return Err(Error::InvalidInput(format!("Invalid output_mode: {}", output_mode)));
                }
            }
        }
        
        // Additional optional flags (enhancements)
        if output_mode == "content" {
            if input["-n"].as_bool().unwrap_or(false) {
                cmd.arg("-n");
            }
            
            if let Some(after) = input["-A"].as_u64() {
                cmd.arg("-A").arg(after.to_string());
            }
            
            if let Some(before) = input["-B"].as_u64() {
                cmd.arg("-B").arg(before.to_string());
            }
            
            if let Some(context) = input["-C"].as_u64() {
                cmd.arg("-C").arg(context.to_string());
            }
        }
        
        // Override multiline setting if explicitly set to false
        if let Some(false) = input["multiline"].as_bool() {
            // Need to rebuild command without -U flag
            let mut new_cmd = Command::new("rg");
            if output_mode == "files_with_matches" {
                new_cmd.arg("-l");
            }
            if input["-i"].as_bool().unwrap_or(true) {
                new_cmd.arg("-i");
            }
            new_cmd.arg(pattern);
            cmd = new_cmd;
        }
        
        // Additional glob patterns (enhancement)
        if let Some(glob_pattern) = input["glob"].as_str() {
            cmd.arg("--glob").arg(glob_pattern);
        }
        
        // File type filter (enhancement)
        if let Some(file_type) = input["type"].as_str() {
            cmd.arg("--type").arg(file_type);
        }
        
        // Add path
        cmd.arg(path);
        
        // Execute command
        let output = cmd.output()?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("No files were searched") {
                return Ok("No files found".to_string());
            }
            if output.status.code() == Some(1) {
                // Exit code 1 means no matches found
                return Ok("No files found".to_string());
            }
            return Err(Error::ToolExecution(format!("ripgrep failed: {}", stderr)));
        }
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        
        // For files_with_matches mode (default), return results sorted by modification time
        if output_mode == "files_with_matches" {
            let files: Vec<&str> = stdout.lines().collect();
            
            if files.is_empty() {
                return Ok("No files found".to_string());
            }
            
            // Sort by modification time (newest first) like JavaScript
            let mut file_stats: Vec<(String, SystemTime)> = Vec::new();
            for file in files {
                match fs::metadata(file) {
                    Ok(metadata) => {
                        if let Ok(modified) = metadata.modified() {
                            file_stats.push((file.to_string(), modified));
                        } else {
                            file_stats.push((file.to_string(), SystemTime::UNIX_EPOCH));
                        }
                    }
                    Err(_) => {
                        // If we can't get metadata, use epoch time
                        file_stats.push((file.to_string(), SystemTime::UNIX_EPOCH));
                    }
                }
            }
            
            // Sort by modification time (newest first)
            file_stats.sort_by(|a, b| b.1.cmp(&a.1));
            
            // Apply head_limit if specified
            let limit = input["head_limit"].as_u64().unwrap_or(100) as usize;
            let truncated = file_stats.len() > limit;
            let files_to_show: Vec<String> = file_stats
                .into_iter()
                .take(limit)
                .map(|(file, _)| file)
                .collect();
            
            // Format output similar to JavaScript
            let mut result = format!("Found {} file{}\n", 
                files_to_show.len(),
                if files_to_show.len() == 1 { "" } else { "s" }
            );
            result.push_str(&files_to_show.join("\n"));
            
            if truncated {
                result.push_str("\n(Results are truncated. Consider using a more specific path or pattern.)");
            }
            
            Ok(result)
        } else {
            // For other modes, apply head_limit if specified
            let mut result = stdout.to_string();
            
            if let Some(limit) = input["head_limit"].as_u64() {
                let lines: Vec<&str> = result.lines().take(limit as usize).collect();
                if lines.len() == limit as usize && result.lines().count() > limit as usize {
                    result = format!("{}\n\n[Output limited to first {} entries]", lines.join("\n"), limit);
                } else {
                    result = lines.join("\n");
                }
            }
            
            if result.is_empty() {
                Ok("No matches found.".to_string())
            } else {
                Ok(result)
            }
        }
    }
}

/// Glob tool - fast file pattern matching
pub struct GlobTool;

#[async_trait::async_trait]
impl ToolHandler for GlobTool {
    fn description(&self) -> String {
        "- Fast file pattern matching tool that works with any codebase size\n- Supports glob patterns like \"**/*.js\" or \"src/**/*.ts\"\n- Returns matching file paths sorted by modification time\n- Use this tool when you need to find files by name patterns\n- When you are doing an open ended search that may require multiple rounds of globbing and grepping, use the Agent tool instead\n- You have the capability to call multiple tools in a single response. It is always better to speculatively perform multiple searches as a batch that are potentially useful.".to_string()
    }
    
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "The glob pattern to match files against"
                },
                "path": {
                    "type": "string",
                    "description": "The directory to search in. If not specified, the current working directory will be used. IMPORTANT: Omit this field to use the default directory. DO NOT enter \"undefined\" or \"null\" - simply omit it for the default behavior. Must be a valid directory path if provided."
                }
            },
            "required": ["pattern"]
        })
    }
    
    fn action_description(&self, input: &serde_json::Value) -> String {
        let pattern = input["pattern"].as_str().unwrap_or("<unknown>");
        let path = input["path"].as_str().unwrap_or(".");
        format!("Find files matching '{}' in {}", pattern, path)
    }
    
    fn permission_details(&self, input: &serde_json::Value) -> String {
        let pattern = input["pattern"].as_str().unwrap_or("<unknown>");
        let path = input["path"].as_str().unwrap_or(".");
        format!("Pattern: {}, Path: {}", pattern, path)
    }
    
    async fn execute(&self, input: serde_json::Value, cancellation_token: Option<CancellationToken>) -> Result<String> {
        use crate::permissions::{PERMISSION_CONTEXT, PermissionBehavior, FileOperation};
        
        let pattern = input["pattern"]
            .as_str()
            .ok_or_else(|| Error::InvalidInput("Missing 'pattern' field".to_string()))?;
            
        let base_path = input["path"].as_str().unwrap_or(".");
        let path_obj = Path::new(base_path);
        
        // Check permissions before directory access
        {
            let mut ctx = PERMISSION_CONTEXT.lock().await;
            let perm_result = ctx.check_file_operation(path_obj, FileOperation::Read, "Glob");
            match perm_result.behavior {
                PermissionBehavior::Deny | PermissionBehavior::Never => {
                    return Err(Error::PermissionDenied(format!("Permission denied to search in: {}", base_path)));
                },
                PermissionBehavior::Ask => {
                    return Err(Error::PermissionDenied(format!("Permission required to search in: {} (use /add-dir to allow directory access)", base_path)));
                },
                _ => {} // Allow or AlwaysAllow - proceed
            }
        }
        
        // Resolve base path to absolute
        let base_path = Path::new(base_path).canonicalize()
            .map_err(|e| Error::NotFound(format!("Invalid base path '{}': {}", base_path, e)))?;
            
        // Construct full pattern
        let full_pattern = base_path.join(pattern);
        let pattern_str = full_pattern.to_string_lossy();
        
        // Create a vector to store files with metadata
        let mut files_with_time: Vec<(PathBuf, SystemTime)> = Vec::new();
        
        // Execute glob pattern matching
        for entry in glob(&pattern_str)? {
            match entry {
                Ok(path) => {
                    // Only include files, not directories
                    if path.is_file() {
                        // Get modification time
                        match fs::metadata(&path) {
                            Ok(metadata) => {
                                match metadata.modified() {
                                    Ok(modified) => {
                                        files_with_time.push((path, modified));
                                    }
                                    Err(_) => {
                                        // If we can't get modification time, use epoch
                                        files_with_time.push((path, SystemTime::UNIX_EPOCH));
                                    }
                                }
                            }
                            Err(_) => {
                                // If we can't get metadata, still include the file
                                files_with_time.push((path, SystemTime::UNIX_EPOCH));
                            }
                        }
                    }
                }
                Err(e) => {
                    // Log glob errors but continue
                    eprintln!("Glob error: {}", e);
                }
            }
        }
        
        // Sort by modification time - newest first
        // JavaScript sorts by oldest first: (a.mtimeMs ?? 0) - (b.mtimeMs ?? 0)
        // Then the result is reversed later when displayed
        files_with_time.sort_by(|a, b| b.1.cmp(&a.1));
        
        if files_with_time.is_empty() {
            return Ok(format!("No files found matching pattern: {}", pattern));
        }
        
        // Format output - return full paths like the JavaScript does
        let result: Vec<String> = files_with_time
            .into_iter()
            .map(|(path, _)| path.display().to_string())
            .collect();
        
        Ok(result.join("\n"))
    }
}

/// Edit file tool
pub struct EditFileTool;

#[async_trait::async_trait]
impl ToolHandler for EditFileTool {
    fn description(&self) -> String {
        "Performs exact string replacements in files. 

Usage:
- You must use your `Read` tool at least once in the conversation before editing. This tool will error if you attempt an edit without reading the file. 
- When editing text from Read tool output, ensure you preserve the exact indentation (tabs/spaces) as it appears AFTER the line number prefix. The line number prefix format is: spaces + line number + tab. Everything after that tab is the actual file content to match. Never include any part of the line number prefix in the old_string or new_string.
- ALWAYS prefer editing existing files in the codebase. NEVER write new files unless explicitly required.
- Only use emojis if the user explicitly requests it. Avoid adding emojis to files unless asked.
- The edit will FAIL if `old_string` is not unique in the file. Either provide a larger string with more surrounding context to make it unique or use `replace_all` to change every instance of `old_string`. 
- Use `replace_all` for replacing and renaming strings across the file. This parameter is useful if you want to rename a variable for instance.".to_string()
    }
    
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The absolute path to the file to modify"
                },
                "old_string": {
                    "type": "string",
                    "description": "The text to replace"
                },
                "new_string": {
                    "type": "string",
                    "description": "The text to replace it with (must be different from old_string)"
                },
                "replace_all": {
                    "type": "boolean",
                    "description": "Replace all occurences of old_string (default false)",
                    "default": false
                }
            },
            "required": ["file_path", "old_string", "new_string"]
        })
    }
    
    fn action_description(&self, input: &serde_json::Value) -> String {
        format!("Edit file: {}", input["file_path"].as_str().unwrap_or("<unknown>"))
    }
    
    fn permission_details(&self, input: &serde_json::Value) -> String {
        format!("Path: {}", input["file_path"].as_str().unwrap_or("<unknown>"))
    }
    
    async fn execute(&self, input: serde_json::Value, cancellation_token: Option<CancellationToken>) -> Result<String> {
        use crate::permissions::{PERMISSION_CONTEXT, PermissionBehavior, FileOperation};
        
        tracing::debug!("EditFileTool::execute called");
        
        let file_path = input["file_path"]
            .as_str()
            .ok_or_else(|| Error::InvalidInput("Missing 'file_path' field".to_string()))?;
        
        let old_string = input["old_string"]
            .as_str()
            .ok_or_else(|| Error::InvalidInput("Missing 'old_string' field".to_string()))?;
            
        let new_string = input["new_string"]
            .as_str()
            .ok_or_else(|| Error::InvalidInput("Missing 'new_string' field".to_string()))?;
            
        let replace_all = input["replace_all"].as_bool().unwrap_or(false);
        
        tracing::debug!("DEBUG: Editing file: {}, old_string length: {}, new_string length: {}, replace_all: {}", 
            file_path, old_string.len(), new_string.len(), replace_all);
        
        let path_obj = Path::new(file_path);
        
        // Check permissions before file access
        tracing::debug!("DEBUG: Checking permissions for edit operation on: {}", file_path);
        {
            let mut ctx = PERMISSION_CONTEXT.lock().await;
            let perm_result = ctx.check_file_operation(path_obj, FileOperation::Edit, "Edit");
            tracing::debug!("DEBUG: Permission check for {}: {:?}", file_path, perm_result.behavior);
            match perm_result.behavior {
                PermissionBehavior::Deny | PermissionBehavior::Never => {
                    tracing::warn!("DEBUG: Permission denied for file edit: {}", file_path);
                    return Err(Error::PermissionDenied(format!("Permission denied to edit file: {}", file_path)));
                },
                PermissionBehavior::Ask => {
                    tracing::debug!("DEBUG: Permission required for file edit: {}", file_path);
                    return Err(Error::PermissionDenied(format!("Permission required to edit file: {} (use /add-dir to allow directory access)", file_path)));
                },
                _ => {
                    tracing::debug!("DEBUG: Permission granted for file edit: {}", file_path);
                }
            }
        }
        
        // Check if old_string and new_string are the same
        if old_string == new_string {
            tracing::error!("DEBUG: old_string and new_string are identical");
            return Err(Error::InvalidInput("old_string and new_string are exactly the same".to_string()));
        }
        
        // Read file
        tracing::debug!("DEBUG: Reading file content for editing: {}", file_path);
        let content = match async_fs::read_to_string(file_path).await {
            Ok(content) => {
                tracing::debug!("DEBUG: Successfully read file {} ({} bytes)", file_path, content.len());
                content
            },
            Err(e) => {
                tracing::error!("DEBUG: Failed to read file {}: {}", file_path, e);
                return Err(Error::from(e));
            }
        };
        
        // Perform replacement
        let result = if replace_all {
            content.replace(old_string, new_string)
        } else {
            // Replace only first occurrence
            if let Some(pos) = content.find(old_string) {
                let mut new_content = String::new();
                new_content.push_str(&content[..pos]);
                new_content.push_str(new_string);
                new_content.push_str(&content[pos + old_string.len()..]);
                new_content
            } else {
                return Err(Error::InvalidInput("String not found in file. Failed to apply edit.".to_string()));
            }
        };
        
        // Check if content actually changed
        if result == content {
            return Err(Error::InvalidInput("Original and edited file match exactly. Failed to apply edit.".to_string()));
        }
        
        // Generate diff display
        let diff = crate::ai::diff_display::DiffDisplay::new(
            content.clone(),
            result.clone(),
            file_path.to_string()
        );
        
        // Write back
        async_fs::write(file_path, &result).await?;
        
        // Return summary with inline diff for context
        let summary = diff.summary();
        let inline_diff = diff.inline_diff();
        
        // Combine summary with a compact diff view
        let message = if !inline_diff.is_empty() && inline_diff.len() < 500 {
            format!("{}\n\n{}", summary, inline_diff)
        } else {
            summary
        };
        
        Ok(message)
    }
}

/// Multi-edit file tool
pub struct FileMultiEditTool;

#[async_trait::async_trait]
impl ToolHandler for FileMultiEditTool {
    fn description(&self) -> String {
        "This is a tool for making multiple edits to a single file in one operation. It is built on top of the Edit tool and allows you to perform multiple find-and-replace operations efficiently. Prefer this tool over the Edit tool when you need to make multiple edits to the same file.

Before using this tool:

1. Use the Read tool to understand the file's contents and context
2. Verify the directory path is correct

To make multiple file edits, provide the following:
1. file_path: The absolute path to the file to modify (must be absolute, not relative)
2. edits: An array of edit operations to perform, where each edit contains:
   - old_string: The text to replace (must match the file contents exactly, including all whitespace and indentation)
   - new_string: The edited text to replace the old_string
   - replace_all: Replace all occurences of old_string. This parameter is optional and defaults to false.

IMPORTANT:
- All edits are applied in sequence, in the order they are provided
- Each edit operates on the result of the previous edit
- All edits must be valid for the operation to succeed - if any edit fails, none will be applied
- This tool is ideal when you need to make several changes to different parts of the same file
- For Jupyter notebooks (.ipynb files), use the NotebookEdit instead

CRITICAL REQUIREMENTS:
1. All edits follow the same requirements as the single Edit tool
2. The edits are atomic - either all succeed or none are applied
3. Plan your edits carefully to avoid conflicts between sequential operations

WARNING:
- The tool will fail if edits.old_string doesn't match the file contents exactly (including whitespace)
- The tool will fail if edits.old_string and edits.new_string are the same
- Since edits are applied in sequence, ensure that earlier edits don't affect the text that later edits are trying to find

When making edits:
- Ensure all edits result in idiomatic, correct code
- Do not leave the code in a broken state
- Always use absolute file paths (starting with /)
- Only use emojis if the user explicitly requests it. Avoid adding emojis to files unless asked.
- Use replace_all for replacing and renaming strings across the file. This parameter is useful if you want to rename a variable for instance.

If you want to create a new file, use:
- A new file path, including dir name if needed
- First edit: empty old_string and the new file's contents as new_string
- Subsequent edits: normal edit operations on the created content".to_string()
    }
    
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The absolute path to the file to modify"
                },
                "edits": {
                    "type": "array",
                    "description": "Array of edit operations to perform sequentially on the file",
                    "items": {
                        "type": "object",
                        "properties": {
                            "old_string": {
                                "type": "string",
                                "description": "The text to replace"
                            },
                            "new_string": {
                                "type": "string",
                                "description": "The text to replace it with"
                            },
                            "replace_all": {
                                "type": "boolean",
                                "description": "Replace all occurences of old_string (default false)",
                                "default": false
                            }
                        },
                        "required": ["old_string", "new_string"]
                    },
                    "minItems": 1
                }
            },
            "required": ["file_path", "edits"]
        })
    }
    
    fn action_description(&self, input: &serde_json::Value) -> String {
        let edit_count = input["edits"].as_array().map(|a| a.len()).unwrap_or(0);
        format!("Multi-edit file: {} ({} edits)", 
            input["file_path"].as_str().unwrap_or("<unknown>"),
            edit_count
        )
    }
    
    fn permission_details(&self, input: &serde_json::Value) -> String {
        let edit_count = input["edits"].as_array().map(|a| a.len()).unwrap_or(0);
        format!("Path: {}, Number of edits: {}", 
            input["file_path"].as_str().unwrap_or("<unknown>"),
            edit_count
        )
    }
    
    async fn execute(&self, input: serde_json::Value, cancellation_token: Option<CancellationToken>) -> Result<String> {
        use crate::permissions::{PERMISSION_CONTEXT, PermissionBehavior, FileOperation};
        
        let file_path = input["file_path"]
            .as_str()
            .ok_or_else(|| Error::InvalidInput("Missing 'file_path' field".to_string()))?;
        
        let path_obj = Path::new(file_path);
        
        // Check permissions before file access
        {
            let mut ctx = PERMISSION_CONTEXT.lock().await;
            let perm_result = ctx.check_file_operation(path_obj, FileOperation::Edit, "MultiEdit");
            match perm_result.behavior {
                PermissionBehavior::Deny | PermissionBehavior::Never => {
                    return Err(Error::PermissionDenied(format!("Permission denied to edit file: {}", file_path)));
                },
                PermissionBehavior::Ask => {
                    return Err(Error::PermissionDenied(format!("Permission required to edit file: {} (use /add-dir to allow directory access)", file_path)));
                },
                _ => {} // Allow or AlwaysAllow - proceed
            }
        }
        
        let edits = input["edits"]
            .as_array()
            .ok_or_else(|| Error::InvalidInput("Missing 'edits' field".to_string()))?;
            
        if edits.is_empty() {
            return Err(Error::InvalidInput("No edits specified".to_string()));
        }
        
        // Check if file exists
        if !Path::new(file_path).exists() {
            return Err(Error::NotFound(format!("File not found: {}", file_path)));
        }
        
        // Read the file content
        let mut content = async_fs::read_to_string(file_path).await?;
        let original_content = content.clone();
        
        let mut applied_edits = Vec::new();
        let mut failed_edits = Vec::new();
        
        // Apply each edit sequentially
        for (idx, edit) in edits.iter().enumerate() {
            let old_string = edit["old_string"]
                .as_str()
                .ok_or_else(|| Error::InvalidInput(format!("Missing 'old_string' in edit {}", idx + 1)))?;
                
            let new_string = edit["new_string"]
                .as_str()
                .ok_or_else(|| Error::InvalidInput(format!("Missing 'new_string' in edit {}", idx + 1)))?;
                
            let replace_all = edit["replace_all"].as_bool().unwrap_or(false);
            
            // Check if old_string exists in current content
            if !content.contains(old_string) {
                failed_edits.push(format!("Edit {}: Text not found: '{}'", idx + 1, 
                    if old_string.len() > 50 { 
                        format!("{}...", &old_string[..50]) 
                    } else { 
                        old_string.to_string() 
                    }
                ));
                continue;
            }
            
            // Apply the edit
            if replace_all {
                let count = content.matches(old_string).count();
                content = content.replace(old_string, new_string);
                applied_edits.push(format!("Edit {}: Replaced {} occurrences", idx + 1, count));
            } else {
                // Replace first occurrence only
                if let Some(pos) = content.find(old_string) {
                    content.replace_range(pos..pos + old_string.len(), new_string);
                    applied_edits.push(format!("Edit {}: Replaced 1 occurrence", idx + 1));
                }
            }
        }
        
        // Check if any edits were applied
        if applied_edits.is_empty() {
            return Err(Error::InvalidInput(format!(
                "No edits could be applied. {}",
                failed_edits.join("; ")
            )));
        }
        
        // Only write if content changed
        if content != original_content {
            async_fs::write(file_path, &content).await?;
            
            // Generate diff display
            let diff = crate::ai::diff_display::DiffDisplay::new(
                original_content.clone(),
                content.clone(),
                file_path.to_string()
            );
            
            // Build result message with diff summary
            let summary = diff.summary();
            let mut result = format!("{}\nApplied {} of {} edits\n", 
                summary, applied_edits.len(), edits.len());
            
            // Add compact diff if not too large
            let inline_diff = diff.inline_diff();
            if !inline_diff.is_empty() && inline_diff.len() < 500 {
                result.push_str("\nChanges:\n");
                result.push_str(&inline_diff);
            }
            
            if !failed_edits.is_empty() {
                result.push_str("\nFailed edits:\n");
                for edit in failed_edits {
                    result.push_str(&format!("  - {}\n", edit));
                }
            }
            
            Ok(result.trim().to_string())
        } else {
            // No changes made
            let mut result = format!("No changes made to {}\n", file_path);
            if !failed_edits.is_empty() {
                result.push_str("\nFailed edits:\n");
                for edit in failed_edits {
                    result.push_str(&format!("  - {}\n", edit));
                }
            }
            Ok(result.trim().to_string())
        }
    }
}

/// Bash command tool with persistent shell session
pub struct BashTool;

#[async_trait::async_trait]
impl ToolHandler for BashTool {
    fn description(&self) -> String {
        "Executes a given bash command in a persistent shell session with optional timeout, ensuring proper handling and security measures.

Before executing the command, please follow these steps:

1. Directory Verification:
   - If the command will create new directories or files, first use `ls` to verify the parent directory exists and is the correct location
   - For example, before running \"mkdir foo/bar\", first use `ls` to check that \"foo\" exists and is the intended parent directory

2. Command Execution:
   - Always quote file paths that contain spaces with double quotes (e.g., cd \"path with spaces/file.txt\")
   - Examples of proper quoting:
     - cd \"/Users/name/My Documents\" (correct)
     - cd /Users/name/My Documents (incorrect - will fail)
     - python \"/path/with spaces/script.py\" (correct)
     - python /path/with spaces/script.py (incorrect - will fail)
   - After ensuring proper quoting, execute the command.
   - Capture the output of the command.

Usage notes:
  - The command argument is required.
  - You can specify an optional timeout in milliseconds (up to 600000ms / 10 minutes). If not specified, commands will timeout after 120000ms (2 minutes).
  - It is very helpful if you write a clear, concise description of what this command does in 5-10 words.
  - If the output exceeds 30000 characters, output will be truncated before being returned to you.
  - You can use the `run_in_background` parameter to run the command in the background, which allows you to continue working while the command runs. You can monitor the output using the Bash tool as it becomes available. Never use `run_in_background` to run 'sleep' as it will return immediately. You do not need to use '&' at the end of the command when using this parameter.
  - VERY IMPORTANT: You MUST avoid using search commands like `find` and `grep`. Instead use Grep, Glob, or Task to search. You MUST avoid read tools like `cat`, `head`, and `tail`, and use Read to read files.
 - If you _still_ need to run `grep`, STOP. ALWAYS USE ripgrep at `rg` first, which all Claude Code users have pre-installed.
  - When issuing multiple commands, use the ';' or '&&' operator to separate them. DO NOT use newlines (newlines are ok in quoted strings).
  - Try to maintain your current working directory throughout the session by using absolute paths and avoiding usage of `cd`. You may use `cd` if the User explicitly requests it.
    <good-example>
    pytest /foo/bar/tests
    </good-example>
    <bad-example>
    cd /foo/bar && pytest tests
    </bad-example>".to_string()
    }
    
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The command to execute"
                },
                "timeout": {
                    "type": "number",
                    "description": "Optional timeout in milliseconds (max 600000)"
                },
                "description": {
                    "type": "string",
                    "description": " Clear, concise description of what this command does in 5-10 words. Examples:\nInput: ls\nOutput: Lists files in current directory\n\nInput: git status\nOutput: Shows working tree status\n\nInput: npm install\nOutput: Installs package dependencies\n\nInput: mkdir foo\nOutput: Creates directory 'foo'"
                },
                "dangerouslyDisableSandbox": {
                    "type": "boolean",
                    "description": "Set this to true to dangerously override sandbox mode and run commands without sandboxing."
                },
                "shellExecutable": {
                    "type": "string",
                    "description": "Optional shell path to use instead of the default shell. The snapshot path will be set to undefined as well. Used primarily for testing."
                },
                "working_dir": {
                    "type": "string",
                    "description": "Optional working directory"
                },
                "env": {
                    "type": "object",
                    "description": "Optional environment variables",
                    "additionalProperties": {
                        "type": "string"
                    }
                },
                "shell_id": {
                    "type": "string",
                    "description": "Optional shell session ID for persistent sessions"
                },
                "stream": {
                    "type": "boolean",
                    "description": "Whether to stream output (default: false)"
                },
                "advanced_persistence": {
                    "type": "boolean",
                    "description": "Enable advanced shell variable persistence (default: false, uses JS-compatible working directory persistence only)"
                },
                "run_in_background": {
                    "type": "boolean",
                    "description": "Run command in background and return immediately (default: false)"
                }
            },
            "required": ["command"]
        })
    }
    
    fn action_description(&self, input: &serde_json::Value) -> String {
        format!("Execute: {}", input["command"].as_str().unwrap_or("<unknown>"))
    }
    
    fn permission_details(&self, input: &serde_json::Value) -> String {
        format!(
            "Command: {}, Directory: {}, Timeout: {}ms",
            input["command"].as_str().unwrap_or("<unknown>"),
            input["working_dir"].as_str().unwrap_or("<current>"),
            input["timeout"].as_u64().unwrap_or(120000)
        )
    }
    
    async fn execute(&self, input: serde_json::Value, cancellation_token: Option<CancellationToken>) -> Result<String> {
        use crate::permissions::{PERMISSION_CONTEXT, PermissionBehavior};
        
        tracing::debug!("BashTool::execute called");
        
        let command = input["command"]
            .as_str()
            .ok_or_else(|| Error::InvalidInput("Missing 'command' field".to_string()))?;
        
        let timeout_ms = input["timeout"]
            .as_u64()
            .unwrap_or(120000)
            .min(600000); // Max 10 minutes
            
        let working_dir = input["working_dir"].as_str().map(PathBuf::from);
        let env_vars = input["env"].as_object();
        let shell_id = input["shell_id"].as_str().unwrap_or("default");
        let stream_output = input["stream"].as_bool().unwrap_or(false);
        // JavaScript: dangerouslyDisableSandbox defaults to false (sandbox ENABLED by default)
        // So is_sandboxed = !dangerouslyDisableSandbox
        let dangerously_disable_sandbox = input["dangerouslyDisableSandbox"].as_bool().unwrap_or(false);
        let is_sandboxed = !dangerously_disable_sandbox;
        let shell_executable = input["shellExecutable"].as_str().map(String::from);
        let advanced_persistence = input["advanced_persistence"].as_bool().unwrap_or(false);
        let run_in_background = input["run_in_background"].as_bool().unwrap_or(false);
        
        tracing::debug!("DEBUG: Shell command execution: '{}', timeout: {}ms, shell_id: {}, background: {}, advanced_persistence: {}", 
            command, timeout_ms, shell_id, run_in_background, advanced_persistence);
        tracing::debug!("DEBUG: Shell parameters - working_dir: {:?}, sandboxed: {}, stream: {}", 
            working_dir, is_sandboxed, stream_output);
        
        // Note: Permission checking is now handled in execute_bash_with_suspension
        // when called through execute_with_context. Direct calls to this execute method
        // (without context) will bypass permission checks for backward compatibility.
        
        // Handle background execution (like JavaScript moveToBackground)
        if run_in_background {
            let shell_id = BACKGROUND_SHELLS.generate_shell_id().await;
            let mut shell = BackgroundShell::new(shell_id.clone(), command.to_string());
            
            // Create the command to run
            let mut cmd = tokio::process::Command::new(shell_executable.as_deref().unwrap_or("/bin/bash"));
            cmd.arg("-c");
            cmd.arg(command);
            cmd.kill_on_drop(true);
            // Disable color output and TTY detection to prevent terminal corruption
            cmd.env("NO_COLOR", "1");
            cmd.env("TERM", "dumb");
            cmd.env("CARGO_TERM_COLOR", "never");
            cmd.env("RUST_LOG_STYLE", "never");
            cmd.env("CLICOLOR", "0");
            cmd.env("CLICOLOR_FORCE", "0");
            cmd.stdout(std::process::Stdio::piped());
            cmd.stderr(std::process::Stdio::piped());
            
            if let Some(dir) = working_dir {
                cmd.current_dir(dir);
            }
            
            if let Some(env) = env_vars {
                for (key, value) in env {
                    if let Some(val_str) = value.as_str() {
                        cmd.env(key, val_str);
                    }
                }
            }
            
            // Spawn the process
            match cmd.spawn() {
                Ok(mut child) => {
                    // Set up output collection
                    let stdout_arc = shell.stdout.clone();
                    let stderr_arc = shell.stderr.clone();
                    let shell_id_clone = shell_id.clone();
                    
                    if let Some(stdout) = child.stdout.take() {
                        let stdout_arc_clone = stdout_arc.clone();
                        tokio::spawn(async move {
                            use tokio::io::{AsyncBufReadExt, BufReader};
                            let reader = BufReader::new(stdout);
                            let mut lines = reader.lines();
                            while let Ok(Some(line)) = lines.next_line().await {
                                let mut output = stdout_arc_clone.lock().await;
                                output.push_str(&line);
                                output.push('\n');
                            }
                        });
                    }
                    
                    if let Some(stderr) = child.stderr.take() {
                        let stderr_arc_clone = stderr_arc.clone();
                        tokio::spawn(async move {
                            use tokio::io::{AsyncBufReadExt, BufReader};
                            let reader = BufReader::new(stderr);
                            let mut lines = reader.lines();
                            while let Ok(Some(line)) = lines.next_line().await {
                                let mut output = stderr_arc_clone.lock().await;
                                output.push_str(&line);
                                output.push('\n');
                            }
                        });
                    }
                    
                    // Store process handle
                    shell.process = Some(Arc::new(Mutex::new(child)));
                    
                    // Monitor process completion
                    let shell_id_monitor = shell_id.clone();
                    let process_arc = shell.process.clone().unwrap();
                    tokio::spawn(async move {
                        let mut process = process_arc.lock().await;
                        match process.wait().await {
                            Ok(status) => {
                                if let Some(shell) = BACKGROUND_SHELLS.get_shell(&shell_id_monitor).await {
                                    let mut shells = BACKGROUND_SHELLS.shells.lock().await;
                                    if let Some(shell_mut) = shells.get_mut(&shell_id_monitor) {
                                        shell_mut.exit_code = status.code();
                                        shell_mut.status = if status.success() {
                                            "completed".to_string()
                                        } else {
                                            "failed".to_string()
                                        };
                                    }
                                }
                            }
                            Err(_) => {
                                if let Some(shell) = BACKGROUND_SHELLS.get_shell(&shell_id_monitor).await {
                                    let mut shells = BACKGROUND_SHELLS.shells.lock().await;
                                    if let Some(shell_mut) = shells.get_mut(&shell_id_monitor) {
                                        shell_mut.status = "failed".to_string();
                                    }
                                }
                            }
                        }
                    });
                    
                    // Add to background shells
                    BACKGROUND_SHELLS.add_background_shell(shell).await;
                    
                    return Ok(format!(
                        "Command running in background (shell ID: {})\n\nTo check output, use BashOutput tool with shell_id: {}\nTo kill, use KillBash tool with shell_id: {}",
                        shell_id, shell_id, shell_id
                    ));
                }
                Err(e) => {
                    return Err(Error::ToolExecution(format!("Failed to start background process: {}", e)));
                }
            }
        }
        
        // Validate sandbox availability
        if is_sandboxed && !cfg!(target_os = "macos") {
            return Err(Error::InvalidInput("Sandbox mode requested but not available on this system".to_string()));
        }
        
        let start = std::time::Instant::now();
        
        // Get or create shell session state (like JavaScript implementation)
        let sessions = SHELL_SESSIONS.clone();
        let mut sessions_guard = sessions.lock().await;
        
        let mut session_state = if let Some(existing_session) = sessions_guard.get_mut(shell_id) {
            // Clone existing session but check if working_dir parameter overrides it
            let mut cloned_state = existing_session.clone();
            
            // If working_dir is provided as parameter, update the session's working directory
            // This allows changing working directory via parameter
            if let Some(new_working_dir) = working_dir {
                cloned_state.working_dir = new_working_dir;
                // Also add to allowed directories if not already present
                cloned_state.add_working_directory(cloned_state.working_dir.clone());
            }
            
            // Update advanced persistence mode if specified
            cloned_state.set_advanced_persistence(advanced_persistence);
            
            cloned_state
        } else {
            // Create new shell session state
            let mut session_state = ShellSessionState::new(shell_executable, working_dir.clone(), is_sandboxed);
            session_state.set_advanced_persistence(advanced_persistence);
            
            // Add working_dir to allowed directories if provided
            if let Some(ref wd) = working_dir {
                session_state.add_working_directory(wd.clone());
            }
            
            sessions_guard.insert(shell_id.to_string(), session_state.clone());
            session_state
        };
        
        drop(sessions_guard); // Release the sessions lock
        
        // Convert environment variables to HashMap for easier handling
        let additional_env = if let Some(vars) = env_vars {
            let mut env_map = HashMap::new();
            for (key, value) in vars {
                if let Some(val_str) = value.as_str() {
                    env_map.insert(key.clone(), val_str.to_string());
                }
            }
            Some(env_map)
        } else {
            None
        };
        
        // Execute command with state persistence and cancellation support
        let (stdout, stderr, exit_code) = session_state.execute_command(command, timeout_ms, additional_env.as_ref(), cancellation_token).await?;
        
        // Update session state back to global store
        let mut sessions_guard = SHELL_SESSIONS.lock().await;
        sessions_guard.insert(shell_id.to_string(), session_state);
        drop(sessions_guard);
        
        let elapsed = start.elapsed();
        
        let mut result = String::new();
        
        // Add execution info - be clear about success/failure
        if exit_code == 0 {
            result.push_str(&format!("Executed successfully in {:.2}ms\n\n", elapsed.as_secs_f64() * 1000.0));
        } else {
            result.push_str(&format!("Command FAILED with exit code {} after {:.2}ms\n\n", exit_code, elapsed.as_secs_f64() * 1000.0));
        }
        
        // Format output to match JavaScript's expected format
        if !stdout.is_empty() {
            result.push_str("STDOUT:\n");
            result.push_str(&stdout);
            if !stdout.ends_with('\n') {
                result.push('\n');
            }
        }
        
        if !stderr.is_empty() {
            result.push_str("\nSTDERR:\n");
            result.push_str(&stderr);
            if !stderr.ends_with('\n') {
                result.push('\n');
            }
        }
        
        // If command failed but no stderr, add a note
        if exit_code != 0 && stderr.is_empty() && stdout.is_empty() {
            result.push_str("(No output captured - command may have failed to execute or been blocked)\n");
        }
        
        // Truncate output if too long (matches JavaScript behavior)
        if result.len() > 30000 {
            result.truncate(30000);
            result.push_str("\n\n[Output truncated to 30000 characters]");
        }
        
        Ok(result.trim().to_string())
    }
}

/// HTTP request tool
struct HttpRequestTool;

#[async_trait::async_trait]
impl ToolHandler for HttpRequestTool {
    fn description(&self) -> String {
        "Make an HTTP request".to_string()
    }
    
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "URL to request"
                },
                "method": {
                    "type": "string",
                    "description": "HTTP method",
                    "enum": ["GET", "POST", "PUT", "DELETE", "PATCH"],
                    "default": "GET"
                },
                "headers": {
                    "type": "object",
                    "description": "HTTP headers (optional)"
                },
                "body": {
                    "type": "string",
                    "description": "Request body (optional)"
                }
            },
            "required": ["url"]
        })
    }
    
    fn action_description(&self, input: &serde_json::Value) -> String {
        format!(
            "{} {}",
            input["method"].as_str().unwrap_or("GET"),
            input["url"].as_str().unwrap_or("<unknown>")
        )
    }
    
    fn permission_details(&self, input: &serde_json::Value) -> String {
        format!(
            "URL: {}, Method: {}",
            input["url"].as_str().unwrap_or("<unknown>"),
            input["method"].as_str().unwrap_or("GET")
        )
    }
    
    async fn execute(&self, input: serde_json::Value, cancellation_token: Option<CancellationToken>) -> Result<String> {
        let url = input["url"]
            .as_str()
            .ok_or_else(|| Error::InvalidInput("Missing 'url' field".to_string()))?;
        
        let method = input["method"].as_str().unwrap_or("GET");
        
        let client = reqwest::Client::new();
        let mut request = match method {
            "GET" => client.get(url),
            "POST" => client.post(url),
            "PUT" => client.put(url),
            "DELETE" => client.delete(url),
            "PATCH" => client.patch(url),
            _ => return Err(Error::InvalidInput(format!("Invalid method: {}", method))),
        };
        
        // Add headers
        if let Some(headers) = input["headers"].as_object() {
            for (key, value) in headers {
                if let Some(value_str) = value.as_str() {
                    request = request.header(key, value_str);
                }
            }
        }
        
        // Add body
        if let Some(body) = input["body"].as_str() {
            request = request.body(body.to_string());
        }
        
        let response = request.send().await?;
        let status = response.status();
        let headers = response.headers().clone();
        let body = response.text().await?;
        
        let mut result = format!("Status: {}\n\nHeaders:\n", status);
        for (key, value) in headers {
            if let Some(key) = key {
                result.push_str(&format!("{}: {}\n", key, value.to_str().unwrap_or("<binary>")));
            }
        }
        
        result.push_str("\nBody:\n");
        result.push_str(&body);
        
        Ok(result)
    }
}

/// BashOutput tool - Get output from background shell
pub struct BashOutputTool;

#[async_trait::async_trait]
impl ToolHandler for BashOutputTool {
    fn description(&self) -> String {
        "Retrieve output from a background bash shell".to_string()
    }
    
    fn input_schema(&self) -> serde_json::Value {
        // JavaScript tool is named TaskOutput with aliases AgentOutputTool, BashOutputTool
        json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "The task ID to get output from"
                },
                "block": {
                    "type": "boolean",
                    "description": "Whether to wait for completion",
                    "default": true
                },
                "timeout": {
                    "type": "number",
                    "description": "Max wait time in ms",
                    "default": 30000,
                    "minimum": 0,
                    "maximum": 600000
                },
                "filter": {
                    "type": "string",
                    "description": "Optional regular expression to filter the output lines (Rust enhancement)"
                }
            },
            "required": ["task_id"]
        })
    }
    
    fn action_description(&self, input: &serde_json::Value) -> String {
        format!("Get output from task: {}", input["task_id"].as_str().unwrap_or("<unknown>"))
    }

    fn permission_details(&self, input: &serde_json::Value) -> String {
        format!("Task ID: {}", input["task_id"].as_str().unwrap_or("<unknown>"))
    }

    async fn execute(&self, input: serde_json::Value, cancellation_token: Option<CancellationToken>) -> Result<String> {
        let task_id = input["task_id"]
            .as_str()
            .ok_or_else(|| Error::InvalidInput("Missing 'task_id' field".to_string()))?;

        let block = input["block"].as_bool().unwrap_or(true);
        let timeout_ms = input["timeout"].as_u64().unwrap_or(30000).min(600000);
        let filter = input["filter"].as_str();

        // TODO: Implement proper blocking and timeout behavior like JavaScript
        // For now, just get the output directly
        let output_json = BACKGROUND_SHELLS.get_shell_output(task_id).await;
        
        // Apply filter if provided
        if let Some(filter_regex) = filter {
            if let Ok(re) = regex::Regex::new(filter_regex) {
                if let Some(stdout) = output_json["stdout"].as_str() {
                    let filtered_lines: Vec<&str> = stdout
                        .lines()
                        .filter(|line| re.is_match(line))
                        .collect();
                    
                    let mut result = output_json.clone();
                    result["stdout"] = json!(filtered_lines.join("\n"));
                    return Ok(serde_json::to_string_pretty(&result)?);
                }
            }
        }
        
        Ok(serde_json::to_string_pretty(&output_json)?)
    }
}

/// KillBash tool - Kill a background shell
pub struct KillBashTool;

#[async_trait::async_trait]
impl ToolHandler for KillBashTool {
    fn description(&self) -> String {
        "Terminate a background bash shell".to_string()
    }
    
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "shell_id": {
                    "type": "string",
                    "description": "The ID of the background shell to kill"
                }
            },
            "required": ["shell_id"]
        })
    }
    
    fn action_description(&self, input: &serde_json::Value) -> String {
        format!("Kill shell: {}", input["shell_id"].as_str().unwrap_or("<unknown>"))
    }
    
    fn permission_details(&self, input: &serde_json::Value) -> String {
        format!("Shell ID: {}", input["shell_id"].as_str().unwrap_or("<unknown>"))
    }
    
    async fn execute(&self, input: serde_json::Value, cancellation_token: Option<CancellationToken>) -> Result<String> {
        let shell_id = input["shell_id"]
            .as_str()
            .ok_or_else(|| Error::InvalidInput("Missing 'shell_id' field".to_string()))?;
        
        let killed = BACKGROUND_SHELLS.kill_shell(shell_id).await?;
        
        if killed {
            Ok(format!("Successfully killed background shell: {}", shell_id))
        } else {
            Ok(format!("Shell {} not found or already terminated", shell_id))
        }
    }
}

/// Helper function to list files recursively
fn list_files_recursive(dir: &Path, files: &mut Vec<String>) -> Result<()> {
    let entries = fs::read_dir(dir)?;
    
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let relative_path = path.strip_prefix(dir).unwrap_or(&path);
        
        if path.is_dir() {
            files.push(format!("{}/", relative_path.display()));
            list_files_recursive(&path, files)?;
        } else {
            files.push(relative_path.display().to_string());
        }
    }
    
    Ok(())
}

/// Helper function to search files recursively
fn search_files_recursive(
    dir: &Path,
    pattern: &str,
    file_pattern: &str,
    matches: &mut Vec<String>,
) -> Result<()> {
    let entries = fs::read_dir(dir)?;
    
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        
        if path.is_dir() {
            search_files_recursive(&path, pattern, file_pattern, matches)?;
        } else if let Some(file_name) = path.file_name() {
            let file_name_str = file_name.to_string_lossy();
            
            // Check file pattern
            if file_pattern != "*" && !file_name_str.contains(file_pattern) {
                continue;
            }
            
            // Read file and search for pattern
            if let Ok(content) = fs::read_to_string(&path) {
                if content.contains(pattern) {
                    matches.push(path.display().to_string());
                }
            }
        }
    }
    
    Ok(())
}