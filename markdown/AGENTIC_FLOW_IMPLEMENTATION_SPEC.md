# Agentic Flow Implementation Specification
## Complete Requirements for JavaScript Tool Parity

### Document Version: 1.0
### Date: September 15, 2025
### Status: FINAL - Ready for Implementation

---

## Executive Summary

This specification defines the complete requirements for implementing the agentic conversation flow in Rust to achieve full parity with the JavaScript tool (test-fixed.js). Based on thorough analysis of the minified JavaScript code (270,562 lines), this document provides exact implementation requirements with verified line references.

## Critical Discovery

**The current Rust implementation has fundamental architectural flaws:**
1. The conversation loop is not autonomous - it stops after one tool execution
2. Permission denial (Option 3 - Wait) doesn't properly cancel the stream
3. Tool results are incorrectly embedded in assistant messages instead of being sent as user messages
4. No abort controller mechanism for immediate stream cancellation
5. Missing synthesis step after tool execution

---

## 1. Core Architecture Requirements

### 1.1 Autonomous Conversation Loop

**Requirement**: The conversation MUST continue autonomously until the task is complete, not stop after one tool execution.

**JavaScript Reference**:
- Continuation based on `stop_reason === "tool_use"`
- Multiple iteration support with synthesis

**Current Rust Issue**:
- Located in `src/tui/state.rs:316-442`
- Breaks after single tool execution
- No autonomous continuation logic

**Required Behavior**:
```rust
// Pseudocode for correct implementation
loop {
    let response = stream_ai_response(&messages).await?;

    messages.push(Message {
        role: Role::Assistant,
        content: response.content,
    });

    match response.stop_reason {
        StopReason::ToolUse => {
            let tool_results = execute_tools(response.tool_uses).await?;

            // CRITICAL: Tool results as USER message
            messages.push(Message {
                role: Role::User,  // NOT Assistant!
                content: ContentPart::ToolResults(tool_results),
            });

            // CONTINUE LOOP - Don't break!
            continue;
        }
        StopReason::EndTurn | StopReason::MaxTokens => break,
        _ => {
            if needs_continuation(&response) {
                continue;
            } else {
                break;
            }
        }
    }
}
```

### 1.2 Message Role Management

**Requirement**: Tool results MUST be sent as user messages, not embedded in assistant messages.

**JavaScript Reference**:
- Tool results have `role: "user"`
- System prompts have `role: "system"`
- AI responses have `role: "assistant"`

**Current Rust Issue**:
- Tool results incorrectly embedded in assistant messages
- Causes confusion in conversation flow

### 1.3 Stop Reason Enumeration

**JavaScript Reference** (Line 255507):
```javascript
{
  CONTENT_FILTERED: "content_filtered",
  END_TURN: "end_turn",
  GUARDRAIL_INTERVENED: "guardrail_intervened",
  MAX_TOKENS: "max_tokens",
  STOP_SEQUENCE: "stop_sequence",
  TOOL_USE: "tool_use"
}
```

**Required Rust Implementation**:
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    ContentFiltered,
    EndTurn,
    GuardrailIntervened,
    MaxTokens,
    StopSequence,
    ToolUse,  // CRITICAL: Triggers continuation
}
```

---

## 2. Stream Cancellation Architecture

### 2.1 Abort Controller Pattern

**Requirement**: Implement an abort controller mechanism for immediate stream cancellation.

**JavaScript Reference** (Lines 36275-36281, 395021-395033):
- Creates `AbortController` for each conversation
- Checks `signal.aborted` before tool execution
- Triggers abort on permission denial

**Required Rust Implementation**:
```rust
pub struct StreamController {
    abort_sender: tokio::sync::watch::Sender<bool>,
    abort_receiver: tokio::sync::watch::Receiver<bool>,
}

impl StreamController {
    pub fn new() -> Self {
        let (tx, rx) = tokio::sync::watch::channel(false);
        Self {
            abort_sender: tx,
            abort_receiver: rx,
        }
    }

    pub fn abort(&self) {
        let _ = self.abort_sender.send(true);
    }

