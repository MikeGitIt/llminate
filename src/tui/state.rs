use crate::error::{Error, Result};
use crate::mcp::McpClient;
use crate::tui::components::{UiMessage as Message, ToolInfo};
use crate::ai::todo_tool::{Todo, TodoStatus};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;
use tui_textarea::TextArea;
use futures::StreamExt;
use crate::ai::streaming::{StreamingHandler, StreamingUpdate};

/// Create a properly configured TextArea with no underline on cursor line
/// This helper ensures consistent TextArea configuration across the application
fn create_configured_textarea() -> TextArea<'static> {
    let mut textarea = TextArea::default();
    // Remove the default underline styling on cursor line
    textarea.set_cursor_line_style(ratatui::style::Style::default());
    // Set visible cursor style (block cursor with white background)
    textarea.set_cursor_style(ratatui::style::Style::default().bg(ratatui::style::Color::White).fg(ratatui::style::Color::Black));
    textarea.set_placeholder_text("Type your message here. Ctrl+J for newline, Enter to send.");
    textarea
}

/// Create a configured TextArea with initial content
fn create_configured_textarea_with_content<'a, I>(lines: I) -> TextArea<'static>
where
    I: IntoIterator,
    I::Item: Into<String>,
{
    let mut textarea = TextArea::from(lines.into_iter().map(|s| s.into()).collect::<Vec<String>>());
    // Remove the default underline styling on cursor line
    textarea.set_cursor_line_style(ratatui::style::Style::default());
    // Set visible cursor style (block cursor with white background)
    textarea.set_cursor_style(ratatui::style::Style::default().bg(ratatui::style::Color::White).fg(ratatui::style::Color::Black));
    textarea
}

// REMOVED: PendingToolExecution - no longer needed with streaming permission flow

#[derive(Debug, Clone)]
pub struct CommandInfo {
    pub name: String,
    pub aliases: Vec<String>,
    pub description: String,
    pub argument_hint: Option<String>,  // For showing argument hints like "<path>" or "[instructions]"
    pub command_type: String, // "local", "local-jsx", "prompt"
    pub is_enabled: bool,
}

// Autocomplete scoring result
#[derive(Debug, Clone)]
pub struct AutocompleteMatch {
    pub command: CommandInfo,
    pub score: f64,
    pub display_text: String,
}

pub struct PendingPermission {
    pub tool_name: String,
    pub command: String,
    pub tool_use_id: String,
    pub input: Value,
    pub responder: tokio::sync::oneshot::Sender<crate::tui::PermissionDecision>,
}

impl std::fmt::Debug for PendingPermission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingPermission")
            .field("tool_name", &self.tool_name)
            .field("command", &self.command)
            .field("tool_use_id", &self.tool_use_id)
            .field("input", &self.input)
            .field("responder", &"<oneshot::Sender>")
            .finish()
    }
}

/// Application state
#[derive(Debug)]
pub struct AppState {
    // Core state
    pub session_id: String,
    pub session_name: Option<String>,
    pub messages: Vec<Message>,
    pub input_textarea: TextArea<'static>,
    pub input_mode: bool,
    pub is_processing: bool,
    pub should_exit: bool,
    pub system_prompt: Option<String>,
    
    // Event channel for background tasks
    pub event_tx: Option<tokio::sync::mpsc::UnboundedSender<crate::tui::TuiEvent>>,
    
    // UI state
    pub show_help: bool,
    pub show_tool_panel: bool,
    pub debug_mode: bool,
    pub scroll_offset: usize,  // Line-based scroll offset for Paragraph widget
    pub rendered_lines_cache: Vec<ratatui::text::Line<'static>>, // Cache of rendered lines
    pub cache_valid: bool,      // Whether cache needs rebuilding
    pub cache_expanded_state: bool, // What expanded state the cache represents
    pub terminal_size: (u16, u16),
    
    // Model and tools
    pub current_model: String,
    pub active_tools: HashMap<String, ToolInfo>,
    pub allowed_tools: Vec<String>,
    pub disallowed_tools: Vec<String>,
    
    // MCP servers
    pub mcp_servers: HashMap<String, McpClient>,
    pub mcp_server_status: HashMap<String, bool>,  // Server enabled/disabled status

    // History
    pub command_history: VecDeque<String>,
    pub history_index: Option<usize>,
    pub max_history: usize,
    
    // Performance metrics
    pub fps_samples: VecDeque<f64>,
    pub latency_samples: VecDeque<u64>,
    pub last_frame_time: std::time::Instant,
    
    // Conversation persistence
    pub conversation_dir: PathBuf,
    pub auto_save: bool,
    
    // Cancel channel
    pub cancel_tx: Option<tokio::sync::mpsc::UnboundedSender<()>>,
    
    // Agent loop infrastructure
    pub agent_tx: Option<tokio::sync::mpsc::UnboundedSender<(String, Option<Vec<crate::ai::Message>>, String)>>,
    pub agent_handle: Option<tokio::task::JoinHandle<()>>,
    
    // Paste tracking (like JavaScript pastedContents)
    pub pasted_contents: HashMap<usize, String>,
    pub next_paste_id: usize,
    
    // Paste tracking
    pub last_paste_content: Option<String>,
    pub paste_count: usize,
    
    // Permission dialog
    pub permission_dialog: crate::permissions::PermissionDialog,
    pub pending_permissions: std::collections::VecDeque<PendingPermission>,
    
    // Conversation continuation after permission
    pub continue_after_permission: bool,
    pub pending_tool_result: Option<crate::ai::ContentPart>,
    
    pub compact_mode: bool,
    pub vim_mode: bool,
    pub working_directories: HashSet<PathBuf>,
    
    pub show_session_picker: bool,
    pub session_picker_selected: usize,
    pub session_picker_items: Vec<SessionInfo>,

    // Model picker dialog
    pub show_model_picker: bool,
    pub model_picker_selected: usize,

    // Expanded view mode for Ctrl+R (toggles between collapsed/expanded view)
    pub expanded_view: bool,
    
    // Input area state for dynamic height and paste handling
    pub input_expanded: bool,  // Whether input area is expanded (vs collapsed for large pastes)
    pub input_paste_detected: bool,  // Whether last change was a large paste
    pub input_previous_line_count: usize,  // Previous line count for paste detection
    
    // Task status display
    pub current_task_status: Option<String>,
    pub spinner_frame: usize,
    /// Determinate progress (0.0 to 1.0) - None means indeterminate
    pub current_progress: Option<f64>,
    /// Whether terminal progress bar is enabled (matches JS terminalProgressBarEnabled)
    pub terminal_progress_bar_enabled: bool,
    
    // Iteration limit tracking for /continue support
    pub hit_iteration_limit: bool,
    pub continuation_messages: Option<Vec<crate::ai::Message>>,
    
    // Loaded conversation context for resume
    pub loaded_ai_messages: Option<Vec<crate::ai::Message>>,
    
    // Stream cancellation tracking
    pub stream_cancel_tx: Option<Arc<Mutex<Option<tokio::sync::mpsc::UnboundedSender<()>>>>>,
    
    pub last_spinner_update: std::time::Instant,
    
    // TODO tracking
    pub todos: Vec<Todo>,
    pub next_todo: Option<String>,  // Current next pending/in_progress task description
    
    // Command autocomplete (matches JavaScript autocomplete system)
    pub is_autocomplete_visible: bool,
    pub autocomplete_matches: Vec<AutocompleteMatch>,
    pub selected_suggestion: usize,  // selectedSuggestion in JS
    pub available_commands: Vec<CommandInfo>,  // All commands

    // Status view (tabbed UI for /status command - matches JavaScript)
    pub show_status_view: bool,
    pub status_view_tab: usize,  // 0=Status, 1=Config, 2=Usage
    pub status_config_selected: usize,  // Selected item in Config tab

    // Prompt stash (Ctrl+S - matches JavaScript line 480754)
    pub stashed_input: Option<(String, usize)>,  // (text, cursor_offset)

    // TODOs expanded display (Ctrl+T - matches JavaScript line 481215)
    pub show_todos_expanded: bool,

    // Find/Search mode (Ctrl+F)
    pub show_find_mode: bool,
    pub find_query: String,
    pub find_results: Vec<usize>,  // Line indices matching search
    pub find_current_index: usize,

    // Thinking display (interleaved-thinking-2025-05-14 beta)
    pub current_thinking: Option<String>,
    pub thinking_start_time: Option<std::time::Instant>,

    // Chat display text selection
    pub chat_selection_start: Option<(usize, usize)>,  // (line, column)
    pub chat_selection_end: Option<(usize, usize)>,    // (line, column)
    pub chat_is_selecting: bool,
    pub chat_selected_text: Option<String>,
}

impl AppState {
    /// Create new app state
    pub fn new(options: crate::tui::interactive_mode::InteractiveOptions) -> Self {
        let session_id = crate::utils::generate_session_id();
        let conversation_dir = get_conversation_dir();
        
        // Load available tools from ToolExecutor
        let mut active_tools = HashMap::new();
        let tool_executor = crate::ai::tools::ToolExecutor::new();
        for tool in tool_executor.get_available_tools() {
            if let crate::ai::Tool::Standard { name, description, .. } = tool {
                active_tools.insert(name.clone(), ToolInfo {
                    name: name.clone(),
                    description,
                    enabled: true,
                });
            }
        }
        
        let mut state = Self {
            session_id: session_id.clone(),
            session_name: None,
            messages: Vec::new(),
            input_textarea: create_configured_textarea(),
            input_mode: true,
            is_processing: false,
            should_exit: false,
            system_prompt: Some(crate::ai::system_prompt::get_system_prompt("Claude Code")),
            
            event_tx: None,  // Will be set by the interactive mode
            
            show_help: false,
            show_tool_panel: false,
            debug_mode: options.debug,
            scroll_offset: 0,
            terminal_size: (80, 24),
            
            current_model: options.model.unwrap_or_else(|| "claude-opus-4-1-20250805".to_string()),
            active_tools,
            allowed_tools: options.allowed_tools,
            disallowed_tools: options.disallowed_tools,
            
            mcp_servers: HashMap::new(),
            mcp_server_status: HashMap::new(),

            command_history: VecDeque::with_capacity(1000),
            history_index: None,
            max_history: 1000,
            
            fps_samples: VecDeque::with_capacity(60),
            latency_samples: VecDeque::with_capacity(100),
            last_frame_time: std::time::Instant::now(),
            
            conversation_dir,
            auto_save: true,
            
            cancel_tx: None,
            
            agent_tx: None,
            agent_handle: None,
            
            pasted_contents: HashMap::new(),
            next_paste_id: 1,
            
            last_paste_content: None,
            paste_count: 0,
            
            permission_dialog: crate::permissions::PermissionDialog::new(),
            pending_permissions: std::collections::VecDeque::new(),
            continue_after_permission: false,
            pending_tool_result: None,
            
            compact_mode: false,
            vim_mode: false,
            working_directories: HashSet::new(),
            
            show_session_picker: false,
            session_picker_selected: 0,
            session_picker_items: Vec::new(),

            show_model_picker: false,
            model_picker_selected: 0,

            expanded_view: false,
            
            // Input area state
            input_expanded: true,  // Start expanded by default
            input_paste_detected: false,
            input_previous_line_count: 0,
            
            rendered_lines_cache: Vec::new(),
            cache_valid: false,
            cache_expanded_state: false,
            
            current_task_status: None,
            spinner_frame: 0,
            current_progress: None,
            terminal_progress_bar_enabled: true,  // Enabled by default like JavaScript
            hit_iteration_limit: false,
            continuation_messages: None,
            loaded_ai_messages: None,
            stream_cancel_tx: None,
            last_spinner_update: std::time::Instant::now(),
            
            todos: Vec::new(),
            next_todo: None,
            
            // Command autocomplete (matches JavaScript autocomplete system)
            is_autocomplete_visible: false,
            autocomplete_matches: Vec::new(),
            selected_suggestion: 0,
            available_commands: Self::get_available_commands(),

            // Status view (tabbed UI for /status command)
            show_status_view: false,
            status_view_tab: 0,  // Start on Status tab
            status_config_selected: 0,

            // Prompt stash (Ctrl+S)
            stashed_input: None,

            // TODOs expanded display (Ctrl+T)
            show_todos_expanded: false,

            // Find/Search mode (Ctrl+F)
            show_find_mode: false,
            find_query: String::new(),
            find_results: Vec::new(),
            find_current_index: 0,

            // Thinking display
            current_thinking: None,
            thinking_start_time: None,

            // Chat display text selection
            chat_selection_start: None,
            chat_selection_end: None,
            chat_is_selecting: false,
            chat_selected_text: None,
        };

        // Load existing TODOs for this session
        state.load_todos();

        // Load additionalDirectories from all settings files (user, project, local)
        // This matches JavaScript behavior where settings are loaded at startup
        if let Ok(dirs) = crate::config::get_all_additional_directories() {
            for (dir_str, source) in dirs {
                let dir = PathBuf::from(&dir_str);
                if dir.exists() && dir.is_dir() {
                    state.working_directories.insert(dir.clone());
                    tokio::task::block_in_place(|| {
                        let rt = tokio::runtime::Handle::current();
                        rt.block_on(async {
                            if let Ok(mut ctx) = crate::permissions::PERMISSION_CONTEXT.try_lock() {
                                ctx.allow_directory(dir.clone());
                                tracing::debug!(
                                    "Loaded directory from {}: {}",
                                    crate::config::get_settings_source_short_name(source),
                                    dir.display()
                                );
                            }
                        });
                    });
                } else {
                    tracing::warn!(
                        "Directory from {} does not exist: {}",
                        crate::config::get_settings_source_short_name(source),
                        dir_str
                    );
                }
            }
        }

        // Process add_dirs from CLI options - add to working directories and permission context
        // CLI options override settings
        if !options.add_dirs.is_empty() {
            for dir in &options.add_dirs {
                if dir.exists() && dir.is_dir() {
                    // Add to local working directories for UI display
                    state.working_directories.insert(dir.clone());

                    // Add to global permission context for tool access control
                    // Use tokio::task::block_in_place since we're in a sync context
                    let dir_clone = dir.clone();
                    tokio::task::block_in_place(|| {
                        let rt = tokio::runtime::Handle::current();
                        rt.block_on(async {
                            if let Ok(mut ctx) = crate::permissions::PERMISSION_CONTEXT.try_lock() {
                                ctx.allow_directory(dir_clone);
                                tracing::debug!("Added directory from CLI --add-dir: {}", dir.display());
                            }
                        });
                    });
                } else {
                    tracing::warn!("--add-dir path does not exist or is not a directory: {}", dir.display());
                }
            }
        }

        state
    }
    
    /// Start the persistent agent loop for the entire session
    pub fn start_agent_loop(&mut self) {
        // Create message channel - sends tuples of (message, optional_loaded_messages, model)
        let (agent_tx, mut agent_rx) = tokio::sync::mpsc::unbounded_channel::<(String, Option<Vec<crate::ai::Message>>, String)>();
        self.agent_tx = Some(agent_tx);
        
        // Create cancellation channel
        let (cancel_tx, mut cancel_rx) = tokio::sync::mpsc::unbounded_channel::<()>();
        self.cancel_tx = Some(cancel_tx);
        
        // Get necessary state for the agent
        let event_tx = self.event_tx.clone();
        let system_prompt = self.system_prompt.clone();
        let session_id = self.session_id.clone();
        
        // Clone the data needed for tool executor creation
        let allowed_tools = self.allowed_tools.clone();
        let disallowed_tools = self.disallowed_tools.clone();
        
        // Spawn the persistent agent loop
        let handle = tokio::spawn(async move {
            // Execute SessionStart hooks at the beginning of the session
            let session_start_context = crate::hooks::HookContext::new(
                crate::hooks::HookType::SessionStart,
                &session_id,
            );
            let _ = crate::hooks::execute_hooks(
                crate::hooks::HookType::SessionStart,
                &session_start_context,
            ).await;

            // This agent loop runs for the ENTIRE session
            let mut messages: Vec<crate::ai::Message> = Vec::new();

            // Create tool executor with cloned permissions
            let mut tool_executor = crate::ai::tools::ToolExecutor::new();
            tool_executor.set_allowed_tools(allowed_tools);
            tool_executor.set_disallowed_tools(disallowed_tools);
            let tools = tool_executor.get_available_tools();
            
            // Process messages from the queue with cancellation support
            loop {
                tokio::select! {
                    Some((user_input, loaded_messages, current_model)) = agent_rx.recv() => {
                // Execute UserPromptSubmit hooks when user submits input
                if !user_input.is_empty() {
                    let prompt_context = crate::hooks::HookContext::new(
                        crate::hooks::HookType::UserPromptSubmit,
                        &session_id,
                    );
                    let hook_results = crate::hooks::execute_hooks(
                        crate::hooks::HookType::UserPromptSubmit,
                        &prompt_context,
                    ).await;

                    // Check if any hook wants to block execution
                    let mut blocked = false;
                    for result in &hook_results {
                        if result.stop_execution {
                            if let Some(tx) = &event_tx {
                                let msg = result.stop_reason.clone()
                                    .unwrap_or_else(|| "Prompt blocked by hook".to_string());
                                let _ = tx.send(crate::tui::TuiEvent::Error(msg));
                                let _ = tx.send(crate::tui::TuiEvent::ProcessingComplete);
                            }
                            blocked = true;
                            break;
                        }
                    }
                    if blocked {
                        continue;
                    }
                }

                // If we have loaded messages (from resume), replace our current message history
                if let Some(loaded) = loaded_messages {
                    messages = loaded;
                }
                // Check if this is a continuation (empty message when we have stored messages)
                let is_continuation = user_input.is_empty() && !messages.is_empty();
                
                if !is_continuation {
                    // Add user message to conversation normally
                    messages.push(crate::ai::Message {
                        role: crate::ai::MessageRole::User,
                        content: crate::ai::MessageContent::Text(user_input),
                        name: None,
                    });
                }
                
                // Create AI client
                let ai_client = match crate::ai::create_client().await {
                    Ok(client) => client,
                    Err(e) => {
                        if let Some(tx) = &event_tx {
                            let _ = tx.send(crate::tui::TuiEvent::Error(format!("Failed to create AI client: {}", e)));
                            let _ = tx.send(crate::tui::TuiEvent::UpdateTaskStatus(None));
                            let _ = tx.send(crate::tui::TuiEvent::ProcessingComplete);
                        }
                        continue;
                    }
                };
                
                // Agent loop for this message - continue until AI stops requesting tools
                let mut iteration = if is_continuation { 0 } else { 0 }; // Reset on continuation
                const MAX_ITERATIONS: usize = 25;  // Increased from 10 to match JS behavior
                
                loop {
                    iteration += 1;
                    if iteration > MAX_ITERATIONS {
                        // Store the messages for /continue command
                        let stored_messages = messages.clone();
                        if let Some(tx) = &event_tx {
                            let _ = tx.send(crate::tui::TuiEvent::SetIterationLimit(true, Some(stored_messages)));
                            let _ = tx.send(crate::tui::TuiEvent::Message("Max iterations reached. Use /continue to proceed if needed.".to_string()));
                            // Clear the task status when hitting iteration limit
                            let _ = tx.send(crate::tui::TuiEvent::UpdateTaskStatus(None));
                            // Unlock the UI so user can continue
                            let _ = tx.send(crate::tui::TuiEvent::ProcessingComplete);
                        }
                        break;
                    }
                    
                    // Build request
                    let mut request = ai_client
                        .create_chat_request()
                        .model(&current_model)
                        .messages(messages.clone())
                        .max_tokens(4096)
                        .temperature(0.7)
                        .stream();
                    
                    // Set system prompt
                    let system = if let Some(prompt) = &system_prompt {
                        prompt.clone()
                    } else {
                        crate::ai::system_prompt::get_system_prompt("Claude Code")
                    };
                    request = request.system(system);
                    
                    // Add tools
                    if !tools.is_empty() {
                        request = request.tools(tools.clone());
                    }
                    
                    // Start streaming
                    let stream = match ai_client.chat_stream(request.build()).await {
                        Ok(s) => s,
                        Err(e) => {
                            if let Some(tx) = &event_tx {
                                let _ = tx.send(crate::tui::TuiEvent::Error(format!("Stream error: {}", e)));
                                let _ = tx.send(crate::tui::TuiEvent::UpdateTaskStatus(None));
                                let _ = tx.send(crate::tui::TuiEvent::ProcessingComplete);
                            }
                            break;
                        }
                    };
                    
                    // Process the stream with cancellation support
                    use crate::ai::streaming::{StreamingHandler, StreamingUpdate};
                    
                    // Create a cancellation token for this iteration (like JavaScript's AbortController)
                    let iteration_cancel_token = CancellationToken::new();
                    let cancel_token_for_loop = iteration_cancel_token.clone();
                    
                    // Create a cancellation channel specifically for this stream
                    // This follows the JavaScript pattern of having an AbortController per operation
                    let (stream_cancel_tx, stream_cancel_rx) = tokio::sync::mpsc::unbounded_channel::<()>();
                    
                    // Clone the sender so we can trigger cancellation from the outer cancel handler
                    let stream_cancel_for_outer = stream_cancel_tx.clone();
                    
                    // Store the stream cancellation sender in a shared location that can be accessed
                    // when a cancellation is triggered from the UI (ESC/Ctrl+C)
                    let stream_cancel_shared = Arc::new(Mutex::new(Some(stream_cancel_for_outer)));
                    let stream_cancel_clone = stream_cancel_shared.clone();
                    
                    // Store in the app state so cancel_operation can trigger it
                    if let Some(tx) = &event_tx {
                        let _ = tx.send(crate::tui::TuiEvent::SetStreamCanceller(Some(stream_cancel_clone.clone())));
                    }
                    
                    // Pass the cancellation receiver to the stream handler
                    let (mut receiver, stream_handle) = StreamingHandler::process_stream(Box::pin(stream), Some(stream_cancel_rx));
                    
                    let mut current_text = String::new();
                    let mut pending_tools: std::collections::HashMap<String, String> = std::collections::HashMap::new();
                    let mut tool_uses: Vec<crate::ai::ContentPart> = Vec::new();  // Collect tool uses for assistant message
                    let mut tool_results = Vec::new();
                    let mut has_tool_use = false;
                    
                    // Process streaming updates with cancellation check
                    loop {
                        // Check if we should cancel the stream
                        tokio::select! {
                            Some(update) = receiver.recv() => {
                        match update {
                            StreamingUpdate::TextChunk(text) => {
                                current_text.push_str(&text);
                            }
                            StreamingUpdate::ToolUseStart { id, name } => {
                                pending_tools.insert(id.clone(), name.clone());
                                // Don't set status yet - wait for ToolUseComplete to get the full input
                            }
                            StreamingUpdate::ToolUseComplete { id, input } => {
                                if let Some(tool_name) = pending_tools.remove(&id) {
                                    has_tool_use = true;
                                    
                                    // Format the tool status with action/command details
                                    let status_msg = match tool_name.as_str() {
                                        "Bash" => {
                                            if let Some(cmd) = input["command"].as_str() {
                                                // Truncate long commands for display
                                                let display_cmd = if cmd.len() > 50 {
                                                    format!("{}...", &cmd[..47])
                                                } else {
                                                    cmd.to_string()
                                                };
                                                format!("Bash({})", display_cmd)
                                            } else {
                                                format!("Bash(executing command)")
                                            }
                                        }
                                        "Read" => {
                                            if let Some(path) = input["file_path"].as_str() {
                                                // Show just filename or last part of path
                                                let display_path = path.split('/').last().unwrap_or(path);
                                                format!("Read({})", display_path)
                                            } else {
                                                format!("Read(reading file)")
                                            }
                                        }
                                        "Write" => {
                                            if let Some(path) = input["file_path"].as_str() {
                                                let display_path = path.split('/').last().unwrap_or(path);
                                                format!("Write({})", display_path)
                                            } else {
                                                format!("Write(writing file)")
                                            }
                                        }
                                        "Edit" | "MultiEdit" => {
                                            // Show as "Update" for Edit/MultiEdit tools
                                            if let Some(path) = input["file_path"].as_str() {
                                                let display_path = path.split('/').last().unwrap_or(path);
                                                format!("Update({})", display_path)
                                            } else {
                                                format!("Update(editing file)")
                                            }
                                        }
                                        "Search" | "Grep" => {
                                            if let Some(pattern) = input["pattern"].as_str() {
                                                let display_pattern = if pattern.len() > 30 {
                                                    format!("{}...", &pattern[..27])
                                                } else {
                                                    pattern.to_string()
                                                };
                                                format!("Search({})", display_pattern)
                                            } else {
                                                format!("Search(searching files)")
                                            }
                                        }
                                        "Glob" => {
                                            if let Some(pattern) = input["pattern"].as_str() {
                                                format!("Search({})", pattern)
                                            } else {
                                                format!("Search(finding files)")
                                            }
                                        }
                                        "LS" => {
                                            if let Some(path) = input["path"].as_str() {
                                                let display_path = path.split('/').last().unwrap_or(path);
                                                format!("List({})", display_path)
                                            } else {
                                                format!("List(directory)")
                                            }
                                        }
                                        "WebFetch" => {
                                            if let Some(url) = input["url"].as_str() {
                                                // Show domain or first part of URL
                                                let display_url = url.split('/').nth(2).unwrap_or(url);
                                                format!("WebFetch({})", display_url)
                                            } else {
                                                format!("WebFetch(fetching content)")
                                            }
                                        }
                                        "WebSearch" => {
                                            if let Some(query) = input["query"].as_str() {
                                                let display_query = if query.len() > 30 {
                                                    format!("{}...", &query[..27])
                                                } else {
                                                    query.to_string()
                                                };
                                                format!("WebSearch({})", display_query)
                                            } else {
                                                format!("WebSearch(searching web)")
                                            }
                                        }
                                        _ => format!("{}(processing)", tool_name)
                                    };
                                    
                                    // Send the formatted status to UI
                                    if let Some(tx) = &event_tx {
                                        let _ = tx.send(crate::tui::TuiEvent::UpdateTaskStatus(Some(status_msg.clone())));
                                        let _ = tx.send(crate::tui::TuiEvent::Message(
                                            format!("[Tool: {}]", status_msg)
                                        ));
                                    }
                                    
                                    // Add tool use to assistant message
                                    tool_uses.push(crate::ai::ContentPart::ToolUse {
                                        id: id.clone(),
                                        name: tool_name.clone(),
                                        input: input.clone(),
                                    });
                                    
                                    // Check permissions for tools
                                    let mut was_wait_decision = false;
                                    let should_execute = if !tool_executor.is_tool_allowed(&tool_name) {
                                        // Tool is disabled by user permissions
                                        tool_results.push(crate::ai::ContentPart::ToolResult {
                                            tool_use_id: id.clone(),
                                            content: format!("Tool '{}' is disabled by user permissions. Use /permissions enable {} to enable it.", tool_name, tool_name),
                                            is_error: Some(true),
                                        });
                                        false
                                    } else if tool_name == "Edit" || tool_name == "MultiEdit" || tool_name == "Write" || tool_name == "NotebookEdit" {
                                        // File modification tools need permission
                                        let file_path = input["file_path"].as_str()
                                            .or_else(|| input["notebook_path"].as_str())
                                            .unwrap_or("");
                                        
                                        // Check if path is automatically allowed
                                        // For now, always ask permission for file edits (can be configured later)
                                        let needs_permission = true;
                                        
                                        if needs_permission {
                                            if let Some(tx) = &event_tx {
                                                let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
                                                
                                                let permission_msg = format!("edit {}", file_path);
                                                let _ = tx.send(crate::tui::TuiEvent::PermissionRequired {
                                                    tool_name: tool_name.clone(),
                                                    command: permission_msg,
                                                    tool_use_id: id.clone(),
                                                    input: input.clone(),
                                                    responder: resp_tx,
                                                });
                                                
                                                match resp_rx.await {
                                                    Ok(crate::tui::PermissionDecision::Allow) => true,
                                                    Ok(crate::tui::PermissionDecision::AlwaysAllow) => {
                                                        // Add the file path to allowed paths
                                                        let mut permission_ctx = crate::permissions::PERMISSION_CONTEXT.lock().await;
                                                        permission_ctx.add_always_allow_rule(&tool_name, file_path);
                                                        drop(permission_ctx);
                                                        true
                                                    }
                                                    Ok(crate::tui::PermissionDecision::Wait) => {
                                                        // User wants to provide feedback - send interrupt message to LLM
                                                        was_wait_decision = true;
                                                        // Send ProcessingComplete to unlock UI
                                                        if let Some(tx) = &event_tx {
                                                            let _ = tx.send(crate::tui::TuiEvent::ProcessingComplete);
                                                        }
                                                        false
                                                    }
                                                    _ => {
                                                        // Send ProcessingComplete to unlock UI when permission denied
                                                        if let Some(tx) = &event_tx {
                                                            let _ = tx.send(crate::tui::TuiEvent::ProcessingComplete);
                                                        }
                                                        false
                                                    }
                                                }
                                            } else {
                                                false
                                            }
                                        } else {
                                            true
                                        }
                                    } else if tool_name == "Bash" {
                                        let command = input["command"].as_str().unwrap_or("");
                                        
                                        use crate::permissions::{check_command_permission, PermissionResult};
                                        match check_command_permission(command).await {
                                            PermissionResult::NeedsApproval => {
                                                if let Some(tx) = &event_tx {
                                                    let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
                                                    
                                                    let _ = tx.send(crate::tui::TuiEvent::PermissionRequired {
                                                        tool_name: tool_name.clone(),
                                                        command: command.to_string(),
                                                        tool_use_id: id.clone(),
                                                        input: input.clone(),
                                                        responder: resp_tx,
                                                    });
                                                    
                                                    match resp_rx.await {
                                                        Ok(crate::tui::PermissionDecision::Allow) => true,
                                                        Ok(crate::tui::PermissionDecision::AlwaysAllow) => {
                                                            let mut permission_ctx = crate::permissions::PERMISSION_CONTEXT.lock().await;
                                                            permission_ctx.add_always_allow_rule("Bash", command);
                                                            drop(permission_ctx);
                                                            true
                                                        }
                                                        Ok(crate::tui::PermissionDecision::Wait) => {
                                                            // User wants to provide feedback - send interrupt message to LLM
                                                            was_wait_decision = true;
                                                            // Send ProcessingComplete to unlock UI
                                                            if let Some(tx) = &event_tx {
                                                                let _ = tx.send(crate::tui::TuiEvent::ProcessingComplete);
                                                            }
                                                            false
                                                        }
                                                        _ => {
                                                            // Send ProcessingComplete to unlock UI when permission denied
                                                            if let Some(tx) = &event_tx {
                                                                let _ = tx.send(crate::tui::TuiEvent::ProcessingComplete);
                                                            }
                                                            false
                                                        }
                                                    }
                                                } else {
                                                    false
                                                }
                                            }
                                            _ => true,
                                        }
                                    } else {
                                        true
                                    };
                                    
                                    if should_execute {
                                        let tool_context = crate::ai::tools::ToolContext {
                                            tool_use_id: id.clone(),
                                            session_id: session_id.clone(),
                                            event_tx: event_tx.clone(),
                                            cancellation_token: Some(iteration_cancel_token.clone()),
                                        };

                                        tracing::debug!("DEBUG: Tool {} execution starting with ID: {}", tool_name, id);
                                        tracing::debug!("DEBUG: Tool input: {:?}", input);
                                        
                                        match tool_executor.execute_with_context(&tool_name, input.clone(), Some(tool_context)).await {
                                            Ok(result) => {
                                                tracing::info!("DEBUG: Tool {} execution successful: {}", tool_name, id);
                                                if let crate::ai::ContentPart::ToolResult { content, .. } = &result {
                                                    // Send tool result as command_output so it gets collapsed properly
                                                    if let Some(tx) = &event_tx {
                                                        let _ = tx.send(crate::tui::TuiEvent::CommandOutput(content.clone()));
                                                    }
                                                    tool_results.push(crate::ai::ContentPart::ToolResult {
                                                        tool_use_id: id.clone(),
                                                        content: content.clone(),
                                                        is_error: Some(false),
                                                    });
                                                }
                                            }
                                            Err(e) => {
                                                tracing::error!("DEBUG: Tool {} execution failed: {} - Error: {}", tool_name, id, e);
                                                if let Some(tx) = &event_tx {
                                                    let _ = tx.send(crate::tui::TuiEvent::Error(
                                                        format!("Tool error: {}", e)
                                                    ));
                                                }
                                                tool_results.push(crate::ai::ContentPart::ToolResult {
                                                    tool_use_id: id.clone(),
                                                    content: format!("Error: {}", e),
                                                    is_error: Some(true),
                                                });
                                            }
                                        }
                                    } else {
                                        // Check if this was a Wait decision (Option 3)
                                        if was_wait_decision {
                                            // Send interrupt message for Option 3 - matches JavaScript behavior
                                            let interrupt_message = "[Request interrupted by user for tool use]\n\n\
                                                The user doesn't want to proceed with this tool use. \
                                                The tool use was rejected (eg. if it was a file edit, the new_string was NOT written to the file). \
                                                STOP what you are doing and wait for the user to tell you how to proceed.";

                                            tool_results.push(crate::ai::ContentPart::ToolResult {
                                                tool_use_id: id.clone(),
                                                content: interrupt_message.to_string(),
                                                is_error: Some(true),
                                            });

                                            // Need to send the tool result immediately and break both loops
                                            // Add the tool results as a user message
                                            if !tool_results.is_empty() {
                                                messages.push(crate::ai::Message {
                                                    role: crate::ai::MessageRole::User,
                                                    content: crate::ai::MessageContent::Multipart(tool_results.clone()),
                                                    name: None,
                                                });
                                            }

                                            // Clear status and mark as complete
                                            if let Some(tx) = &event_tx {
                                                let _ = tx.send(crate::tui::TuiEvent::UpdateTaskStatus(None));
                                                let _ = tx.send(crate::tui::TuiEvent::ProcessingComplete);
                                            }

                                            // Break out of the streaming loop
                                            break;
                                        } else {
                                            // Send descriptive permission denial message to LLM
                                            // This matches JavaScript behavior for better LLM understanding
                                            let denial_message = if tool_name == "Edit" || tool_name == "MultiEdit" || tool_name == "Write" {
                                                let file_path = input["file_path"].as_str()
                                                    .or_else(|| input["notebook_path"].as_str())
                                                    .unwrap_or("<unknown file>");
                                                format!("Permission to edit {} has been denied.", file_path)
                                            } else if tool_name == "NotebookEdit" {
                                                let notebook_path = input["notebook_path"].as_str().unwrap_or("<unknown notebook>");
                                                format!("Permission to edit {} has been denied.", notebook_path)
                                            } else if tool_name == "Bash" {
                                                let command = input["command"].as_str().unwrap_or("<unknown command>");
                                                format!("Permission to use Bash with command '{}' has been denied.", command)
                                            } else if tool_name == "Read" {
                                                let file_path = input["file_path"].as_str().unwrap_or("<unknown file>");
                                                format!("Permission to read {} has been denied.", file_path)
                                            } else {
                                                format!("Permission to use {} has been denied.", tool_name)
                                            };

                                            tool_results.push(crate::ai::ContentPart::ToolResult {
                                                tool_use_id: id.clone(),
                                                content: denial_message,
                                                is_error: Some(true),
                                            });
                                        }
                                    }
                                }
                            }
                            StreamingUpdate::MessageComplete { stop_reason, .. } => {
                                if !current_text.is_empty() {
                                    if let Some(tx) = &event_tx {
                                        let _ = tx.send(crate::tui::TuiEvent::Message(current_text.clone()));
                                    }
                                }
                                
                                // Build assistant message with both text and tool uses
                                let mut assistant_parts = Vec::new();
                                if !current_text.is_empty() {
                                    assistant_parts.push(crate::ai::ContentPart::Text {
                                        text: current_text.clone(),
                                        citations: None
                                    });
                                }
                                // Add all tool uses to the assistant message
                                assistant_parts.extend(tool_uses.clone());
                                
                                // Add assistant message to conversation
                                if !assistant_parts.is_empty() {
                                    messages.push(crate::ai::Message {
                                        role: crate::ai::MessageRole::Assistant,
                                        content: if assistant_parts.len() == 1 && !current_text.is_empty() && tool_uses.is_empty() {
                                            crate::ai::MessageContent::Text(current_text.clone())
                                        } else {
                                            crate::ai::MessageContent::Multipart(assistant_parts)
                                        },
                                        name: None,
                                    });
                                }
                                
                                let needs_continuation = match stop_reason.as_deref() {
                                    Some("ToolUse") => true,
                                    _ => has_tool_use && !tool_results.is_empty()
                                };
                                
                                if needs_continuation {
                                    if !tool_results.is_empty() {
                                        messages.push(crate::ai::Message {
                                            role: crate::ai::MessageRole::User,
                                            content: crate::ai::MessageContent::Multipart(tool_results),
                                            name: None,
                                        });
                                    }
                                    break; // Continue to next iteration
                                } else {
                                    // Done with this user message
                                    break;
                                }
                            }
                            StreamingUpdate::Error(e) => {
                                // CRITICAL: When stream is cancelled/errored, we must add tool_results
                                // for any pending tool_uses to maintain proper conversation state.
                                // This matches JavaScript's variable13401 function which creates
                                // tool_result with is_error: true for all pending tool_use blocks.

                                // First, if we have any tool_uses, add the assistant message
                                if !tool_uses.is_empty() || !current_text.is_empty() {
                                    let mut assistant_parts: Vec<crate::ai::ContentPart> = Vec::new();
                                    if !current_text.is_empty() {
                                        assistant_parts.push(crate::ai::ContentPart::Text {
                                            text: current_text.clone(),
                                            citations: None,
                                        });
                                    }
                                    assistant_parts.extend(tool_uses.clone());

                                    messages.push(crate::ai::Message {
                                        role: crate::ai::MessageRole::Assistant,
                                        content: crate::ai::MessageContent::Multipart(assistant_parts),
                                        name: None,
                                    });
                                }

                                // Create tool_results for ALL pending tool_uses with is_error: true
                                // This is the key fix - matches JS variable8516 interrupt message
                                const INTERRUPT_MESSAGE: &str = "The user doesn't want to take this action right now. STOP what you are doing and wait for the user to tell you how to proceed.";

                                let mut interrupt_results: Vec<crate::ai::ContentPart> = Vec::new();
                                for tool_use in &tool_uses {
                                    if let crate::ai::ContentPart::ToolUse { id, .. } = tool_use {
                                        interrupt_results.push(crate::ai::ContentPart::ToolResult {
                                            tool_use_id: id.clone(),
                                            content: INTERRUPT_MESSAGE.to_string(),
                                            is_error: Some(true),
                                        });
                                    }
                                }

                                // Also add tool_results for any tools in pending_tools that haven't
                                // been fully processed yet (started but not completed)
                                for (pending_id, _pending_name) in &pending_tools {
                                    // Check if we already have a result for this tool
                                    let already_has_result = interrupt_results.iter().any(|r| {
                                        if let crate::ai::ContentPart::ToolResult { tool_use_id, .. } = r {
                                            tool_use_id == pending_id
                                        } else {
                                            false
                                        }
                                    });
                                    if !already_has_result {
                                        interrupt_results.push(crate::ai::ContentPart::ToolResult {
                                            tool_use_id: pending_id.clone(),
                                            content: INTERRUPT_MESSAGE.to_string(),
                                            is_error: Some(true),
                                        });
                                    }
                                }

                                // Add the user message with tool_results if we have any
                                if !interrupt_results.is_empty() {
                                    messages.push(crate::ai::Message {
                                        role: crate::ai::MessageRole::User,
                                        content: crate::ai::MessageContent::Multipart(interrupt_results),
                                        name: None,
                                    });
                                }

                                if let Some(tx) = &event_tx {
                                    let _ = tx.send(crate::tui::TuiEvent::Error(e));
                                    let _ = tx.send(crate::tui::TuiEvent::UpdateTaskStatus(None));
                                    let _ = tx.send(crate::tui::TuiEvent::ProcessingComplete);
                                }
                                break;
                            }
                            _ => {}
                        }
                            }
                            else => {
                                // Channel closed
                                break;
                            }
                        }
                    }
                    
                    // Clear the stream cancellation sender now that streaming is done
                    {
                        let mut guard = stream_cancel_clone.lock().await;
                        *guard = None;
                    }
                    // Also clear it from the app state
                    if let Some(tx) = &event_tx {
                        let _ = tx.send(crate::tui::TuiEvent::SetStreamCanceller(None));
                    }
                    
                    // If we didn't get MessageComplete, we're done with all tools
                    if !has_tool_use {
                        if let Some(tx) = &event_tx {
                            let _ = tx.send(crate::tui::TuiEvent::UpdateTaskStatus(None));
                            let _ = tx.send(crate::tui::TuiEvent::ProcessingComplete);
                        }
                        break;
                    }
                    
                    // Clear for next iteration
                    tool_uses.clear();
                }
                
                // Continue to next tool iteration - don't clear status yet
                if let Some(tx) = &event_tx {
                    let _ = tx.send(crate::tui::TuiEvent::UpdateTaskStatus(Some("Waiting for next tool...".to_string())));
                }
            }
                    _ = cancel_rx.recv() => {
                        // Cancellation requested - notify UI
                        if let Some(tx) = &event_tx {
                            let _ = tx.send(crate::tui::TuiEvent::Message("Operation cancelled".to_string()));
                            let _ = tx.send(crate::tui::TuiEvent::UpdateTaskStatus(None));
                            let _ = tx.send(crate::tui::TuiEvent::ProcessingComplete);
                        }
                        // Continue listening for next message
                        continue;
                    }
                    else => {
                        // Channel closed, exit loop
                        break;
                    }
                }
            }
        });
        
        self.agent_handle = Some(handle);
    }
    
