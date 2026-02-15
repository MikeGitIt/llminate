# Complete Permission & Agentic Flow Architecture Analysis & Implementation Plan

## Executive Summary

This document provides a comprehensive analysis of the JavaScript Claude Code implementation's permission and agentic flow architecture, identifies critical gaps in the current Rust implementation, and provides a detailed plan to achieve full parity.

## JavaScript Permission Flow Architecture

### 1. Core Permission Context Structure

The permission system is built around a **toolPermissionContext** object:

```javascript
// Created by lz() function
{
  mode: "default",                              // Permission mode
  additionalWorkingDirectories: new Set(),      // Allowed directories
  alwaysAllowRules: {},                         // Auto-approve rules
  alwaysDenyRules: {},                          // Auto-deny rules
  isBypassPermissionsModeAvailable: false,      // Bypass availability
}
```

### 2. Tool Execution Flow States

The system uses three main execution states:
- **PreToolUse** - Before tool execution
- **PostToolUse** - After tool execution  
- **Notification** - When notifications are sent

### 3. Promise-Based Permission Flow

#### Core Architecture
The system uses **Promise-based suspension and resumption**:

```javascript
// Permission dialog promise setup
let permissionPromise = new Promise((resolve) => {
  resolver = resolve;  // Resolver function stored for later
});

// Tool execution waits on this promise
let result = await Promise.race([
  shellExecution,
  permissionPromise.then(decision => handleDecision(decision))
]);
```

#### Key Insight: Suspension, Not Re-execution
When permission is needed:
1. Tool execution creates a Promise and suspends
2. UI dialog is shown
3. User decision resolves the Promise
4. Tool execution continues in the same context
5. **No re-invocation or re-execution occurs**

### 4. Bash Tool Implementation

The bash tool execution follows this flow:

1. **Input Processing**: Extract command, timeout, shellExecutable
2. **Permission Check**: Consult toolPermissionContext
3. **If Permission Needed**:
   - Create Promise for dialog result
   - Show permission dialog via setToolJSX
   - Use Promise.race() between execution and permission
4. **User Decision Options**:
   - **Allow**: Continue execution
   - **Background**: Move to background shell
   - **Kill**: Terminate execution
   - **Deny**: Return error

### 5. Background Shell Management

JavaScript has sophisticated background shell support:
- `addBackgroundShell()`: Creates new background shell session
- `getShellOutput()`: Retrieves output from background shell
- `onKillShell()`: Terminates background shell
- Each shell has unique ID for tracking

### 6. State Management

#### UI-Tool Communication
- **setToolJSX function**: Updates UI with permission dialog
- **dialogResultPromise**: Carries user decision back to tool
- **onOptionSelected callback**: Resolves permission promise

#### No Global Permission State
- Each execution is independent
- No "temporarily_allowed_command" that affects future executions
- Permission decisions are per-execution via Promise resolution

## Current Rust Implementation Problems

### 1. Incorrect Flow Architecture
**Problem**: Tool execution fails with PermissionRequired error, then tries to re-execute after permission
**JavaScript**: Tool suspends on Promise, resumes with decision

### 2. Deadlock Issues
**Problem**: Spawning background task that tries to acquire PERMISSION_CONTEXT lock
```rust
// WRONG - causes deadlock
tokio::spawn(async move {
    let mut permission_ctx = PERMISSION_CONTEXT.lock().await; // Deadlock!
});
```

### 3. Global State Contamination
**Problem**: Using `temporarily_allowed_command` and `_permission_already_granted` affects ALL future commands
```rust
// WRONG - affects all future executions
permission_ctx.temporarily_allowed_command = Some(command);
```

### 4. Missing Background Shell Support
**Problem**: No ability to move long-running commands to background

## Correct Implementation Plan

### Phase 1: Implement Async Permission Flow

#### 1.1 Add Permission Event with Oneshot Channel
```rust
// In tui/mod.rs
pub enum TuiEvent {
    // ... existing events
    PermissionRequired {
        tool_name: String,
        command: String,
        tool_use_id: String,
        input: serde_json::Value,
        responder: oneshot::Sender<PermissionDecision>,
    },
}

pub enum PermissionDecision {
    Allow,
    Deny,
    Background,
}
```

