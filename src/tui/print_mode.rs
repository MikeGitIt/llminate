use crate::ai::client::ContentDelta;
use crate::ai::streaming::{StreamDelta, StreamEvent as AIStreamEvent};
use crate::error::{Error, Result};
use crate::mcp;
use crate::progress::create_progress_spinner;
use crate::telemetry;
use crate::tui::state::get_conversation_dir;
use futures::StreamExt;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{self, BufRead};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot, Mutex};

/// Whether the Emacs bridge (keep-alive mode) is active.
/// Used by EmacsCommandTool to fail fast when running in TUI mode
/// instead of hanging for 60s waiting for a response that will never come.
pub static EMACS_BRIDGE_ACTIVE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// System prompt addendum injected in keep-alive mode (Emacs bridge).
/// Tells the AI about EmacsCommand and when to prefer it over shell tools.
const EMACS_BRIDGE_SYSTEM_PROMPT: &str = r#"
# Emacs Integration

You are running inside an Emacs editor session. You have access to the **EmacsCommand** tool which lets you call Emacs functions directly in the user's live editor. Use it for:

- **Opening files**: `find-file` or `find-file-other-window` instead of Read when the user wants to *see* the file in their editor
- **Git operations**: `magit-status`, `magit-stage-file`, `magit-commit`, `magit-diff-buffer-file` instead of shelling out to git
- **LSP/navigation**: `xref-find-definitions`, `xref-find-references`, `eglot-rename`, `eglot-find-implementation`
- **Compilation**: `compile`, `recompile`, `next-error`
- **Buffer/window management**: `switch-to-buffer`, `split-window-right`, `delete-other-windows`, `balance-windows`
- **Diagnostics**: `flymake-diagnostics` to get current errors/warnings

**When to use EmacsCommand vs file tools:**
- Use EmacsCommand when the user wants to *interact* with their editor (open a file in a buffer, trigger magit, run compile)
- Use Read/Write/Edit when you need to read or modify file *contents* programmatically
- Use Bash for commands that don't have Emacs equivalents

**EmacsCommand parameters:**
- `command`: The Emacs function name as a string (e.g. "find-file", "magit-status")
- `args`: Optional array of arguments to pass to the function
- `description`: Optional human-readable description of the action
"#;

/// Pending tool approval requests awaiting host response.
/// Key: request_id, Value: oneshot sender to deliver the approval decision.
static PENDING_TOOL_APPROVALS: Lazy<Arc<Mutex<HashMap<String, oneshot::Sender<bool>>>>> =
    Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

/// Output format options
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OutputFormat {
    Text,
    Json,
    StreamJson,
}

/// Input format options
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputFormat {
    Text,
    StreamJson,
}

/// Permission mode options
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PermissionMode {
    Ask,
    Allow,
    Deny,
}

/// Options for print mode
#[derive(Debug, Clone)]
pub struct PrintOptions {
    pub prompt: Option<String>,
    pub output_format: OutputFormat,
    pub input_format: InputFormat,
    pub debug: bool,
    pub verbose: bool,
    pub max_turns: Option<usize>,
    pub allowed_tools: Vec<String>,
    pub disallowed_tools: Vec<String>,
    pub system_prompt: Option<String>,
    pub append_system_prompt: Option<String>,
    pub permission_mode: Option<PermissionMode>,
    pub model: Option<String>,
    pub fallback_model: Option<String>,
    pub add_dirs: Vec<PathBuf>,
    pub continue_conversation: bool,
    pub resume_session_id: Option<String>,
    pub mcp_config: Option<String>,
    pub permission_prompt_tool: Option<String>,
    pub dangerously_skip_permissions: bool,
    /// Keep the process alive after the first response, reading subsequent
    /// prompts from stdin. Used by the Emacs bridge for long-lived sessions.
    pub keep_alive: bool,
}

/// Message structure for JSON output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonMessage {
    pub role: String,
    pub content: String,
    pub timestamp: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use: Option<ToolUse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Tool use information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUse {
    pub name: String,
    pub input: Value,
    pub output: Option<Value>,
}

/// Stream event for streaming JSON
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum StreamEvent {
    Start { session_id: String, model: String },
    Message { role: String, content: String },
    ToolUse { name: String, input: Value },
    ToolResult { output: Value },
    Error { message: String },
    End { reason: String },
    /// Emitted after End in keep-alive mode to signal "ready for next input"
    Ready {},
    /// Permission prompt sent to the host (e.g. Emacs bridge)
    ToolApproval {
        tool_name: String,
        description: String,
        input: Value,
        request_id: String,
    },
    /// Permission response received from the host via stdin
    ToolApprovalResponse {
        request_id: String,
        approved: bool,
    },
    /// Reverse command: llminate asks the host to execute a function
    EmacsEval {
        command: String,
        args: Value,
        request_id: String,
    },
    /// Result of an EmacsEval, sent back from the host via stdin
    EmacsEvalResult {
        request_id: String,
        success: bool,
        result: Value,
    },
    /// Session resume: sends previous conversation history to the host
    SessionResume {
        session_id: String,
        model: String,
        messages: Vec<SessionResumeMessage>,
    },
}

/// Lightweight message struct for SessionResume events.
/// Avoids sending tool_use/error details that the host doesn't need for display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionResumeMessage {
    pub role: String,
    pub content: String,
    pub timestamp: u64,
}