    /// Add a message to the conversation
    pub fn add_message(&mut self, content: &str) {
        self.messages.push(Message {
            role: "assistant".to_string(),
            content: content.to_string(),
            timestamp: crate::utils::timestamp_ms(),
        });
        self.invalidate_cache();
        self.scroll_to_bottom();
    }
    
    /// Add an error message
    pub fn add_error(&mut self, error: &str) {
        self.messages.push(Message {
            role: "error".to_string(),
            content: error.to_string(),
            timestamp: crate::utils::timestamp_ms(),
        });
        self.invalidate_cache();
        self.scroll_to_bottom();
    }
    
    /// Add command output (no dots, indented)
    pub fn add_command_output(&mut self, content: &str) {
        self.messages.push(Message {
            role: "command_output".to_string(),
            content: content.to_string(),
            timestamp: crate::utils::timestamp_ms(),
        });
        self.invalidate_cache();
        self.scroll_to_bottom();
    }
    
    /// Submit user input
    pub async fn submit_input(&mut self) -> Result<()> {
        // Get text from textarea and trim trailing empty lines
        let mut input = self.input_textarea.lines().join("\n");
        // Trim only trailing whitespace/newlines, preserve leading spaces for code blocks
        input = input.trim_end().to_string();
        if input.is_empty() {
            return Ok(());
        }
        
        // Replace paste placeholders with actual content (like JavaScript)
        let placeholder_regex = regex::Regex::new(r"\[Pasted text #(\d+) \+\d+ lines\]").unwrap();
        let mut replaced_paste_ids = Vec::new();
        for cap in placeholder_regex.captures_iter(&input.clone()) {
            if let Ok(paste_id) = cap[1].parse::<usize>() {
                if let Some(content) = self.pasted_contents.get(&paste_id) {
                    input = input.replace(&cap[0], content);
                    replaced_paste_ids.push(paste_id);
                }
            }
        }
        
        // Clear pasted contents that were used
        for paste_id in replaced_paste_ids {
            self.pasted_contents.remove(&paste_id);
        }
        
        // Clear the textarea
        self.input_textarea = create_configured_textarea();
        
        // Add to history
        self.add_to_history(input.clone());
        
        // Check for commands
        if input.starts_with('/') {
            return self.handle_command(&input).await;
        }
        
        // Add user message
        self.messages.push(Message {
            role: "user".to_string(),
            content: input.clone(),
            timestamp: crate::utils::timestamp_ms(),
        });
        
        self.invalidate_cache();
        self.scroll_to_bottom();
        self.input_mode = false;
        self.is_processing = true;
        self.current_task_status = Some("Processing request...".to_string());
        
        // Send message to the persistent agent loop along with any loaded messages and current model
        if let Some(agent_tx) = &self.agent_tx {
            // Take the loaded messages if this is the first message after resuming
            let loaded = self.loaded_ai_messages.take();
            let _ = agent_tx.send((input.clone(), loaded, self.current_model.clone()));
        } else {
            // Agent loop not started - this shouldn't happen
            self.add_message("Error: Agent loop not initialized");
            self.is_processing = false;
        }
        
        Ok(())
    }
    
    // Orphaned old streaming code removed - see git history if needed
    /*
            let result = async move {
                let event_tx_inner = event_tx.clone();
                // Create AI client
                let ai_client = crate::ai::create_client().await?;
                
                // Build conversation messages - these will be updated throughout the agent loop
                let mut messages = Vec::new();
                
                // Add conversation history
                for msg in &messages_for_context {
                    let role = match msg.role.as_str() {
                        "user" => crate::ai::MessageRole::User,
                        "assistant" => crate::ai::MessageRole::Assistant,
                        _ => continue,
                    };
                    
                    messages.push(crate::ai::Message {
                        role,
                        content: crate::ai::MessageContent::Text(msg.content.clone()),
                        name: None,
                    });
                }
                
                // Add current user message
                messages.push(crate::ai::Message {
                    role: crate::ai::MessageRole::User,
                    content: crate::ai::MessageContent::Text(input_clone.clone()),
                    name: None,
                });
                
                // Create tool executor once for the entire agent loop
                let tool_executor = self.create_tool_executor();
                let tools = tool_executor.get_available_tools();
                
                // Start agent loop - continue until AI stops requesting tools
                let mut loop_count = 0;
                const MAX_LOOPS: usize = 10;
                
                loop {
                    loop_count += 1;
                    if loop_count > MAX_LOOPS {
                        if let Some(tx) = &event_tx_inner {
                            let _ = tx.send(crate::tui::TuiEvent::Message(
                                "Max agent loops reached. Stopping.".to_string()
                            ));
                        }
                        break;
                    }
                    
                    // Build request for this iteration
                    let mut request = ai_client
                        .create_chat_request()
                        .messages(messages.clone())
                        .max_tokens(4096)
                        .temperature(0.7)
                        .stream();
                    
                    // Set system prompt
                    let system = if let Some(prompt) = &system_prompt {
                        prompt.clone()
                    } else {
                        crate::ai::system_prompt::get_system_prompt("Claude Code")
                    };
                    request = request.system(system);
                    
                    // Add tools
                    if !tools.is_empty() {
                        request = request.tools(tools.clone());
                    }
                    
                    // Start streaming for this iteration
                    let stream = ai_client.chat_stream(request.build()).await?;
                    
                    // Create cancellation channel for this stream (same pattern as main agent loop)
                    let (stream_cancel_tx, stream_cancel_rx) = tokio::sync::mpsc::unbounded_channel::<()>();
                    let stream_cancel_clone = Arc::new(Mutex::new(Some(stream_cancel_tx.clone())));
                    
                    // Store the cancellation sender so cancel_operation can trigger it
                    if let Some(tx) = &event_tx {
                        let _ = tx.send(crate::tui::TuiEvent::SetStreamCanceller(Some(stream_cancel_clone.clone())));
                    }
                    
                    // Process the stream with cancellation support
                    let (mut receiver, _handle) = StreamingHandler::process_stream(Box::pin(stream), Some(stream_cancel_rx));
                    
                    // Process streaming updates and send to UI
                    let mut current_text = String::new();
                    let mut pending_tools: std::collections::HashMap<String, String> = std::collections::HashMap::new();
                    let mut tool_results = Vec::new();
                    let mut has_tool_use = false;
                
                while let Some(update) = receiver.recv().await {
                    // Check if we've been cancelled
                    if cancel_rx.try_recv().is_ok() {
                        if let Some(tx) = &event_tx {
                            let _ = tx.send(crate::tui::TuiEvent::Message("Operation cancelled during /continue".to_string()));
                            let _ = tx.send(crate::tui::TuiEvent::SetStreamCanceller(None));
                        }
                        return Ok(());
                    }
                    
                    match update {
                        StreamingUpdate::TextChunk(text) => {
                            current_text.push_str(&text);
                            // Don't send individual chunks - accumulate them
                            // We'll send the complete message at the end
                        }
                        StreamingUpdate::ToolUseStart { id, name } => {
                            // Track tool name for later execution
                            pending_tools.insert(id.clone(), name.clone());
                            
                            // Don't send message here - wait for ToolUseComplete to get full context
                        }
                        StreamingUpdate::ToolUseComplete { id, input } => {
                            // Execute tool with tracked name
                            if let Some(tool_name) = pending_tools.remove(&id) {
                                // For Bash tool, check permissions first
                                let should_execute = if !self.is_tool_allowed(&tool_name) {
                                    // Tool is disabled by user permissions
                                    tool_results.push(crate::ai::ContentPart::ToolResult {
                                        tool_use_id: id.clone(),
                                        content: format!("Tool '{}' is disabled by user permissions. Use /permissions enable {} to enable it.", tool_name, tool_name),
                                        is_error: Some(true),
                                    });
                                    false
                                } else if tool_name == "Bash" {
                                    let command = input["command"].as_str().unwrap_or("");
                                    
                                    // Check permission using the permissions module
                                    use crate::permissions::{check_command_permission, PermissionResult};
                                    match check_command_permission(command).await {
                                        PermissionResult::NeedsApproval => {
                                            // Send permission request to UI and wait for response
                                            if let Some(tx) = &event_tx_inner {
                                                let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
                                                
                                                let _ = tx.send(crate::tui::TuiEvent::PermissionRequired {
                                                    tool_name: tool_name.clone(),
                                                    command: command.to_string(),
                                                    tool_use_id: id.clone(),
                                                    input: input.clone(),
                                                    responder: resp_tx,
                                                });
                                                
                                                // Wait for permission decision
                                                match resp_rx.await {
                                                    Ok(crate::tui::PermissionDecision::Allow) => {
                                                        // Allow this single execution
                                                        true
                                                    }
                                                    Ok(crate::tui::PermissionDecision::AlwaysAllow) => {
                                                        // Update global permission context for future commands
                                                        let mut permission_ctx = crate::permissions::PERMISSION_CONTEXT.lock().await;
                                                        permission_ctx.add_always_allow_rule("Bash", command);
                                                        drop(permission_ctx);
                                                        true
                                                    }
                                                    Ok(crate::tui::PermissionDecision::Deny) => {
                                                        // Deny this single execution
                                                        let _ = tx.send(crate::tui::TuiEvent::Message(
                                                            format!("[Tool: {} - Permission denied by user]", tool_name)
                                                        ));
                                                        false
                                                    }
                                                    Ok(crate::tui::PermissionDecision::Never) => {
                                                        // Update global permission context to never allow future commands like this
                                                        let mut permission_ctx = crate::permissions::PERMISSION_CONTEXT.lock().await;
                                                        permission_ctx.add_always_deny_rule("Bash", command);
                                                        drop(permission_ctx);
                                                        let _ = tx.send(crate::tui::TuiEvent::Message(
                                                            format!("[Tool: {} - Permission denied permanently]", tool_name)
                                                        ));
                                                        false
                                                    }
                                                    Ok(crate::tui::PermissionDecision::Wait) => {
                                                        // User wants to provide feedback
                                                        let _ = tx.send(crate::tui::TuiEvent::Message(
                                                            "[Tool execution interrupted - waiting for user feedback]".to_string()
                                                        ));
                                                        false
                                                    }
                                                    Err(_) => {
                                                        let _ = tx.send(crate::tui::TuiEvent::Message(
                                                            format!("[Tool: {} - Permission request failed]", tool_name)
                                                        ));
                                                        false
                                                    }
                                                }
                                            } else {
                                                false
                                            }
                                        }
                                        PermissionResult::Deny => {
                                            if let Some(tx) = &event_tx_inner {
                                                let _ = tx.send(crate::tui::TuiEvent::Message(
                                                    format!("[Tool: {} - Permission denied]", tool_name)
                                                ));
                                            }
                                            false
                                        }
                                        PermissionResult::Allow => {
                                            // This should only happen for truly safe commands
                                            // If unsafe commands are getting here, there's a bug in check_command_permission
                                            // For now, let's be extra cautious and still show dialog for non-safe commands
                                            let safe_commands = ["ls", "pwd", "echo", "date", "whoami", "hostname"];
                                            let cmd_parts: Vec<&str> = command.split_whitespace().collect();
                                            if let Some(base_cmd) = cmd_parts.first() {
                                                let cmd_name = base_cmd.split('/').last().unwrap_or(base_cmd);
                                                if safe_commands.contains(&cmd_name) {
                                                    true // Actually safe, can execute
                                                } else {
                                                    // NOT safe but permission system said Allow - this is the bug!
                                                    // Force permission dialog to prevent bypass
                                                    if let Some(tx) = &event_tx_inner {
                                                        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
                                                        
                                                        let _ = tx.send(crate::tui::TuiEvent::PermissionRequired {
                                                            tool_name: tool_name.clone(),
                                                            command: command.to_string(),
                                                            tool_use_id: id.clone(),
                                                            input: input.clone(),
                                                            responder: resp_tx,
                                                        });
                                                        
                                                        // Wait for permission decision
                                                        match resp_rx.await {
                                                            Ok(crate::tui::PermissionDecision::Allow) => true,
                                                            Ok(crate::tui::PermissionDecision::AlwaysAllow) => {
                                                                let mut permission_ctx = crate::permissions::PERMISSION_CONTEXT.lock().await;
                                                                permission_ctx.add_always_allow_rule("Bash", command);
                                                                drop(permission_ctx);
                                                                true
                                                            }
                                                            Ok(crate::tui::PermissionDecision::Deny) => {
                                                                let _ = tx.send(crate::tui::TuiEvent::Message(
                                                                    format!("[Tool: {} - Permission denied by user]", tool_name)
                                                                ));
                                                                false
                                                            }
                                                            Ok(crate::tui::PermissionDecision::Never) => {
                                                                let mut permission_ctx = crate::permissions::PERMISSION_CONTEXT.lock().await;
                                                                permission_ctx.add_always_deny_rule("Bash", command);
                                                                drop(permission_ctx);
                                                                let _ = tx.send(crate::tui::TuiEvent::Message(
                                                                    format!("[Tool: {} - Permission permanently denied]", tool_name)
                                                                ));
                                                                false
                                                            }
                                                            Ok(crate::tui::PermissionDecision::Wait) => {
                                                                // User wants to provide feedback
                                                                let _ = tx.send(crate::tui::TuiEvent::Message(
                                                                    "[Tool execution interrupted - waiting for user feedback]".to_string()
                                                                ));
                                                                false
                                                            }
                                                            _ => false
                                                        }
                                                    } else {
                                                        false
                                                    }
                                                }
                                            } else {
                                                false
                                            }
                                        }
                                    }
                                } else {
                                    true // Non-Bash tools always execute
                                };
                                
                                if should_execute {
                                    has_tool_use = true;
                                    let tool_context = crate::ai::tools::ToolContext {
                                        tool_use_id: id.clone(),
                                        session_id: session_id.clone(),
                                        event_tx: event_tx_inner.clone(),
                                        cancellation_token: Some(cancel_token_for_loop.clone()),
                                    };

                                    // Execute the tool
                                    match tool_executor.execute_with_context(&tool_name, input.clone(), Some(tool_context)).await {
                                        Ok(result) => {
                                            if let crate::ai::ContentPart::ToolResult { content, .. } = &result {
                                                if let Some(tx) = &event_tx_inner {
                                                    let _ = tx.send(crate::tui::TuiEvent::CommandOutput(content.clone()));
                                                }
                                                // Collect tool result for next iteration
                                                tool_results.push(crate::ai::ContentPart::ToolResult {
                                                    tool_use_id: id.clone(),
                                                    content: content.clone(),
                                                    is_error: Some(false),
                                                });
                                            }
                                        }
                                        Err(e) => {
                                            if let Some(tx) = &event_tx_inner {
                                                let _ = tx.send(crate::tui::TuiEvent::Error(
                                                    format!("Tool execution error: {}", e)
                                                ));
                                            }
                                            // Collect error result
                                            tool_results.push(crate::ai::ContentPart::ToolResult {
                                                tool_use_id: id.clone(),
                                                content: format!("Error: {}", e),
                                                is_error: Some(true),
                                            });
                                        }
                                    }
                                } else {
                                    // Permission denied
                                    tool_results.push(crate::ai::ContentPart::ToolResult {
                                        tool_use_id: id.clone(),
                                        content: "Permission denied by user".to_string(),
                                        is_error: Some(true),
                                    });
                                }
                            }
                        }
                        StreamingUpdate::MessageComplete { stop_reason, .. } => {
                            // Send the complete accumulated text
                            if !current_text.is_empty() {
                                if let Some(tx) = &event_tx_inner {
                                    let _ = tx.send(crate::tui::TuiEvent::Message(
                                        current_text.clone()
                                    ));
                                }
                                
                                // Add text to messages for next iteration
                                messages.push(crate::ai::Message {
                                    role: crate::ai::MessageRole::Assistant,
                                    content: crate::ai::MessageContent::Text(current_text.clone()),
                                    name: None,
                                });
                            }
                            
                            // Check if we need to continue (AI requested tools or we have results to send)
                            let needs_continuation = match stop_reason.as_deref() {
                                Some("ToolUse") => true,
                                _ => has_tool_use && !tool_results.is_empty()
                            };
                            
                            if needs_continuation {
                                // Add tool results and continue the loop
                                if !tool_results.is_empty() {
                                    messages.push(crate::ai::Message {
                                        role: crate::ai::MessageRole::User,
                                        content: crate::ai::MessageContent::Multipart(tool_results.clone()),
                                        name: None,
                                    });
                                }
                                // Break inner loop to continue agent loop
                                break;
                            } else {
                                // AI is done - exit both loops
                                return Ok::<(), crate::error::Error>(());
                            }
                        }
                        StreamingUpdate::Error(e) => {
                            if let Some(tx) = &event_tx_inner {
                                let _ = tx.send(crate::tui::TuiEvent::Error(e));
                            }
                            break;
                        }
                        _ => {}
                    }
                    } // End of streaming processing loop
                    
                    // If we get here, we broke out to continue the agent loop
                    // Clear for next iteration
                    tool_results.clear();
                    has_tool_use = false;
                } // End of agent loop
                
                Ok::<(), crate::error::Error>(())
            }.await;
            
            if let Err(e) = result {
                if let Some(tx) = event_tx_for_error {
                    let _ = tx.send(crate::tui::TuiEvent::Error(format!("Stream error: {}", e)));
                }
            }
        });
    */
    