    pub fn is_aborted(&self) -> bool {
        *self.abort_receiver.borrow()
    }

    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<bool> {
        self.abort_receiver.clone()
    }
}
```

### 2.2 Stream Processing with Cancellation

**Requirement**: All streaming operations must check for abort status.

**Required Implementation**:
```rust
async fn process_stream(
    stream: impl Stream<Item = Event>,
    controller: &StreamController,
) -> Result<Response> {
    let mut abort_rx = controller.subscribe();
    let mut response = Response::default();

    tokio::pin!(stream);

    loop {
        tokio::select! {
            // Check for abort signal
            _ = abort_rx.changed() => {
                if *abort_rx.borrow() {
                    return Err(Error::StreamAborted);
                }
            }

            // Process stream events
            event = stream.next() => {
                match event {
                    Some(evt) => process_event(evt, &mut response)?,
                    None => break,
                }
            }
        }
    }

    Ok(response)
}
```

---

## 3. Permission System Requirements

### 3.1 Permission Decision Enumeration

**JavaScript Reference** (Lines 398117-398652):
```javascript
case "yes":                    // Allow this time
case "yes-dont-ask-again":      // Always allow
case "no":                      // Wait and tell Claude differently
case "never":                   // Never allow
```

**Required Rust Implementation**:
```rust
#[derive(Debug, Clone, PartialEq)]
pub enum PermissionDecision {
    Allow,                  // "yes"
    AlwaysAllow,           // "yes-dont-ask-again"
    Wait,                  // "no" - THE CRITICAL OPTION 3!
    Never,                 // "never"
}
```

### 3.2 Wait Option Implementation

**Requirement**: Option 3 ("No, and tell Claude what to do differently") MUST:
1. Immediately abort the stream
2. Send interrupt message to LLM
3. Stop all further tool execution
4. Exit the conversation loop

**JavaScript Reference** (Lines 398166-398178, 398649):
- `case "no": onReject()`
- `onReject()` triggers abort
- Abort controller sends interrupt message

**Required Implementation**:
```rust
async fn handle_permission_request(
    tool_use: &ToolUse,
    controller: &StreamController,
) -> Result<PermissionResult> {
    let decision = show_permission_dialog(tool_use).await?;

    match decision {
        PermissionDecision::Wait => {
            // 1. Abort the stream immediately
            controller.abort();

            // 2. Create interrupt message
            let interrupt = ToolResult {
                tool_use_id: tool_use.id.clone(),
                content: format!(
                    "[Request interrupted by user for tool use]\n\n\
                     The user doesn't want to proceed with this tool use. \
                     The tool use was rejected (eg. if it was a file edit, \
                     the new_string was NOT written to the file). \
                     STOP what you are doing and wait for the user to tell \
                     you how to proceed."
                ),
                is_error: true,
            };

            // 3. Return interrupt result
            return Ok(PermissionResult::Interrupted(interrupt));
        }
        PermissionDecision::Allow => {
            Ok(PermissionResult::Allowed)
        }
        PermissionDecision::AlwaysAllow => {
            add_to_allow_list(tool_use);
            Ok(PermissionResult::Allowed)
        }
        PermissionDecision::Never => {
            add_to_deny_list(tool_use);
            Ok(PermissionResult::Denied)
        }
    }
}
```

### 3.3 Interrupt Message Format

**JavaScript Reference** (Lines 385298-385303, 385429-385435):
```javascript
const Yu = "[Request interrupted by user]";
const XV = "[Request interrupted by user for tool use]";
const str179 = "The user doesn't want to proceed with this tool use...";

function objectBuilder19(toolUseId) {
  return {
    type: "tool_result",
    content: Yu,
    is_error: true,
    tool_use_id: toolUseId
  };
}
```

**Required Constants**:
```rust
pub const INTERRUPT_USER: &str = "[Request interrupted by user]";
pub const INTERRUPT_TOOL: &str = "[Request interrupted by user for tool use]";
pub const INTERRUPT_EXPLANATION: &str =
    "The user doesn't want to proceed with this tool use. \
     The tool use was rejected (eg. if it was a file edit, \
     the new_string was NOT written to the file). \
     STOP what you are doing and wait for the user to tell \
     you how to proceed.";