#### 1.2 Modify Bash Tool to Suspend on Permission
```rust
// In ai/tools.rs - Bash tool execute method
impl BashTool {
    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ContentPart> {
        let command = input["command"].as_str().unwrap_or("");
        
        // Check permissions
        if !is_sandboxed {
            let permission_result = check_permission(command).await;
            
            match permission_result {
                PermissionResult::NeedsApproval => {
                    // Create oneshot channel for response
                    let (tx, rx) = oneshot::channel();
                    
                    // Send permission request to UI
                    if let Some(event_tx) = &context.event_tx {
                        event_tx.send(TuiEvent::PermissionRequired {
                            tool_name: "Bash".to_string(),
                            command: command.to_string(),
                            tool_use_id: context.tool_use_id.clone(),
                            input: input.clone(),
                            responder: tx,
                        }).await?;
                        
                        // SUSPEND HERE - wait for UI decision
                        match rx.await? {
                            PermissionDecision::Allow => {
                                // Continue with execution below
                            }
                            PermissionDecision::Deny => {
                                return Ok(ContentPart::ToolResult {
                                    tool_use_id: context.tool_use_id.clone(),
                                    content: "Permission denied".to_string(),
                                    is_error: Some(true),
                                });
                            }
                            PermissionDecision::Background => {
                                // Move to background shell
                                let shell_id = create_background_shell(command).await?;
                                return Ok(ContentPart::ToolResult {
                                    tool_use_id: context.tool_use_id.clone(),
                                    content: format!("Command moved to background (shell ID: {})", shell_id),
                                    is_error: Some(false),
                                });
                            }
                        }
                    }
                }
                PermissionResult::Allow => {
                    // Continue with execution
                }
                PermissionResult::Deny => {
                    return Err(Error::PermissionDenied);
                }
            }
        }
        
        // Execute the command
        let output = execute_bash_command(command).await?;
        
        Ok(ContentPart::ToolResult {
            tool_use_id: context.tool_use_id.clone(),
            content: output,
            is_error: Some(false),
        })
    }
}
```

#### 1.3 Handle Permission in UI Layer
```rust
// In tui/interactive_mode.rs
match event {
    TuiEvent::PermissionRequired { tool_name, command, responder, .. } => {
        // Store responder in app state
        app_state.pending_permission_responder = Some(responder);
        
        // Show permission dialog
        app_state.permission_dialog.show(PermissionRequest {
            tool_name,
            command,
            // ...
        });
        
        needs_redraw = true;
    }
}

// When user makes decision (in handle_key_event)
if app_state.permission_dialog.visible {
    if let Some(decision) = app_state.permission_dialog.handle_key(key) {
        // Send decision through the stored responder
        if let Some(responder) = app_state.pending_permission_responder.take() {
            let permission_decision = match decision {
                PermissionBehavior::Allow => PermissionDecision::Allow,
                PermissionBehavior::Deny => PermissionDecision::Deny,
                // Add background option
            };
            let _ = responder.send(permission_decision);
        }
        
        app_state.permission_dialog.hide();
    }
}
```

### Phase 2: Remove Global State Issues

#### 2.1 Remove `temporarily_allowed_command`
- Delete this field from PermissionContext
- Permission decisions go through oneshot channel only

#### 2.2 Remove `_permission_already_granted` flag
- Delete this hack from the code
- Each execution handles its own permission

#### 2.3 Update PermissionContext
```rust
// Simplified PermissionContext - no temporary state
pub struct PermissionContext {
    pub mode: PermissionMode,
    pub allowed_directories: HashSet<PathBuf>,
    pub always_allow_rules: HashMap<String, Vec<String>>,
    pub always_deny_rules: HashMap<String, Vec<String>>,
    // Remove: temporarily_allowed_command
}
```

### Phase 3: Add Background Shell Support (Optional Enhancement)

```rust
pub struct BackgroundShellManager {
    shells: Arc<Mutex<HashMap<String, BackgroundShell>>>,
}

pub struct BackgroundShell {
    id: String,
    command: String,
    process: Child,
    stdout: Arc<Mutex<VecDeque<String>>>,
    stderr: Arc<Mutex<VecDeque<String>>>,
    created_at: Instant,
}

impl BackgroundShellManager {
    pub async fn create_shell(&self, command: String) -> Result<String> {
        let id = generate_shell_id();
        let process = Command::new("bash")
            .arg("-c")
            .arg(&command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        
        let shell = BackgroundShell {
            id: id.clone(),
            command,
            process,
            stdout: Arc::new(Mutex::new(VecDeque::new())),
            stderr: Arc::new(Mutex::new(VecDeque::new())),
            created_at: Instant::now(),
        };
        
        self.shells.lock().await.insert(id.clone(), shell);
        Ok(id)
    }
    
    pub async fn get_output(&self, shell_id: &str) -> Result<String> {
        // Return buffered output
    }
    
    pub async fn kill_shell(&self, shell_id: &str) -> Result<()> {
        // Terminate the shell
    }
}
```