/// Run print mode
pub async fn run(options: PrintOptions) -> Result<()> {
    // Initialize session
    let session_id = if options.continue_conversation {
        get_last_session_id().await?
    } else if let Some(id) = &options.resume_session_id {
        id.clone()
    } else {
        crate::utils::generate_session_id()
    };

    // Track telemetry
    telemetry::track(
        "print_mode_start",
        Some(serde_json::json!({
            "output_format": format!("{:?}", options.output_format),
            "input_format": format!("{:?}", options.input_format),
            "has_prompt": options.prompt.is_some(),
            "keep_alive": options.keep_alive,
        })),
    )
    .await;

    // If keep-alive mode is enabled with stream-json I/O, use the persistent loop
    if options.keep_alive
        && options.output_format == OutputFormat::StreamJson
        && options.input_format == InputFormat::StreamJson
    {
        let result = run_keep_alive(options, session_id).await;
        telemetry::track("print_mode_end", None::<serde_json::Value>).await;
        return result;
    }

    // Get input
    let input = match options.input_format {
        InputFormat::Text => get_text_input(&options).await?,
        InputFormat::StreamJson => get_stream_json_input().await?,
    };

    if input.trim().is_empty() {
        return Err(Error::InvalidInput("No input provided".to_string()));
    }

    // Initialize conversation context
    let mut context = ConversationContext::new(session_id, options.clone());

    // Load MCP servers if configured
    if let Some(mcp_config) = &options.mcp_config {
        context.load_mcp_servers(mcp_config).await?;
    }

    // Set up system prompt
    let system_prompt = build_system_prompt(&options)?;
    if !system_prompt.is_empty() {
        context.add_system_message(&system_prompt);
    }

    // Process the conversation
    match options.output_format {
        OutputFormat::Text => process_text_output(&mut context, &input).await?,
        OutputFormat::Json => process_json_output(&mut context, &input).await?,
        OutputFormat::StreamJson => process_stream_json_output(&mut context, &input).await?,
    }

    // Track telemetry
    telemetry::track("print_mode_end", None::<serde_json::Value>).await;

    Ok(())
}

/// Conversation context
struct ConversationContext {
    session_id: String,
    options: PrintOptions,
    messages: Vec<JsonMessage>,
    mcp_clients: Vec<mcp::McpClient>,
    turn_count: usize,
}

impl ConversationContext {
    fn new(session_id: String, options: PrintOptions) -> Self {
        Self {
            session_id,
            options,
            messages: Vec::new(),
            mcp_clients: Vec::new(),
            turn_count: 0,
        }
    }

    fn add_system_message(&mut self, content: &str) {
        self.messages.push(JsonMessage {
            role: "system".to_string(),
            content: content.to_string(),
            timestamp: crate::utils::timestamp_ms(),
            tool_use: None,
            error: None,
        });
    }

    fn add_user_message(&mut self, content: &str) {
        self.messages.push(JsonMessage {
            role: "user".to_string(),
            content: content.to_string(),
            timestamp: crate::utils::timestamp_ms(),
            tool_use: None,
            error: None,
        });
    }

    fn get_ai_messages(&self) -> Vec<crate::ai::Message> {
        let mut messages = Vec::new();

        for msg in &self.messages {
            let role = match msg.role.as_str() {
                "user" => crate::ai::MessageRole::User,
                "assistant" => crate::ai::MessageRole::Assistant,
                "system" => crate::ai::MessageRole::System,
                _ => continue,
            };

            messages.push(crate::ai::Message {
                role,
                content: crate::ai::MessageContent::Text(msg.content.clone()),
                name: None,
            });
        }

        messages
    }

    fn add_assistant_message(&mut self, content: &str) {
        self.messages.push(JsonMessage {
            role: "assistant".to_string(),
            content: content.to_string(),
            timestamp: crate::utils::timestamp_ms(),
            tool_use: None,
            error: None,
        });
    }

    fn add_tool_use(&mut self, name: &str, input: Value, output: Option<Value>) {
        self.messages.push(JsonMessage {
            role: "assistant".to_string(),
            content: format!("Using tool: {}", name),
            timestamp: crate::utils::timestamp_ms(),
            tool_use: Some(ToolUse {
                name: name.to_string(),
                input,
                output,
            }),
            error: None,
        });
    }

    fn add_error(&mut self, error: &str) {
        self.messages.push(JsonMessage {
            role: "system".to_string(),
            content: "An error occurred".to_string(),
            timestamp: crate::utils::timestamp_ms(),
            tool_use: None,
            error: Some(error.to_string()),
        });
    }