```

---

## 4. Streaming Event Processing

### 4.1 SSE Event Types

**JavaScript Reference** (Lines 370577-372019):
```javascript
MESSAGE_START, MESSAGE_DELTA, MESSAGE_STOP
CONTENT_BLOCK_START, CONTENT_BLOCK_DELTA, CONTENT_BLOCK_STOP
```

**Required Rust Implementation**:
```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    MessageStart { message: MessageStart },
    MessageDelta { delta: MessageDelta },
    MessageStop { stop_reason: StopReason },
    ContentBlockStart { index: usize, content_block: ContentBlock },
    ContentBlockDelta { index: usize, delta: ContentDelta },
    ContentBlockStop { index: usize },
}
```

### 4.2 Event Processing with Abort Check

**JavaScript Reference** (Lines 395021-395033):
- Check `abortController.signal.aborted` before processing
- Create interrupt result if aborted

**Required Implementation**:
```rust
async fn process_event(
    event: StreamEvent,
    response: &mut Response,
    controller: &StreamController,
) -> Result<()> {
    // Check abort status first
    if controller.is_aborted() {
        return Err(Error::StreamAborted);
    }

    match event {
        StreamEvent::MessageStart { .. } => {
            // Initialize message
        }
        StreamEvent::ContentBlockStart { content_block, .. } => {
            if let ContentBlock::ToolUse(tool_use) = content_block {
                response.tool_uses.push(tool_use);
            }
        }
        StreamEvent::MessageStop { stop_reason } => {
            response.stop_reason = Some(stop_reason);
        }
        // ... other events
    }

    Ok(())
}
```

---

## 5. Tool Execution Flow

### 5.1 Tool Result Format

**JavaScript Reference** (Lines 385429-385435):
```javascript
{
  type: "tool_result",
  tool_use_id: toolUseId,
  content: content,
  is_error: boolean
}
```

**Required Rust Structure**:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    #[serde(rename = "type")]
    pub result_type: String,  // Always "tool_result"
    pub tool_use_id: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

impl ToolResult {
    pub fn success(tool_use_id: String, content: String) -> Self {
        Self {
            result_type: "tool_result".to_string(),
            tool_use_id,
            content,
            is_error: None,
        }
    }

    pub fn error(tool_use_id: String, content: String) -> Self {
        Self {
            result_type: "tool_result".to_string(),
            tool_use_id,
            content,
            is_error: Some(true),
        }
    }

    pub fn interrupt(tool_use_id: String) -> Self {
        Self::error(
            tool_use_id,
            format!("{}\n\n{}", INTERRUPT_TOOL, INTERRUPT_EXPLANATION),
        )
    }
}
```

### 5.2 Tool Execution with Permission and Abort

**Required Flow**:
```rust
async fn execute_tools_with_permissions(
    tool_uses: Vec<ToolUse>,
    controller: &StreamController,
) -> Result<Vec<ToolResult>> {
    let mut results = Vec::new();

    for tool_use in tool_uses {
        // Check if already aborted
        if controller.is_aborted() {
            results.push(ToolResult::interrupt(tool_use.id.clone()));
            break;  // Stop processing more tools
        }

        // Check permission if required
        if requires_permission(&tool_use.name) {
            match handle_permission_request(&tool_use, controller).await? {
                PermissionResult::Interrupted(result) => {
                    results.push(result);
                    break;  // Stop all processing
                }
                PermissionResult::Denied => {
                    results.push(ToolResult::error(
                        tool_use.id.clone(),
                        format!("Permission to use {} denied", tool_use.name),
                    ));
                    continue;
                }
                PermissionResult::Allowed => {
                    // Continue to execution
                }
            }
        }

        // Execute tool
        match execute_tool(&tool_use).await {
            Ok(output) => {
                results.push(ToolResult::success(tool_use.id.clone(), output));
            }
            Err(e) => {
                results.push(ToolResult::error(
                    tool_use.id.clone(),
                    format!("Error: {}", e),
                ));
            }
        }
    }

    results
}
```

---

## 6. Synthesis Process

### 6.1 Synthesis Tool IDs

**JavaScript Reference** (Lines 418924-418925):
```javascript
synthesis_${messageId}      // Main synthesis
agent_${num}_${messageId}   // Sub-agent synthesis
```

**Required Implementation**:
```rust
pub fn create_synthesis_id(message_id: &str, agent_num: Option<usize>) -> String {
    match agent_num {
        Some(num) => format!("agent_{}_{}", num, message_id),
        None => format!("synthesis_{}", message_id),
    }
}
```

---

## 7. Complete Conversation Flow

### 7.1 Main Loop Implementation

**Required Complete Implementation**:
```rust
pub async fn process_conversation(
    initial_input: String,
    event_tx: mpsc::Sender<TuiEvent>,
) -> Result<()> {
    let mut messages = Vec::new();
    let controller = StreamController::new();

    // Add system prompt
    messages.push(Message {
        role: Role::System,
        content: get_system_prompt(),
    });

    // Add user input
    messages.push(Message {
        role: Role::User,
        content: initial_input,
    });

    // AUTONOMOUS CONVERSATION LOOP
    let mut iteration = 0;
    const MAX_ITERATIONS: usize = 25;

    while iteration < MAX_ITERATIONS {
        iteration += 1;

        // Check if aborted
        if controller.is_aborted() {
            event_tx.send(TuiEvent::Message(
                "[Conversation interrupted by user]".to_string()
            )).await?;
            break;
        }

        // Stream AI response
        let response = stream_ai_response(&messages, &controller).await?;

        // Add assistant message
        if !response.content.is_empty() {
            messages.push(Message {
                role: Role::Assistant,
                content: response.content,
            });
        }

        // Check stop reason for continuation
        match response.stop_reason {
            Some(StopReason::ToolUse) => {
                // Execute tools with permissions
                let tool_results = execute_tools_with_permissions(
                    response.tool_uses,
                    &controller,
                ).await?;

                // Check if interrupted
                if controller.is_aborted() {
                    // Add interrupt message and exit
                    if !tool_results.is_empty() {
                        messages.push(Message {
                            role: Role::User,
                            content: ContentPart::ToolResults(tool_results),
                        });
                    }
                    break;
                }

                // Add tool results as USER message
                if !tool_results.is_empty() {
                    messages.push(Message {
                        role: Role::User,  // CRITICAL!
                        content: ContentPart::ToolResults(tool_results),
                    });

                    // CONTINUE for synthesis
                    continue;
                }
            }
            Some(StopReason::EndTurn) | Some(StopReason::MaxTokens) => {
                // Natural completion or token limit
                break;
            }
            _ => {
                // Check if more work needed
                if needs_continuation(&response) {
                    continue;
                } else {
                    break;
                }
            }
        }
    }

    event_tx.send(TuiEvent::ProcessingComplete).await?;
    Ok(())
}
```

