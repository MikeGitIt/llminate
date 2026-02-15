use crate::error::{Error, Result};
use crate::mcp;
use crate::telemetry;
use crate::ai::streaming::{StreamEvent as AIStreamEvent, StreamDelta};
use crate::ai::client::ContentDelta;
use anyhow::Context;
use colored::Colorize;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use futures::StreamExt;

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
    Start {
        session_id: String,
        model: String,
    },
    Message {
        role: String,
        content: String,
    },
    ToolUse {
        name: String,
        input: Value,
    },
    ToolResult {
        output: Value,
    },
    Error {
        message: String,
    },
    End {
        reason: String,
    },
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
    telemetry::track("print_mode_start", Some(serde_json::json!({
        "output_format": format!("{:?}", options.output_format),
        "input_format": format!("{:?}", options.input_format),
        "has_prompt": options.prompt.is_some(),
    }))).await;
    
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
    
    // Send request
    let response = ai_client.chat(request.build()).await?;
    
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
                    let tool_executor = crate::ai::tools::ToolExecutor::new();
                    match tool_executor.execute(name, input.clone()).await {
                        Ok(result) => {
                            if let crate::ai::ContentPart::ToolResult { content, .. } = result {
                                response_text.push_str(&format!("Result: {}\n", content));
                            }
                        }
                        Err(e) => {
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
    
    // Send request
    let response = ai_client.chat(request.build()).await?;
    
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
            crate::ai::ContentPart::ToolUse { id, name, input } => {
                let tool_output = if !context.options.dangerously_skip_permissions {
                    let tool_executor = crate::ai::tools::ToolExecutor::new();
                    match tool_executor.execute(name, input.clone()).await {
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
    
    // Send start event
    let start_event = StreamEvent::Start {
        session_id: context.session_id.clone(),
        model: context.options.model.clone().unwrap_or_else(|| "claude-opus-4-1-20250805".to_string()),
    };
    writer.write_all(serde_json::to_string(&start_event)?.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    
    // Send user message
    let user_event = StreamEvent::Message {
        role: "user".to_string(),
        content: input.to_string(),
    };
    writer.write_all(serde_json::to_string(&user_event)?.as_bytes()).await?;
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
    let mut stream = Box::pin(stream);
    let mut accumulated_text = String::new();
    
    while let Some(event) = stream.next().await {
        match event {
            Ok(chunk) => {
                match chunk {
                    AIStreamEvent::ContentStart { .. } => {},
                    AIStreamEvent::ContentDelta { delta } => {
                        if let StreamDelta::TextDelta { text } = delta {
                            accumulated_text.push_str(&text);
                            
                            // Send text delta
                            let message_event = StreamEvent::Message {
                                role: "assistant".to_string(),
                                content: text,
                            };
                            writer.write_all(serde_json::to_string(&message_event)?.as_bytes()).await?;
                            writer.write_all(b"\n").await?;
                            writer.flush().await?;
                        }
                    }
                    AIStreamEvent::ContentStop => {},
                    AIStreamEvent::ContentBlockStart { .. } => {},
                    AIStreamEvent::ContentBlockDelta { delta, .. } => {
                        match delta {
                            ContentDelta::TextDelta { text } => {
                                accumulated_text.push_str(&text);
                                
                                // Send text delta
                                let message_event = StreamEvent::Message {
                                    role: "assistant".to_string(),
                                    content: text,
                                };
                                writer.write_all(serde_json::to_string(&message_event)?.as_bytes()).await?;
                                writer.write_all(b"\n").await?;
                                writer.flush().await?;
                            }
                            ContentDelta::InputJsonDelta { .. } => {}
                        }
                    }
                    AIStreamEvent::ContentBlockStop { .. } => {},
                    AIStreamEvent::MessageStart { .. } => {},
                    AIStreamEvent::MessageDelta { .. } => {},
                    AIStreamEvent::MessageStop => {},
                    AIStreamEvent::ToolUseStart { id, name } => {
                        let tool_event = StreamEvent::ToolUse {
                            name: name.clone(),
                            input: serde_json::Value::Null,
                        };
                        writer.write_all(serde_json::to_string(&tool_event)?.as_bytes()).await?;
                        writer.write_all(b"\n").await?;
                        writer.flush().await?;
                    }
                    AIStreamEvent::ToolUseDelta { .. } => {},
                    AIStreamEvent::ToolUseStop { id, name, input } => {
                        if !context.options.dangerously_skip_permissions {
                            let tool_executor = crate::ai::tools::ToolExecutor::new();
                            match tool_executor.execute(&name, input.clone()).await {
                                Ok(result) => {
                                    if let crate::ai::ContentPart::ToolResult { content, .. } = result {
                                        let result_event = StreamEvent::ToolResult {
                                            output: serde_json::json!({ "result": content }),
                                        };
                                        writer.write_all(serde_json::to_string(&result_event)?.as_bytes()).await?;
                                        writer.write_all(b"\n").await?;
                                        writer.flush().await?;
                                    }
                                }
                                Err(e) => {
                                    let error_event = StreamEvent::Error {
                                        message: e.to_string(),
                                    };
                                    writer.write_all(serde_json::to_string(&error_event)?.as_bytes()).await?;
                                    writer.write_all(b"\n").await?;
                                    writer.flush().await?;
                                }
                            }
                        }
                    }
                    AIStreamEvent::Ping => {},
                    AIStreamEvent::Error(error) => {
                        let error_event = StreamEvent::Error {
                            message: error,
                        };
                        writer.write_all(serde_json::to_string(&error_event)?.as_bytes()).await?;
                        writer.write_all(b"\n").await?;
                        writer.flush().await?;
                    }
                }
            }
            Err(e) => {
                let error_event = StreamEvent::Error {
                    message: e.to_string(),
                };
                writer.write_all(serde_json::to_string(&error_event)?.as_bytes()).await?;
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
    writer.write_all(serde_json::to_string(&end_event)?.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    
    Ok(())
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