    /// Save conversation to `.claude/conversations/{session_id}.json`.
    /// Uses the same directory and a compatible format as TUI's `AppState::save_conversation()`.
    fn save_conversation(&self) -> Result<()> {
        let conversation_dir = get_conversation_dir();
        std::fs::create_dir_all(&conversation_dir)?;

        let path = conversation_dir.join(format!("{}.json", self.session_id));

        // Build a structure compatible with TUI's ConversationData.
        // TUI expects { session_id, model, messages: [{role, content, timestamp}], timestamp }.
        // Our JsonMessage has extra fields (tool_use, error) but serde will just include them;
        // the TUI deserializer ignores unknown fields via #[serde(deny_unknown_fields)] NOT being set.
        let data = serde_json::json!({
            "session_id": self.session_id,
            "model": self.options.model.as_deref().unwrap_or("claude-opus-4-1-20250805"),
            "messages": self.messages,
            "timestamp": crate::utils::timestamp_ms(),
        });

        let json = serde_json::to_string_pretty(&data)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load conversation from `.claude/conversations/{session_id}.json`.
    /// Populates `self.messages` with the saved history so the AI has context.
    fn load_conversation(&mut self) -> Result<bool> {
        let conversation_dir = get_conversation_dir();
        let path = conversation_dir.join(format!("{}.json", self.session_id));

        if !path.exists() {
            return Ok(false);
        }

        let json = std::fs::read_to_string(&path)?;
        let data: serde_json::Value = serde_json::from_str(&json)?;

        // Extract messages array
        if let Some(messages) = data["messages"].as_array() {
            for msg in messages {
                let role = msg["role"].as_str().unwrap_or("system").to_string();
                let content = msg["content"].as_str().unwrap_or("").to_string();
                let timestamp = msg["timestamp"].as_u64().unwrap_or(0);

                // Reconstruct tool_use if present
                let tool_use = if let Some(tu) = msg.get("tool_use") {
                    if !tu.is_null() {
                        serde_json::from_value(tu.clone()).ok()
                    } else {
                        None
                    }
                } else {
                    None
                };

                let error = msg["error"].as_str().map(|s| s.to_string());

                self.messages.push(JsonMessage {
                    role,
                    content,
                    timestamp,
                    tool_use,
                    error,
                });
            }
        }

        // Restore model if present
        if let Some(model) = data["model"].as_str() {
            self.options.model = Some(model.to_string());
        }

        Ok(true)
    }

    /// Build SessionResume messages from current conversation history.
    fn get_resume_messages(&self) -> Vec<SessionResumeMessage> {
        self.messages
            .iter()
            .filter(|m| m.role == "user" || m.role == "assistant")
            .map(|m| SessionResumeMessage {
                role: m.role.clone(),
                content: m.content.clone(),
                timestamp: m.timestamp,
            })
            .collect()
    }

    async fn load_mcp_servers(&mut self, config: &str) -> Result<()> {
        let servers = mcp::parse_config(config)?;

        for (name, server_config) in servers {
            match mcp::start_client(name.clone(), server_config).await {
                Ok(client) => {
                    self.mcp_clients.push(client);
                }
                Err(e) => {
                    if self.options.debug {
                        eprintln!("Failed to start MCP server {}: {}", name, e);
                    }
                }
            }
        }

        Ok(())
    }

    fn should_continue(&self) -> bool {
        if let Some(max_turns) = self.options.max_turns {
            self.turn_count < max_turns
        } else {
            true
        }
    }

    fn increment_turn(&mut self) {
        self.turn_count += 1;
    }
}

/// Get text input
async fn get_text_input(options: &PrintOptions) -> Result<String> {
    if let Some(prompt) = &options.prompt {
        Ok(prompt.clone())
    } else {
        // Read from stdin
        let mut input = String::new();
        let stdin = io::stdin();
        let mut handle = stdin.lock();

        loop {
            let mut line = String::new();
            if handle.read_line(&mut line)? == 0 {
                break;
            }
            input.push_str(&line);
        }

        Ok(input)
    }
}

/// Get streaming JSON input
async fn get_stream_json_input() -> Result<String> {
    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin);
    let mut messages = Vec::new();

    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).await? == 0 {
            break;
        }

        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        match serde_json::from_str::<StreamEvent>(line) {
            Ok(event) => match event {
                StreamEvent::Message { content, .. } => {
                    messages.push(content);
                }
                StreamEvent::End { .. } => break,
                _ => {}
            },
            Err(e) => {
                if !line.starts_with('{') {
                    // Treat as plain text if not JSON
                    messages.push(line.to_string());
                } else {
                    return Err(Error::InvalidInput(format!("Invalid JSON: {}", e)));
                }
            }
        }
    }

    Ok(messages.join("\n"))
}

/// Build system prompt
fn build_system_prompt(options: &PrintOptions) -> Result<String> {
    let mut prompt = String::new();

    if let Some(system_prompt) = &options.system_prompt {
        prompt = system_prompt.clone();
    }

    if let Some(append) = &options.append_system_prompt {
        if !prompt.is_empty() {
            prompt.push('\n');
        }
        prompt.push_str(append);
    }

    Ok(prompt)
}