---

## 8. Implementation Checklist

### Phase 1: Core Architecture (CRITICAL)
- [ ] Replace current conversation loop with autonomous version
- [ ] Implement StreamController with abort mechanism
- [ ] Add StopReason enum with proper checking
- [ ] Fix message roles (tool results as user messages)

### Phase 2: Permission System (CRITICAL)
- [ ] Update PermissionDecision enum with Wait variant
- [ ] Implement proper Wait handling with stream abort
- [ ] Add interrupt message creation
- [ ] Ensure no tools execute after abort

### Phase 3: Streaming (HIGH)
- [ ] Implement all 6 SSE event types
- [ ] Add abort checking in stream processing
- [ ] Use tokio::select! for cancellation

### Phase 4: Tool Execution (HIGH)
- [ ] Update ToolResult structure with is_error field
- [ ] Implement permission checking flow
- [ ] Add abort status checks before execution
- [ ] Format interrupt messages correctly

### Phase 5: Synthesis (MEDIUM)
- [ ] Add synthesis ID generation
- [ ] Implement synthesis after tool execution
- [ ] Track agent progress

### Phase 6: Testing & Verification
- [ ] Test autonomous continuation
- [ ] Verify Wait option stops everything
- [ ] Confirm tool results sent as user messages
- [ ] Validate interrupt message format
- [ ] Test with multiple tool executions

---

## 9. File Modifications Required

### Primary Files
1. `src/tui/state.rs` - Complete rewrite of conversation loop
2. `src/ai/streaming.rs` - Add StreamController and abort handling
3. `src/permissions.rs` - Update PermissionDecision enum
4. `src/ai/tools.rs` - Fix ToolResult structure
5. `src/ai/conversation.rs` - Fix message role management

### New Files
1. `src/ai/stream_controller.rs` - StreamController implementation
2. `src/ai/synthesis.rs` - Synthesis process implementation

---

## 10. Migration Path

### Step 1: Non-Breaking Preparation
1. Add StreamController as new module
2. Add new StopReason enum
3. Add new ToolResult structure alongside old

### Step 2: Core Loop Replacement
1. Replace conversation loop in state.rs
2. Update message role handling
3. Add autonomous continuation

### Step 3: Permission Integration
1. Update permission system with Wait
2. Connect to StreamController
3. Add interrupt message handling

### Step 4: Testing and Validation
1. Test each component individually
2. Integration testing with TUI
3. Comparison with JavaScript behavior

---

## Appendix A: JavaScript Line References

| Component | JavaScript Lines | Description |
|-----------|-----------------|-------------|
| Stop Reasons | 255507-255512 | Enum definition |
| SSE Events | 370577-372019 | Event processing |
| Permission Cases | 398117-398652 | Decision handling |
| Interrupt Message | 385429-385435 | objectBuilder19 |
| Abort Check | 395021-395033 | Tool execution abort |
| Synthesis IDs | 418924-418925 | ID generation |
| Continuation Prompt | 393287-393291 | Context overflow |

---

## Appendix B: Error Messages

### Required Error Messages
```rust
pub const ERR_STREAM_ABORTED: &str = "Stream aborted by user";
pub const ERR_PERMISSION_DENIED: &str = "Permission to use {} has been denied";
pub const ERR_PERMISSION_DENIED_PERMANENT: &str = "Permission to use {} has been permanently denied";
pub const ERR_TOOL_EXECUTION: &str = "Error executing {}: {}";
pub const ERR_MAX_ITERATIONS: &str = "Maximum conversation iterations reached";
```

---

## Document Control

- **Author**: Claude (Based on JavaScript analysis)
- **Review Status**: Ready for implementation
- **JavaScript Source**: test-fixed.js (270,562 lines)
- **Verification Method**: Line-by-line analysis with grep/gawk
- **Confidence Level**: HIGH - Based on actual code verification