## Key Architecture Principles

1. **Suspension, Not Re-execution**: Tools suspend waiting for permission, they don't fail and retry
2. **Channel-based Communication**: Use oneshot channels, not global state
3. **Per-execution Decisions**: Each tool execution is independent
4. **No Global Permission State**: No state that affects future executions
5. **Background Support**: Allow long-running commands to move to background

## Testing Strategy

1. **Test Permission Flow**:
   - Tool requests permission
   - Dialog shown
   - User approves
   - Tool continues and completes
   - Next command also requests permission (no bypass)

2. **Test Denial Flow**:
   - Tool requests permission
   - User denies
   - Tool returns error
   - AI can handle the error

3. **Test Background Flow** (if implemented):
   - Long-running command
   - User chooses background
   - Command moves to background shell
   - Can query output later

## Migration Steps

1. **Step 1**: Add oneshot channel infrastructure
2. **Step 2**: Modify Bash tool to use suspension model
3. **Step 3**: Update UI to handle new event type
4. **Step 4**: Remove all global permission state
5. **Step 5**: Test thoroughly
6. **Step 6**: Add background shell support (optional)

## Success Criteria

- [ ] No UI hangs when permission dialog shown
- [ ] Commands execute after permission granted
- [ ] Each command requests permission independently
- [ ] No deadlocks or race conditions
- [ ] Output displayed correctly
- [ ] Permission decisions don't affect future commands

## Agentic Flow Synthesis

### JavaScript Agentic Flow Architecture

The JavaScript implementation uses a **streaming-first architecture** where the AI conversation never stops:

1. **Continuous Streaming**: AI responses stream continuously, tools are discovered during streaming
2. **In-stream Tool Execution**: Tools execute as they're discovered in the stream
3. **Seamless Continuation**: Tool results are injected back into the stream, AI continues naturally
4. **Non-blocking Permissions**: Permission dialogs don't stop the stream, they suspend specific tools

### Current Rust Agentic Flow

The Rust implementation uses a **discrete request-response architecture**:

1. **Synchronous Loop**: Uses `ai_client.chat()` in a loop with MAX_LOOPS protection
2. **Batch Tool Processing**: All tools from a response are executed sequentially
3. **Manual Continuation**: Explicitly checks `stop_reason` to decide whether to continue
4. **Blocking Permissions**: Permission dialogs block the entire flow

### Key Architectural Differences

| Aspect | JavaScript | Rust (Current) | Gap |
|--------|------------|----------------|-----|
| **Conversation Flow** | Continuous streaming | Discrete request/response | Need streaming integration |
| **Tool Discovery** | During streaming | After complete response | Need stream-based discovery |
| **Tool Execution** | Parallel, non-blocking | Sequential, blocking | Need async parallel execution |
| **Permission Model** | Promise suspension | Error and retry | Need suspension model |
| **Tool Results** | Injected into stream | Added to next request | Need stream injection |
| **UI Updates** | Real-time streaming | Batch after completion | Need streaming UI |
| **Background Jobs** | Sophisticated shell system | Not implemented | Need background shells |

## Complete Parity Implementation Plan

### Phase 1: Streaming-First Architecture (Week 1)

#### 1.1 Replace Synchronous Chat with Streaming
```rust
// Replace in state.rs
pub async fn process_user_message_streaming(&mut self, content: String) -> Result<()> {
    // Create streaming request
    let stream = self.ai_client.stream_chat(request).await?;
    
    // Process stream events
    let mut stream_handler = StreamingHandler::new();
    while let Some(event) = stream.next().await {
        match event {
            StreamEvent::ContentBlockStart { index, content_block } => {
                if let ContentBlock::ToolUse { id, name, .. } = content_block {
                    // Start tool execution tracking
                    self.pending_tools.insert(id.clone(), PendingTool {
                        id,
                        name,
                        input: Value::Object(Map::new()),
                    });
                }
            }
            StreamEvent::ContentBlockDelta { index, delta } => {
                match delta {
                    BlockDelta::TextDelta { text } => {
                        // Stream text to UI in real-time
                        self.append_streaming_text(text);
                    }
                    BlockDelta::InputJsonDelta { partial_json } => {
                        // Accumulate tool input
                        if let Some(tool) = self.pending_tools.get_mut(&index) {
                            tool.accumulate_input(partial_json);
                        }
                    }
                }
            }
            StreamEvent::ContentBlockStop { index } => {
                // Execute tool when complete
                if let Some(tool) = self.pending_tools.remove(&index) {
                    self.execute_tool_async(tool).await;
                }
            }
        }
    }
}
```