/// Process text output
async fn process_text_output(context: &mut ConversationContext, input: &str) -> Result<()> {
    context.add_user_message(input);

    // Create AI client
    let ai_client = crate::ai::create_client().await?;

    // Build request
    let mut request = ai_client
        .create_chat_request()
        .messages(context.get_ai_messages())
        .max_tokens(4096);

    if let Some(system) = &context.options.system_prompt {
        request = request.system(system.clone());
    }

    // Add tools if not disabled
    if !context.options.dangerously_skip_permissions {
        let tool_executor = crate::ai::tools::ToolExecutor::new();
        let tools = tool_executor.get_available_tools();
        if !tools.is_empty() {
            request = request.tools(tools);
        }
    }

    // Show spinner while waiting for response
    let progress = create_progress_spinner("Thinking...");

    // Send request
    let response = ai_client.chat(request.build()).await?;

    // Finish progress bar
    progress.finish_and_clear();

    // Process response
    let mut response_text = String::new();

    for part in &response.content {
        match part {
            crate::ai::ContentPart::Text { text, .. } => {
                response_text.push_str(text);
            }
            crate::ai::ContentPart::ToolUse { name, input, .. } => {
                response_text.push_str(&format!("\n[Tool: {}]\n", name));

                // Execute tool if allowed
                if !context.options.dangerously_skip_permissions {
                    // Show spinner for tool execution
                    let tool_progress = create_progress_spinner(format!("Executing {}...", name));

                    let tool_executor = crate::ai::tools::ToolExecutor::new();
                    match tool_executor.execute(name, input.clone()).await {
                        Ok(result) => {
                            tool_progress.finish_and_clear();
                            if let crate::ai::ContentPart::ToolResult { content, .. } = result {
                                response_text.push_str(&format!("Result: {}\n", content));
                            }
                        }
                        Err(e) => {
                            tool_progress.abandon_with_message("Failed");
                            response_text.push_str(&format!("Error: {}\n", e));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    context.add_assistant_message(&response_text);
    println!("{}", response_text);

    Ok(())
}

/// Process JSON output
async fn process_json_output(context: &mut ConversationContext, input: &str) -> Result<()> {
    context.add_user_message(input);

    // Create AI client
    let ai_client = crate::ai::create_client().await?;

    // Build request
    let mut request = ai_client
        .create_chat_request()
        .messages(context.get_ai_messages())
        .max_tokens(4096);

    if let Some(system) = &context.options.system_prompt {
        request = request.system(system.clone());
    }

    // Add tools if not disabled
    if !context.options.dangerously_skip_permissions {
        let tool_executor = crate::ai::tools::ToolExecutor::new();
        let tools = tool_executor.get_available_tools();
        if !tools.is_empty() {
            request = request.tools(tools);
        }
    }

    // Show spinner while waiting for response
    let progress = create_progress_spinner("Processing...");

    // Send request
    let response = ai_client.chat(request.build()).await?;

    // Finish progress bar
    progress.finish_and_clear();

    // Convert response to JSON format
    let mut response_messages = Vec::new();
    for part in &response.content {
        match part {
            crate::ai::ContentPart::Text { text, .. } => {
                response_messages.push(JsonMessage {
                    role: "assistant".to_string(),
                    content: text.clone(),
                    timestamp: crate::utils::timestamp_ms(),
                    tool_use: None,
                    error: None,
                });
            }
            crate::ai::ContentPart::ToolUse { id: _, name, input } => {
                let tool_output = if !context.options.dangerously_skip_permissions {
                    // Show spinner for tool execution
                    let tool_progress = create_progress_spinner(format!("Executing {}...", name));

                    let tool_executor = crate::ai::tools::ToolExecutor::new();
                    let result = tool_executor.execute(name, input.clone()).await;
                    tool_progress.finish_and_clear();

                    match result {
                        Ok(result) => {
                            if let crate::ai::ContentPart::ToolResult { content, .. } = result {
                                Some(serde_json::json!({ "result": content }))
                            } else {
                                None
                            }
                        }
                        Err(e) => Some(serde_json::json!({ "error": e.to_string() })),
                    }
                } else {
                    None
                };

                response_messages.push(JsonMessage {
                    role: "assistant".to_string(),
                    content: format!("Using tool: {}", name),
                    timestamp: crate::utils::timestamp_ms(),
                    tool_use: Some(ToolUse {
                        name: name.clone(),
                        input: input.clone(),
                        output: tool_output,
                    }),
                    error: None,
                });
            }
            crate::ai::ContentPart::ServerToolUse { .. } => {
                // Server-side tool use - handled by Claude API
            }
            crate::ai::ContentPart::WebSearchToolResult { .. } => {
                // Web search results - handled by Claude API
            }
            _ => {}
        }
    }

    // Add response messages to context
    for msg in &response_messages {
        context.messages.push(msg.clone());
    }

    let output = serde_json::json!({
        "session_id": context.session_id,
        "messages": context.messages,
        "model": context.options.model.as_ref().unwrap_or(&response.model),
        "usage": {
            "input_tokens": response.usage.input_tokens,
            "output_tokens": response.usage.output_tokens,
        },
        "stop_reason": response.stop_reason,
    });

    println!("{}", serde_json::to_string_pretty(&output)?);

    Ok(())
}

/// Process streaming JSON output
async fn process_stream_json_output(context: &mut ConversationContext, input: &str) -> Result<()> {
    let stdout = tokio::io::stdout();
    let mut writer = tokio::io::BufWriter::new(stdout);

    // Show spinner during initial connection
    let progress = create_progress_spinner("Connecting...");

    // Send start event
    let start_event = StreamEvent::Start {
        session_id: context.session_id.clone(),
        model: context
            .options
            .model
            .clone()
            .unwrap_or_else(|| "claude-opus-4-1-20250805".to_string()),
    };
    writer
        .write_all(serde_json::to_string(&start_event)?.as_bytes())
        .await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;

    // Send user message
    let user_event = StreamEvent::Message {
        role: "user".to_string(),
        content: input.to_string(),
    };
    writer
        .write_all(serde_json::to_string(&user_event)?.as_bytes())
        .await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;

    context.add_user_message(input);

    // Create AI client
    let ai_client = crate::ai::create_client().await?;

    // Build request
    let mut request = ai_client
        .create_chat_request()
        .messages(context.get_ai_messages())
        .max_tokens(4096)
        .stream();

    if let Some(system) = &context.options.system_prompt {
        request = request.system(system.clone());
    }

    // Add tools if not disabled
    if !context.options.dangerously_skip_permissions {
        let tool_executor = crate::ai::tools::ToolExecutor::new();
        let tools = tool_executor.get_available_tools();
        if !tools.is_empty() {
            request = request.tools(tools);
        }
    }

    // Send request and stream response
    let stream = ai_client.chat_stream(request.build()).await?;

    // Finish the connection spinner - streaming has started
    progress.finish_and_clear();

    let mut stream = stream;
    let mut accumulated_text = String::new();

    while let Some(event) = stream.next().await {
        match event {
            Ok(chunk) => {
                match chunk {
                    AIStreamEvent::ContentStart { .. } => {}
                    AIStreamEvent::ContentDelta { delta } => {
                        if let StreamDelta::TextDelta { text } = delta {
                            accumulated_text.push_str(&text);

                            // Send text delta
                            let message_event = StreamEvent::Message {
                                role: "assistant".to_string(),
                                content: text,
                            };
                            writer
                                .write_all(serde_json::to_string(&message_event)?.as_bytes())
                                .await?;
                            writer.write_all(b"\n").await?;
                            writer.flush().await?;
                        }
                    }
                    AIStreamEvent::ContentStop => {}
                    AIStreamEvent::ContentBlockStart { .. } => {}
                    AIStreamEvent::ContentBlockDelta { delta, .. } => {
                        match delta {
                            ContentDelta::TextDelta { text } => {
                                accumulated_text.push_str(&text);

                                // Send text delta
                                let message_event = StreamEvent::Message {
                                    role: "assistant".to_string(),
                                    content: text,
                                };
                                writer
                                    .write_all(serde_json::to_string(&message_event)?.as_bytes())
                                    .await?;
                                writer.write_all(b"\n").await?;
                                writer.flush().await?;
                            }
                            ContentDelta::InputJsonDelta { .. } => {}
                            ContentDelta::ThinkingDelta { .. } => {
                                // Thinking deltas not displayed in print mode
                            }
                            ContentDelta::SignatureDelta { .. } => {
                                // Signature deltas are internal
                            }
                        }
                    }
                    AIStreamEvent::ContentBlockStop { .. } => {}
                    AIStreamEvent::MessageStart { .. } => {}
                    AIStreamEvent::MessageDelta { .. } => {}
                    AIStreamEvent::MessageStop => {}
                    AIStreamEvent::ToolUseStart { id: _, name } => {
                        let tool_event = StreamEvent::ToolUse {
                            name: name.clone(),
                            input: serde_json::Value::Null,
                        };
                        writer
                            .write_all(serde_json::to_string(&tool_event)?.as_bytes())
                            .await?;
                        writer.write_all(b"\n").await?;
                        writer.flush().await?;
                    }
                    AIStreamEvent::ToolUseDelta { .. } => {}
                    AIStreamEvent::ToolUseStop { id: _, name, input } => {
                        if !context.options.dangerously_skip_permissions {
                            // Show spinner for tool execution
                            let tool_progress =
                                create_progress_spinner(format!("Executing {}...", name));

                            let tool_executor = crate::ai::tools::ToolExecutor::new();
                            let result = tool_executor.execute(&name, input.clone()).await;
                            tool_progress.finish_and_clear();

                            match result {
                                Ok(result) => {
                                    if let crate::ai::ContentPart::ToolResult { content, .. } =
                                        result
                                    {
                                        let result_event = StreamEvent::ToolResult {
                                            output: serde_json::json!({ "result": content }),
                                        };
                                        writer
                                            .write_all(
                                                serde_json::to_string(&result_event)?.as_bytes(),
                                            )
                                            .await?;
                                        writer.write_all(b"\n").await?;
                                        writer.flush().await?;
                                    }
                                }
                                Err(e) => {
                                    let error_event = StreamEvent::Error {
                                        message: e.to_string(),
                                    };
                                    writer
                                        .write_all(serde_json::to_string(&error_event)?.as_bytes())
                                        .await?;
                                    writer.write_all(b"\n").await?;
                                    writer.flush().await?;
                                }
                            }
                        }
                    }
                    AIStreamEvent::Ping => {}
                    AIStreamEvent::Error(error) => {
                        let error_event = StreamEvent::Error { message: error };
                        writer
                            .write_all(serde_json::to_string(&error_event)?.as_bytes())
                            .await?;
                        writer.write_all(b"\n").await?;
                        writer.flush().await?;
                    }
                }
            }
            Err(e) => {
                let error_event = StreamEvent::Error {
                    message: e.to_string(),
                };
                writer
                    .write_all(serde_json::to_string(&error_event)?.as_bytes())
                    .await?;
                writer.write_all(b"\n").await?;
                writer.flush().await?;
            }
        }
    }

    context.add_assistant_message(&accumulated_text);

    // Send end event
    let end_event = StreamEvent::End {
        reason: "completed".to_string(),
    };
    writer
        .write_all(serde_json::to_string(&end_event)?.as_bytes())
        .await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Keep-alive mode: long-lived subprocess for editor integrations (e.g. Emacs)
// ---------------------------------------------------------------------------

/// Deliver a ToolApprovalResponse to the pending approval request.
/// Called by the stdin reader when it receives a ToolApprovalResponse event.
/// Returns true if the result was delivered, false if no pending request was found.
async fn deliver_tool_approval_response(request_id: &str, approved: bool) -> bool {
    let mut pending = PENDING_TOOL_APPROVALS.lock().await;
    if let Some(tx) = pending.remove(request_id) {
        tx.send(approved).is_ok()
    } else {
        false
    }
}

/// Events received from stdin in keep-alive mode.
enum StdinEvent {
    /// A new user prompt to process.
    UserMessage(String),
    /// End of input (EOF or explicit End event from the host).
    Eof,
}

/// Spawn a background task that reads stdin lines, parses them as `StreamEvent`,
/// and dispatches them:
/// - `EmacsEvalResult` → delivered to pending Emacs requests (via `deliver_emacs_eval_result`)
/// - User `Message` → forwarded to the returned channel for the main loop
/// - `End` / EOF → sends `Eof` to the channel
fn spawn_stdin_reader() -> mpsc::UnboundedReceiver<StdinEvent> {
    let (tx, rx) = mpsc::unbounded_channel();

    tokio::spawn(async move {
        let stdin = tokio::io::stdin();
        let mut reader = BufReader::new(stdin);
        let mut line = String::new();

        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => {
                    // EOF
                    let _ = tx.send(StdinEvent::Eof);
                    break;
                }
                Ok(_) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }

                    match serde_json::from_str::<StreamEvent>(trimmed) {
                        Ok(StreamEvent::Message { content, role }) if role == "user" => {
                            let _ = tx.send(StdinEvent::UserMessage(content));
                        }
                        Ok(StreamEvent::EmacsEvalResult {
                            request_id,
                            success,
                            result,
                        }) => {
                            // Route to the pending emacs request — resolves the
                            // oneshot channel that EmacsCommandTool::execute() awaits.
                            crate::ai::emacs_tool::deliver_emacs_eval_result(
                                &request_id,
                                success,
                                result,
                            )
                            .await;
                        }
                        Ok(StreamEvent::ToolApprovalResponse {
                            request_id,
                            approved,
                        }) => {
                            // Route to the pending approval request — resolves the
                            // oneshot channel that request_tool_approval() awaits.
                            deliver_tool_approval_response(&request_id, approved).await;
                        }
                        Ok(StreamEvent::End { .. }) => {
                            let _ = tx.send(StdinEvent::Eof);
                            break;
                        }
                        Ok(_) => {
                            // Ignore other event types from stdin
                        }
                        Err(_) => {
                            // If it doesn't parse as JSON at all, treat as plain text prompt
                            if !trimmed.starts_with('{') {
                                let _ = tx.send(StdinEvent::UserMessage(trimmed.to_string()));
                            }
                            // Malformed JSON is silently ignored
                        }
                    }
                }
                Err(_) => {
                    let _ = tx.send(StdinEvent::Eof);
                    break;
                }
            }
        }
    });

    rx
}

/// Run the keep-alive loop: read prompts from stdin, stream responses to stdout,
/// emit Ready between turns, and handle EmacsEvalResult events concurrently.
async fn run_keep_alive(options: PrintOptions, session_id: String) -> Result<()> {
    // Signal that the Emacs bridge is active — EmacsCommandTool checks this
    // to fail fast in TUI mode instead of hanging for 60s.
    EMACS_BRIDGE_ACTIVE.store(true, std::sync::atomic::Ordering::Relaxed);

    let stdout = tokio::io::stdout();
    let mut writer = tokio::io::BufWriter::new(stdout);

    // Create persistent conversation context
    let mut context = ConversationContext::new(session_id.clone(), options.clone());

    // If resuming a previous session, load the conversation history
    let is_resume = options.resume_session_id.is_some();
    if is_resume {
        match context.load_conversation() {
            Ok(true) => {
                // Conversation loaded successfully — messages are populated
            }
            Ok(false) => {
                // No saved conversation file found — start fresh
            }
            Err(e) => {
                // Log but don't fail — start fresh if load fails
                eprintln!("[llminate] Warning: failed to load conversation: {}", e);
            }
        }
    }

    // Load MCP servers
    if let Some(mcp_config) = &options.mcp_config {
        context.load_mcp_servers(mcp_config).await?;
    }

    // Build system prompt
    let system_prompt = build_system_prompt(&options)?;
    if !system_prompt.is_empty() && !is_resume {
        // Only add system prompt for fresh sessions — resumed sessions already have it
        context.add_system_message(&system_prompt);
    }

    // In keep-alive mode (Emacs bridge), inject EmacsCommand guidance so
    // the AI knows to prefer EmacsCommand for editor operations.
    if !is_resume {
        context.add_system_message(EMACS_BRIDGE_SYSTEM_PROMPT);
    }

    // Emit initial Start event
    let model = options
        .model
        .clone()
        .unwrap_or_else(|| "claude-opus-4-1-20250805".to_string());

    let start_event = StreamEvent::Start {
        session_id: session_id.clone(),
        model: model.clone(),
    };
    if !emit_event(&mut writer, &start_event).await? {
        return Ok(()); // Host disconnected before we started
    }

    // If resuming, emit SessionResume with conversation history
    // so the host (Emacs) can restore the chat display
    if is_resume && !context.messages.is_empty() {
        let resume_event = StreamEvent::SessionResume {
            session_id: session_id.clone(),
            model: model.clone(),
            messages: context.get_resume_messages(),
        };
        if !emit_event(&mut writer, &resume_event).await? {
            return Ok(());
        }
    }

    // Spawn background stdin reader
    let mut stdin_rx = spawn_stdin_reader();

    // If a prompt was provided via CLI, use it as first input; otherwise wait for stdin
    let first_input = if let Some(prompt) = &options.prompt {
        Some(prompt.clone())
    } else {
        // Wait for first user message
        loop {
            match stdin_rx.recv().await {
                Some(StdinEvent::UserMessage(content)) => break Some(content),
                Some(StdinEvent::Eof) | None => break None,
            }
        }
    };

    let mut current_input = match first_input {
        Some(input) if !input.trim().is_empty() => input,
        Some(_) | None => {
            // No input — emit End and exit cleanly (ignore pipe close)
            let _ = emit_event(
                &mut writer,
                &StreamEvent::End {
                    reason: "no_input".to_string(),
                },
            )
            .await;
            return Ok(());
        }
    };

    // Main keep-alive loop: process turns until EOF or host disconnect
    loop {
        // Process one AI turn — returns false if host disconnected mid-turn
        if !process_keep_alive_turn(&mut context, &current_input, &mut writer).await? {
            // Save before exiting even on disconnect
            let _ = context.save_conversation();
            return Ok(()); // Host disconnected — clean exit
        }

        // Auto-save conversation after each turn
        if let Err(e) = context.save_conversation() {
            eprintln!("[llminate] Warning: failed to save conversation: {}", e);
        }

        // Emit End + Ready to signal the host that we're ready for the next prompt
        if !emit_event(
            &mut writer,
            &StreamEvent::End {
                reason: "completed".to_string(),
            },
        )
        .await?
        {
            return Ok(()); // Host disconnected
        }
        if !emit_event(&mut writer, &StreamEvent::Ready {}).await? {
            return Ok(()); // Host disconnected
        }

        // Wait for the next user message
        loop {
            match stdin_rx.recv().await {
                Some(StdinEvent::UserMessage(content)) => {
                    current_input = content;
                    break;
                }
                Some(StdinEvent::Eof) | None => {
                    return Ok(()); // Clean shutdown — stdin closed
                }
            }
        }
    }
}

/// Helper: serialize a StreamEvent as JSON-line to the writer and flush.
/// Returns Ok(true) on success, Ok(false) on BrokenPipe (host disconnected).
async fn emit_event<W: tokio::io::AsyncWrite + Unpin>(
    writer: &mut tokio::io::BufWriter<W>,
    event: &StreamEvent,
) -> Result<bool> {
    let json = serde_json::to_string(event)?;
    match writer.write_all(json.as_bytes()).await {
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => return Ok(false),
        Err(e) => return Err(e.into()),
        Ok(()) => {}
    }
    match writer.write_all(b"\n").await {
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => return Ok(false),
        Err(e) => return Err(e.into()),
        Ok(()) => {}
    }
    match writer.flush().await {
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => return Ok(false),
        Err(e) => return Err(e.into()),
        Ok(()) => {}
    }
    Ok(true)
}

/// Check whether a tool execution needs host approval.
/// Read-only tools never need approval. If dangerously_skip_permissions is set,
/// nothing needs approval. Otherwise, Bash checks the permission context, and
/// write/execute tools always require approval.
async fn should_request_approval(
    options: &PrintOptions,
    tool_name: &str,
    input: &Value,
) -> bool {
    if options.dangerously_skip_permissions {
        return false;
    }

    match tool_name {
        // Read-only tools — never need approval
        "Read" | "Glob" | "Grep" | "LS" | "Search" | "WebFetch" | "WebSearch"
        | "TaskList" | "TaskGet" | "NotebookRead" => false,

        // Bash — check permission context
        "Bash" | "BashCommand" => {
            let command = input["command"].as_str().unwrap_or("");
            matches!(
                crate::permissions::check_command_permission(command).await,
                crate::permissions::PermissionResult::NeedsApproval
            )
        }

        // Write/Edit/Execute tools — always need approval
        "Write" | "Edit" | "MultiEdit" | "NotebookEdit"
        | "Task" | "Skill" | "EnterPlanMode" => true,

        // Unknown tools — ask to be safe
        _ => true,
    }
}

/// Build a human-readable description for a tool approval event.
fn build_approval_description(tool_name: &str, input: &Value) -> String {
    match tool_name {
        "Bash" | "BashCommand" => {
            let cmd = input["command"].as_str().unwrap_or("(unknown)");
            let desc = input["description"].as_str().unwrap_or("");
            if desc.is_empty() {
                format!("Execute shell command: {}", cmd)
            } else {
                desc.to_string()
            }
        }
        "Edit" | "FileEdit" => {
            let path = input["file_path"].as_str().unwrap_or("unknown");
            format!("Edit file: {}", path)
        }
        "Write" | "FileWrite" => {
            let path = input["file_path"].as_str().unwrap_or("unknown");
            format!("Write file: {}", path)
        }
        "MultiEdit" => {
            let path = input["file_path"].as_str().unwrap_or("unknown");
            format!("Multi-edit file: {}", path)
        }
        "NotebookEdit" => {
            let path = input["notebook_path"].as_str().unwrap_or("unknown");
            format!("Edit notebook: {}", path)
        }
        _ => format!("Execute tool: {}", tool_name),
    }
}

/// Emit a ToolApproval event to stdout and wait for the host's response.
/// Returns Ok(true) if approved, Ok(false) if denied or timed out.
/// Returns Err if a non-pipe IO error occurs.
async fn request_tool_approval<W: tokio::io::AsyncWrite + Unpin>(
    writer: &mut tokio::io::BufWriter<W>,
    tool_name: &str,
    input: &Value,
) -> Result<bool> {
    let request_id = uuid::Uuid::new_v4().to_string();
    let description = build_approval_description(tool_name, input);

    // Create oneshot channel
    let (tx, rx) = oneshot::channel::<bool>();

    // Register pending request
    {
        let mut pending = PENDING_TOOL_APPROVALS.lock().await;
        pending.insert(request_id.clone(), tx);
    }

    // Emit ToolApproval event to stdout
    if !emit_event(
        writer,
        &StreamEvent::ToolApproval {
            tool_name: tool_name.to_string(),
            description,
            input: input.clone(),
            request_id: request_id.clone(),
        },
    )
    .await?
    {
        // Host disconnected — clean up and signal disconnection
        let mut pending = PENDING_TOOL_APPROVALS.lock().await;
        pending.remove(&request_id);
        return Err(Error::Network("Host disconnected during approval".to_string()));
    }

    // Wait for response with 120s timeout
    let timeout = tokio::time::Duration::from_secs(120);
    match tokio::time::timeout(timeout, rx).await {
        Ok(Ok(approved)) => Ok(approved),
        Ok(Err(_)) => {
            // Channel closed — host disconnected
            Ok(false)
        }
        Err(_) => {
            // Timeout — clean up and treat as denial
            let mut pending = PENDING_TOOL_APPROVALS.lock().await;
            pending.remove(&request_id);
            Ok(false)
        }
    }
}

/// Execute a tool and emit the result as a StreamEvent.
/// Returns Ok(true) on success, Ok(false) if the host disconnected (BrokenPipe).
async fn execute_and_emit_tool<W: tokio::io::AsyncWrite + Unpin>(
    writer: &mut tokio::io::BufWriter<W>,
    name: &str,
    input: Value,
) -> Result<bool> {
    let tool_executor = crate::ai::tools::ToolExecutor::new();
    let result = tool_executor.execute(name, input).await;

    match result {
        Ok(tool_result) => {
            if let crate::ai::ContentPart::ToolResult { content, .. } = tool_result {
                if !emit_event(
                    writer,
                    &StreamEvent::ToolResult {
                        output: serde_json::json!({ "result": content }),
                    },
                )
                .await?
                {
                    return Ok(false);
                }
            }
        }
        Err(e) => {
            if !emit_event(
                writer,
                &StreamEvent::Error {
                    message: e.to_string(),
                },
            )
            .await?
            {
                return Ok(false);
            }
        }
    }

    Ok(true)
}

/// Process a single AI turn in keep-alive mode.
/// Sends the user message, streams the AI response (including tool execution),
/// and writes all events to the writer. Does NOT emit End/Ready (the caller does that).
/// Returns Ok(true) on success, Ok(false) if the host disconnected (BrokenPipe).
async fn process_keep_alive_turn<W: tokio::io::AsyncWrite + Unpin>(
    context: &mut ConversationContext,
    input: &str,
    writer: &mut tokio::io::BufWriter<W>,
) -> Result<bool> {
    // Emit user message event
    if !emit_event(
        writer,
        &StreamEvent::Message {
            role: "user".to_string(),
            content: input.to_string(),
        },
    )
    .await?
    {
        return Ok(false);
    }

    context.add_user_message(input);

    // Create AI client
    let ai_client = crate::ai::create_client().await?;

    // Build request
    let mut request = ai_client
        .create_chat_request()
        .messages(context.get_ai_messages())
        .max_tokens(4096)
        .stream();

    if let Some(system) = &context.options.system_prompt {
        request = request.system(system.clone());
    }

    // Add tools
    if !context.options.dangerously_skip_permissions {
        let tool_executor = crate::ai::tools::ToolExecutor::new();
        let tools = tool_executor.get_available_tools();
        if !tools.is_empty() {
            request = request.tools(tools);
        }
    }

    // Stream the AI response
    let stream = ai_client.chat_stream(request.build()).await?;
    let mut stream = stream;
    let mut accumulated_text = String::new();

    while let Some(event) = stream.next().await {
        match event {
            Ok(chunk) => {
                match chunk {
                    AIStreamEvent::ContentBlockDelta { delta, .. } => match delta {
                        ContentDelta::TextDelta { text } => {
                            accumulated_text.push_str(&text);
                            if !emit_event(
                                writer,
                                &StreamEvent::Message {
                                    role: "assistant".to_string(),
                                    content: text,
                                },
                            )
                            .await?
                            {
                                return Ok(false);
                            }
                        }
                        _ => {}
                    },
                    AIStreamEvent::ContentDelta { delta } => {
                        if let StreamDelta::TextDelta { text } = delta {
                            accumulated_text.push_str(&text);
                            if !emit_event(
                                writer,
                                &StreamEvent::Message {
                                    role: "assistant".to_string(),
                                    content: text,
                                },
                            )
                            .await?
                            {
                                return Ok(false);
                            }
                        }
                    }
                    AIStreamEvent::ToolUseStart { name, .. } => {
                        if !emit_event(
                            writer,
                            &StreamEvent::ToolUse {
                                name,
                                input: Value::Null,
                            },
                        )
                        .await?
                        {
                            return Ok(false);
                        }
                    }
                    AIStreamEvent::ToolUseStop { name, input, .. } => {
                        // Check if this tool needs approval from the host
                        if should_request_approval(&context.options, &name, &input).await {
                            // Request approval from host (e.g. Emacs)
                            match request_tool_approval(writer, &name, &input).await {
                                Ok(true) => {
                                    // Approved — execute the tool
                                    if !execute_and_emit_tool(writer, &name, input.clone())
                                        .await?
                                    {
                                        return Ok(false);
                                    }
                                }
                                Ok(false) => {
                                    // Denied — emit a ToolResult with denial message
                                    if !emit_event(
                                        writer,
                                        &StreamEvent::ToolResult {
                                            output: serde_json::json!({
                                                "result": format!(
                                                    "Tool '{}' was denied by the user.",
                                                    name
                                                )
                                            }),
                                        },
                                    )
                                    .await?
                                    {
                                        return Ok(false);
                                    }
                                }
                                Err(e) => {
                                    // Approval error (e.g. host disconnected)
                                    if !emit_event(
                                        writer,
                                        &StreamEvent::Error {
                                            message: format!("Approval error: {}", e),
                                        },
                                    )
                                    .await?
                                    {
                                        return Ok(false);
                                    }
                                }
                            }
                        } else {
                            // No approval needed — execute directly
                            if !execute_and_emit_tool(writer, &name, input.clone()).await? {
                                return Ok(false);
                            }
                        }
                    }
                    AIStreamEvent::Error(error) => {
                        if !emit_event(writer, &StreamEvent::Error { message: error }).await? {
                            return Ok(false);
                        }
                    }
                    // Ignore other stream events (Ping, MessageStart, etc.)
                    _ => {}
                }
            }
            Err(e) => {
                if !emit_event(
                    writer,
                    &StreamEvent::Error {
                        message: e.to_string(),
                    },
                )
                .await?
                {
                    return Ok(false);
                }
            }
        }
    }

    context.add_assistant_message(&accumulated_text);

    Ok(true)
}

/// Get last session ID
async fn get_last_session_id() -> Result<String> {
    // Load last session ID from config
    let config = crate::config::load_config(crate::config::ConfigScope::User)?;

    if let Some(session_id) = config.last_session_id {
        if !session_id.is_empty() {
            return Ok(session_id);
        }
    }

    // Generate new ID if none exists
    Ok(crate::utils::generate_session_id())
}