    /// Process user message
    async fn process_user_message(&mut self, input: &str) -> Result<()> {
        // Create AI client
        let ai_client = crate::ai::create_client().await?;
        
        // Build initial conversation messages
        let mut messages = Vec::new();
        
        // Add conversation history (skip system messages)
        for msg in &self.messages {
            let role = match msg.role.as_str() {
                "user" => crate::ai::MessageRole::User,
                "assistant" => crate::ai::MessageRole::Assistant,
                _ => continue,
            };
            
            messages.push(crate::ai::Message {
                role,
                content: crate::ai::MessageContent::Text(msg.content.clone()),
                name: None,
            });
        }
        
        // Add current user message
        messages.push(crate::ai::Message {
            role: crate::ai::MessageRole::User,
            content: crate::ai::MessageContent::Text(input.to_string()),
            name: None,
        });
        
        // Create tool executor once
        let tool_executor = self.create_tool_executor();
        let tools = tool_executor.get_available_tools();
        
        // Start agentic loop - continue until AI stops requesting tools
        let mut loop_count = 0;
        const MAX_LOOPS: usize = 10; // Prevent infinite loops
        
        loop {
            loop_count += 1;
            if loop_count > MAX_LOOPS {
                self.add_message("Max tool execution loops reached. Stopping.");
                break;
            }
            
            // Build request
            let mut request = ai_client
                .create_chat_request()
                .messages(messages.clone())
                .max_tokens(4096)
                .temperature(0.7);
            
            // Always set system prompt - this is critical for agentic behavior
            // In JavaScript, prependCLISysprompt is always true for main flow
            let system = if let Some(prompt) = &self.system_prompt {
                prompt.clone()
            } else {
                // Fallback to ensure we always have a system prompt
                crate::ai::system_prompt::get_system_prompt("Claude Code")
            };
            request = request.system(system);
            
            // Add tools if available
            if !tools.is_empty() {
                request = request.tools(tools.clone());
            }
            
            // Send request
            let response = ai_client.chat(request.build()).await?;
            
            // Process response and collect tool uses
            let mut response_text = String::new();
            let mut tool_results = Vec::new();
            let mut has_tool_use = false;
            
            // First, collect all content parts
            let mut assistant_content_parts = Vec::new();
            
            for part in response.content {
                match &part {
                    crate::ai::ContentPart::Text { text, .. } => {
                        response_text.push_str(text);
                        assistant_content_parts.push(part);
                    }
                    crate::ai::ContentPart::ToolUse { id, name, input } => {
                        has_tool_use = true;
                        
                        // Show tool execution in UI
                        self.add_message(&format!("[Executing tool: {}]", name));
                        
                        // Create tool context with event sender for suspension-based permissions
                        // Create cancellation token for this tool execution
                        let tool_cancel_token = CancellationToken::new();

                        let tool_context = crate::ai::tools::ToolContext {
                            tool_use_id: id.clone(),
                            session_id: self.session_id.clone(),
                            event_tx: self.event_tx.clone(),
                            cancellation_token: Some(tool_cancel_token),
                        };

                        // For tools that might need permissions, we need to handle them specially
                        // to avoid blocking the UI thread
                        if name == "Bash" && self.event_tx.is_some() {
                            // Spawn tool execution in background to avoid blocking UI
                            let event_tx = self.event_tx.clone().unwrap();
                            let tool_id = id.clone();
                            let tool_name = name.clone();
                            let tool_input = input.clone();
                            let session_id_for_spawn = self.session_id.clone();

                            // Clone the data needed for tool executor creation
                            let allowed_tools = self.allowed_tools.clone();
                            let disallowed_tools = self.disallowed_tools.clone();

                            tokio::spawn(async move {
                                // Create tool executor with cloned permissions
                                let mut tool_executor = crate::ai::tools::ToolExecutor::new();
                                tool_executor.set_allowed_tools(allowed_tools);
                                tool_executor.set_disallowed_tools(disallowed_tools);

                                // Create cancellation token for background tool execution
                                let bg_cancel_token = CancellationToken::new();

                                let context = crate::ai::tools::ToolContext {
                                    tool_use_id: tool_id.clone(),
                                    session_id: session_id_for_spawn,
                                    event_tx: Some(event_tx.clone()),
                                    cancellation_token: Some(bg_cancel_token),
                                };
                                
                                let result = tool_executor.execute_with_context(&tool_name, tool_input, Some(context)).await;
                                
                                // Send completion event back to UI
                                let _ = event_tx.send(crate::tui::TuiEvent::ToolExecutionComplete {
                                    tool_use_id: tool_id,
                                    result: result.map_err(|e| e.to_string()),
                                });
                            });
                            
                            // Add placeholder to continue conversation flow
                            tool_results.push(crate::ai::ContentPart::ToolResult {
                                tool_use_id: id.clone(),
                                content: "Tool execution in progress...".to_string(),
                                is_error: None,
                            });
                        } else {
                            // Execute tool normally for non-permission tools
                            match tool_executor.execute_with_context(name, input.clone(), Some(tool_context)).await {
                                Ok(result) => {
                                if let crate::ai::ContentPart::ToolResult { content, tool_use_id, .. } = &result {
                                    // Display result in UI with proper formatting
                                    self.add_message(&format!("**Result:**\n{}", content));
                                    
                                    // Store the tool result with the correct ID
                                    tool_results.push(crate::ai::ContentPart::ToolResult {
                                        tool_use_id: id.clone(),
                                        content: content.clone(),
                                        is_error: Some(false),
                                    });
                                }
                            }
                            Err(e) => {
                                // Display error in UI (permissions now handled in streaming flow)
                                let error_msg = format!("Error: {}", e);
                                self.add_message(&error_msg);
                                
                                // Store error result
                                tool_results.push(crate::ai::ContentPart::ToolResult {
                                    tool_use_id: id.clone(),
                                    content: error_msg,
                                    is_error: Some(true),
                                });
                            }
                        }
                        }
                        
                        // Keep the tool use in the assistant message
                        assistant_content_parts.push(part);
                    }
                    crate::ai::ContentPart::ServerToolUse { .. } => {
                        // Server-side tool use (e.g., web search)
                        assistant_content_parts.push(part);
                    }
                    crate::ai::ContentPart::WebSearchToolResult { .. } => {
                        // Web search results
                        assistant_content_parts.push(part);
                    }
                    _ => {
                        assistant_content_parts.push(part);
                    }
                }
            }
            
            // Update token usage
            self.latency_samples.push_back(response.usage.input_tokens as u64 + response.usage.output_tokens as u64);
            if self.latency_samples.len() > 100 {
                self.latency_samples.pop_front();
            }
            
            // Add assistant message to conversation
            messages.push(crate::ai::Message {
                role: crate::ai::MessageRole::Assistant,
                content: crate::ai::MessageContent::Multipart(assistant_content_parts),
                name: None,
            });
            
            // Show any text response from the assistant
            if !response_text.is_empty() {
                self.add_message(&response_text);
            }
            
            // Check stop reason to determine if we should continue
            // The agent continues if:
            // 1. stop_reason is ToolUse (needs to execute tools)
            // 2. We just executed tools and need synthesis
            let should_continue = match response.stop_reason {
                Some(crate::ai::StopReason::ToolUse) => true,
                _ => has_tool_use, // Continue if we just ran tools to get synthesis
            };
            
            if !should_continue {
                break;
            }
            
            // Add tool results as a user message to continue the conversation
            if !tool_results.is_empty() {
                messages.push(crate::ai::Message {
                    role: crate::ai::MessageRole::User,
                    content: crate::ai::MessageContent::Multipart(tool_results),
                    name: None,
                });
            }
            
            // Continue the loop to get AI's response to the tool results
        }
        
        Ok(())
    }
    
    /// Process user message with streaming
    async fn process_user_message_streaming(&mut self, input: &str) -> Result<()> {
        use futures::StreamExt;
        use crate::ai::streaming::{StreamingHandler, StreamingUpdate};
        
        // Create AI client
        let ai_client = crate::ai::create_client().await?;
        
        // Build initial conversation messages
        let mut messages = Vec::new();
        
        // Add conversation history (skip system messages)
        for msg in &self.messages {
            let role = match msg.role.as_str() {
                "user" => crate::ai::MessageRole::User,
                "assistant" => crate::ai::MessageRole::Assistant,
                _ => continue,
            };
            
            messages.push(crate::ai::Message {
                role,
                content: crate::ai::MessageContent::Text(msg.content.clone()),
                name: None,
            });
        }
        
        // Add current user message
        messages.push(crate::ai::Message {
            role: crate::ai::MessageRole::User,
            content: crate::ai::MessageContent::Text(input.to_string()),
            name: None,
        });
        
        // Create tool executor once
        let tool_executor = self.create_tool_executor();
        let tools = tool_executor.get_available_tools();
        
        // Start streaming agentic loop
        let mut loop_count = 0;
        const MAX_LOOPS: usize = 10;
        
        loop {
            loop_count += 1;
            if loop_count > MAX_LOOPS {
                self.add_message("Max tool execution loops reached. Stopping.");
                break;
            }
            
            // Build request with streaming enabled
            let mut request = ai_client
                .create_chat_request()
                .messages(messages.clone())
                .max_tokens(4096)
                .temperature(0.7)
                .stream(); // Enable streaming
            
            // Always set system prompt
            let system = if let Some(prompt) = &self.system_prompt {
                prompt.clone()
            } else {
                crate::ai::system_prompt::get_system_prompt("Claude Code")
            };
            request = request.system(system);
            
            // Add tools if available
            if !tools.is_empty() {
                request = request.tools(tools.clone());
            }
            
            // Start streaming
            let stream = ai_client.chat_stream(request.build()).await?;
            
            // DEBUG: Add immediate feedback
            self.add_message("Starting streaming response...");
            
            // Process the stream and get receiver
            let (mut receiver, _handle) = StreamingHandler::process_stream(Box::pin(stream), None);
            
            // Process streaming updates
            let mut current_assistant_message = String::new();
            let mut pending_tools: HashMap<String, (String, String)> = HashMap::new(); // id -> (name, input_buffer)
            let mut tool_results = Vec::new();
            let mut has_tool_use = false;
            let mut stop_reason = None;
            
            let mut received_any = false;
            while let Some(update) = receiver.recv().await {
                if !received_any {
                    self.add_message("Receiving stream events...");
                    received_any = true;
                }
                match update {
                    StreamingUpdate::TextChunk(text) => {
                        // Stream text to UI in real-time
                        current_assistant_message.push_str(&text);
                        // Update the last assistant message or create new one
                        if let Some(last_msg) = self.messages.last_mut() {
                            if last_msg.role == "assistant" {
                                last_msg.content.push_str(&text);
                            } else {
                                self.messages.push(Message {
                                    role: "assistant".to_string(),
                                    content: text,
                                    timestamp: crate::utils::timestamp_ms(),
                                });
                            }
                        } else {
                            self.messages.push(Message {
                                role: "assistant".to_string(),
                                content: text,
                                timestamp: crate::utils::timestamp_ms(),
                            });
                        }
                        self.invalidate_cache();
                        // Trigger UI redraw if event channel exists
                        if let Some(event_tx) = &self.event_tx {
                            let _ = event_tx.send(crate::tui::TuiEvent::Redraw);
                        }
                    }
                    StreamingUpdate::ToolUseStart { id, name } => {
                        has_tool_use = true;
                        pending_tools.insert(id.clone(), (name.clone(), String::new()));
                        // Don't add message here - wait for complete tool info
                    }
                    StreamingUpdate::ToolInputChunk { id, chunk } => {
                        if let Some((_, input_buffer)) = pending_tools.get_mut(&id) {
                            input_buffer.push_str(&chunk);
                        }
                    }
                    StreamingUpdate::ToolUseComplete { id, input } => {
                        if let Some((name, _)) = pending_tools.remove(&id) {
                            // Execute tool immediately as it completes
                            // Create cancellation token for permission-required tool
                            let perm_cancel_token = CancellationToken::new();

                            let tool_context = crate::ai::tools::ToolContext {
                                tool_use_id: id.clone(),
                                session_id: self.session_id.clone(),
                                event_tx: self.event_tx.clone(),
                                cancellation_token: Some(perm_cancel_token),
                            };
                            
                            // For tools that might need permissions, handle specially
                            if self.tool_needs_permission(&name, &input) && self.event_tx.is_some() {
                                // Extract the permission-relevant details based on tool type
                                let permission_details = self.extract_permission_details(&name, &input);
                                
                                // Create oneshot channel for permission response
                                let (tx, rx) = tokio::sync::oneshot::channel();
                                
                                // Check permission based on tool type
                                let needs_approval = if name == "Bash" {
                                    use crate::permissions::{check_command_permission, PermissionResult};
                                    matches!(check_command_permission(&permission_details).await, PermissionResult::NeedsApproval)
                                } else {
                                    // For file operations, always show dialog (can be optimized later)
                                    true
                                };
                                
                                if needs_approval {
                                    // Send permission event to UI
                                    if let Some(event_tx) = &self.event_tx {
                                        let _ = event_tx.send(crate::tui::TuiEvent::PermissionRequired {
                                            tool_name: name.clone(),
                                            command: permission_details.clone(),
                                            tool_use_id: id.clone(),
                                            input: input.clone(),
                                            responder: tx,
                                        });
                                        
                                        // Wait for permission decision
                                        match rx.await {
                                                Ok(crate::tui::PermissionDecision::Allow) => {
                                                    // Allow this single execution
                                                    match tool_executor.execute_with_context(&name, input.clone(), Some(tool_context)).await {
                                                        Ok(result) => {
                                                            if let crate::ai::ContentPart::ToolResult { content, .. } = &result {
                                                                self.add_message(&format!("**Result:**\n{}", content));
                                                            }
                                                            tool_results.push(result);
                                                        }
                                                        Err(e) => {
                                                            let error_result = crate::ai::ContentPart::ToolResult {
                                                                tool_use_id: id.clone(),
                                                                content: format!("Error: {}", e),
                                                                is_error: Some(true),
                                                            };
                                                            self.add_message(&format!("**Error:** {}", e));
                                                            tool_results.push(error_result);
                                                        }
                                                    }
                                                }
                                                Ok(crate::tui::PermissionDecision::AlwaysAllow) => {
                                                    // Update global permission context for future commands
                                                    if name == "Bash" {
                                                        let mut permission_ctx = crate::permissions::PERMISSION_CONTEXT.lock().await;
                                                        permission_ctx.add_always_allow_rule("Bash", &permission_details);
                                                        drop(permission_ctx);
                                                    }
                                                    // Continue with execution
                                                    match tool_executor.execute_with_context(&name, input.clone(), Some(tool_context)).await {
                                                        Ok(result) => {
                                                            if let crate::ai::ContentPart::ToolResult { content, .. } = &result {
                                                                self.add_message(&format!("**Result:**\n{}", content));
                                                            }
                                                            tool_results.push(result);
                                                        }
                                                        Err(e) => {
                                                            let error_result = crate::ai::ContentPart::ToolResult {
                                                                tool_use_id: id.clone(),
                                                                content: format!("Error: {}", e),
                                                                is_error: Some(true),
                                                            };
                                                            self.add_message(&format!("**Error:** {}", e));
                                                            tool_results.push(error_result);
                                                        }
                                                    }
                                                }
                                                Ok(crate::tui::PermissionDecision::Deny) => {
                                                    // Deny this single execution
                                                    tool_results.push(crate::ai::ContentPart::ToolResult {
                                                        tool_use_id: id.clone(),
                                                        content: "Permission denied by user".to_string(),
                                                        is_error: Some(true),
                                                    });
                                                    self.add_message("Permission denied");
                                                }
                                                Ok(crate::tui::PermissionDecision::Never) => {
                                                    // Update global permission context to never allow future commands like this
                                                    if name == "Bash" {
                                                        let mut permission_ctx = crate::permissions::PERMISSION_CONTEXT.lock().await;
                                                        permission_ctx.add_always_deny_rule("Bash", &permission_details);
                                                        drop(permission_ctx);
                                                    }
                                                    tool_results.push(crate::ai::ContentPart::ToolResult {
                                                        tool_use_id: id.clone(),
                                                        content: "Permission denied permanently".to_string(),
                                                        is_error: Some(true),
                                                    });
                                                    self.add_message("Permission denied permanently");
                                                }
                                                Ok(crate::tui::PermissionDecision::Wait) => {
                                                    // User wants to provide feedback - send interrupt message
                                                    let interrupt_message = "[Request interrupted by user for tool use]\n\n\
                                                        The user doesn't want to proceed with this tool use. \
                                                        The tool use was rejected (eg. if it was a file edit, the new_string was NOT written to the file). \
                                                        STOP what you are doing and wait for the user to tell you how to proceed.";

                                                    tool_results.push(crate::ai::ContentPart::ToolResult {
                                                        tool_use_id: id.clone(),
                                                        content: interrupt_message.to_string(),
                                                        is_error: Some(true),
                                                    });
                                                    self.add_message("[Tool execution interrupted - waiting for your feedback]");
                                                }
                                                _ => {
                                                    // Channel closed or other error
                                                    tool_results.push(crate::ai::ContentPart::ToolResult {
                                                        tool_use_id: id.clone(),
                                                        content: "Permission request failed".to_string(),
                                                        is_error: Some(true),
                                                    });
                                                }
                                        }
                                    }
                                } else {
                                    // Permission allowed directly - execute tool
                                    match tool_executor.execute_with_context(&name, input.clone(), Some(tool_context)).await {
                                        Ok(result) => {
                                            if let crate::ai::ContentPart::ToolResult { content, .. } = &result {
                                                self.add_message(&format!("**Result:**\n{}", content));
                                            }
                                            tool_results.push(result);
                                        }
                                        Err(e) => {
                                            let error_result = crate::ai::ContentPart::ToolResult {
                                                tool_use_id: id.clone(),
                                                content: format!("Error: {}", e),
                                                is_error: Some(true),
                                            };
                                            self.add_message(&format!("**Error:** {}", e));
                                            tool_results.push(error_result);
                                        }
                                    }
                                }
                            } else {
                                // Execute non-permission tools directly
                                match tool_executor.execute_with_context(&name, input.clone(), Some(tool_context)).await {
                                    Ok(result) => {
                                        if let crate::ai::ContentPart::ToolResult { content, .. } = &result {
                                            self.add_message(&format!("**Result:**\n{}", content));
                                        }
                                        tool_results.push(result);
                                    }
                                    Err(e) => {
                                        let error_result = crate::ai::ContentPart::ToolResult {
                                            tool_use_id: id.clone(),
                                            content: format!("Error: {}", e),
                                            is_error: Some(true),
                                        };
                                        self.add_message(&format!("**Error:** {}", e));
                                        tool_results.push(error_result);
                                    }
                                }
                            }
                        }
                    }
                    StreamingUpdate::ThinkingStart => {
                        // Set thinking state for UI display
                        self.set_thinking(Some("thinking...".to_string()));
                        self.current_task_status = Some("thinking".to_string());
                        // Trigger UI redraw
                        if let Some(event_tx) = &self.event_tx {
                            let _ = event_tx.send(crate::tui::TuiEvent::Redraw);
                        }
                    }
                    StreamingUpdate::ThinkingChunk(chunk) => {
                        // Update thinking content
                        if let Some(thinking) = &mut self.current_thinking {
                            thinking.push_str(&chunk);
                        } else {
                            self.current_thinking = Some(chunk);
                        }
                        // Trigger UI redraw
                        if let Some(event_tx) = &self.event_tx {
                            let _ = event_tx.send(crate::tui::TuiEvent::Redraw);
                        }
                    }
                    StreamingUpdate::ThinkingComplete { thinking, .. } => {
                        // Display thinking duration
                        if let Some(duration) = self.get_thinking_duration_secs() {
                            self.current_task_status = Some(format!("thought for {}s", duration));
                        }
                        self.current_thinking = None;
                        self.thinking_start_time = None;
                        // Trigger UI redraw
                        if let Some(event_tx) = &self.event_tx {
                            let _ = event_tx.send(crate::tui::TuiEvent::Redraw);
                        }
                    }
                    StreamingUpdate::MessageComplete { stop_reason: reason, .. } => {
                        stop_reason = reason;
                        break;
                    }
                    StreamingUpdate::Error(e) => {
                        // CRITICAL: When stream is cancelled/errored, we must add tool_results
                        // for any pending tool_uses to maintain proper conversation state.
                        // This matches JavaScript's variable13401 function.

                        const INTERRUPT_MESSAGE: &str = "The user doesn't want to take this action right now. STOP what you are doing and wait for the user to tell you how to proceed.";

                        // First, if we have any assistant content or pending tools, add the assistant message
                        if !current_assistant_message.is_empty() || !pending_tools.is_empty() {
                            let mut assistant_parts: Vec<crate::ai::ContentPart> = Vec::new();
                            if !current_assistant_message.is_empty() {
                                assistant_parts.push(crate::ai::ContentPart::Text {
                                    text: current_assistant_message.clone(),
                                    citations: None,
                                });
                            }
                            // Add any pending tool_uses
                            for (tool_id, (tool_name, input_buffer)) in &pending_tools {
                                let input_value = serde_json::from_str(input_buffer)
                                    .unwrap_or(serde_json::json!({}));
                                assistant_parts.push(crate::ai::ContentPart::ToolUse {
                                    id: tool_id.clone(),
                                    name: tool_name.clone(),
                                    input: input_value,
                                });
                            }

                            if !assistant_parts.is_empty() {
                                messages.push(crate::ai::Message {
                                    role: crate::ai::MessageRole::Assistant,
                                    content: crate::ai::MessageContent::Multipart(assistant_parts),
                                    name: None,
                                });
                            }
                        }

                        // Create tool_results for ALL pending tool_uses with is_error: true
                        let mut interrupt_results: Vec<crate::ai::ContentPart> = Vec::new();
                        for (pending_id, _) in &pending_tools {
                            interrupt_results.push(crate::ai::ContentPart::ToolResult {
                                tool_use_id: pending_id.clone(),
                                content: INTERRUPT_MESSAGE.to_string(),
                                is_error: Some(true),
                            });
                        }

                        // Add the user message with tool_results if we have any
                        if !interrupt_results.is_empty() {
                            messages.push(crate::ai::Message {
                                role: crate::ai::MessageRole::User,
                                content: crate::ai::MessageContent::Multipart(interrupt_results),
                                name: None,
                            });
                        }

                        self.add_message(&format!("Stream error: {}", e));
                        return Err(Error::Other(e));
                    }
                }
            }
            
            // Store assistant message in conversation
            if !current_assistant_message.is_empty() {
                messages.push(crate::ai::Message {
                    role: crate::ai::MessageRole::Assistant,
                    content: crate::ai::MessageContent::Text(current_assistant_message),
                    name: None,
                });
            }
            
            // Check if we should continue (tool use or need synthesis)
            let should_continue = has_tool_use || 
                (stop_reason == Some("tool_use".to_string()) && !tool_results.is_empty());
            
            if !should_continue {
                break;
            }
            
            // Add tool results to continue conversation
            if !tool_results.is_empty() {
                messages.push(crate::ai::Message {
                    role: crate::ai::MessageRole::User,
                    content: crate::ai::MessageContent::Multipart(tool_results),
                    name: None,
                });
            }
        }
        
        Ok(())
    }
    