#### 1.2 Implement Streaming UI Updates
```rust
// Add to state.rs
pub fn append_streaming_text(&mut self, text: &str) {
    if let Some(last_message) = self.messages.last_mut() {
        if last_message.role == "assistant" {
            last_message.content.push_str(text);
        }
    } else {
        self.messages.push(UiMessage {
            role: "assistant".to_string(),
            content: text.to_string(),
            timestamp: timestamp_ms(),
        });
    }
    self.invalidate_cache();
    // Trigger UI redraw
}
```

### Phase 2: Non-blocking Permission System (Week 1-2)

#### 2.1 Implement Permission Suspension
```rust
// Add to tools.rs
pub struct ToolContext {
    pub tool_use_id: String,
    pub event_tx: Option<UnboundedSender<TuiEvent>>,
    pub permission_tx: Option<oneshot::Sender<PermissionDecision>>,
}

impl ToolExecutor {
    pub async fn execute_with_permission(
        &self,
        name: &str,
        input: Value,
        context: ToolContext,
    ) -> Result<ContentPart> {
        // Check if permission needed
        if self.needs_permission(name, &input) {
            let (tx, rx) = oneshot::channel();
            
            // Send permission request event
            if let Some(event_tx) = &context.event_tx {
                event_tx.send(TuiEvent::PermissionRequired {
                    tool_name: name.to_string(),
                    input: input.clone(),
                    tool_use_id: context.tool_use_id.clone(),
                    responder: tx,
                })?;
                
                // Suspend here waiting for permission
                match rx.await? {
                    PermissionDecision::Allow => {
                        // Continue to execution
                    }
                    PermissionDecision::Deny => {
                        return Ok(ContentPart::ToolResult {
                            tool_use_id: context.tool_use_id,
                            content: "Permission denied by user".to_string(),
                            is_error: Some(true),
                        });
                    }
                    PermissionDecision::Background => {
                        return self.execute_in_background(name, input, context).await;
                    }
                }
            }
        }
        
        // Execute the tool
        self.execute_tool(name, input, context).await
    }
}
```

#### 2.2 Update UI Permission Handler
```rust
// Update interactive_mode.rs
TuiEvent::PermissionRequired { tool_name, input, tool_use_id, responder } => {
    // Store responder
    app_state.pending_permission = Some(PendingPermission {
        tool_name,
        input,
        tool_use_id,
        responder,
    });
    
    // Show dialog
    app_state.permission_dialog.show(PermissionRequest {
        tool_name: tool_name.clone(),
        details: format_tool_input(&input),
    });
    
    needs_redraw = true;
}

// In key handler
if app_state.permission_dialog.visible {
    if let Some(decision) = app_state.permission_dialog.handle_key(key) {
        if let Some(pending) = app_state.pending_permission.take() {
            let _ = pending.responder.send(match decision {
                PermissionBehavior::Allow => PermissionDecision::Allow,
                PermissionBehavior::Deny => PermissionDecision::Deny,
                // Add background option
                _ => PermissionDecision::Deny,
            });
        }
        app_state.permission_dialog.hide();
        needs_redraw = true;
    }
}
```

### Phase 3: Tool Result Streaming (Week 2)

#### 3.1 Implement Tool Result Injection
```rust
// Add to state.rs
pub async fn inject_tool_result(&mut self, result: ContentPart) -> Result<()> {
    // Add tool result to conversation
    self.messages.push(UiMessage {
        role: "tool".to_string(),
        content: format_tool_result(&result),
        timestamp: timestamp_ms(),
    });
    
    // Continue AI stream with tool result
    let continuation_request = self.build_continuation_request(result)?;
    let stream = self.ai_client.continue_stream(continuation_request).await?;
    
    // Process continuation stream
    self.process_continuation_stream(stream).await
}
```

### Phase 4: Background Shell System (Week 2-3)