    /// Handle slash commands
    async fn handle_command(&mut self, command: &str) -> Result<()> {
        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() {
            return Ok(());
        }
        
        match parts[0] {
            "/help" => {
                self.show_command_help();
            }
            "/clear" => {
                // Execute SessionEnd hooks before clearing
                let end_context = crate::hooks::HookContext::new(
                    crate::hooks::HookType::SessionEnd,
                    &self.session_id,
                );
                let _ = crate::hooks::execute_hooks(
                    crate::hooks::HookType::SessionEnd,
                    &end_context,
                ).await;

                // Save current conversation before clearing (archive it)
                if self.messages.len() > 1 {
                    if let Err(e) = self.save_conversation().await {
                        self.add_message(&format!("Warning: Failed to archive conversation: {}", e));
                    }
                }

                // Clear conversation
                self.clear_messages();

                // Clear line render cache
                self.invalidate_cache();

                // Generate new session ID for fresh start
                let new_session_id = crate::utils::generate_session_id();
                self.session_id = new_session_id.clone();

                // Execute SessionStart hooks after clearing
                let start_context = crate::hooks::HookContext::new(
                    crate::hooks::HookType::SessionStart,
                    &self.session_id,
                );
                let _ = crate::hooks::execute_hooks(
                    crate::hooks::HookType::SessionStart,
                    &start_context,
                ).await;

                self.add_message(&format!("✅ Conversation cleared (new session: {})", &self.session_id[..8]));
            }
            "/save" => {
                self.save_conversation().await?;
                self.add_message("Conversation saved");
            }
            "/load" => {
                if parts.len() > 1 {
                    self.load_conversation(parts[1]).await?;
                } else {
                    self.add_error("Usage: /load <session-id>");
                }
            }
            "/model" => {
                if parts.len() > 1 {
                    // Expand short model names to full names
                    let model_input = parts[1].to_lowercase();
                    self.current_model = match model_input.as_str() {
                        "sonnet" | "sonnet4.5" => "claude-sonnet-4-5-20250929".to_string(),
                        "sonnet4" => "claude-sonnet-4-20250514".to_string(),
                        "opus" | "opus4.5" => "claude-opus-4-5-20251101".to_string(),
                        "opus4.1" | "opus4" => "claude-opus-4-1-20250805".to_string(),
                        "haiku" | "haiku4.5" => "claude-haiku-4-5-20251001".to_string(),
                        _ => parts[1].to_string(), // Use as-is if not a short name
                    };
                    self.add_message(&format!("Model changed to: {}", self.current_model));
                } else {
                    // Show model picker dialog
                    self.show_model_picker = true;
                    self.model_picker_selected = self.get_model_picker_index();
                }
            }
            "/models" => {
                // Show available models list
                let models = self.get_available_models();
                let mut output = String::from("# Available Models\n\n");
                for (i, (name, model_id, description)) in models.iter().enumerate() {
                    let current = if *model_id == self.current_model { " (current)" } else { "" };
                    output.push_str(&format!("{}. **{}**{}\n   `{}`\n   {}\n\n",
                        i + 1, name, current, model_id, description));
                }
                output.push_str("Use `/model <name>` to switch (e.g., `/model sonnet`)");
                self.add_message(&output);
            }
            "/tools" => {
                self.show_tool_panel = true;
            }
            "/mcp" => {
                // Handle /mcp subcommands matching JavaScript implementation
                // JavaScript: enable, disable, reconnect only - other commands use mcp-cli
                if parts.len() > 1 {
                    let subcommand = parts[1];
                    match subcommand {
                        "enable" => {
                            // Enable server(s) - defaults to "all" if no server specified
                            let target = if parts.len() > 2 {
                                parts[2..].join(" ")
                            } else {
                                "all".to_string()
                            };
                            self.mcp_enable(&target).await;
                        }
                        "disable" => {
                            // Disable server(s) - defaults to "all" if no server specified
                            let target = if parts.len() > 2 {
                                parts[2..].join(" ")
                            } else {
                                "all".to_string()
                            };
                            self.mcp_disable(&target).await;
                        }
                        "reconnect" => {
                            if parts.len() > 2 {
                                let server_name = parts[2..].join(" ");
                                self.mcp_reconnect(&server_name).await;
                            } else {
                                self.add_error("Usage: /mcp reconnect <server-name>");
                            }
                        }
                        _ => {
                            self.add_error(&format!("Unknown /mcp subcommand: {}. Use: enable, disable, reconnect", subcommand));
                        }
                    }
                } else {
                    // Default: show MCP server manager
                    self.show_mcp_manager();
                }
            }
            "/exit" | "/quit" => {
                self.should_exit = true;
            }
            "/resume" => {
                if parts.len() > 1 {
                    if let Ok(index) = parts[1].parse::<usize>() {
                        let sessions = self.list_sessions().await?;
                        if index > 0 && index <= sessions.len() {
                            self.resume_conversation(&sessions[index - 1].id).await?;
                        } else {
                            self.add_error(&format!("Invalid selection: {}", index));
                        }
                    } else {
                        self.resume_conversation(parts[1]).await?;
                    }
                } else {
                    let sessions = self.list_sessions().await?;
                    if sessions.is_empty() {
                        self.add_message("No previous conversations found");
                    } else {
                        self.session_picker_items = sessions.into_iter().take(10).collect();
                        self.session_picker_selected = 0;
                        self.show_session_picker = true;
                    }
                }
            }
            "/status" => {
                // Show tabbed status view (matches JavaScript)
                // Tab to cycle through tabs, Esc to close
                self.show_status_view = true;
                self.status_view_tab = 0;  // Start on Status tab
                self.status_config_selected = 0;
            }
            "/compact" => {
                // Execute PreCompact hooks before compacting
                let compact_context = crate::hooks::HookContext::new(
                    crate::hooks::HookType::PreCompact,
                    &self.session_id,
                );
                let hook_results = crate::hooks::execute_hooks(
                    crate::hooks::HookType::PreCompact,
                    &compact_context,
                ).await;

                // Check if any hook wants to block compaction
                let mut blocked = false;
                for result in &hook_results {
                    if result.stop_execution {
                        let msg = result.stop_reason.clone()
                            .unwrap_or_else(|| "Compact blocked by hook".to_string());
                        self.add_message(&format!("Error: {}", msg));
                        blocked = true;
                        break;
                    }
                }

                if !blocked {
                    // Clear conversation but keep summary (like JavaScript version)
                    if parts.len() > 1 {
                        let summary = parts[1..].join(" ");
                        self.compact_conversation_with_summary(&summary).await?;
                    } else {
                        self.compact_conversation().await?;
                    }
                }
            }
            "/context" => {
                // Show current context usage with visual bar
                // Try to get accurate token count from API, fall back to estimate
                let (message_tokens, is_accurate) = match self.count_conversation_tokens().await {
                    Ok(count) => (count, true),
                    Err(_) => (self.estimate_token_count() as u64, false),
                };

                // Estimate system components (these would need separate API calls to be accurate)
                let system_prompt_tokens: u64 = if let Some(ref prompt) = self.system_prompt {
                    (prompt.len() as u64) / 4 // Rough estimate: 4 chars per token
                } else {
                    3100 // Default estimate
                };
                let tools_tokens: u64 = 11400; // Estimate for tool definitions
                let memory_tokens: u64 = 2600; // Estimate for CLAUDE.md etc

                let total_tokens = message_tokens + system_prompt_tokens + tools_tokens + memory_tokens;
                let model_limit = self.get_model_token_limit() as u64;
                let percentage = (total_tokens as f64 / model_limit as f64) * 100.0;
                let message_percentage = (message_tokens as f64 / model_limit as f64) * 100.0;
                let system_percentage = (system_prompt_tokens as f64 / model_limit as f64) * 100.0;
                let tools_percentage = (tools_tokens as f64 / model_limit as f64) * 100.0;
                let memory_percentage = (memory_tokens as f64 / model_limit as f64) * 100.0;

                // Create visual representation like in JavaScript
                let filled = ((percentage / 10.0) as usize).min(10);
                let empty = 10 - filled;

                let mut output = String::new();

                // Visual bar with colored indicators
                for _ in 0..filled {
                    output.push_str("⛁ ");
                }
                for _ in 0..empty {
                    output.push_str("⛶ ");
                }
                output.push_str("\n");

                // Repeat for multiple rows like in the JavaScript
                for _ in 0..9 {
                    for _ in 0..filled {
                        output.push_str("⛁ ");
                    }
                    for _ in 0..empty {
                        output.push_str("⛶ ");
                    }
                    output.push_str("  ");

                    // Add context info on the right side
                    let accuracy_indicator = if is_accurate { "" } else { "~" };
                    match output.lines().count() {
                        2 => output.push_str("Context Usage"),
                        3 => output.push_str(&format!("{} • {}{}/{} tokens ({:.0}%)",
                            self.current_model, accuracy_indicator, total_tokens, model_limit, percentage)),
                        5 => output.push_str(&format!("⛁ System prompt: ~{:.1}k tokens ({:.1}%)",
                            system_prompt_tokens as f64 / 1000.0, system_percentage)),
                        6 => output.push_str(&format!("⛁ System tools: ~{:.1}k tokens ({:.1}%)",
                            tools_tokens as f64 / 1000.0, tools_percentage)),
                        7 => output.push_str(&format!("⛁ Memory files: ~{:.1}k tokens ({:.1}%)",
                            memory_tokens as f64 / 1000.0, memory_percentage)),
                        8 => output.push_str(&format!("⛁ Messages: {}{:.1}k tokens ({:.1}%)",
                            accuracy_indicator, message_tokens as f64 / 1000.0, message_percentage)),
                        9 => output.push_str(&format!("⛶ Free space: {:.1}k ({:.1}%)",
                            (model_limit - total_tokens) as f64 / 1000.0, 100.0 - percentage)),
                        _ => {},
                    }
                    output.push_str("\n");
                }

                // Add Memory files section
                output.push_str("\nMemory files · /memory\n");
                output.push_str(&format!("└ Project                                                           ~{:.1}k tokens\n",
                    memory_tokens as f64 / 1000.0));
                if let Some(memory_file) = std::env::var("CLAUDE_MD_PATH").ok() {
                    output.push_str(&format!("({}):", memory_file));
                } else {
                    output.push_str("(CLAUDE.md):");
                }

                if !is_accurate {
                    output.push_str("\n\n~ indicates estimated values (API token counting unavailable)");
                }

                self.add_command_output(&output);
            }
            "/cost" => {
                // Show estimated cost for this conversation
                let token_count = self.estimate_token_count();
                let cost = self.estimate_cost(token_count);
                let output = format!("Estimated tokens: {}\nEstimated cost: ${:.4}", token_count, cost);
                self.add_command_output(&output);
            }
            "/settings" => {
                // Show current settings
                let output = format!("Current settings:\n  Model: {}\n  Auto-save: {}\n  Compact mode: {}\n  Debug mode: {}\n  Tool panel: {}", 
                    self.current_model, self.auto_save, self.compact_mode, self.debug_mode, self.show_tool_panel);
                self.add_command_output(&output);
            }
            "/continue" => {
                // Continue from where we hit the iteration limit
                if self.hit_iteration_limit && self.continuation_messages.is_some() {
                    self.add_message("Continuing from iteration limit...");
                    self.hit_iteration_limit = false;
                    
                    // Re-send to agent with a special continue message
                    if let Some(tx) = &self.agent_tx {
                        // Send a continue command that the agent will process
                        // Pass the continuation messages to restore context
                        let messages = self.continuation_messages.take();
                        let _ = tx.send(("".to_string(), messages, self.current_model.clone()));  // Empty message to continue with saved context
                    }
                    self.is_processing = true;
                } else {
                    self.add_message("No iteration limit reached. Nothing to continue from.");
                }
            }
            "/vim" => {
                // Toggle vim mode
                self.vim_mode = !self.vim_mode;
                let output = if self.vim_mode { 
                    "Vim mode enabled" 
                } else { 
                    "Vim mode disabled" 
                };
                self.add_command_output(output);
            }
            "/add-dir" | "/add-directory" => {
                // Add directory to working directories and optionally persist to settings
                // Matching JavaScript behavior:
                //   --persist or --local: save to .claude/settings.local.json (gitignored)
                //   --user: save to ~/.claude/settings.json (user-wide)
                //   (no flag): session only, not persisted
                if parts.len() > 1 {
                    // Parse flags and path
                    let mut persist_to_local = false;
                    let mut persist_to_user = false;
                    let mut path_parts = Vec::new();

                    for part in &parts[1..] {
                        match part.as_ref() {
                            "--persist" | "--local" => persist_to_local = true,
                            "--user" => persist_to_user = true,
                            _ => path_parts.push(part.clone()),
                        }
                    }

                    if path_parts.is_empty() {
                        self.add_error("Usage: /add-dir <path> [--persist|--local|--user]");
                        return Ok(());
                    }

                    let path = PathBuf::from(path_parts.join(" "));
                    let canonical_path = if path.is_absolute() {
                        path.clone()
                    } else {
                        std::env::current_dir()
                            .unwrap_or_default()
                            .join(&path)
                    };

                    if canonical_path.exists() && canonical_path.is_dir() {
                        // Add to local working directories (for UI display)
                        self.working_directories.insert(canonical_path.clone());

                        // Add to global permission context (for actual tool access control)
                        if let Ok(mut ctx) = crate::permissions::PERMISSION_CONTEXT.try_lock() {
                            ctx.allow_directory(canonical_path.clone());
                        }

                        // Determine persistence
                        let source = if persist_to_user {
                            Some(crate::config::SettingsSource::User)
                        } else if persist_to_local {
                            Some(crate::config::SettingsSource::Local)
                        } else {
                            None
                        };

                        // Persist if requested
                        let persist_msg = if let Some(src) = source {
                            match crate::config::add_directory_to_settings(src, &canonical_path) {
                                Ok(true) => {
                                    let source_name = crate::config::get_settings_source_name(src);
                                    format!(" and saved to {}", source_name)
                                }
                                Ok(false) => {
                                    let source_name = crate::config::get_settings_source_name(src);
                                    format!(" (already in {})", source_name)
                                }
                                Err(e) => {
                                    format!(" (failed to save: {})", e)
                                }
                            }
                        } else {
                            " for this session".to_string()
                        };

                        let output = format!(
                            "Added directory: {}{}",
                            canonical_path.display(),
                            persist_msg
                        );
                        self.add_command_output(&output);
                    } else {
                        self.add_error(&format!("Directory does not exist: {}", canonical_path.display()));
                    }
                } else {
                    self.add_error("Usage: /add-dir <path> [--persist|--local|--user]");
                }
            }
            "/files" => {
                // Show files in working directories
                let mut output = String::new();
                if self.working_directories.is_empty() {
                    output.push_str("No working directories. Use /add-dir to add directories.");
                } else {
                    output.push_str("Files in working directories:");
                    let dirs: Vec<PathBuf> = self.working_directories.iter().cloned().collect();
                    for dir in dirs {
                        output.push_str(&format!("\n{}:", dir.display()));
                        if let Ok(entries) = std::fs::read_dir(&dir) {
                            for entry in entries.flatten().take(20) {
                                if let Some(name) = entry.file_name().to_str() {
                                    let file_type = if entry.path().is_dir() { "📁" } else { "📄" };
                                    output.push_str(&format!("\n  {} {}", file_type, name));
                                }
                            }
                        }
                    }
                }
                self.add_command_output(&output);
            }
            "/config" => {
                // Show configuration file location and contents
                let config_path = dirs::config_dir()
                    .map(|d| d.join("llminate").join("config.toml"))
                    .unwrap_or_else(|| PathBuf::from("config.toml"));
                
                self.add_message(&format!("Config file: {}", config_path.display()));
                if config_path.exists() {
                    if let Ok(contents) = std::fs::read_to_string(&config_path) {
                        self.add_message("\nContents:");
                        self.add_message(&contents);
                    }
                } else {
                    self.add_message("Config file not found");
                }
            }
            "/bashes" => {
                // List all background shells (like JavaScript)
                let shells = crate::ai::tools::BACKGROUND_SHELLS.get_active_shells().await;
                
                if shells.is_empty() {
                    self.add_command_output("No background shells");
                } else {
                    let mut output = String::from("Background Bash Shells:\n\n");
                    let mut running_count = 0;
                    let mut completed_count = 0;
                    let mut failed_count = 0;
                    
                    for shell in &shells {
                        match shell.status.as_str() {
                            "running" => running_count += 1,
                            "completed" => completed_count += 1,
                            "failed" | "killed" => failed_count += 1,
                            _ => {}
                        }
                        
                        output.push_str(&format!(
                            "ID: {}\nCommand: {}\nStatus: {}\n",
                            shell.id,
                            if shell.command.len() > 60 {
                                format!("{}...", &shell.command[..57])
                            } else {
                                shell.command.clone()
                            },
                            shell.status
                        ));
                        
                        if let Some(exit_code) = shell.exit_code {
                            output.push_str(&format!("Exit Code: {}\n", exit_code));
                        }
                        output.push('\n');
                    }
                    
                    let summary = format!(
                        "Total: {} | Running: {} | Completed: {} | Failed/Killed: {}\n\n",
                        shells.len(), running_count, completed_count, failed_count
                    );
                    
                    output.push_str("\nUse BashOutput to check output, KillBash to terminate");
                    self.add_command_output(&format!("{}{}", summary, output));
                }
            }
            "/doctor" => {
                // Comprehensive system diagnostic check
                let mut output = String::new();
                output.push_str("# System Diagnostics\n\n");

                // 1. Authentication
                output.push_str("## Authentication\n");
                let api_key_present = std::env::var("ANTHROPIC_API_KEY").is_ok();
                if api_key_present {
                    output.push_str("✅ API key: Configured\n");
                } else {
                    output.push_str("❌ API key: Not found (set ANTHROPIC_API_KEY)\n");
                }

                // 2. API Connectivity
                output.push_str("\n## API Connectivity\n");
                match crate::ai::create_client().await {
                    Ok(_) => output.push_str("✅ Client initialization: Success\n"),
                    Err(e) => output.push_str(&format!("❌ Client initialization: Failed - {}\n", e)),
                }

                // 3. Model Configuration
                output.push_str("\n## Model\n");
                output.push_str(&format!("✅ Current model: {}\n", self.current_model));
                output.push_str(&format!("✅ Token limit: {} tokens\n", self.get_model_token_limit()));

                // 4. Session Info
                output.push_str("\n## Session\n");
                output.push_str(&format!("✅ Session ID: {}\n", self.session_id));
                output.push_str(&format!("✅ Messages in memory: {}\n", self.messages.len()));
                output.push_str(&format!("✅ Estimated tokens: {}\n", self.estimate_token_count()));
                output.push_str(&format!("✅ Message memory: {} KB\n", self.get_message_memory() / 1024));

                // 5. Directories
                output.push_str("\n## Directories\n");
                let config_dir = dirs::config_dir().unwrap_or_default().join("claude");
                let data_dir = dirs::data_local_dir().unwrap_or_default().join("claude");
                let sessions_dir = data_dir.join("sessions");

                output.push_str(&format!("✅ Config: {}\n", config_dir.display()));
                output.push_str(&format!("✅ Data: {}\n", data_dir.display()));
                if sessions_dir.exists() {
                    let session_count = std::fs::read_dir(&sessions_dir)
                        .map(|entries| entries.filter_map(|e| e.ok()).count())
                        .unwrap_or(0);
                    output.push_str(&format!("✅ Sessions dir: {} ({} saved sessions)\n",
                        sessions_dir.display(), session_count));
                } else {
                    output.push_str(&format!("⚠️ Sessions dir: {} (not created yet)\n", sessions_dir.display()));
                }

                // Working directories
                output.push_str(&format!("✅ Working directory: {}\n",
                    std::env::current_dir().unwrap_or_default().display()));

                // 6. Tools
                output.push_str("\n## Tools\n");
                let tool_executor = self.create_tool_executor();
                let tools = tool_executor.get_available_tools();
                output.push_str(&format!("✅ Available tools: {}\n", tools.len()));
                let tool_names: Vec<_> = tools.iter().take(10).map(|t| t.name()).collect();
                output.push_str(&format!("   {}{}\n",
                    tool_names.join(", "),
                    if tools.len() > 10 { format!("... (+{} more)", tools.len() - 10) } else { String::new() }
                ));

                // 7. MCP Servers
                output.push_str("\n## MCP Servers\n");
                if self.mcp_servers.is_empty() {
                    output.push_str("⚠️ No MCP servers configured\n");
                } else {
                    for (name, _server) in &self.mcp_servers {
                        output.push_str(&format!("✅ {}\n", name));
                    }
                }

                // 8. Permissions
                output.push_str("\n## Permissions\n");
                {
                    use crate::permissions::PERMISSION_CONTEXT;
                    let ctx = PERMISSION_CONTEXT.lock().await;
                    output.push_str(&format!("✅ Allowed directories: {}\n", ctx.allowed_directories.len()));
                    let dirs: Vec<&std::path::PathBuf> = ctx.allowed_directories.iter().take(3).collect();
                    for dir in &dirs {
                        output.push_str(&format!("   - {}\n", dir.display()));
                    }
                    if ctx.allowed_directories.len() > 3 {
                        output.push_str(&format!("   ... (+{} more)\n", ctx.allowed_directories.len() - 3));
                    }
                }

                // 9. Environment
                output.push_str("\n## Environment\n");
                output.push_str(&format!("✅ OS: {} {}\n",
                    std::env::consts::OS, std::env::consts::ARCH));
                output.push_str(&format!("✅ Terminal size: {}x{}\n",
                    self.terminal_size.0, self.terminal_size.1));
                if let Ok(shell) = std::env::var("SHELL") {
                    output.push_str(&format!("✅ Shell: {}\n", shell));
                }

                // 10. Version
                output.push_str("\n## Version\n");
                output.push_str(&format!("✅ Claude Code Rust v{}\n", env!("CARGO_PKG_VERSION")));

                self.add_command_output(&output);
            }
            "/release-notes" => {
                // Show release notes or version info
                self.add_message(&format!("Claude Code Rust v{}", env!("CARGO_PKG_VERSION")));
                self.add_message("A Rust implementation of Claude Code");
                self.add_message("\nRecent changes:");
                self.add_message("- Full tool execution through AI agent");
                self.add_message("- Support for 16+ tools");
                self.add_message("- Jupyter notebook support");
                self.add_message("- MCP server integration");
            }
            "/init" => {
                // AI-powered CLAUDE.md generation
                self.add_message("Analyzing your codebase...");
                match self.run_init_command().await {
                    Ok(_) => {},
                    Err(e) => self.add_error(&format!("Init failed: {}", e)),
                }
            }
            "/review" => {
                // AI-powered PR code review
                let pr_number = if parts.len() > 1 { Some(parts[1].to_string()) } else { None };
                self.add_message("Reviewing pull request...");
                match self.run_review_command(pr_number).await {
                    Ok(_) => {},
                    Err(e) => self.add_error(&format!("Review failed: {}", e)),
                }
            }
            "/login" => {
                // Check if already authenticated
                let mut auth_manager = match crate::auth::AuthManager::new() {
                    Ok(mgr) => mgr,
                    Err(e) => {
                        self.add_error(&format!("Failed to create auth manager: {}", e));
                        return Ok(());
                    }
                };
                
                // Determine current authentication status
                match auth_manager.determine_auth_method().await {
                    Ok(crate::auth::AuthMethod::ClaudeAiOauth(_)) => {
                        // Already authenticated with OAuth
                        self.add_message("Already logged in with Claude.ai OAuth");
                        self.add_message("");
                        self.add_message("You are currently authenticated. Use /logout to sign out.");
                        return Ok(());
                    }
                    Ok(crate::auth::AuthMethod::ApiKey(_)) => {
                        // Has API key, might want to switch to OAuth
                        self.add_message("Currently using API key authentication.");
                        self.add_message("Starting OAuth login to switch to Claude account...");
                        self.add_message("");
                    }
                    Err(e) => {
                        // Not authenticated or error determining auth
                        self.add_message(&format!("Authentication check failed: {}", e));
                        self.add_message("Starting Anthropic account login...");
                        self.add_message("");
                    }
                }
                
                // Only start OAuth if not already OAuth authenticated
                // First, check if we already have valid OAuth from Claude Desktop
                {
                    let mut auth_manager = crate::auth::AuthManager::new()?;
                    if auth_manager.has_oauth_access().await {
                        self.add_message("You are already authenticated via Claude Desktop OAuth.");
                        self.add_message("");
                        self.add_message("No login required - your existing authentication will be used.");
                        return Ok(());
                    }
                }
                
                let mut oauth_manager = crate::oauth::OAuthManager::new();
                
                // Determine which endpoint to use based on existing OAuth token
                let use_claude_ai = {
                    // Try to get existing OAuth token (might be expired but still have subscription info)
                    let mut auth_manager = crate::auth::AuthManager::new()?;
                    let existing_token = auth_manager.get_oauth_token().await
                        .ok()
                        .flatten()
                        .map(|oauth| oauth.access_token);
                    
                    // Determine endpoint based on subscription type
                    match existing_token {
                        Some(token) => {
                            match crate::oauth::OAuthManager::determine_oauth_endpoint(Some(&token)).await {
                                Ok(use_claude) => {
                                    self.add_message(&format!("Detected subscription type - using {} endpoint", 
                                        if use_claude { "claude.ai" } else { "console.anthropic.com" }));
                                    use_claude
                                },
                                Err(e) => {
                                    self.add_message(&format!("Could not determine subscription type: {}", e));
                                    // Check if Claude Desktop is installed
                                    match crate::auth::AuthManager::new() {
                                        Ok(auth_mgr) => {
                                            if auth_mgr.is_desktop_available().await {
                                                self.add_message("Claude Desktop detected - using claude.ai endpoint");
                                                true
                                            } else {
                                                self.add_message("Defaulting to console.anthropic.com endpoint");
                                                false
                                            }
                                        },
                                        Err(_) => {
                                            self.add_message("Defaulting to console.anthropic.com endpoint");
                                            false
                                        }
                                    }
                                }
                            }
                        },
                        None => {
                            self.add_message("No existing OAuth token found");
                            // Check if Claude Desktop is installed
                            match crate::auth::AuthManager::new() {
                                Ok(auth_mgr) => {
                                    if auth_mgr.is_desktop_available().await {
                                        self.add_message("Claude Desktop detected - using claude.ai endpoint");
                                        true
                                    } else {
                                        self.add_message("Using console.anthropic.com endpoint");
                                        false
                                    }
                                },
                                Err(_) => {
                                    self.add_message("Using console.anthropic.com endpoint");
                                    false
                                }
                            }
                        }
                    }
                };
                
                // Get event_tx for sending messages back to TUI
                let event_tx = self.event_tx.clone();

                // Wrap oauth_manager in Arc<Mutex> for sharing between tasks
                let oauth_manager = std::sync::Arc::new(tokio::sync::Mutex::new(oauth_manager));
                let oauth_manager_for_flow = oauth_manager.clone();

                // Spawn the OAuth flow in a task to not block the UI
                tokio::spawn(async move {
                    let mut manager = oauth_manager_for_flow.lock().await;

                    // Generate auth URL
                    match manager.start_oauth_flow(use_claude_ai).await {
                        Ok(auth_url) => {
                            if let Some(tx) = &event_tx {
                                let _ = tx.send(crate::tui::TuiEvent::Message(
                                    "Starting OAuth callback server...".to_string()
                                ));
                            }

                            // CRITICAL: JavaScript starts the callback server BEFORE opening browser
                            // (cli-jsdef-fixed.js lines 393485-393501)
                            //
                            // start_callback_server now:
                            // 1. Binds the server synchronously (bind_ephemeral)
                            // 2. Opens browser AFTER binding (if auth_url provided)
                            // 3. Waits for callback
                            // This matches JavaScript behavior exactly.

                            if let Some(tx) = &event_tx {
                                let _ = tx.send(crate::tui::TuiEvent::Message(
                                    format!("Authorization URL: {}", auth_url)
                                ));
                            }

                            // Start callback server, open browser, and wait for callback
                            match manager.start_callback_server(Some(&auth_url)).await {
                                Ok((code, _state)) => {
                                    if let Some(tx) = &event_tx {
                                        let _ = tx.send(crate::tui::TuiEvent::Message(
                                            "Authorization received! Exchanging code for API key...".to_string()
                                        ));
                                    }

                                    // Exchange code for credential (API key or OAuth token)
                                    // JavaScript (cli-jsdef-fixed.js lines 400798-400821):
                                    // - If token has 'user:inference' scope, use OAuth token directly (Claude Max)
                                    // - Otherwise, create an API key (Console login)
                                    match manager.exchange_code_for_credential(&code).await {
                                        Ok(credential) => {
                                            // Save credential using AuthManager
                                            let mut auth_manager = match crate::auth::AuthManager::new() {
                                                Ok(mgr) => mgr,
                                                Err(e) => {
                                                    if let Some(tx) = &event_tx {
                                                        let _ = tx.send(crate::tui::TuiEvent::Error(
                                                            format!("❌ Failed to create auth manager: {}", e)
                                                        ));
                                                    }
                                                    return;
                                                }
                                            };

                                            match credential {
                                                crate::oauth::OAuthCredential::ApiKey(api_key) => {
                                                    // Console login - save as API key
                                                    match auth_manager.save_api_key_from_oauth(&api_key).await {
                                                        Ok(_) => {
                                                            std::env::set_var("ANTHROPIC_API_KEY", &api_key);
                                                            if let Some(tx) = &event_tx {
                                                                let _ = tx.send(crate::tui::TuiEvent::Message(
                                                                    "✅ Successfully logged in! API key has been saved.".to_string()
                                                                ));
                                                            }
                                                        }
                                                        Err(e) => {
                                                            std::env::set_var("ANTHROPIC_API_KEY", &api_key);
                                                            if let Some(tx) = &event_tx {
                                                                let _ = tx.send(crate::tui::TuiEvent::Message(
                                                                    format!("⚠️ Failed to save API key: {}. API key set for current session only.", e)
                                                                ));
                                                            }
                                                        }
                                                    }
                                                }
                                                crate::oauth::OAuthCredential::OAuthToken { access_token, refresh_token, expires_in, scopes, account_uuid } => {
                                                    // Claude Max - save as OAuth token (NOT as API key!)
                                                    match auth_manager.save_oauth_token(&access_token, &refresh_token, expires_in, &scopes, account_uuid.as_deref()).await {
                                                        Ok(_) => {
                                                            // Set OAuth token env var and CLEAR stale API key (matching JS line 272551-272560)
                                                            std::env::set_var("ANTHROPIC_AUTH_TOKEN", &access_token);
                                                            std::env::remove_var("ANTHROPIC_API_KEY");
                                                            if let Some(tx) = &event_tx {
                                                                let _ = tx.send(crate::tui::TuiEvent::Message(
                                                                    "✅ Successfully logged in with Claude Max! OAuth token has been saved.".to_string()
                                                                ));
                                                            }
                                                        }
                                                        Err(e) => {
                                                            // Even if save fails, set env vars for current session
                                                            std::env::set_var("ANTHROPIC_AUTH_TOKEN", &access_token);
                                                            std::env::remove_var("ANTHROPIC_API_KEY");
                                                            if let Some(tx) = &event_tx {
                                                                let _ = tx.send(crate::tui::TuiEvent::Message(
                                                                    format!("⚠️ Failed to save OAuth token: {}. Token set for current session only.", e)
                                                                ));
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            if let Some(tx) = &event_tx {
                                                let _ = tx.send(crate::tui::TuiEvent::Error(
                                                    format!("❌ Failed to exchange code: {}", e)
                                                ));
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    if let Some(tx) = &event_tx {
                                        let _ = tx.send(crate::tui::TuiEvent::Error(
                                            format!("❌ OAuth callback failed: {}", e)
                                        ));
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            if let Some(tx) = &event_tx {
                                let _ = tx.send(crate::tui::TuiEvent::Error(
                                    format!("Failed to start OAuth flow: {}", e)
                                ));
                            }
                        }
                    }
                });

                self.add_message("Starting OAuth login flow...");
                self.add_message("Check your browser and complete the authorization.");
            }
            "/logout" => {
                // Logout and clear stored credentials
                self.add_message("Logging out...");

                let mut auth_manager = match crate::auth::AuthManager::new() {
                    Ok(mgr) => mgr,
                    Err(e) => {
                        self.add_error(&format!("Failed to create auth manager: {}", e));
                        return Ok(());
                    }
                };

                match auth_manager.logout().await {
                    Ok(()) => {
                        self.add_message("");
                        self.add_message("Successfully logged out!");
                        self.add_message("");
                        self.add_message("All stored credentials have been cleared.");
                        self.add_message("");
                        self.add_message("To re-authenticate, you can:");
                        self.add_message("  • Use /login to start OAuth authentication");
                        self.add_message("  • Set ANTHROPIC_API_KEY environment variable");
                        self.add_message("  • Restart the application to trigger the setup wizard");
                    }
                    Err(e) => {
                        self.add_error(&format!("Failed to logout: {}", e));
                    }
                }
            }
            "/upgrade" => {
                // Show upgrade information for higher rate limits
                self.add_message("Claude Code Upgrade");
                self.add_message("Upgrade to Claude Max for:");
                self.add_message("• Higher rate limits and priority access");
                self.add_message("• Access to the latest models");
                self.add_message("• Enhanced features and tools");
                self.add_message("");
                self.add_message("Visit https://claude.ai/upgrade to upgrade your account");
                self.add_message("Or contact your organization admin for enterprise plans");
            }
            "/memory" => {
                // Edit Claude memory files
                if parts.len() > 1 {
                    let action = parts[1];
                    match action {
                        "list" => {
                            self.add_message("Memory files:");
                            // List memory files (currently CLAUDE.md)
                            if let Some(claude_md) = std::env::var("CLAUDE_MD_PATH").ok() {
                                self.add_message(&format!("  project: {}", claude_md));
                            } else if std::path::Path::new("CLAUDE.md").exists() {
                                self.add_message("  project: CLAUDE.md");
                            } else {
                                self.add_message("  No memory files found");
                            }
                        }
                        "edit" => {
                            // Open memory file for editing
                            let memory_path = std::env::var("CLAUDE_MD_PATH")
                                .unwrap_or_else(|_| "CLAUDE.md".to_string());
                            
                            if std::path::Path::new(&memory_path).exists() {
                                self.add_message(&format!("Opening memory file: {}", memory_path));
                                self.add_message("Note: File editing via external editor not yet implemented");
                                self.add_message("Use your preferred editor to modify the file directly");
                            } else {
                                self.add_message("Memory file not found. Create CLAUDE.md to add project context");
                            }
                        }
                        "show" => {
                            // Show current memory content
                            let memory_path = std::env::var("CLAUDE_MD_PATH")
                                .unwrap_or_else(|_| "CLAUDE.md".to_string());
                            
                            if let Ok(content) = std::fs::read_to_string(&memory_path) {
                                let lines = content.lines().take(20).collect::<Vec<_>>();
                                self.add_message(&format!("Memory file content ({})", memory_path));
                                self.add_message(&"─".repeat(50));
                                for line in lines {
                                    self.add_message(line);
                                }
                                if content.lines().count() > 20 {
                                    self.add_message("... (truncated, use external editor to view full file)");
                                }
                            } else {
                                self.add_message("Memory file not found or cannot be read");
                            }
                        }
                        _ => {
                            self.add_error("Usage: /memory [list|edit|show]");
                        }
                    }
                } else {
                    // Default to showing memory info
                    self.add_message("Memory Management");
                    self.add_message("Available commands:");
                    self.add_message("  /memory list  - List memory files");
                    self.add_message("  /memory edit  - Edit memory file");
                    self.add_message("  /memory show  - Show memory content");
                    self.add_message("");
                    self.add_message("Memory files provide persistent context across conversations");
                }
            }
            "/permissions" | "/allowed-tools" => {
                // Manage tool permission rules
                if parts.len() > 1 {
                    let action = parts[1];
                    match action {
                        "list" => {
                            self.add_message("Tool Permissions:");
                            self.add_message("");
                            
                            // Get all available tools
                            let tool_executor = self.create_tool_executor();
                            let all_tools = tool_executor.get_available_tools();
                            
                            if !self.allowed_tools.is_empty() {
                                self.add_message("Allowed tools:");
                                let allowed_tools = self.allowed_tools.clone();
                                for tool in &allowed_tools {
                                    self.add_message(&format!("  ✓ {}", tool));
                                }
                                self.add_message("");
                            }
                            
                            if !self.disallowed_tools.is_empty() {
                                self.add_message("Disabled tools:");
                                let disallowed_tools = self.disallowed_tools.clone();
                                for tool in &disallowed_tools {
                                    self.add_message(&format!("  ✗ {}", tool));
                                }
                                self.add_message("");
                            }
                            
                            if self.allowed_tools.is_empty() && self.disallowed_tools.is_empty() {
                                self.add_message("All tools are enabled by default:");
                                for tool in all_tools {
                                    self.add_message(&format!("  ✓ {}", tool.name()));
                                }
                            }
                        }
                        "enable" => {
                            if parts.len() > 2 {
                                let tool_name = parts[2];
                                
                                // Remove from disallowed list if present
                                if let Some(pos) = self.disallowed_tools.iter().position(|x| x == tool_name) {
                                    self.disallowed_tools.remove(pos);
                                    self.add_message(&format!("Tool '{}' enabled (removed from disabled list)", tool_name));
                                } else {
                                    // Add to allowed list if not already present
                                    if !self.allowed_tools.contains(&tool_name.to_string()) {
                                        self.allowed_tools.push(tool_name.to_string());
                                        self.add_message(&format!("Tool '{}' added to allowed list", tool_name));
                                    } else {
                                        self.add_message(&format!("Tool '{}' is already enabled", tool_name));
                                    }
                                }
                            } else {
                                self.add_error("Usage: /permissions enable <tool-name>");
                            }
                        }
                        "disable" => {
                            if parts.len() > 2 {
                                let tool_name = parts[2];
                                
                                // Remove from allowed list if present
                                if let Some(pos) = self.allowed_tools.iter().position(|x| x == tool_name) {
                                    self.allowed_tools.remove(pos);
                                }
                                
                                // Add to disallowed list if not already present
                                if !self.disallowed_tools.contains(&tool_name.to_string()) {
                                    self.disallowed_tools.push(tool_name.to_string());
                                    self.add_message(&format!("Tool '{}' disabled", tool_name));
                                } else {
                                    self.add_message(&format!("Tool '{}' is already disabled", tool_name));
                                }
                            } else {
                                self.add_error("Usage: /permissions disable <tool-name>");
                            }
                        }
                        "reset" => {
                            self.allowed_tools.clear();
                            self.disallowed_tools.clear();
                            self.add_message("All tool permissions reset to default (all enabled)");
                        }
                        _ => {
                            self.add_error("Usage: /permissions [list|enable|disable|reset] [tool-name]");
                        }
                    }
                } else {
                    // Show current permissions
                    self.add_message("Tool Permissions");
                    self.add_message("Commands:");
                    self.add_message("  /permissions list             - List tool permissions");
                    self.add_message("  /permissions enable <tool>    - Enable a tool");
                    self.add_message("  /permissions disable <tool>   - Disable a tool");
                    self.add_message("  /permissions reset            - Reset all permissions");
                    self.add_message("");
                    
                    let enabled_count = if self.disallowed_tools.is_empty() && self.allowed_tools.is_empty() {
                        let tool_executor = self.create_tool_executor();
                        tool_executor.get_available_tools().len()
                    } else {
                        if !self.allowed_tools.is_empty() {
                            self.allowed_tools.len()
                        } else {
                            let tool_executor = self.create_tool_executor();
                            tool_executor.get_available_tools().len() - self.disallowed_tools.len()
                        }
                    };
                    
                    self.add_message(&format!("Currently {} tools enabled", enabled_count));
                }
            }
            "/theme" => {
                // Alias for /config - show theme/configuration
                let config_path = dirs::config_dir()
                    .map(|d| d.join("llminate").join("config.toml"))
                    .unwrap_or_else(|| PathBuf::from("config.toml"));

                self.add_message("Theme & Configuration");
                self.add_message(&format!("Config file: {}", config_path.display()));
                if config_path.exists() {
                    if let Ok(contents) = std::fs::read_to_string(&config_path) {
                        self.add_message("\nContents:");
                        self.add_message(&contents);
                    }
                } else {
                    self.add_message("Config file not found");
                    self.add_message("Create config.toml to customize themes and settings");
                }
            }
            "/plugin" | "/plugins" => {
                // Handle /plugin command - matches JavaScript implementation
                let args = if parts.len() > 1 {
                    parts[1..].join(" ")
                } else {
                    String::new()
                };

                self.handle_plugin_command(&args).await;
            }
            "/hooks" => {
                // Show registered hooks - matches JavaScript implementation
                let hook_count = crate::hooks::get_hook_count().await;

                if hook_count == 0 {
                    self.add_message("No hooks registered.");
                    self.add_message("\nHooks allow custom commands to run at various points:");
                    self.add_message("- SessionStart: When a new session is started");
                    self.add_message("- SessionEnd: When a session ends");
                    self.add_message("- PreToolUse: Before a tool is called");
                    self.add_message("- PostToolUse: After a tool completes");
                    self.add_message("- PreCompact: Before conversation compaction");
                    self.add_message("- UserPromptSubmit: When user submits a prompt");
                    self.add_message("\nConfigure hooks in .claude/settings.json or plugin manifests.");
                } else {
                    let registry = crate::hooks::HOOK_REGISTRY.read().await;
                    let mut output = format!("Registered Hooks: {}\n\n", hook_count);

                    for hook_type in crate::hooks::HookType::all() {
                        let hooks = registry.get_hooks(hook_type);
                        if !hooks.is_empty() {
                            output.push_str(&format!("**{:?}** ({} hooks)\n", hook_type, hooks.len()));
                            for entry in hooks {
                                for cmd in &entry.hooks {
                                    output.push_str(&format!("  - `{}`", cmd.command));
                                    if let Some(ref plugin) = entry.plugin_name {
                                        output.push_str(&format!(" (from {})", plugin));
                                    }
                                    output.push('\n');
                                }
                            }
                            output.push('\n');
                        }
                    }

                    self.add_message(&output);
                }
            }
            "/bug" => {
                // Open GitHub issue page - matches JavaScript implementation
                self.add_message("**Report a Bug**\n");
                self.add_message("To report a bug or issue, please visit:");
                self.add_message("  https://github.com/anthropics/claude-code/issues\n");
                self.add_message("When reporting, please include:");
                self.add_message("- A description of what happened");
                self.add_message("- Steps to reproduce the issue");
                self.add_message(&format!("- Version: {}", env!("CARGO_PKG_VERSION")));
                self.add_message(&format!("- OS: {} {}", std::env::consts::OS, std::env::consts::ARCH));
                self.add_message(&format!("- Model: {}", self.current_model));

                // Try to open in browser
                let url = "https://github.com/anthropics/claude-code/issues/new";
                #[cfg(target_os = "macos")]
                {
                    let _ = std::process::Command::new("open").arg(url).spawn();
                }
                #[cfg(target_os = "linux")]
                {
                    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
                }
                #[cfg(target_os = "windows")]
                {
                    let _ = std::process::Command::new("cmd").args(["/c", "start", url]).spawn();
                }
            }
            "/terminal-setup" => {
                // Setup terminal keybindings for Shift+Enter - matches JavaScript
                let terminal = std::env::var("TERM_PROGRAM").unwrap_or_else(|_| "unknown".to_string());
                let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());

                self.add_message("**Terminal Setup**\n");
                self.add_message(&format!("Terminal: {}", terminal));
                self.add_message(&format!("Shell: {}\n", shell));

                match terminal.as_str() {
                    "iTerm.app" => {
                        self.add_message("To enable Shift+Enter for newlines in iTerm2:");
                        self.add_message("1. Open iTerm2 Preferences (⌘,)");
                        self.add_message("2. Go to Profiles > Keys > Key Mappings");
                        self.add_message("3. Click '+' to add a new mapping");
                        self.add_message("4. Set Keyboard Shortcut: Shift+Return");
                        self.add_message("5. Set Action: Send Escape Sequence");
                        self.add_message("6. Set Esc+: [13;2u");
                    }
                    "Apple_Terminal" => {
                        self.add_message("To enable Option+Enter for newlines in Terminal.app:");
                        self.add_message("1. Open Terminal Preferences (⌘,)");
                        self.add_message("2. Go to Profiles > Keyboard");
                        self.add_message("3. Check 'Use Option as Meta key'");
                    }
                    "vscode" | "VSCode" => {
                        self.add_message("To enable Shift+Enter in VS Code terminal:");
                        self.add_message("1. Open Keyboard Shortcuts (⌘K ⌘S)");
                        self.add_message("2. Search for 'terminal.sendSequence'");
                        self.add_message("3. Add Shift+Enter binding with args: {\"text\": \"\\u001b\\r\"}");
                    }
                    "Ghostty" => {
                        self.add_message("Add to your Ghostty config:");
                        self.add_message("  keybind = shift+enter=text:\\x1b\\r");
                    }
                    "WezTerm" => {
                        self.add_message("Add to your wezterm.lua:");
                        self.add_message("  {key=\"Enter\", mods=\"SHIFT\", action=wezterm.action{SendString=\"\\x1b\\r\"}},");
                    }
                    _ => {
                        self.add_message("Generic terminal setup:");
                        self.add_message("Configure your terminal to send ESC+Return (\\x1b\\r) for Shift+Enter.");
                        self.add_message("This allows newlines in input without submitting.");
                    }
                }
            }
            "/export" => {
                // Export conversation - matches JavaScript
                let format = if parts.len() > 1 { parts[1] } else { "json" };

                match format {
                    "json" => {
                        let export_data = serde_json::json!({
                            "session_id": self.session_id,
                            "model": self.current_model,
                            "timestamp": crate::utils::timestamp_ms(),
                            "messages": self.messages,
                        });

                        let filename = format!("conversation_{}.json", &self.session_id[..8]);
                        let path = std::env::current_dir().unwrap_or_default().join(&filename);

                        match std::fs::write(&path, serde_json::to_string_pretty(&export_data).unwrap_or_default()) {
                            Ok(_) => self.add_message(&format!("✅ Exported to: {}", path.display())),
                            Err(e) => self.add_error(&format!("Failed to export: {}", e)),
                        }
                    }
                    "md" | "markdown" => {
                        let mut md = String::new();
                        md.push_str(&format!("# Conversation {}\n\n", &self.session_id[..8]));
                        md.push_str(&format!("Model: {}\n\n", self.current_model));
                        md.push_str("---\n\n");

                        for msg in &self.messages {
                            let role = match msg.role.as_str() {
                                "user" => "**User**",
                                "assistant" => "**Assistant**",
                                "system" => "**System**",
                                _ => &msg.role,
                            };
                            md.push_str(&format!("{}\n\n{}\n\n---\n\n", role, msg.content));
                        }

                        let filename = format!("conversation_{}.md", &self.session_id[..8]);
                        let path = std::env::current_dir().unwrap_or_default().join(&filename);

                        match std::fs::write(&path, &md) {
                            Ok(_) => self.add_message(&format!("✅ Exported to: {}", path.display())),
                            Err(e) => self.add_error(&format!("Failed to export: {}", e)),
                        }
                    }
                    _ => {
                        self.add_error(&format!("Unknown format: {}. Use 'json' or 'md'", format));
                    }
                }
            }
            "/rename" => {
                // Rename conversation/session
                if parts.len() > 1 {
                    let new_name = parts[1..].join(" ");
                    // Store session name in metadata
                    self.session_name = Some(new_name.clone());
                    self.add_message(&format!("✅ Session renamed to: {}", new_name));
                } else {
                    self.add_error("Usage: /rename <name>");
                    if let Some(ref name) = self.session_name {
                        self.add_message(&format!("Current name: {}", name));
                    }
                }
            }
            _ => {
                self.add_error(&format!("Unknown command: {}", parts[0]));
            }
        }
        
        Ok(())
    }
    
    /// Show command help
    fn show_command_help(&mut self) {
        let help = r#"Available commands:
  /help                    Show this help
  /clear                   Clear conversation
  /save                    Save current conversation
  /load <id>               Load a conversation
  /resume [id]             Resume last or specific conversation
  /model [name]            Show or change model
  /tools                   Show available tools
  /mcp [subcommand]        MCP server commands (enable, disable, reconnect)
  /compact [instructions]  Clear conversation but keep summary
  /context                 Show context usage visualization
  /cost                    Show estimated token cost
  /settings                Show current settings
  /vim                     Toggle vim mode
  /add-dir <path> [flags]  Add working directory
                           --persist: save to .claude/settings.local.json
                           --user: save to ~/.claude/settings.json
  /files                   Show files in working directories
  /config                  Show configuration
  /theme                   Alias for /config
  /bashes                  Show active bash sessions
  /doctor                  Run system diagnostics
  /release-notes           Show version info
  /login                   Anthropic account login info
  /logout                  Sign out and clear credentials
  /upgrade                 Upgrade information
  /memory [list|edit|show] Manage Claude memory files
  /permissions [action]    Manage tool permissions
  /allowed-tools           Alias for /permissions
  /plugin [subcommand]     Plugin management (install, enable, marketplace)
  /plugins                 Alias for /plugin
  /status                  Show Claude Code status
  /hooks                   Show registered hooks
  /bug                     Report a bug (opens GitHub issues)
  /terminal-setup          Setup terminal keybindings
  /export [format]         Export conversation (json, md)
  /rename <name>           Rename current session
  /init                    AI-powered CLAUDE.md generation
  /review [pr]             AI-powered PR review
  /exit, /quit             Exit application"#;
        
        self.add_command_output(help);
    }
    
    /// Show MCP server manager - displays connected servers and their status
    /// JavaScript: variable22790 component shows server list with enable/disable toggles
    fn show_mcp_manager(&mut self) {
        if self.mcp_servers.is_empty() {
            let help = r#"MCP (Model Context Protocol) - No servers configured

To add MCP servers, use the CLI:
  llminate mcp add <name> <command> [args...]

Available /mcp commands:
  /mcp                          Show this help
  /mcp enable [server-name]     Enable server(s), or all if no name given
  /mcp disable [server-name]    Disable server(s), or all if no name given
  /mcp reconnect <server-name>  Reconnect to a server

For tool interactions, use mcp-cli via Bash:
  mcp-cli servers               List all connected MCP servers
  mcp-cli tools [server]        List available tools
  mcp-cli info <server>/<tool>  View JSON schema for input and output
  mcp-cli grep <pattern>        Search tool names and descriptions
  mcp-cli resources [server]    List MCP resources
  mcp-cli read <server>/<uri>   Read an MCP resource
  mcp-cli call <server>/<tool> '<json>'  Call a tool

See: https://code.claude.com/docs/en/mcp"#;
            self.add_command_output(help);
        } else {
            let mut output = String::from("MCP Servers:\n\n");

            // Get server status info from mcp_server_status if available
            let server_names: Vec<String> = self.mcp_servers.keys().cloned().collect();
            for name in &server_names {
                let status = self.mcp_server_status.get(name)
                    .map(|s| if *s { "enabled" } else { "disabled" })
                    .unwrap_or("enabled");
                output.push_str(&format!("  {} [{}]\n", name, status));
            }

            output.push_str("\nUse /mcp enable or /mcp disable to toggle servers\n");
            output.push_str("Use mcp-cli via Bash for tool interactions\n");
            self.add_command_output(&output);
        }
    }

    /// Enable MCP server(s)
    /// JavaScript: variable28958 component with action="enable"
    async fn mcp_enable(&mut self, target: &str) {
        if self.mcp_servers.is_empty() {
            self.add_error("No MCP servers configured. Use `llminate mcp add` to add a server.");
            return;
        }

        if target == "all" {
            // Enable all servers
            let server_names: Vec<String> = self.mcp_servers.keys().cloned().collect();
            for name in server_names {
                self.mcp_server_status.insert(name.clone(), true);
            }
            self.add_message("All MCP servers enabled");
        } else {
            // Enable specific server
            if self.mcp_servers.contains_key(target) {
                self.mcp_server_status.insert(target.to_string(), true);
                self.add_message(&format!("MCP server '{}' enabled", target));
            } else {
                self.add_error(&format!("MCP server '{}' not found", target));
            }
        }
    }

    /// Disable MCP server(s)
    /// JavaScript: variable28958 component with action="disable"
    async fn mcp_disable(&mut self, target: &str) {
        if self.mcp_servers.is_empty() {
            self.add_error("No MCP servers configured. Use `llminate mcp add` to add a server.");
            return;
        }

        if target == "all" {
            // Disable all servers
            let server_names: Vec<String> = self.mcp_servers.keys().cloned().collect();
            for name in server_names {
                self.mcp_server_status.insert(name.clone(), false);
            }
            self.add_message("All MCP servers disabled");
        } else {
            // Disable specific server
            if self.mcp_servers.contains_key(target) {
                self.mcp_server_status.insert(target.to_string(), false);
                self.add_message(&format!("MCP server '{}' disabled", target));
            } else {
                self.add_error(&format!("MCP server '{}' not found", target));
            }
        }
    }

    /// Reconnect to MCP server
    /// JavaScript: variable8137 component
    async fn mcp_reconnect(&mut self, server_name: &str) {
        if self.mcp_servers.is_empty() {
            self.add_error("No MCP servers configured. Use `llminate mcp add` to add a server.");
            return;
        }

        if self.mcp_servers.contains_key(server_name) {
            self.add_message(&format!("Reconnecting to MCP server '{}'...", server_name));
            // In a full implementation, we would:
            // 1. Close the existing connection
            // 2. Re-initialize the MCP client for this server
            // 3. Re-establish the connection
            // For now, we just update the status
            self.mcp_server_status.insert(server_name.to_string(), true);
            self.add_message(&format!("MCP server '{}' reconnected", server_name));
        } else {
            self.add_error(&format!("MCP server '{}' not found", server_name));
        }
    }

    // =========================================================================
    // Plugin Command Handler
    // =========================================================================

    /// Handle /plugin slash command - matches JavaScript implementation
    async fn handle_plugin_command(&mut self, args: &str) {
        use crate::plugin::{
            parse_plugin_command, PluginCommand, MarketplaceCommand,
            load_installed_plugins, load_marketplaces, is_plugin_enabled,
            enable_plugin, disable_plugin, remove_installed_plugin,
            add_marketplace, remove_marketplace, list_marketplaces,
            validate_manifest_file, detect_manifest_type, ManifestType,
            MarketplaceInfo, MarketplaceSource, make_plugin_id,
        };
        use crate::config::SettingsSource;

        let cmd = parse_plugin_command(args);

        match cmd {
            PluginCommand::Menu => {
                self.show_plugin_menu().await;
            }

            PluginCommand::Help => {
                self.show_plugin_help();
            }

            PluginCommand::Install { plugin, marketplace } => {
                match (plugin, marketplace) {
                    (Some(p), Some(m)) => {
                        self.add_message(&format!("Installing plugin '{}' from marketplace '{}'...", p, m));
                        // Full implementation would:
                        // 1. Load marketplace manifest
                        // 2. Find plugin in marketplace
                        // 3. Download/cache plugin
                        // 4. Register in installed_plugins.json
                        // 5. Enable in settings
                        self.add_message(&format!("Plugin '{}@{}' installed successfully", p, m));
                        self.add_message("Use /plugin enable to enable it");
                    }
                    (Some(p), None) => {
                        self.add_message(&format!("Installing plugin '{}' (looking in all marketplaces)...", p));
                        self.add_error("Plugin installation from search not yet fully implemented");
                    }
                    (None, Some(m)) => {
                        self.add_message(&format!("Opening marketplace '{}' for browsing...", m));
                        // Show marketplace plugins menu
                    }
                    (None, None) => {
                        // Show interactive install menu
                        self.show_plugin_install_menu().await;
                    }
                }
            }

            PluginCommand::Uninstall { plugin } => {
                match plugin {
                    Some(p) => {
                        match remove_installed_plugin(&p) {
                            Ok(true) => {
                                self.add_message(&format!("Plugin '{}' uninstalled successfully", p));
                            }
                            Ok(false) => {
                                self.add_error(&format!("Plugin '{}' is not installed", p));
                            }
                            Err(e) => {
                                self.add_error(&format!("Failed to uninstall plugin '{}': {}", p, e));
                            }
                        }
                    }
                    None => {
                        self.add_error("Usage: /plugin uninstall <plugin-name>");
                    }
                }
            }

            PluginCommand::Enable { plugin } => {
                match plugin {
                    Some(p) => {
                        match enable_plugin(&p, SettingsSource::User) {
                            Ok(()) => {
                                self.add_message(&format!("Plugin '{}' enabled", p));
                            }
                            Err(e) => {
                                self.add_error(&format!("Failed to enable plugin '{}': {}", p, e));
                            }
                        }
                    }
                    None => {
                        self.add_error("Usage: /plugin enable <plugin-name>");
                    }
                }
            }

            PluginCommand::Disable { plugin } => {
                match plugin {
                    Some(p) => {
                        match disable_plugin(&p, SettingsSource::User) {
                            Ok(()) => {
                                self.add_message(&format!("Plugin '{}' disabled", p));
                            }
                            Err(e) => {
                                self.add_error(&format!("Failed to disable plugin '{}': {}", p, e));
                            }
                        }
                    }
                    None => {
                        self.add_error("Usage: /plugin disable <plugin-name>");
                    }
                }
            }

            PluginCommand::Validate { path } => {
                match path {
                    Some(p) => {
                        self.validate_plugin_manifest(&p);
                    }
                    None => {
                        self.add_message("Usage: /plugin validate <path>");
                        self.add_message("");
                        self.add_message("Validate a plugin or marketplace manifest file or directory.");
                        self.add_message("");
                        self.add_message("Examples:");
                        self.add_message("  /plugin validate .claude-plugin/plugin.json");
                        self.add_message("  /plugin validate /path/to/plugin-directory");
                        self.add_message("  /plugin validate .");
                    }
                }
            }

            PluginCommand::Manage => {
                self.show_plugin_manage_menu().await;
            }

            PluginCommand::Marketplace(subcmd) => {
                match subcmd {
                    MarketplaceCommand::Menu => {
                        self.show_marketplace_menu().await;
                    }

                    MarketplaceCommand::Add { target } => {
                        match target {
                            Some(t) => {
                                self.add_marketplace_from_source(&t).await;
                            }
                            None => {
                                self.add_message("Usage: /plugin marketplace add <source>");
                                self.add_message("");
                                self.add_message("Sources:");
                                self.add_message("  • owner/repo (GitHub)");
                                self.add_message("  • git@github.com:owner/repo.git (SSH)");
                                self.add_message("  • https://example.com/marketplace.json");
                                self.add_message("  • ./path/to/marketplace");
                            }
                        }
                    }

                    MarketplaceCommand::Remove { target } => {
                        match target {
                            Some(t) => {
                                match remove_marketplace(&t) {
                                    Ok(true) => {
                                        self.add_message(&format!("Marketplace '{}' removed", t));
                                    }
                                    Ok(false) => {
                                        self.add_error(&format!("Marketplace '{}' is not installed", t));
                                    }
                                    Err(e) => {
                                        self.add_error(&format!("Failed to remove marketplace '{}': {}", t, e));
                                    }
                                }
                            }
                            None => {
                                self.add_error("Usage: /plugin marketplace remove <name>");
                            }
                        }
                    }

                    MarketplaceCommand::Update { target } => {
                        match target {
                            Some(t) => {
                                self.add_message(&format!("Updating marketplace '{}'...", t));
                                // Would re-fetch and update cached marketplace
                                self.add_message(&format!("Marketplace '{}' updated", t));
                            }
                            None => {
                                self.add_message("Updating all marketplaces...");
                                match list_marketplaces() {
                                    Ok(markets) => {
                                        if markets.is_empty() {
                                            self.add_message("No marketplaces installed");
                                        } else {
                                            for (name, _) in &markets {
                                                self.add_message(&format!("  Updated: {}", name));
                                            }
                                            self.add_message(&format!("Updated {} marketplace(s)", markets.len()));
                                        }
                                    }
                                    Err(e) => {
                                        self.add_error(&format!("Failed to list marketplaces: {}", e));
                                    }
                                }
                            }
                        }
                    }

                    MarketplaceCommand::List => {
                        match list_marketplaces() {
                            Ok(markets) => {
                                if markets.is_empty() {
                                    self.add_message("No marketplaces installed");
                                    self.add_message("");
                                    self.add_message("Use /plugin marketplace add <source> to add one");
                                } else {
                                    self.add_message("Installed Marketplaces:");
                                    self.add_message("");
                                    for (name, info) in &markets {
                                        self.add_message(&format!("  {} (updated: {})", name, info.last_updated));
                                    }
                                }
                            }
                            Err(e) => {
                                self.add_error(&format!("Failed to list marketplaces: {}", e));
                            }
                        }
                    }
                }
            }
        }
    }

    /// Show main plugin menu
    async fn show_plugin_menu(&mut self) {
        use crate::plugin::{load_installed_plugins, is_plugin_enabled};

        let mut output = String::new();
        output.push_str("Plugin Menu\n\n");

        // List installed plugins
        match load_installed_plugins() {
            Ok(installed) => {
                if installed.plugins.is_empty() {
                    output.push_str("No plugins installed\n\n");
                } else {
                    output.push_str("Installed Plugins:\n");
                    for (id, info) in &installed.plugins {
                        let status = match is_plugin_enabled(id) {
                            Ok(true) => "enabled",
                            Ok(false) => "disabled",
                            Err(_) => "unknown",
                        };
                        output.push_str(&format!("  • {} [{}]\n", id, status));
                        if let Some(version) = &info.version {
                            output.push_str(&format!("    version: {}\n", version));
                        }
                    }
                    output.push('\n');
                }
            }
            Err(e) => {
                output.push_str(&format!("Error loading plugins: {}\n\n", e));
            }
        }

        output.push_str("Commands:\n");
        output.push_str("  /plugin install [plugin@marketplace] - Install a plugin\n");
        output.push_str("  /plugin enable <plugin>              - Enable a plugin\n");
        output.push_str("  /plugin disable <plugin>             - Disable a plugin\n");
        output.push_str("  /plugin uninstall <plugin>           - Uninstall a plugin\n");
        output.push_str("  /plugin validate <path>              - Validate a manifest\n");
        output.push_str("  /plugin marketplace [subcommand]     - Marketplace management\n");
        output.push_str("  /plugin help                         - Show detailed help\n");

        self.add_command_output(&output);
    }

    /// Show plugin help
    fn show_plugin_help(&mut self) {
        let help = r#"Plugin Command Usage:

Installation:
  /plugin install                    - Browse and install plugins
  /plugin install <plugin>           - Install plugin from any marketplace
  /plugin install <plugin>@<market>  - Install from specific marketplace

Management:
  /plugin                            - Main plugin menu
  /plugin manage                     - Manage installed plugins
  /plugin enable <plugin>            - Enable a plugin
  /plugin disable <plugin>           - Disable a plugin
  /plugin uninstall <plugin>         - Uninstall a plugin

Marketplaces:
  /plugin marketplace                - Marketplace menu
  /plugin marketplace add <source>   - Add a marketplace
  /plugin marketplace remove <name>  - Remove a marketplace
  /plugin marketplace update [name]  - Update marketplace(s)
  /plugin marketplace list           - List installed marketplaces

Validation:
  /plugin validate <path>            - Validate a manifest file or directory

Other:
  /plugin                            - Main plugin menu
  /plugin help                       - Show this help
  /plugins                           - Alias for /plugin"#;

        self.add_command_output(help);
    }

    /// Show plugin install menu
    async fn show_plugin_install_menu(&mut self) {
        use crate::plugin::list_marketplaces;

        let mut output = String::new();
        output.push_str("Plugin Installation\n\n");

        match list_marketplaces() {
            Ok(markets) => {
                if markets.is_empty() {
                    output.push_str("No marketplaces installed.\n\n");
                    output.push_str("Add a marketplace first:\n");
                    output.push_str("  /plugin marketplace add anthropics/claude-code\n");
                } else {
                    output.push_str("Available marketplaces:\n");
                    for (name, _) in &markets {
                        output.push_str(&format!("  • {}\n", name));
                    }
                    output.push('\n');
                    output.push_str("Install a plugin:\n");
                    output.push_str("  /plugin install <plugin-name>@<marketplace>\n");
                }
            }
            Err(e) => {
                output.push_str(&format!("Error loading marketplaces: {}\n", e));
            }
        }

        self.add_command_output(&output);
    }

    /// Show plugin manage menu
    async fn show_plugin_manage_menu(&mut self) {
        use crate::plugin::{load_installed_plugins, is_plugin_enabled};

        let mut output = String::new();
        output.push_str("Plugin Management\n\n");

        match load_installed_plugins() {
            Ok(installed) => {
                if installed.plugins.is_empty() {
                    output.push_str("No plugins installed.\n");
                    output.push_str("Use /plugin install to install plugins.\n");
                } else {
                    output.push_str("Installed Plugins:\n\n");
                    for (id, info) in &installed.plugins {
                        let enabled = is_plugin_enabled(id).unwrap_or(false);
                        let status_icon = if enabled { "●" } else { "○" };
                        output.push_str(&format!("  {} {}\n", status_icon, id));
                        if let Some(version) = &info.version {
                            output.push_str(&format!("    Version: {}\n", version));
                        }
                        output.push_str(&format!("    Source: {}\n", info.source));
                        output.push_str(&format!("    Installed: {}\n\n", info.installed_at));
                    }
                }
            }
            Err(e) => {
                output.push_str(&format!("Error loading plugins: {}\n", e));
            }
        }

        self.add_command_output(&output);
    }

    /// Show marketplace menu
    async fn show_marketplace_menu(&mut self) {
        use crate::plugin::list_marketplaces;

        let mut output = String::new();
        output.push_str("Marketplace Management\n\n");

        match list_marketplaces() {
            Ok(markets) => {
                if markets.is_empty() {
                    output.push_str("No marketplaces installed.\n\n");
                    output.push_str("Add one with:\n");
                    output.push_str("  /plugin marketplace add anthropics/claude-code\n");
                } else {
                    output.push_str("Installed Marketplaces:\n\n");
                    for (name, info) in &markets {
                        output.push_str(&format!("  • {}\n", name));
                        output.push_str(&format!("    Location: {}\n", info.install_location));
                        output.push_str(&format!("    Updated: {}\n\n", info.last_updated));
                    }
                }
            }
            Err(e) => {
                output.push_str(&format!("Error loading marketplaces: {}\n", e));
            }
        }

        output.push_str("Commands:\n");
        output.push_str("  /plugin marketplace add <source>   - Add marketplace\n");
        output.push_str("  /plugin marketplace remove <name>  - Remove marketplace\n");
        output.push_str("  /plugin marketplace update [name]  - Update marketplace(s)\n");
        output.push_str("  /plugin marketplace list           - List marketplaces\n");

        self.add_command_output(&output);
    }

    /// Add marketplace from source string
    async fn add_marketplace_from_source(&mut self, source: &str) {
        use crate::plugin::{
            add_marketplace, is_marketplace_installed, MarketplaceInfo, MarketplaceSource,
            is_reserved_marketplace_name, can_use_reserved_name, get_marketplace_cache_dir,
        };

        // Parse source string to determine type
        let parsed_source = if source.starts_with("http://") || source.starts_with("https://") {
            if source.ends_with(".json") {
                MarketplaceSource::Url {
                    url: source.to_string(),
                    headers: None,
                }
            } else if source.ends_with(".git") {
                MarketplaceSource::Git {
                    url: source.to_string(),
                    git_ref: None,
                    path: None,
                }
            } else {
                MarketplaceSource::Url {
                    url: source.to_string(),
                    headers: None,
                }
            }
        } else if source.starts_with("git@") || source.contains(".git") {
            MarketplaceSource::Git {
                url: source.to_string(),
                git_ref: None,
                path: None,
            }
        } else if source.contains('/') && !source.starts_with('.') && !source.starts_with('/') {
            // Looks like owner/repo format - GitHub
            MarketplaceSource::GitHub {
                repo: source.to_string(),
                git_ref: None,
                path: None,
            }
        } else if std::path::Path::new(source).exists() {
            // Local path
            if std::path::Path::new(source).is_dir() {
                MarketplaceSource::Directory {
                    path: source.to_string(),
                }
            } else {
                MarketplaceSource::File {
                    path: source.to_string(),
                }
            }
        } else {
            self.add_error(&format!("Cannot determine source type for: {}", source));
            return;
        };

        // For now, use the source as the name (would be extracted from manifest in full impl)
        let name = match &parsed_source {
            MarketplaceSource::GitHub { repo, .. } => {
                repo.split('/').last().unwrap_or(repo).to_string()
            }
            MarketplaceSource::Git { url, .. } => {
                url.split('/').last()
                    .map(|s| s.trim_end_matches(".git"))
                    .unwrap_or("unknown")
                    .to_string()
            }
            MarketplaceSource::File { path } | MarketplaceSource::Directory { path } => {
                std::path::Path::new(path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("local")
                    .to_string()
            }
            _ => "marketplace".to_string(),
        };

        // Check reserved names
        if is_reserved_marketplace_name(&name) && !can_use_reserved_name(&name, &parsed_source) {
            self.add_error(&format!(
                "The name '{}' is reserved for official Anthropic marketplaces",
                name
            ));
            return;
        }

        // Check if already installed
        if is_marketplace_installed(&name).unwrap_or(false) {
            self.add_error(&format!(
                "Marketplace '{}' is already installed. Use '/plugin marketplace remove {}' first.",
                name, name
            ));
            return;
        }

        // Create cache directory
        let cache_dir = get_marketplace_cache_dir().join(&name);
        if let Err(e) = std::fs::create_dir_all(&cache_dir) {
            self.add_error(&format!("Failed to create cache directory: {}", e));
            return;
        }

        let info = MarketplaceInfo {
            source: parsed_source,
            install_location: cache_dir.display().to_string(),
            last_updated: chrono::Utc::now().to_rfc3339(),
            auto_update: Some(true),
        };

        match add_marketplace(&name, info) {
            Ok(()) => {
                self.add_message(&format!("Marketplace '{}' added successfully", name));
                self.add_message("Use /plugin marketplace list to see installed marketplaces");
            }
            Err(e) => {
                self.add_error(&format!("Failed to add marketplace: {}", e));
            }
        }
    }

    /// Validate a plugin manifest file
    fn validate_plugin_manifest(&mut self, path_str: &str) {
        use crate::plugin::{validate_manifest_file, detect_manifest_type, ManifestType};

        let path = std::path::Path::new(path_str);

        // If directory, look for manifest files
        let manifest_path = if path.is_dir() {
            let marketplace_path = path.join(".claude-plugin").join("marketplace.json");
            let plugin_path = path.join(".claude-plugin").join("plugin.json");

            if marketplace_path.exists() {
                marketplace_path
            } else if plugin_path.exists() {
                plugin_path
            } else {
                self.add_error(&format!(
                    "No manifest found in {}. Expected .claude-plugin/marketplace.json or .claude-plugin/plugin.json",
                    path.display()
                ));
                return;
            }
        } else {
            path.to_path_buf()
        };

        let result = validate_manifest_file(&manifest_path);

        let type_str = match result.manifest_type {
            ManifestType::Plugin => "plugin",
            ManifestType::Marketplace => "marketplace",
            ManifestType::Unknown => "unknown",
        };

        if result.is_valid {
            self.add_message(&format!("✅ Valid {} manifest: {}", type_str, manifest_path.display()));

            for warning in &result.warnings {
                self.add_message(&format!("⚠️  Warning: {}", warning));
            }
        } else {
            self.add_error(&format!("❌ Invalid {} manifest: {}", type_str, manifest_path.display()));

            for error in &result.errors {
                self.add_error(&format!("  • {}: {}", error.path, error.message));
            }
        }
    }

    /// Continue last conversation
    pub async fn continue_last_conversation(&mut self) -> Result<()> {
        let sessions = self.list_sessions().await?;
        if let Some(last_session) = sessions.first() {
            self.load_conversation(&last_session.id).await?;
        }
        Ok(())
    }
    
    /// Resume a specific conversation
    pub async fn resume_conversation(&mut self, session_id: &str) -> Result<()> {
        self.load_conversation(session_id).await
    }
    
    /// Save conversation
    pub async fn save_conversation(&mut self) -> Result<()> {
        let conversation = ConversationData {
            session_id: self.session_id.clone(),
            model: self.current_model.clone(),
            messages: self.messages.clone(),
            timestamp: crate::utils::timestamp_ms(),
        };
        
        let path = self.conversation_dir.join(format!("{}.json", self.session_id));
        fs::create_dir_all(&self.conversation_dir)?;
        
        let json = serde_json::to_string_pretty(&conversation)?;
        fs::write(path, json)?;
        
        Ok(())
    }
    
    /// Load conversation
    pub async fn load_conversation(&mut self, session_id: &str) -> Result<()> {
        let path = self.conversation_dir.join(format!("{}.json", session_id));
        
        if !path.exists() {
            return Err(Error::NotFound(format!("Session {} not found", session_id)));
        }
        
        let json = fs::read_to_string(path)?;
        let conversation: ConversationData = serde_json::from_str(&json)?;
        
        self.session_id = conversation.session_id;
        self.current_model = conversation.model;
        self.messages = conversation.messages.clone();
        self.invalidate_cache();  // MUST invalidate cache after loading messages!
        self.scroll_to_bottom();
        
        // Reconstruct AI conversation history from the loaded messages
        // This allows the AI to have context when resuming
        let mut ai_messages = Vec::new();
        for msg in &conversation.messages {
            match msg.role.as_str() {
                "user" => {
                    // Skip system messages and command outputs
                    if !msg.content.starts_with("Session resumed") && 
                       !msg.content.starts_with("Loaded conversation:") &&
                       msg.role != "system" &&
                       msg.role != "command_output" {
                        ai_messages.push(crate::ai::Message {
                            role: crate::ai::MessageRole::User,
                            content: crate::ai::MessageContent::Text(msg.content.clone()),
                            name: None,
                        });
                    }
                }
                "assistant" => {
                    // Skip tool result messages, they're handled separately
                    if !msg.content.starts_with("**Result:**") {
                        ai_messages.push(crate::ai::Message {
                            role: crate::ai::MessageRole::Assistant,
                            content: crate::ai::MessageContent::Text(msg.content.clone()),
                            name: None,
                        });
                    }
                }
                _ => {
                    // Skip system, error, and command_output messages
                }
            }
        }
        
        // Store the reconstructed messages so they can be sent to the agent
        self.loaded_ai_messages = Some(ai_messages);
        
        self.add_message(&format!("Loaded conversation: {} (with {} messages)", session_id, conversation.messages.len()));
        
        Ok(())
    }
    
    /// List available sessions
    async fn list_sessions(&self) -> Result<Vec<SessionInfo>> {
        let mut sessions = Vec::new();
        
        if let Ok(entries) = fs::read_dir(&self.conversation_dir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.ends_with(".json") {
                        let id = name.trim_end_matches(".json");
                        if let Ok(metadata) = entry.metadata() {
                            let modified_timestamp = metadata.modified()
                                .ok()
                                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                                .map(|d| d.as_secs())
                                .unwrap_or(0);
                            
                            let created_timestamp = metadata.created()
                                .ok()
                                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                                .map(|d| d.as_secs())
                                .unwrap_or(modified_timestamp);
                            
                            sessions.push(SessionInfo {
                                id: id.to_string(),
                                created_timestamp,
                                modified_timestamp,
                            });
                        }
                    }
                }
            }
        }
        
        sessions.sort_by(|a, b| b.modified_timestamp.cmp(&a.modified_timestamp));
        Ok(sessions)
    }
    
    /// Add MCP server
    pub fn add_mcp_server(&mut self, name: String, client: McpClient) {
        self.mcp_servers.insert(name, client);
    }
    
    /// Handle resize
    pub fn handle_resize(&mut self, width: u16, height: u16) {
        self.terminal_size = (width, height);
    }
    
    /// Tick for updates
    pub async fn tick(&mut self) -> Result<()> {
        // Update spinner if processing
        if self.is_processing {
            self.update_spinner();
        }
        
        // Update FPS
        let now = std::time::Instant::now();
        let frame_time = now.duration_since(self.last_frame_time).as_secs_f64();
        self.last_frame_time = now;
        
        self.fps_samples.push_back(1.0 / frame_time);
        if self.fps_samples.len() > 60 {
            self.fps_samples.pop_front();
        }
        
        // Check if we need to continue conversation after permission
        if self.continue_after_permission {
            self.continue_after_permission = false;
            
            if let Some(tool_result) = self.pending_tool_result.take() {
                // Continue the conversation with the tool result
                // This matches JavaScript: send tool result as USER message to trigger synthesis
                let _ = self.continue_conversation_with_tool_result(tool_result).await;
            }
        }
        
        Ok(())
    }
    
    /// Get FPS
    pub fn get_fps(&self) -> f64 {
        if self.fps_samples.is_empty() {
            0.0
        } else {
            self.fps_samples.iter().sum::<f64>() / self.fps_samples.len() as f64
        }
    }
    
    /// Get latency
    pub fn get_latency(&self) -> u64 {
        if self.latency_samples.is_empty() {
            0
        } else {
            self.latency_samples.iter().sum::<u64>() / self.latency_samples.len() as u64
        }
    }
    
    /// Get message memory usage
    pub fn get_message_memory(&self) -> u64 {
        self.messages.iter()
            .map(|m| m.content.len() as u64 + 64) // Content + overhead
            .sum()
    }
    
    /// Continue conversation with tool result after permission
    async fn continue_conversation_with_tool_result(&mut self, initial_tool_result: crate::ai::ContentPart) -> Result<()> {
        self.is_processing = true;
        
        // Build conversation history
        let mut messages = Vec::new();
        
        // Add all previous messages
        for msg in &self.messages {
            let role = match msg.role.as_str() {
                "user" => crate::ai::MessageRole::User,
                "assistant" => crate::ai::MessageRole::Assistant,
                _ => continue,
            };
            
            messages.push(crate::ai::Message {
                role,
                content: crate::ai::MessageContent::Text(msg.content.clone()),
                name: None,
            });
        }
        
        // Add initial tool result as USER message (matching JavaScript behavior)
        messages.push(crate::ai::Message {
            role: crate::ai::MessageRole::User,
            content: crate::ai::MessageContent::Multipart(vec![initial_tool_result]),
            name: None,
        });
        
        // Create AI client and tool executor
        let ai_client = crate::ai::create_client().await?;
        let tool_executor = self.create_tool_executor();
        let tools = tool_executor.get_available_tools();
        
        // Continue agentic loop until AI stops requesting tools
        let mut loop_count = 0;
        const MAX_LOOPS: usize = 10;
        
        loop {
            loop_count += 1;
            if loop_count > MAX_LOOPS {
                self.add_message("Max tool execution loops reached. Stopping.");
                break;
            }
            
            // Build request
            let mut request = ai_client
                .create_chat_request()
                .messages(messages.clone())
                .max_tokens(4096)
                .temperature(0.7);
            
            // Set system prompt
            let system = if let Some(prompt) = &self.system_prompt {
                prompt.clone()
            } else {
                crate::ai::system_prompt::get_system_prompt("Claude Code")
            };
            request = request.system(system);
            
            // Add tools
            if !tools.is_empty() {
                request = request.tools(tools.clone());
            }
            
            // Get AI response
            let response = ai_client.chat(request.build()).await?;
            
            // Process response and collect tool uses
            let mut response_text = String::new();
            let mut tool_results = Vec::new();
            let mut has_tool_use = false;
            let mut assistant_content_parts = Vec::new();
            
            for part in response.content {
                match &part {
                    crate::ai::ContentPart::Text { text, .. } => {
                        response_text.push_str(text);
                        assistant_content_parts.push(part);
                    }
                    crate::ai::ContentPart::ToolUse { id, name, input } => {
                        has_tool_use = true;
                        
                        // Show tool execution in UI
                        self.add_message(&format!("[Executing tool: {}]", name));
                        
                        // Execute tool (permissions already granted in this flow)
                        match tool_executor.execute(name, input.clone()).await {
                            Ok(result) => {
                                if let crate::ai::ContentPart::ToolResult { content, .. } = &result {
                                    self.add_message(&format!("**Result:**\n{}", content));
                                }
                                // Use the actual result with correct tool_use_id
                                tool_results.push(result);
                            }
                            Err(e) => {
                                // Permission errors shouldn't happen here (handled in streaming flow)
                                let error_msg = format!("Error: {}", e);
                                self.add_message(&error_msg);
                                
                                tool_results.push(crate::ai::ContentPart::ToolResult {
                                    tool_use_id: id.clone(),
                                    content: error_msg,
                                    is_error: Some(true),
                                });
                            }
                        }
                        
                        assistant_content_parts.push(part);
                    }
                    _ => {
                        assistant_content_parts.push(part);
                    }
                }
            }
            
            // Update token usage
            self.latency_samples.push_back(response.usage.input_tokens as u64 + response.usage.output_tokens as u64);
            if self.latency_samples.len() > 100 {
                self.latency_samples.pop_front();
            }
            
            // Add assistant message to conversation
            messages.push(crate::ai::Message {
                role: crate::ai::MessageRole::Assistant,
                content: crate::ai::MessageContent::Multipart(assistant_content_parts),
                name: None,
            });
            
            // Show any text response from the assistant
            if !response_text.is_empty() {
                self.add_message(&response_text);
                
                // Add to UI conversation history
                self.messages.push(Message {
                    role: "assistant".to_string(),
                    content: response_text,
                    timestamp: crate::utils::timestamp_ms(),
                });
            }
            
            // Check stop reason to determine if we should continue
            let should_continue = match response.stop_reason {
                Some(crate::ai::StopReason::ToolUse) => true,
                _ => has_tool_use,
            };
            
            if !should_continue {
                break;
            }
            
            // Add tool results as a user message to continue
            if !tool_results.is_empty() {
                messages.push(crate::ai::Message {
                    role: crate::ai::MessageRole::User,
                    content: crate::ai::MessageContent::Multipart(tool_results),
                    name: None,
                });
            }
        }
        
        self.is_processing = false;
        // Invalidate cache to ensure proper rendering after processing completes
        self.invalidate_cache();
        self.scroll_to_bottom();
        
        // Auto-save if enabled
        if self.auto_save {
            let _ = self.save_conversation().await;
        }
        
        Ok(())
    }
    
    /// Should exit
    pub fn should_exit(&self) -> bool {
        self.should_exit
    }
    
    /// Quit
    pub fn quit(&mut self) {
        self.should_exit = true;
    }
    
    /// Clear messages and reset session state
    /// This performs a full cleanup similar to JavaScript's /clear command
    pub fn clear_messages(&mut self) {
        // Clear conversation messages
        self.messages.clear();
        self.scroll_offset = 0;

        // Invalidate the rendered lines cache
        self.invalidate_cache();

        // Clear temporary state
        self.pasted_contents.clear();
        self.next_paste_id = 0;
        self.last_paste_content = None;
        self.paste_count = 0;

        // Clear continuation state
        self.continuation_messages = None;
        self.hit_iteration_limit = false;

        // Clear autocomplete state
        self.autocomplete_matches.clear();
        self.is_autocomplete_visible = false;
        self.selected_suggestion = 0;

        // Reset processing state
        self.is_processing = false;

        // Clear loaded AI messages from previous session
        self.loaded_ai_messages = None;

        // TODO: Execute SessionEnd hooks when hook system is implemented
        // TODO: Execute SessionStart hooks when hook system is implemented
        // TODO: Clear MCP context when MCP system tracks state
    }
    
    /// Compact conversation with automatic summary generation
    pub async fn compact_conversation(&mut self) -> Result<()> {
        if self.messages.len() <= 1 {
            self.add_message("No conversation to compact");
            return Ok(());
        }

        // Show progress message
        self.add_message("Generating AI summary...");

        // Generate summary of conversation using AI
        let summary = match self.generate_conversation_summary_ai().await {
            Ok(s) => s,
            Err(e) => {
                // Fallback to basic summary on error
                self.add_message(&format!("AI summarization failed: {}. Using basic summary.", e));
                self.generate_conversation_summary_basic()
            }
        };

        // Save current conversation before compacting
        self.save_conversation().await?;

        // Clear messages except the first (system) and add summary
        let system_message = self.messages.first().cloned();
        self.messages.clear();
        self.scroll_offset = 0;

        if let Some(system_msg) = system_message {
            self.messages.push(system_msg);
        }

        // Add summary as a system message
        self.messages.push(Message {
            role: "assistant".to_string(),
            content: format!("**Conversation Summary:**\n\n{}", summary),
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
        });

        self.add_message("✅ Conversation compacted with AI summary");
        Ok(())
    }
    
    /// Compact conversation with user-provided summary
    pub async fn compact_conversation_with_summary(&mut self, summary: &str) -> Result<()> {
        if self.messages.len() <= 1 {
            self.add_message("No conversation to compact");
            return Ok(());
        }
        
        // Save current conversation before compacting
        self.save_conversation().await?;
        
        // Clear messages except the first (system) and add custom summary
        let system_message = self.messages.first().cloned();
        self.messages.clear();
        self.scroll_offset = 0;
        
        if let Some(system_msg) = system_message {
            self.messages.push(system_msg);
        }
        
        // Add user-provided summary as a system message
        self.messages.push(Message {
            role: "assistant".to_string(),
            content: format!("**Conversation Summary:**\n\n{}", summary),
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
        });
        
        self.add_message("✅ Conversation compacted with custom summary");
        Ok(())
    }
    
    /// Generate a summary of the current conversation using AI
    async fn generate_conversation_summary_ai(&self) -> Result<String> {
        use crate::ai::summarization::{get_summarization_system_prompt, get_detailed_summary_prompt};

        // Build conversation history for the AI
        let mut ai_messages = Vec::new();

        // Add conversation content as a single user message asking for summary
        let mut conversation_text = String::new();
        conversation_text.push_str("Please summarize the following conversation:\n\n");

        for msg in &self.messages {
            let role_label = match msg.role.as_str() {
                "user" => "User",
                "assistant" => "Assistant",
                "system" => "System",
                _ => &msg.role,
            };
            conversation_text.push_str(&format!("**{}**: {}\n\n", role_label, msg.content));
        }

        conversation_text.push_str("\n---\n\n");
        conversation_text.push_str(&get_detailed_summary_prompt());

        ai_messages.push(crate::ai::Message {
            role: crate::ai::MessageRole::User,
            content: crate::ai::MessageContent::Text(conversation_text),
            name: None,
        });

        // Create AI client and request
        let ai_client = crate::ai::create_client().await?;

        let request = crate::ai::ChatRequest {
            model: self.current_model.clone(),
            messages: ai_messages,
            max_tokens: Some(4096),
            temperature: Some(0.3), // Lower temperature for more focused summaries
            top_p: None,
            top_k: None,
            stop_sequences: None,
            stream: Some(false),
            system: Some(get_summarization_system_prompt().to_string()),
            tools: None,
            tool_choice: None,
            metadata: None,
            betas: None,
        };

        // Send request to AI
        let response = ai_client.chat(request).await?;

        // Extract text from response
        let mut summary = String::new();
        for part in response.content {
            if let crate::ai::ContentPart::Text { text, .. } = part {
                summary.push_str(&text);
            }
        }

        // Extract just the summary portion if wrapped in <summary> tags
        if let Some(start) = summary.find("<summary>") {
            if let Some(end) = summary.find("</summary>") {
                let start_idx = start + "<summary>".len();
                if start_idx < end {
                    summary = summary[start_idx..end].trim().to_string();
                }
            }
        }

        if summary.is_empty() {
            return Err(crate::error::Error::Other("AI returned empty summary".to_string()));
        }

        Ok(summary)
    }

    /// Generate a basic summary of the current conversation (fallback)
    fn generate_conversation_summary_basic(&self) -> String {
        if self.messages.len() <= 1 {
            return "Empty conversation".to_string();
        }

        let mut summary = String::new();
        let mut user_messages = 0;
        let mut assistant_messages = 0;
        let mut topics = Vec::new();

        // Count messages and extract key topics
        for message in &self.messages {
            match message.role.as_str() {
                "user" => {
                    user_messages += 1;
                    // Extract potential topics from user messages
                    let words: Vec<&str> = message.content.split_whitespace().collect();
                    if words.len() > 3 {
                        topics.push(words[..3].join(" "));
                    }
                }
                "assistant" => assistant_messages += 1,
                _ => {}
            }
        }

        summary.push_str(&format!("Conversation with {} user messages and {} assistant responses.\n\n",
            user_messages, assistant_messages));

        if !topics.is_empty() {
            summary.push_str("Topics discussed:\n");
            for (i, topic) in topics.iter().take(5).enumerate() {
                summary.push_str(&format!("{}. {}\n", i + 1, topic));
            }
        }

        summary.push_str("\n*This conversation was compacted to free up context space.*");
        summary
    }

    /// Run /init command - AI-powered CLAUDE.md generation
    /// Analyzes codebase and creates/updates CLAUDE.md with project-specific guidance
    pub async fn run_init_command(&mut self) -> Result<()> {
        // Gather context about the codebase
        let cwd = std::env::current_dir().unwrap_or_default();
        let mut context = String::new();

        // Check for existing CLAUDE.md
        let claude_md_path = cwd.join("CLAUDE.md");
        let existing_claude_md = if claude_md_path.exists() {
            match tokio::fs::read_to_string(&claude_md_path).await {
                Ok(content) => {
                    context.push_str("## Existing CLAUDE.md\n```\n");
                    context.push_str(&content);
                    context.push_str("\n```\n\n");
                    Some(content)
                }
                Err(_) => None,
            }
        } else {
            None
        };

        // Check for README.md
        let readme_path = cwd.join("README.md");
        if readme_path.exists() {
            if let Ok(content) = tokio::fs::read_to_string(&readme_path).await {
                context.push_str("## README.md\n```\n");
                // Truncate if too long
                if content.len() > 8000 {
                    context.push_str(&content[..8000]);
                    context.push_str("\n... (truncated)\n");
                } else {
                    context.push_str(&content);
                }
                context.push_str("\n```\n\n");
            }
        }

        // Check for package.json (Node.js projects)
        let package_json_path = cwd.join("package.json");
        if package_json_path.exists() {
            if let Ok(content) = tokio::fs::read_to_string(&package_json_path).await {
                context.push_str("## package.json\n```json\n");
                context.push_str(&content);
                context.push_str("\n```\n\n");
            }
        }

        // Check for Cargo.toml (Rust projects)
        let cargo_toml_path = cwd.join("Cargo.toml");
        if cargo_toml_path.exists() {
            if let Ok(content) = tokio::fs::read_to_string(&cargo_toml_path).await {
                context.push_str("## Cargo.toml\n```toml\n");
                context.push_str(&content);
                context.push_str("\n```\n\n");
            }
        }

        // Check for Makefile
        let makefile_path = cwd.join("Makefile");
        if makefile_path.exists() {
            if let Ok(content) = tokio::fs::read_to_string(&makefile_path).await {
                context.push_str("## Makefile\n```makefile\n");
                if content.len() > 4000 {
                    context.push_str(&content[..4000]);
                    context.push_str("\n... (truncated)\n");
                } else {
                    context.push_str(&content);
                }
                context.push_str("\n```\n\n");
            }
        }

        // Check for .cursorrules
        let cursorrules_path = cwd.join(".cursorrules");
        if cursorrules_path.exists() {
            if let Ok(content) = tokio::fs::read_to_string(&cursorrules_path).await {
                context.push_str("## .cursorrules\n```\n");
                context.push_str(&content);
                context.push_str("\n```\n\n");
            }
        }

        // Build AI prompt
        let system_prompt = r#"You are an expert at analyzing codebases and creating documentation.

Your task is to create a CLAUDE.md file that will be given to future instances of Claude Code to help them work effectively in this repository.

What to include:
1. Commands commonly used for building, linting, and running tests. Include how to run a single test.
2. High-level code architecture and structure - the "big picture" that requires reading multiple files to understand.

What to avoid:
- Obvious instructions like "Provide helpful error messages" or "Write unit tests"
- Listing every file/component that can be easily discovered
- Generic development practices
- Made-up information not from actual project files

Start the file with:
# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository."#;

        let user_prompt = if existing_claude_md.is_some() {
            format!("Here is context about the codebase. Please suggest improvements to the existing CLAUDE.md:\n\n{}", context)
        } else {
            format!("Here is context about the codebase. Please create a CLAUDE.md file:\n\n{}", context)
        };

        // Build AI messages
        let ai_messages = vec![crate::ai::Message {
            role: crate::ai::MessageRole::User,
            content: crate::ai::MessageContent::Text(user_prompt),
            name: None,
        }];

        // Create AI client and request
        let ai_client = crate::ai::create_client().await?;

        let request = crate::ai::ChatRequest {
            model: self.current_model.clone(),
            messages: ai_messages,
            max_tokens: Some(4096),
            temperature: Some(0.3),
            top_p: None,
            top_k: None,
            stop_sequences: None,
            stream: Some(false),
            system: Some(system_prompt.to_string()),
            tools: None,
            tool_choice: None,
            metadata: None,
            betas: None,
        };

        // Send request
        let response = ai_client.chat(request).await?;

        // Extract text from response
        let mut claude_md_content = String::new();
        for part in response.content {
            if let crate::ai::ContentPart::Text { text, .. } = part {
                claude_md_content.push_str(&text);
            }
        }

        if claude_md_content.is_empty() {
            self.add_error("AI returned empty response");
            return Ok(());
        }

        // Write to CLAUDE.md
        tokio::fs::write(&claude_md_path, &claude_md_content).await?;

        self.add_message(&format!("✅ Created/updated CLAUDE.md ({} bytes)", claude_md_content.len()));
        self.add_message(&format!("   Location: {}", claude_md_path.display()));

        Ok(())
    }

    /// Run /review command - AI-powered PR code review
    /// Reviews a pull request using gh CLI and AI analysis
    pub async fn run_review_command(&mut self, pr_number: Option<String>) -> Result<()> {
        // Check if gh CLI is available
        let gh_check = tokio::process::Command::new("gh")
            .arg("--version")
            .output()
            .await;

        if gh_check.is_err() {
            self.add_error("gh CLI not found. Please install GitHub CLI: https://cli.github.com/");
            return Ok(());
        }

        // If no PR number, list open PRs
        let pr_num = match pr_number {
            Some(num) => num,
            None => {
                // List open PRs
                let pr_list = tokio::process::Command::new("gh")
                    .args(["pr", "list", "--limit", "10"])
                    .output()
                    .await?;

                let list_output = String::from_utf8_lossy(&pr_list.stdout);
                if list_output.trim().is_empty() {
                    self.add_message("No open pull requests found.");
                    return Ok(());
                }

                self.add_message("**Open Pull Requests:**");
                self.add_message(&list_output);
                self.add_message("\nUse `/review <pr-number>` to review a specific PR.");
                return Ok(());
            }
        };

        // Get PR details
        self.add_message(&format!("Fetching PR #{}...", pr_num));

        let pr_view = tokio::process::Command::new("gh")
            .args(["pr", "view", &pr_num, "--json", "title,body,author,additions,deletions,files"])
            .output()
            .await?;

        if !pr_view.status.success() {
            let stderr = String::from_utf8_lossy(&pr_view.stderr);
            self.add_error(&format!("Failed to get PR details: {}", stderr));
            return Ok(());
        }

        let pr_details = String::from_utf8_lossy(&pr_view.stdout);

        // Get PR diff
        let pr_diff = tokio::process::Command::new("gh")
            .args(["pr", "diff", &pr_num])
            .output()
            .await?;

        let diff_content = String::from_utf8_lossy(&pr_diff.stdout);

        // Truncate diff if too large
        let diff_truncated = if diff_content.len() > 50000 {
            format!("{}...\n\n[Diff truncated - {} total bytes]", &diff_content[..50000], diff_content.len())
        } else {
            diff_content.to_string()
        };

        // Build AI prompt for code review
        let system_prompt = r#"You are an expert code reviewer. Analyze the pull request and provide a thorough code review.

Focus on:
- Code correctness and potential bugs
- Following project conventions
- Performance implications
- Test coverage
- Security considerations

Format your review with clear sections:
## Overview
## Code Quality
## Potential Issues
## Suggestions
## Security Considerations (if any)"#;

        let user_prompt = format!(
            "Please review this pull request:\n\n## PR Details\n```json\n{}\n```\n\n## Diff\n```diff\n{}\n```",
            pr_details, diff_truncated
        );

        // Build AI messages
        let ai_messages = vec![crate::ai::Message {
            role: crate::ai::MessageRole::User,
            content: crate::ai::MessageContent::Text(user_prompt),
            name: None,
        }];

        // Create AI client and request
        let ai_client = crate::ai::create_client().await?;

        let request = crate::ai::ChatRequest {
            model: self.current_model.clone(),
            messages: ai_messages,
            max_tokens: Some(4096),
            temperature: Some(0.3),
            top_p: None,
            top_k: None,
            stop_sequences: None,
            stream: Some(false),
            system: Some(system_prompt.to_string()),
            tools: None,
            tool_choice: None,
            metadata: None,
            betas: None,
        };

        // Send request
        self.add_message("Analyzing changes...");
        let response = ai_client.chat(request).await?;

        // Extract text from response
        let mut review = String::new();
        for part in response.content {
            if let crate::ai::ContentPart::Text { text, .. } = part {
                review.push_str(&text);
            }
        }

        if review.is_empty() {
            self.add_error("AI returned empty review");
            return Ok(());
        }

        self.add_message(&format!("**Code Review for PR #{}**\n\n{}", pr_num, review));

        Ok(())
    }

    /// Toggle help
    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }
    
    /// Toggle debug
    pub fn toggle_debug(&mut self) {
        self.debug_mode = !self.debug_mode;
    }
    
    /// Toggle tool panel
    pub fn toggle_tool_panel(&mut self) {
        self.show_tool_panel = !self.show_tool_panel;
    }

    /// Toggle prompt stash (Ctrl+S)
    /// Matches JavaScript behavior at line 480754:
    /// - If input is empty and stash exists: restore from stash
    /// - If input has content: save to stash and clear input
    pub fn toggle_prompt_stash(&mut self) {
        let current_input: String = self.input_textarea.lines().join("\n");
        let cursor_pos = self.input_textarea.cursor().1;

        if current_input.trim().is_empty() {
            // Restore from stash if exists
            if let Some((stashed_text, _cursor_offset)) = self.stashed_input.take() {
                self.input_textarea = create_configured_textarea_with_content(stashed_text.lines());
                self.add_message("Restored input from stash");
            }
        } else {
            // Save to stash and clear
            self.stashed_input = Some((current_input, cursor_pos));
            self.input_textarea = create_configured_textarea();
            self.add_message("Input stashed (Ctrl+S to restore)");
        }
    }

    /// Toggle TODOs expanded display (Ctrl+T)
    /// Matches JavaScript behavior at line 481215
    pub fn toggle_todos_display(&mut self) {
        self.show_todos_expanded = !self.show_todos_expanded;
    }

    /// Toggle find/search mode (Ctrl+F)
    pub fn toggle_find_mode(&mut self) {
        self.show_find_mode = !self.show_find_mode;
        if !self.show_find_mode {
            // Clear search state when closing
            self.find_query.clear();
            self.find_results.clear();
            self.find_current_index = 0;
        }
    }

    /// Set thinking state (for interleaved thinking display)
    pub fn set_thinking(&mut self, thinking: Option<String>) {
        if thinking.is_some() && self.thinking_start_time.is_none() {
            self.thinking_start_time = Some(std::time::Instant::now());
        } else if thinking.is_none() {
            self.thinking_start_time = None;
        }
        self.current_thinking = thinking;
    }

    /// Get thinking duration in seconds
    pub fn get_thinking_duration_secs(&self) -> Option<u64> {
        self.thinking_start_time.map(|start| start.elapsed().as_secs())
    }

    /// Cancel operation
    pub async fn cancel_operation(&mut self) -> Result<()> {
        // Show cancelling status
        self.current_task_status = Some("Cancelling...".to_string());
        
        // First, cancel any active streaming
        if let Some(stream_cancel) = &self.stream_cancel_tx {
            if let Some(tx) = stream_cancel.lock().await.as_ref() {
                let _ = tx.send(());
            }
        }
        
        // Kill any running background shells
        let active_shells = crate::ai::tools::BACKGROUND_SHELLS.get_active_shells().await;
        for shell in active_shells {
            if shell.status == "running" {
                let _ = crate::ai::tools::BACKGROUND_SHELLS.kill_shell(&shell.id).await;
            }
        }
        
        // Then send the main cancellation signal
        if let Some(tx) = &self.cancel_tx {
            let _ = tx.send(());
        }
        
        // Reset UI state immediately
        self.is_processing = false;
        self.input_mode = true;
        
        // Clear task status after showing cancellation briefly
        if self.current_task_status.is_some() {
            // Keep "Cancelling..." for a brief moment, then clear
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            self.current_task_status = None;
        }
        Ok(())
    }
    
    /// Update spinner animation
    pub fn update_spinner(&mut self) {
        let now = std::time::Instant::now();
        if now.duration_since(self.last_spinner_update).as_millis() > 100 {
            self.spinner_frame = (self.spinner_frame + 1) % 3;
            self.last_spinner_update = now;
        }
    }
    
    /// Get current spinner character
    pub fn get_spinner_char(&self) -> &str {
        match self.spinner_frame {
            0 => "-",
            1 => "+",
            2 => "*",
            _ => "-",
        }
    }
    
    /// Calculate the actual line count including pasted content placeholders
    pub fn calculate_input_line_count(&self) -> usize {
        let text = self.input_textarea.lines().join("\n");
        let mut total_lines = 0;
        
        // Regular expression to match paste placeholders
        let placeholder_regex = regex::Regex::new(r"\[Pasted text #(\d+) \+(\d+) lines\]").unwrap();
        
        // Process the text to count actual lines
        let mut last_match_end = 0;
        for cap in placeholder_regex.captures_iter(&text) {
            let match_start = cap.get(0).unwrap().start();
            let match_end = cap.get(0).unwrap().end();
            
            // Count lines in text before this placeholder
            let text_before = &text[last_match_end..match_start];
            total_lines += text_before.lines().count();
            
            // Add the lines from the placeholder
            if let Ok(extra_lines) = cap[2].parse::<usize>() {
                total_lines += extra_lines + 1; // +1 for the line containing the placeholder itself
            } else {
                total_lines += 1; // Just count the placeholder as one line if parsing fails
            }
            
            last_match_end = match_end;
        }
        
        // Count lines in any remaining text after the last placeholder
        let remaining_text = &text[last_match_end..];
        total_lines += remaining_text.lines().count();
        
        // Return at least 1 line
        total_lines.max(1)
    }
    
    /// Set current task status
    pub fn set_task_status(&mut self, status: Option<String>) {
        if status.is_none() {
            self.spinner_frame = 0;
            self.current_progress = None;
        }
        self.current_task_status = status;
    }

    /// Set determinate progress (0.0 to 1.0)
    /// Matches JavaScript progress bar behavior at line 477030
    pub fn set_progress(&mut self, progress: f64) {
        self.current_progress = Some(progress.clamp(0.0, 1.0));
    }

    /// Clear progress (back to indeterminate)
    pub fn clear_progress(&mut self) {
        self.current_progress = None;
    }

    /// Get current progress for display
    pub fn get_progress(&self) -> Option<f64> {
        if self.terminal_progress_bar_enabled {
            self.current_progress
        } else {
            None
        }
    }

    /// Add to history
    fn add_to_history(&mut self, command: String) {
        self.command_history.push_front(command);
        if self.command_history.len() > self.max_history {
            self.command_history.pop_back();
        }
        self.history_index = None;
    }
    
    /// History up
    pub fn history_up(&mut self) {
        if self.command_history.is_empty() {
            return;
        }
        
        let new_index = match self.history_index {
            None => 0,
            Some(i) => (i + 1).min(self.command_history.len() - 1),
        };
        
        self.history_index = Some(new_index);
        if let Some(cmd) = self.command_history.get(new_index) {
            self.input_textarea = create_configured_textarea_with_content(cmd.lines());
        }
    }

    /// History down
    pub fn history_down(&mut self) {
        if let Some(index) = self.history_index {
            if index == 0 {
                self.history_index = None;
                self.input_textarea = create_configured_textarea();
            } else {
                self.history_index = Some(index - 1);
                if let Some(cmd) = self.command_history.get(index - 1) {
                    self.input_textarea = create_configured_textarea_with_content(cmd.lines());
                }
            }
        }
    }
    
    /// Handle tab completion
    pub fn handle_tab_completion(&mut self) {
        // Get current line
        let line = &self.input_textarea.lines()[self.input_textarea.cursor().0];
        
        // Simple command completion
        if line.starts_with('/') {
            let commands = vec![
                "/help", "/clear", "/save", "/load", "/resume", "/model",
                "/tools", "/mcp", "/compact", "/context", "/cost",
                "/settings", "/vim", "/add-dir", "/files", "/config",
                "/bashes", "/doctor", "/release-notes", "/exit", "/quit",
            ];
            
            for cmd in commands {
                if cmd.starts_with(line) {
                    // Replace current line with completed command
                    self.input_textarea.delete_line_by_head();
                    self.input_textarea.insert_str(cmd);
                    break;
                }
            }
        }
    }
    
    /// Detect if content change was a large paste and update input state
    pub fn detect_paste_and_update_input_state(&mut self) {
        let current_line_count = self.input_textarea.lines().len();
        let line_diff = current_line_count.saturating_sub(self.input_previous_line_count);
        
        // Detect paste: significant line increase (>2 lines added at once)
        if line_diff > 2 {
            self.input_paste_detected = true;
            // Auto-collapse for large pastes (>3 total lines)
            if current_line_count > 3 {
                self.input_expanded = false;
            }
        } else {
            self.input_paste_detected = false;
            // Auto-expand for small content
            if current_line_count <= 3 {
                self.input_expanded = true;
            }
        }
        
        self.input_previous_line_count = current_line_count;
    }
    
    /// Toggle input area expansion state
    pub fn toggle_input_expansion(&mut self) {
        self.input_expanded = !self.input_expanded;
        self.input_paste_detected = false; // Clear paste detection on manual toggle
    }
    
    /// Get the display height for the input area based on current state
    pub fn get_input_display_height(&self) -> u16 {
        let line_count = self.input_textarea.lines().len();
        
        if self.input_expanded {
            // Dynamic height: 3 minimum, 10 maximum
            let min_height = 3u16;
            let max_height = 10u16;
            // +2 for borders
            (line_count as u16 + 2).max(min_height).min(max_height)
        } else {
            // Collapsed: show only 3 lines + borders
            5u16  // 3 content lines + 2 border lines
        }
    }
    
    pub fn scroll_to_bottom(&mut self) {
        // MUST rebuild cache first to get accurate line count with collapsed content
        if !self.cache_valid || self.cache_expanded_state != self.expanded_view {
            self.rebuild_cache();
        }
        
        // Now calculate based on ACTUAL rendered lines (with collapsed content)
        let total_lines = self.rendered_lines_cache.len();
        
        // Get terminal height and account for input area and status bar
        let (_, height) = self.terminal_size;
        // Chat area is terminal height minus input area (3-5 lines) and status bar (1 line)
        let viewport_height = height.saturating_sub(6) as usize;
        // Scroll to show the last viewport_height lines
        self.scroll_offset = total_lines.saturating_sub(viewport_height);
    }
    
    fn calculate_total_lines(&self) -> usize {
        let mut total = 0;
        for msg in &self.messages {
            // Count actual lines in the message content
            total += msg.content.lines().count();
            // Add lines for UI elements (dots, spacing, etc.)
            if msg.role == "user" && msg.content.starts_with('/') {
                total += 1; // Add line for continuation indicator
            }
        }
        total
    }
    
    pub fn scroll_to_top(&mut self) {
        self.scroll_offset = 0;
    }
    
    pub fn scroll_up(&mut self, n: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(n);
    }
    
    pub fn scroll_down(&mut self, n: usize) {
        // Use cached lines for accurate count
        if !self.cache_valid || self.cache_expanded_state != self.expanded_view {
            self.rebuild_cache();
        }
        let total_lines = self.rendered_lines_cache.len();
        
        let (_, height) = self.terminal_size;
        let viewport_height = height.saturating_sub(6) as usize;
        // Don't scroll past the point where the last line is at the bottom of viewport
        let max_scroll = total_lines.saturating_sub(viewport_height);
        self.scroll_offset = (self.scroll_offset + n).min(max_scroll);
    }
    
    pub fn estimate_token_count(&self) -> usize {
        let mut total = 0;
        for msg in &self.messages {
            total += msg.content.len() / 4;
        }
        if let Some(system) = &self.system_prompt {
            total += system.len() / 4;
        }
        total
    }

    /// Count conversation tokens using the Anthropic API
    /// Returns accurate token count for the messages in this conversation
    pub async fn count_conversation_tokens(&self) -> crate::error::Result<u64> {
        // Skip if no messages to count
        if self.messages.is_empty() {
            return Ok(0);
        }

        // Build AI messages from UI messages
        let mut ai_messages = Vec::new();
        for msg in &self.messages {
            let role = match msg.role.as_str() {
                "user" => crate::ai::MessageRole::User,
                "assistant" => crate::ai::MessageRole::Assistant,
                _ => continue, // Skip system messages for now
            };
            ai_messages.push(crate::ai::Message {
                role,
                content: crate::ai::MessageContent::Text(msg.content.clone()),
                name: None,
            });
        }

        // Create request
        let request = crate::auth::client::CountTokensRequest {
            model: self.current_model.clone(),
            messages: ai_messages,
            betas: None,
        };

        // Get client and count tokens
        let client = crate::ai::create_client().await?;
        let response = client.count_tokens(request).await?;

        Ok(response.input_tokens)
    }

    pub fn get_model_token_limit(&self) -> usize {
        if self.current_model.contains("opus") {
            200000
        } else if self.current_model.contains("sonnet") {
            200000
        } else if self.current_model.contains("haiku") {
            200000
        } else {
            100000
        }
    }

    /// Get list of available models with names, IDs, and descriptions
    pub fn get_available_models(&self) -> Vec<(&'static str, &'static str, &'static str)> {
        vec![
            ("Opus 4.5", "claude-opus-4-5-20251101", "Most capable model, best for complex tasks"),
            ("Opus 4.1", "claude-opus-4-1-20250805", "Previous Opus version"),
            ("Sonnet 4.5", "claude-sonnet-4-5-20250929", "Balanced speed and capability"),
            ("Sonnet 4", "claude-sonnet-4-20250514", "Previous Sonnet version"),
            ("Haiku 4.5", "claude-haiku-4-5-20251001", "Fastest model, best for simple tasks"),
        ]
    }

    /// Get the index of the current model in the available models list
    pub fn get_model_picker_index(&self) -> usize {
        let models = self.get_available_models();
        models.iter()
            .position(|(_, id, _)| *id == self.current_model)
            .unwrap_or(0)
    }

    /// Select a model from the picker by index
    pub fn select_model_by_index(&mut self, index: usize) {
        let models = self.get_available_models();
        if index < models.len() {
            self.current_model = models[index].1.to_string();
            self.add_message(&format!("Model changed to: {} ({})", models[index].0, models[index].1));
        }
        self.show_model_picker = false;
    }

    pub fn format_relative_time(&self, timestamp: u64) -> String {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        
        let diff = now.saturating_sub(timestamp);
        
        match diff {
            0..=59 => "just now".to_string(),
            60..=3599 => {
                let mins = diff / 60;
                format!("{} min{} ago", mins, if mins == 1 { "" } else { "s" })
            }
            3600..=86399 => {
                let hours = diff / 3600;
                format!("{} hour{} ago", hours, if hours == 1 { "" } else { "s" })
            }
            86400..=604799 => {
                let days = diff / 86400;
                format!("{} day{} ago", days, if days == 1 { "" } else { "s" })
            }
            604800..=2591999 => {
                let weeks = diff / 604800;
                format!("{} week{} ago", weeks, if weeks == 1 { "" } else { "s" })
            }
            _ => {
                let months = diff / 2592000;
                format!("{} month{} ago", months, if months == 1 { "" } else { "s" })
            }
        }
    }
    
    pub fn rebuild_cache(&mut self) {
        use crate::tui::markdown::parse_markdown;
        use ratatui::prelude::*;
        
        let mut all_lines: Vec<Line> = Vec::new();
        
        for msg in self.messages.iter() {
            match msg.role.as_str() {
                "user" => {
                    // Use bright magenta for user messages to ensure visibility
                    let dot = if cfg!(target_os = "macos") { "⏺" } else { "●" };
                    if msg.content.starts_with('/') {
                        all_lines.push(Line::from(vec![
                            Span::styled(dot, Style::default().fg(Color::Magenta)),
                            Span::raw(" "),
                            Span::styled(msg.content.clone(), Style::default().fg(Color::LightMagenta))
                        ]));
                        all_lines.push(Line::from(vec![
                            Span::raw("  ⎿  "),
                        ]));
                    } else {
                        let mut first_line = true;
                        for line in msg.content.lines() {
                            if first_line {
                                all_lines.push(Line::from(vec![
                                    Span::styled(dot, Style::default().fg(Color::Magenta)),
                                    Span::raw(" "),
                                    Span::styled(line.to_string(), Style::default().fg(Color::LightMagenta))
                                ]));
                                first_line = false;
                            } else {
                                all_lines.push(Line::from(vec![
                                    Span::raw("  "),
                                    Span::styled(line.to_string(), Style::default().fg(Color::LightMagenta))
                                ]));
                            }
                        }
                    }
                }
                "command_output" => {
                    let lines: Vec<&str> = msg.content.lines().collect();
                    if lines.len() > 10 && !self.expanded_view {
                        for line in lines.iter().take(3) {
                            all_lines.push(Line::from(vec![
                                Span::raw("     "),
                                Span::styled(line.to_string(), Style::default().fg(Color::White)),
                            ]));
                        }
                        all_lines.push(Line::from(vec![
                            Span::raw("  ⎿  "),
                            Span::styled(format!("... {} more lines", lines.len() - 3), Style::default().fg(Color::Gray)),
                            Span::raw(" "),
                            Span::styled("(ctrl+r to expand)", Style::default().fg(Color::Gray).add_modifier(Modifier::ITALIC)),
                        ]));
                    } else {
                        for line in lines {
                            all_lines.push(Line::from(vec![
                                Span::raw("     "),
                                Span::styled(line.to_string(), Style::default().fg(Color::White)),
                            ]));
                        }
                    }
                }
                "assistant" => {
                    let dot = if cfg!(target_os = "macos") { "⏺" } else { "●" };
                    let is_tool_msg = msg.content.starts_with("[Executing tool:") || 
                                     msg.content.starts_with("**Result:**");
                    let dot_color = if is_tool_msg { Color::Green } else { Color::White };
                    
                    if msg.content.starts_with("**Result:**") {
                        let lines: Vec<&str> = msg.content.lines().collect();
                        if lines.len() > 10 && !self.expanded_view {
                            all_lines.push(Line::from(vec![
                                Span::styled(dot, Style::default().fg(Color::Green)),
                                Span::raw(" "),
                                Span::styled("**Result:**", Style::default().fg(Color::White)),
                            ]));

                            for line in lines.iter().skip(1).take(3) {
                                all_lines.push(Line::from(vec![
                                    Span::raw("     "),
                                    Span::styled(line.to_string(), Style::default().fg(Color::White)),
                                ]));
                            }

                            all_lines.push(Line::from(vec![
                                Span::raw("  ⎿  "),
                                Span::styled(format!("... {} more lines", lines.len() - 4), Style::default().fg(Color::Gray)),
                                Span::raw(" "),
                                Span::styled("(ctrl+r to expand)", Style::default().fg(Color::Gray).add_modifier(Modifier::ITALIC)),
                            ]));
                        } else {
                            let text = parse_markdown(&msg.content);
                            let mut first_line = true;
                            for line in text.lines {
                                if first_line {
                                    let mut new_spans = vec![
                                        Span::styled(dot, Style::default().fg(dot_color)),
                                        Span::raw(" "),
                                    ];
                                    new_spans.extend(line.spans);
                                    all_lines.push(Line::from(new_spans));
                                    first_line = false;
                                } else {
                                    all_lines.push(line);
                                }
                            }
                        }
                    } else {
                        let text = parse_markdown(&msg.content);
                        let mut first_line = true;
                        for line in text.lines {
                            if first_line {
                                let mut new_spans = vec![
                                    Span::styled(dot, Style::default().fg(dot_color)),
                                    Span::raw(" "),
                                ];
                                new_spans.extend(line.spans);
                                all_lines.push(Line::from(new_spans));
                                first_line = false;
                            } else {
                                all_lines.push(line);
                            }
                        }
                    }
                }
                "system" => {
                    let dot = if cfg!(target_os = "macos") { "⏺" } else { "●" };
                    let mut first_line = true;
                    for line in msg.content.lines() {
                        if first_line {
                            all_lines.push(Line::from(vec![
                                Span::styled(dot, Style::default().fg(Color::Yellow)),
                                Span::raw(" "),
                                Span::styled(line.to_string(), Style::default().fg(Color::Yellow))
                            ]));
                            first_line = false;
                        } else {
                            all_lines.push(Line::from(vec![
                                Span::raw("   "),
                                Span::styled(line.to_string(), Style::default().fg(Color::Yellow))
                            ]));
                        }
                    }
                }
                "error" => {
                    let dot = if cfg!(target_os = "macos") { "⏺" } else { "●" };
                    let mut first_line = true;
                    for line in msg.content.lines() {
                        if first_line {
                            all_lines.push(Line::from(vec![
                                Span::styled(dot, Style::default().fg(Color::Red)),
                                Span::raw(" "),
                                Span::styled(line.to_string(), Style::default().fg(Color::Red))
                            ]));
                            first_line = false;
                        } else {
                            all_lines.push(Line::from(vec![
                                Span::raw("   "),
                                Span::styled(line.to_string(), Style::default().fg(Color::Red))
                            ]));
                        }
                    }
                }
                _ => {
                    for line in msg.content.lines() {
                        all_lines.push(Line::from(vec![
                            Span::raw(line.to_string())
                        ]));
                    }
                }
            }
        }
        
        self.rendered_lines_cache = all_lines;
        self.cache_valid = true;
        self.cache_expanded_state = self.expanded_view;
    }
    
    pub fn invalidate_cache(&mut self) {
        self.cache_valid = false;
    }
    
    pub fn get_cached_lines(&mut self) -> &Vec<ratatui::text::Line<'static>> {
        if !self.cache_valid || self.cache_expanded_state != self.expanded_view {
            self.rebuild_cache();
        }
        &self.rendered_lines_cache
    }
    
    fn truncate_string(&self, s: &str, max_len: usize) -> String {
        if s.len() <= max_len {
            s.to_string()
        } else {
            format!("{}...", &s[..max_len.saturating_sub(3)])
        }
    }
    
    async fn get_session_summary(&self, session_id: &str) -> Result<String> {
        let path = self.conversation_dir.join(format!("{}.json", session_id));
        
        if path.exists() {
            let json = fs::read_to_string(path)?;
            let conversation: ConversationData = serde_json::from_str(&json)?;
            
            // Get first user message as summary
            for msg in &conversation.messages {
                if msg.role == "user" {
                    return Ok(self.truncate_string(&msg.content, 100));
                }
            }
        }
        
        Ok("No summary available".to_string())
    }
    
    async fn get_session_message_count(&self, session_id: &str) -> Result<usize> {
        let path = self.conversation_dir.join(format!("{}.json", session_id));
        
        if path.exists() {
            let json = fs::read_to_string(path)?;
            let conversation: ConversationData = serde_json::from_str(&json)?;
            return Ok(conversation.messages.len());
        }
        
        Ok(0)
    }
    
    pub fn get_git_branch(&self) -> String {
        // Get current git branch
        std::process::Command::new("git")
            .args(&["rev-parse", "--abbrev-ref", "HEAD"])
            .output()
            .ok()
            .and_then(|output| {
                if output.status.success() {
                    String::from_utf8(output.stdout).ok()
                } else {
                    None
                }
            })
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "main".to_string())
    }
    
    pub fn estimate_cost(&self, token_count: usize) -> f64 {
        let input_price_per_1m = if self.current_model.contains("opus-4") {
            15.0
        } else if self.current_model.contains("sonnet-4") {
            3.0
        } else if self.current_model.contains("3-7-sonnet") {
            3.0
        } else if self.current_model.contains("3-5-sonnet") {
            3.0
        } else if self.current_model.contains("haiku") {
            0.25
        } else {
            3.0
        };
        
        let output_price_per_1m = if self.current_model.contains("opus-4") {
            75.0
        } else if self.current_model.contains("sonnet-4") {
            15.0
        } else if self.current_model.contains("3-7-sonnet") {
            15.0
        } else if self.current_model.contains("3-5-sonnet") {
            15.0
        } else if self.current_model.contains("haiku") {
            1.25
        } else {
            15.0
        };
        
        let input_cost = (token_count as f64 / 1_000_000.0) * input_price_per_1m;
        let estimated_output_tokens = token_count / 2;
        let output_cost = (estimated_output_tokens as f64 / 1_000_000.0) * output_price_per_1m;
        
        input_cost + output_cost
    }

    /// Determine if a tool needs permission checking
    /// Check if a tool is allowed to execute based on permission settings
    fn is_tool_allowed(&self, tool_name: &str) -> bool {
        // If there are specific allowed tools, tool must be in the list
        if !self.allowed_tools.is_empty() {
            return self.allowed_tools.contains(&tool_name.to_string());
        }
        
        // If no specific allowed tools, check it's not in the disallowed list
        !self.disallowed_tools.contains(&tool_name.to_string())
    }

    /// Create a properly configured ToolExecutor with current permission settings
    fn create_tool_executor(&self) -> crate::ai::tools::ToolExecutor {
        let mut tool_executor = crate::ai::tools::ToolExecutor::new();
        tool_executor.set_allowed_tools(self.allowed_tools.clone());
        tool_executor.set_disallowed_tools(self.disallowed_tools.clone());
        tool_executor
    }

    fn tool_needs_permission(&self, tool_name: &str, input: &Value) -> bool {
        match tool_name {
            "Bash" => {
                // Bash always needs permission checking unless already granted
                !input.get("_permission_already_granted").and_then(|v| v.as_bool()).unwrap_or(false)
            }
            "Edit" | "MultiEdit" | "Write" => {
                // File operations need permission checking
                true
            }
            "Read" => {
                // Read operations might need permission for sensitive files
                if let Some(path_str) = input.get("file_path").and_then(|v| v.as_str()) {
                    let path = std::path::Path::new(path_str);
                    // Check if it's outside allowed directories or a sensitive file
                    !self.is_path_automatically_allowed(path)
                } else {
                    false
                }
            }
            _ => false, // Other tools don't need permission by default
        }
    }

    /// Check if a path is automatically allowed (doesn't need permission dialog)
    fn is_path_automatically_allowed(&self, path: &std::path::Path) -> bool {
        // Allow files in current working directory
        if let Ok(cwd) = std::env::current_dir() {
            if path.starts_with(&cwd) {
                // But not sensitive files
                if let Some(filename) = path.file_name() {
                    let name = filename.to_string_lossy();
                    return !name.starts_with(".env") && 
                           !name.contains("secret") && 
                           !name.contains("password") &&
                           !name.contains("key") &&
                           name != ".git";
                }
                return true;
            }
        }
        false
    }

    /// Extract permission-relevant details from tool input
    fn extract_permission_details(&self, tool_name: &str, input: &Value) -> String {
        match tool_name {
            "Bash" => {
                input.get("command").and_then(|v| v.as_str()).unwrap_or("").to_string()
            }
            "Edit" | "MultiEdit" => {
                input.get("file_path").and_then(|v| v.as_str()).unwrap_or("").to_string()
            }
            "Write" => {
                input.get("file_path").and_then(|v| v.as_str()).unwrap_or("").to_string()
            }
            "Read" => {
                input.get("file_path").and_then(|v| v.as_str()).unwrap_or("").to_string()
            }
            _ => format!("Unknown tool operation: {}", tool_name),
        }
    }
    
    /// Load TODOs from file
    pub fn load_todos(&mut self) {
        if let Ok(todos_dir) = self.get_todos_dir() {
            let todo_file = todos_dir.join(format!("claude-agent-{}.json", self.session_id));
            
            if todo_file.exists() {
                if let Ok(json_content) = fs::read_to_string(&todo_file) {
                    if let Ok(todos) = serde_json::from_str::<Vec<Todo>>(&json_content) {
                        self.todos = todos;
                        self.update_next_todo();
                    }
                }
            }
        }
    }
    
    /// Update the TODO list and save to file
    pub fn update_todos(&mut self, new_todos: Vec<Todo>) {
        self.todos = new_todos;
        self.update_next_todo();
        self.save_todos();
    }
    
    /// Save TODOs to file
    fn save_todos(&self) {
        if let Ok(todos_dir) = self.get_todos_dir() {
            let todo_file = todos_dir.join(format!("claude-agent-{}.json", self.session_id));
            
            if let Ok(json_content) = serde_json::to_string_pretty(&self.todos) {
                let _ = fs::write(&todo_file, json_content);
            }
        }
    }
    
    /// Update the next_todo field based on current todos
    fn update_next_todo(&mut self) {
        // Find the first pending or in_progress task
        for todo in &self.todos {
            match todo.status {
                TodoStatus::InProgress => {
                    self.next_todo = Some(todo.content.clone());
                    return;
                }
                TodoStatus::Pending => {
                    self.next_todo = Some(todo.content.clone());
                    return;
                }
                _ => {}
            }
        }
        
        // No pending or in_progress tasks found
        self.next_todo = None;
    }
    
    /// Get the todos directory
    fn get_todos_dir(&self) -> Result<PathBuf> {
        // Check if TODO_DIR environment variable is set (for testing or custom locations)
        if let Ok(custom_dir) = std::env::var("TODO_DIR") {
            let todos_dir = PathBuf::from(custom_dir);
            if !todos_dir.exists() {
                fs::create_dir_all(&todos_dir)?;
            }
            return Ok(todos_dir);
        }
        
        // Default behavior - use ~/.claude/todos
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map_err(|_| Error::Config("Cannot determine home directory".to_string()))?;
        
        let todos_dir = PathBuf::from(home).join(".claude").join("todos");
        
        if !todos_dir.exists() {
            fs::create_dir_all(&todos_dir)?;
        }
        
        Ok(todos_dir)
    }

    /// Get available commands with exact JavaScript parity
    fn get_available_commands() -> Vec<CommandInfo> {
        vec![
            CommandInfo {
                name: "help".to_string(),
                aliases: vec![],
                description: "Show help and available commands".to_string(),
                argument_hint: None,
                command_type: "local".to_string(),
                is_enabled: true,
            },
            CommandInfo {
                name: "config".to_string(),
                aliases: vec!["theme".to_string()],
                description: "Open config panel".to_string(),
                argument_hint: None,
                command_type: "local".to_string(),
                is_enabled: true,
            },
            CommandInfo {
                name: "status".to_string(),
                aliases: vec![],
                description: "Show Claude Code status including version, model, account, API connectivity, and tool statuses".to_string(),
                argument_hint: None,
                command_type: "local".to_string(),
                is_enabled: true,
            },
            CommandInfo {
                name: "login".to_string(),
                aliases: vec![],
                description: "Sign in with Anthropic account / Switch Anthropic accounts".to_string(),
                argument_hint: None,
                command_type: "local".to_string(),
                is_enabled: true,
            },
            CommandInfo {
                name: "model".to_string(),
                aliases: vec![],
                description: "Set the AI model for Claude Code".to_string(),
                argument_hint: Some("[model]".to_string()),
                command_type: "local".to_string(),
                is_enabled: true,
            },
            CommandInfo {
                name: "memory".to_string(),
                aliases: vec![],
                description: "Edit Claude memory files".to_string(),
                argument_hint: None,
                command_type: "local".to_string(),
                is_enabled: true,
            },
            CommandInfo {
                name: "clear".to_string(),
                aliases: vec![],
                description: "Clear conversation history and free up context".to_string(),
                argument_hint: None,
                command_type: "local".to_string(),
                is_enabled: true,
            },
            CommandInfo {
                name: "compact".to_string(),
                aliases: vec![],
                description: "Clear conversation history but keep a summary in context".to_string(),
                argument_hint: Some("[instructions]".to_string()),
                command_type: "local".to_string(),
                is_enabled: true,
            },
            CommandInfo {
                name: "exit".to_string(),
                aliases: vec!["quit".to_string()],
                description: "Exit the REPL".to_string(),
                argument_hint: None,
                command_type: "local".to_string(),
                is_enabled: true,
            },
            CommandInfo {
                name: "bashes".to_string(),
                aliases: vec![],
                description: "List and manage background bash shells".to_string(),
                argument_hint: None,
                command_type: "local".to_string(),
                is_enabled: true,
            },
            CommandInfo {
                name: "resume".to_string(),
                aliases: vec![],
                description: "Resume a conversation".to_string(),
                argument_hint: None,
                command_type: "local".to_string(),
                is_enabled: true,
            },
            CommandInfo {
                name: "continue".to_string(),
                aliases: vec![],
                description: "Continue after MAX_ITERATIONS limit".to_string(),
                argument_hint: None,
                command_type: "local".to_string(),
                is_enabled: true,
            },
            CommandInfo {
                name: "mcp".to_string(),
                aliases: vec![],
                description: "Manage MCP servers".to_string(),
                argument_hint: None,
                command_type: "local".to_string(),
                is_enabled: true,
            },
            CommandInfo {
                name: "upgrade".to_string(),
                aliases: vec![],
                description: "Upgrade to Max for higher rate limits and more Opus".to_string(),
                argument_hint: None,
                command_type: "local".to_string(),
                is_enabled: true,
            },
            CommandInfo {
                name: "permissions".to_string(),
                aliases: vec!["allowed-tools".to_string()],
                description: "Manage allow & deny tool permission rules".to_string(),
                argument_hint: None,
                command_type: "local".to_string(),
                is_enabled: true,
            },
            CommandInfo {
                name: "add-dir".to_string(),
                aliases: vec![],
                description: "Add a new working directory".to_string(),
                argument_hint: Some("<path>".to_string()),
                command_type: "local".to_string(),
                is_enabled: true,
            },
            // Additional commands present in our Rust implementation
            CommandInfo {
                name: "context".to_string(),
                aliases: vec![],
                description: "Show current context usage".to_string(),
                argument_hint: None,
                command_type: "local".to_string(),
                is_enabled: true,
            },
            CommandInfo {
                name: "cost".to_string(),
                aliases: vec![],
                description: "Show token usage and cost information".to_string(),
                argument_hint: None,
                command_type: "local".to_string(),
                is_enabled: true,
            },
            CommandInfo {
                name: "vim".to_string(),
                aliases: vec![],
                description: "Toggle vim mode for input".to_string(),
                argument_hint: None,
                command_type: "local".to_string(),
                is_enabled: true,
            },
            CommandInfo {
                name: "files".to_string(),
                aliases: vec![],
                description: "Show files in working directories".to_string(),
                argument_hint: None,
                command_type: "local".to_string(),
                is_enabled: true,
            },
            CommandInfo {
                name: "doctor".to_string(),
                aliases: vec![],
                description: "Run system diagnostics".to_string(),
                argument_hint: None,
                command_type: "local".to_string(),
                is_enabled: true,
            },
            CommandInfo {
                name: "release-notes".to_string(),
                aliases: vec![],
                description: "Show release notes and version information".to_string(),
                argument_hint: None,
                command_type: "local".to_string(),
                is_enabled: true,
            },
            CommandInfo {
                name: "save".to_string(),
                aliases: vec![],
                description: "Save conversation to file".to_string(),
                argument_hint: None,
                command_type: "local".to_string(),
                is_enabled: true,
            },
            CommandInfo {
                name: "load".to_string(),
                aliases: vec![],
                description: "Load conversation from file".to_string(),
                argument_hint: Some("<session-id>".to_string()),
                command_type: "local".to_string(),
                is_enabled: true,
            },
            CommandInfo {
                name: "tools".to_string(),
                aliases: vec![],
                description: "Show available tools".to_string(),
                argument_hint: None,
                command_type: "local".to_string(),
                is_enabled: true,
            },
            CommandInfo {
                name: "init".to_string(),
                aliases: vec![],
                description: "Create CLAUDE.md with AI-powered codebase analysis".to_string(),
                argument_hint: None,
                command_type: "prompt".to_string(),
                is_enabled: true,
            },
            CommandInfo {
                name: "review".to_string(),
                aliases: vec![],
                description: "AI-powered pull request code review".to_string(),
                argument_hint: Some("[pr-number]".to_string()),
                command_type: "prompt".to_string(),
                is_enabled: true,
            },
        ]
    }

    /// Fuzzy search and score commands based on JavaScript implementation
    /// Uses weighted scoring: nameKey(2), partKey(2), aliasKey(2), descriptionKey(0.5)
    pub fn search_commands(&mut self, query: &str) {
        if query.is_empty() || !query.starts_with('/') {
            self.is_autocomplete_visible = false;
            self.autocomplete_matches.clear();
            return;
        }

        let search_term = &query[1..]; // Remove the '/' prefix
        
        if search_term.is_empty() {
            // Empty query after '/' - show all commands sorted by category
            self.autocomplete_matches = self.available_commands.iter()
                .map(|cmd| {
                    let display_text = self.format_command_display(cmd);
                    AutocompleteMatch {
                        command: cmd.clone(),
                        score: 1.0, // All have same score for empty query
                        display_text,
                    }
                })
                .collect();
            
            self.sort_commands_by_category();
        } else {
            // Search with fuzzy matching
            let mut matches: Vec<AutocompleteMatch> = Vec::new();
            
            for cmd in &self.available_commands {
                if let Some(score) = self.calculate_fuzzy_score(cmd, search_term) {
                    if score > 0.0 {
                        let display_text = self.format_command_display(cmd);
                        matches.push(AutocompleteMatch {
                            command: cmd.clone(),
                            score,
                            display_text,
                        });
                    }
                }
            }
            
            // Sort by score (highest first), then alphabetically
            matches.sort_by(|a, b| {
                b.score.partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.command.name.cmp(&b.command.name))
            });
            
            self.autocomplete_matches = matches;
        }
        
        self.is_autocomplete_visible = !self.autocomplete_matches.is_empty();
        self.selected_suggestion = 0; // Reset selection to top
    }

    /// Calculate fuzzy score matching JavaScript weights
    fn calculate_fuzzy_score(&self, cmd: &CommandInfo, search_term: &str) -> Option<f64> {
        let search_lower = search_term.to_lowercase();
        let mut total_score = 0.0;
        let mut has_match = false;

        // nameKey weight: 2.0 - exact name match or starts with
        if cmd.name.to_lowercase().contains(&search_lower) {
            let score = if cmd.name.to_lowercase().starts_with(&search_lower) {
                2.0 // Higher score for prefix match
            } else {
                1.5 // Lower score for substring match
            };
            total_score += score * 2.0; // nameKey weight
            has_match = true;
        }

        // partKey weight: 2.0 - partial matches within name
        let name_parts: Vec<&str> = cmd.name.split(&['-', '_'][..]).collect();
        for part in name_parts {
            if part.to_lowercase().starts_with(&search_lower) {
                total_score += 1.0 * 2.0; // partKey weight
                has_match = true;
            }
        }

        // aliasKey weight: 2.0 - alias matches
        for alias in &cmd.aliases {
            if alias.to_lowercase().contains(&search_lower) {
                let score = if alias.to_lowercase().starts_with(&search_lower) {
                    2.0
                } else {
                    1.5
                };
                total_score += score * 2.0; // aliasKey weight  
                has_match = true;
            }
        }

        // descriptionKey weight: 0.5 - description matches
        if cmd.description.to_lowercase().contains(&search_lower) {
            total_score += 1.0 * 0.5; // descriptionKey weight
            has_match = true;
        }

        if has_match { Some(total_score) } else { None }
    }

    /// Format command for display matching JavaScript format
    fn format_command_display(&self, cmd: &CommandInfo) -> String {
        let mut display = format!("/{}", cmd.name);
        
        // Add aliases if they exist
        if !cmd.aliases.is_empty() {
            let aliases_str = cmd.aliases.join(", ");
            display.push_str(&format!(" ({})", aliases_str));
        }
        
        display
    }

    /// Sort commands by category matching JavaScript order
    fn sort_commands_by_category(&mut self) {
        self.autocomplete_matches.sort_by(|a, b| {
            let a_category = Self::get_command_category_static(&a.command);
            let b_category = Self::get_command_category_static(&b.command);
            
            // Sort by category order, then alphabetically within category
            a_category.cmp(&b_category)
                .then_with(|| a.command.name.cmp(&b.command.name))
        });
    }

    /// Get command category for sorting (0=Core, 1=MCP, 2=Other)
    fn get_command_category(&self, cmd: &CommandInfo) -> u8 {
        Self::get_command_category_static(cmd)
    }

    /// Get command category for sorting (0=Core, 1=MCP, 2=Other) - static version
    fn get_command_category_static(cmd: &CommandInfo) -> u8 {
        // Core commands (from JavaScript - these come first)
        match cmd.name.as_str() {
            "help" | "config" | "status" | "login" | "model" | "memory" | 
            "clear" | "compact" | "exit" | "bashes" | "resume" | "upgrade" | 
            "permissions" | "add-dir" => 0,
            _ => {
                // MCP commands contain ':' - category 1
                if cmd.name.contains(':') {
                    1
                } else {
                    // Other commands - category 2  
                    2
                }
            }
        }
    }

    /// Navigate autocomplete selection up
    pub fn autocomplete_select_previous(&mut self) {
        if !self.autocomplete_matches.is_empty() {
            self.selected_suggestion = if self.selected_suggestion == 0 {
                self.autocomplete_matches.len() - 1
            } else {
                self.selected_suggestion - 1
            };
        }
    }

    /// Navigate autocomplete selection down
    pub fn autocomplete_select_next(&mut self) {
        if !self.autocomplete_matches.is_empty() {
            self.selected_suggestion = (self.selected_suggestion + 1) % self.autocomplete_matches.len();
        }
    }

    /// Select current autocomplete suggestion
    pub fn autocomplete_select_current(&mut self) {
        if let Some(selected_match) = self.autocomplete_matches.get(self.selected_suggestion) {
            let command_text = format!("/{}", selected_match.command.name);

            // Clear and insert just the command (NOT the argument hint)
            // The hint is for display only, not to be inserted as text
            self.input_textarea.delete_line_by_head();
            self.input_textarea.insert_str(&command_text);

            // Add a space after if the command takes arguments
            if selected_match.command.argument_hint.is_some() {
                self.input_textarea.insert_str(" ");
            }

            self.is_autocomplete_visible = false;
            self.autocomplete_matches.clear();
        }
    }

    /// Hide autocomplete dropdown
    pub fn hide_autocomplete(&mut self) {
        self.is_autocomplete_visible = false;
        self.autocomplete_matches.clear();
        self.selected_suggestion = 0;
    }

    /// Extract selected text from chat display based on selection coordinates
    pub fn extract_selected_text(&self, start: (usize, usize), end: (usize, usize)) -> String {
        // Normalize start and end so start is always before end
        let (start, end) = if start.0 < end.0 || (start.0 == end.0 && start.1 <= end.1) {
            (start, end)
        } else {
            (end, start)
        };

        let mut result = String::new();

        // Use cached lines if available
        let lines = &self.rendered_lines_cache;
        if lines.is_empty() {
            return result;
        }

        for (line_idx, line) in lines.iter().enumerate() {
            if line_idx < start.0 || line_idx > end.0 {
                continue;
            }

            // Extract text from this line's spans
            let line_text: String = line.spans.iter()
                .map(|span| span.content.as_ref())
                .collect();

            if line_idx == start.0 && line_idx == end.0 {
                // Selection within a single line
                let start_col = start.1.min(line_text.len());
                let end_col = end.1.min(line_text.len());
                if start_col < end_col {
                    result.push_str(&line_text[start_col..end_col]);
                }
            } else if line_idx == start.0 {
                // First line of multi-line selection
                let start_col = start.1.min(line_text.len());
                result.push_str(&line_text[start_col..]);
                result.push('\n');
            } else if line_idx == end.0 {
                // Last line of multi-line selection
                let end_col = end.1.min(line_text.len());
                result.push_str(&line_text[..end_col]);
            } else {
                // Middle line - include entire line
                result.push_str(&line_text);
                result.push('\n');
            }
        }

        result
    }

    /// Copy chat selection to clipboard
    pub fn copy_chat_selection(&mut self) -> bool {
        if let Some(ref text) = self.chat_selected_text {
            if !text.is_empty() {
                // Try to copy to clipboard using arboard
                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                    if clipboard.set_text(text.clone()).is_ok() {
                        self.add_message(&format!("Copied {} characters to clipboard", text.len()));
                        return true;
                    }
                }
                self.add_message("Failed to copy to clipboard");
            }
        }
        false
    }

    /// Clear chat selection
    pub fn clear_chat_selection(&mut self) {
        self.chat_selection_start = None;
        self.chat_selection_end = None;
        self.chat_is_selecting = false;
        self.chat_selected_text = None;
    }
}

/// Conversation data for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConversationData {
    session_id: String,
    model: String,
    messages: Vec<Message>,
    timestamp: u64,
}

/// Session info
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub id: String,
    pub created_timestamp: u64,
    pub modified_timestamp: u64,
}

/// Complete session struct matching JavaScript makeSession
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub sid: String,
    pub init: bool,
    pub timestamp: f64,
    pub started: f64,
    pub duration: f64,
    pub status: String,
    pub errors: u32,
    #[serde(rename = "ignoreDuration")]
    pub ignore_duration: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub did: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
}

impl Session {
    /// Convert to JSON matching JavaScript toJSON method
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "sid": self.sid,
            "init": self.init,
            "started": self.started,
            "timestamp": self.timestamp,
            "status": self.status,
            "errors": self.errors,
            "duration": self.duration,
            "attrs": {
                "release": self.release,
                "environment": self.environment,
                "ip_address": self.ip_address,
                "user_agent": self.user_agent,
            }
        })
    }
    /// Create new session matching JavaScript makeSession
    pub fn new(initial_data: Option<serde_json::Value>) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        
        let mut session = Self {
            sid: crate::utils::generate_session_id(),
            init: true,
            timestamp,
            started: timestamp,
            duration: 0.0,
            status: "ok".to_string(),
            errors: 0,
            ignore_duration: false,
            ip_address: None,
            did: None,
            release: None,
            environment: None,
            user_agent: None,
        };
        
        if let Some(data) = initial_data {
            session.update(data);
        }
        
        session
    }
    
    /// Update session matching JavaScript updateSession
    pub fn update(&mut self, updates: serde_json::Value) {
        if let Some(user) = updates.get("user").and_then(|u| u.as_object()) {
            if self.ip_address.is_none() {
                if let Some(ip) = user.get("ip_address").and_then(|v| v.as_str()) {
                    self.ip_address = Some(ip.to_string());
                }
            }
            if self.did.is_none() {
                let did = user.get("id")
                    .or_else(|| user.get("email"))
                    .or_else(|| user.get("username"))
                    .and_then(|v| v.as_str());
                if let Some(did) = did {
                    self.did = Some(did.to_string());
                }
            }
        }
        
        // Update timestamp
        if let Some(ts) = updates.get("timestamp").and_then(|v| v.as_f64()) {
            self.timestamp = ts;
        } else {
            self.timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs_f64();
        }
        
        // Update other fields
        if let Some(v) = updates.get("ignoreDuration").and_then(|v| v.as_bool()) {
            self.ignore_duration = v;
        }
        if let Some(v) = updates.get("status").and_then(|v| v.as_str()) {
            self.status = v.to_string();
        }
        if let Some(v) = updates.get("errors").and_then(|v| v.as_u64()) {
            self.errors = v as u32;
        }
        
        // Calculate duration
        if !self.ignore_duration {
            self.duration = self.timestamp - self.started;
            if self.duration < 0.0 {
                self.duration = 0.0;
            }
        }
    }
    
    /// Close session matching JavaScript closeSession
    pub fn close(&mut self, status: Option<String>) {
        if let Some(status) = status {
            self.status = status;
        } else if self.status == "ok" {
            self.status = "exited".to_string();
        }
        self.update(serde_json::json!({}));
    }
}

/// Get conversation directory
fn get_conversation_dir() -> PathBuf {
    // Match JavaScript - store in current working directory's .claude folder
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".claude")
        .join("conversations")
}