#### 4.1 Implement Background Shell Manager
```rust
// Add background_shells.rs
use tokio::process::{Child, Command};
use tokio::io::{AsyncBufReadExt, BufReader};

pub struct BackgroundShellManager {
    shells: Arc<Mutex<HashMap<String, BackgroundShell>>>,
    event_tx: UnboundedSender<ShellEvent>,
}

pub struct BackgroundShell {
    id: String,
    command: String,
    process: Child,
    stdout_reader: BufReader<ChildStdout>,
    stderr_reader: BufReader<ChildStderr>,
    output_buffer: Arc<Mutex<VecDeque<String>>>,
    created_at: Instant,
}

impl BackgroundShellManager {
    pub async fn create_shell(&self, command: String) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        
        let mut process = Command::new("bash")
            .arg("-c")
            .arg(&command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
            
        let stdout = process.stdout.take().unwrap();
        let stderr = process.stderr.take().unwrap();
        
        let shell = BackgroundShell {
            id: id.clone(),
            command: command.clone(),
            process,
            stdout_reader: BufReader::new(stdout),
            stderr_reader: BufReader::new(stderr),
            output_buffer: Arc::new(Mutex::new(VecDeque::new())),
            created_at: Instant::now(),
        };
        
        // Start output monitoring
        self.monitor_shell_output(shell.clone());
        
        self.shells.lock().await.insert(id.clone(), shell);
        Ok(id)
    }
    
    fn monitor_shell_output(&self, mut shell: BackgroundShell) {
        let output_buffer = shell.output_buffer.clone();
        let event_tx = self.event_tx.clone();
        let shell_id = shell.id.clone();
        
        // Monitor stdout
        tokio::spawn(async move {
            let mut line = String::new();
            while shell.stdout_reader.read_line(&mut line).await.is_ok() {
                if line.is_empty() {
                    break;
                }
                output_buffer.lock().await.push_back(line.clone());
                let _ = event_tx.send(ShellEvent::Output {
                    shell_id: shell_id.clone(),
                    line: line.clone(),
                    is_stderr: false,
                });
                line.clear();
            }
        });
    }
    
    pub async fn get_output(&self, shell_id: &str) -> Result<Vec<String>> {
        if let Some(shell) = self.shells.lock().await.get(shell_id) {
            let buffer = shell.output_buffer.lock().await;
            Ok(buffer.iter().cloned().collect())
        } else {
            Err(Error::ShellNotFound(shell_id.to_string()))
        }
    }
    
    pub async fn kill_shell(&self, shell_id: &str) -> Result<()> {
        if let Some(mut shell) = self.shells.lock().await.remove(shell_id) {
            shell.process.kill().await?;
            Ok(())
        } else {
            Err(Error::ShellNotFound(shell_id.to_string()))
        }
    }
}
```

### Phase 5: Complete Integration (Week 3)

#### 5.1 Wire Everything Together
1. Initialize BackgroundShellManager in AppState
2. Connect shell events to UI updates
3. Add commands for managing background shells
4. Implement BashOutputTool and KillBashTool
5. Add UI for showing background shell status

#### 5.2 Add Sandbox Mode Support
```rust
// Add to bash tool
if input["sandbox"].as_bool().unwrap_or(false) {
    // Run without permission checks but with restrictions
    return self.execute_sandboxed(command).await;
}
```

### Testing Plan

#### Integration Tests
```rust
#[tokio::test]
async fn test_permission_flow() {
    // 1. Create app state
    // 2. Start tool execution that needs permission
    // 3. Verify permission event sent
    // 4. Send permission decision
    // 5. Verify tool continues and completes
    // 6. Verify next command also asks permission
}

#[tokio::test]
async fn test_streaming_tools() {
    // 1. Create streaming response with tools
    // 2. Verify tools execute as discovered
    // 3. Verify UI updates in real-time
    // 4. Verify tool results continue conversation
}

#[tokio::test]
async fn test_background_shells() {
    // 1. Execute long-running command
    // 2. Choose background option
    // 3. Verify shell created
    // 4. Query output
    // 5. Kill shell
}
```

### Performance Considerations

1. **Streaming Buffering**: Buffer small text deltas before UI updates
2. **Background Shell Limits**: Limit number of concurrent background shells
3. **Output Buffer Limits**: Cap output buffer size per shell
4. **Permission Caching**: Cache permission decisions per session (optional)

### Migration Strategy

1. **Week 1**: Implement streaming architecture in parallel with existing code
2. **Week 2**: Add permission suspension system, test thoroughly
3. **Week 3**: Add background shells, integrate everything
4. **Week 4**: Remove old synchronous code, final testing

### Risk Mitigation

1. **Feature Flag**: Add feature flag to toggle between old and new implementation
2. **Gradual Rollout**: Test with internal users first
3. **Fallback**: Keep old code path available for emergency rollback
4. **Monitoring**: Add metrics for permission decisions and tool execution times

## Final Architecture Overview

```
User Input
    ↓
AI Streaming Response
    ↓
Tool Discovery (during stream)
    ↓
Permission Check (non-blocking)
    ↓
Tool Execution (parallel)
    ├─→ Foreground: Execute and inject result
    └─→ Background: Create shell, continue stream
    ↓
Tool Result Streaming
    ↓
AI Continuation
    ↓
UI Real-time Updates
```

This architecture achieves true parity with the JavaScript implementation while leveraging Rust's strengths in async execution and type safety.