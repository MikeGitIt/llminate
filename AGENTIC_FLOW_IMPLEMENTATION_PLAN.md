# Agentic Flow Implementation Plan
## Step-by-Step Guide to Achieve JavaScript Tool Parity

### Document Version: 1.0
### Date: September 15, 2025
### Urgency: CRITICAL - Current implementation is fundamentally broken

---

## Executive Summary

The current Rust implementation has critical architectural flaws that prevent it from functioning like the JavaScript tool. This plan provides a systematic approach to fixing these issues with minimal disruption while ensuring complete feature parity.

**Critical Issues to Fix:**
1. Non-autonomous conversation loop (stops after one tool)
2. Broken permission system (Option 3 doesn't cancel stream)
3. Wrong message roles (tool results in assistant messages)
4. No stream cancellation mechanism
5. Missing synthesis and continuation logic

---

## Phase 1: Foundation (Days 1-2)
**Goal**: Establish core infrastructure without breaking existing functionality

### Task 1.1: Create StreamController Module
**Priority**: CRITICAL
**File**: `src/ai/stream_controller.rs` (NEW)
**Estimated Time**: 2 hours

```rust
// src/ai/stream_controller.rs
use tokio::sync::watch;
use std::sync::Arc;

#[derive(Clone)]
pub struct StreamController {
    abort_tx: Arc<watch::Sender<bool>>,
    abort_rx: watch::Receiver<bool>,
}

impl StreamController {
    pub fn new() -> Self {
        let (tx, rx) = watch::channel(false);
        Self {
            abort_tx: Arc::new(tx),
            abort_rx: rx,
        }
    }

    pub fn abort(&self) {
        let _ = self.abort_tx.send(true);
    }

    pub fn is_aborted(&self) -> bool {
        *self.abort_rx.borrow()
    }

    pub fn subscribe(&self) -> watch::Receiver<bool> {
        self.abort_rx.clone()
    }
}
```

**Testing**:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_abort_controller() {
        let controller = StreamController::new();
        assert!(!controller.is_aborted());

        controller.abort();
        assert!(controller.is_aborted());

        let mut subscriber = controller.subscribe();
        assert!(*subscriber.borrow());
    }
}
```

### Task 1.2: Update StopReason Enum
**Priority**: CRITICAL
**File**: `src/ai/mod.rs`
**Estimated Time**: 1 hour

```rust
// Add to src/ai/mod.rs
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    ContentFiltered,
    EndTurn,
    GuardrailIntervened,
    MaxTokens,
    StopSequence,
    ToolUse,  // This triggers continuation!
}
```

### Task 1.3: Fix ToolResult Structure
**Priority**: CRITICAL
**File**: `src/ai/tools.rs`
**Estimated Time**: 1 hour

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    #[serde(rename = "type")]
    pub result_type: String,
    pub tool_use_id: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

impl ToolResult {
    pub fn success(id: String, content: String) -> Self {
        Self {
            result_type: "tool_result".to_string(),
            tool_use_id: id,
            content,
            is_error: None,
        }
    }

    pub fn error(id: String, content: String) -> Self {
        Self {
            result_type: "tool_result".to_string(),
            tool_use_id: id,
            content,
            is_error: Some(true),
        }
    }

    pub fn interrupt(id: String) -> Self {
        Self::error(
            id,
            format!("{}\n\n{}", INTERRUPT_TOOL, INTERRUPT_EXPLANATION),
        )
    }
}
```

### Task 1.4: Add Interrupt Message Constants
**Priority**: HIGH
**File**: `src/ai/constants.rs` (NEW)
**Estimated Time**: 30 minutes

```rust
// src/ai/constants.rs
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

## Phase 2: Permission System Fix (Day 3)
**Goal**: Fix Option 3 (Wait) to properly cancel stream and send interrupt

### Task 2.1: Update PermissionDecision Enum
**Priority**: CRITICAL
**File**: `src/permissions.rs`
**Estimated Time**: 1 hour

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum PermissionDecision {
    Allow,          // Option 1: "Yes"
    AlwaysAllow,    // Option 2: "Yes, don't ask again"
    Wait,           // Option 3: "No, and tell Claude differently"
    Never,          // Option 4: "Never"
}

// Update the dialog options
pub fn get_permission_options() -> Vec<(&'static str, PermissionDecision)> {
    vec![
        ("Allow this time", PermissionDecision::Allow),
        ("Always allow", PermissionDecision::AlwaysAllow),
        ("Wait and tell Claude what to do differently", PermissionDecision::Wait),
        ("Never allow", PermissionDecision::Never),
    ]
}
```

### Task 2.2: Implement Permission Handler with Abort
**Priority**: CRITICAL
**File**: `src/tui/state.rs`
**Estimated Time**: 2 hours

```rust
// In handle_permission_request
match decision {
    PermissionDecision::Wait => {
        // 1. Abort the stream
        stream_controller.abort();

        // 2. Create interrupt result
        let interrupt = ToolResult::interrupt(tool_use.id.clone());

        // 3. Send interrupt message to UI
        let _ = event_tx.send(TuiEvent::Message(
            "[Tool execution interrupted - waiting for user feedback]".to_string()
        )).await;

        // 4. Return interrupted result
        Ok(PermissionResult::Interrupted(interrupt))
    }
    // ... other cases
}
```

---

## Phase 3: Conversation Loop Rewrite (Days 4-5)
**Goal**: Replace non-autonomous loop with proper implementation

### Task 3.1: Refactor Main Conversation Loop
**Priority**: CRITICAL
**File**: `src/tui/state.rs` (major refactor)
**Estimated Time**: 4 hours

**Key Changes**:
1. Remove the break after tool execution
2. Add proper stop_reason checking
3. Send tool results as USER messages
4. Add continuation logic

```rust
// Simplified structure of new loop
pub async fn handle_streaming_response(&mut self) -> Result<()> {
    let controller = StreamController::new();
    let mut messages = self.conversation.messages.clone();
    let mut iteration = 0;
    const MAX_ITERATIONS: usize = 25;

    'conversation: while iteration < MAX_ITERATIONS {
        iteration += 1;

        // Check abort
        if controller.is_aborted() {
            self.add_message("[Conversation interrupted]");
            break 'conversation;
        }

        // Stream response
        let response = self.stream_ai_response(&messages, &controller).await?;

        // Add assistant message
        if !response.content.is_empty() {
            messages.push(Message {
                role: Role::Assistant,
                content: response.content,
            });
            self.update_display(&response.content);
        }

        // Check stop reason
        match response.stop_reason {
            Some(StopReason::ToolUse) => {
                // Execute tools
                let tool_results = self.execute_tools_with_permissions(
                    response.tool_uses,
                    &controller,
                ).await?;

                // Check if interrupted
                if controller.is_aborted() {
                    if !tool_results.is_empty() {
                        // Send interrupt as user message
                        messages.push(Message {
                            role: Role::User,
                            content: ContentPart::ToolResults(tool_results),
                        });
                    }
                    break 'conversation;
                }

                // Add tool results as USER message
                if !tool_results.is_empty() {
                    messages.push(Message {
                        role: Role::User,  // CRITICAL!
                        content: ContentPart::ToolResults(tool_results),
                    });
                    // CONTINUE LOOP - Don't break!
                    continue 'conversation;
                }
            }
            Some(StopReason::EndTurn) | Some(StopReason::MaxTokens) => {
                break 'conversation;
            }
            _ => {
                if self.needs_continuation(&response) {
                    continue 'conversation;
                } else {
                    break 'conversation;
                }
            }
        }
    }

    Ok(())
}
```

### Task 3.2: Implement Continuation Check
**Priority**: HIGH
**File**: `src/tui/state.rs`
**Estimated Time**: 1 hour

```rust
fn needs_continuation(&self, response: &Response) -> bool {
    // Check if response indicates more work needed
    let text = response.content.iter()
        .filter_map(|c| match c {
            ContentPart::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<String>()
        .to_lowercase();

    let continuation_phrases = [
        "let me", "i'll now", "i will now", "next, i",
        "continuing with", "moving on to", "i'll proceed",
        "let's", "i need to", "i should"
    ];

    continuation_phrases.iter().any(|phrase| text.contains(phrase))
}
```

---

## Phase 4: Stream Processing Integration (Day 6)
**Goal**: Integrate abort controller with streaming

### Task 4.1: Update Stream Processing
**Priority**: HIGH
**File**: `src/ai/streaming.rs`
**Estimated Time**: 3 hours

```rust
pub async fn process_sse_stream(
    stream: impl Stream<Item = Result<Event, Error>>,
    controller: &StreamController,
) -> Result<Response> {
    let mut abort_rx = controller.subscribe();
    let mut response = Response::default();

    tokio::pin!(stream);

    loop {
        tokio::select! {
            // Check abort signal
            _ = abort_rx.changed() => {
                if *abort_rx.borrow() {
                    return Err(Error::StreamAborted);
                }
            }

            // Process stream
            event = stream.next() => {
                match event {
                    Some(Ok(evt)) => {
                        if controller.is_aborted() {
                            return Err(Error::StreamAborted);
                        }
                        process_event(evt, &mut response)?;
                    }
                    Some(Err(e)) => return Err(e),
                    None => break,
                }
            }
        }
    }

    Ok(response)
}
```

### Task 4.2: Handle All SSE Event Types
**Priority**: HIGH
**File**: `src/ai/streaming.rs`
**Estimated Time**: 2 hours

```rust
fn process_event(event: Event, response: &mut Response) -> Result<()> {
    match event.event_type.as_str() {
        "message_start" => {
            // Initialize message
        }
        "message_delta" => {
            // Update message
        }
        "message_stop" => {
            if let Ok(data) = serde_json::from_str::<MessageStop>(&event.data) {
                response.stop_reason = Some(data.stop_reason);
            }
        }
        "content_block_start" => {
            if let Ok(data) = serde_json::from_str::<ContentBlockStart>(&event.data) {
                if let ContentBlock::ToolUse(tool) = data.content_block {
                    response.tool_uses.push(tool);
                }
            }
        }
        "content_block_delta" => {
            // Handle incremental updates
        }
        "content_block_stop" => {
            // Finalize content block
        }
        _ => {}
    }
    Ok(())
}
```

---

## Phase 5: Testing and Validation (Days 7-8)
**Goal**: Ensure complete functionality and parity with JavaScript

### Task 5.1: Unit Tests
**Priority**: HIGH
**Files**: Various test modules
**Estimated Time**: 4 hours

```rust
// Tests to implement
#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_autonomous_continuation() {
        // Verify loop continues on tool_use stop reason
    }

    #[tokio::test]
    async fn test_wait_option_aborts_stream() {
        // Verify Option 3 cancels everything
    }

    #[tokio::test]
    async fn test_tool_results_as_user_messages() {
        // Verify correct message roles
    }

    #[tokio::test]
    async fn test_interrupt_message_format() {
        // Verify interrupt message matches JS
    }

    #[tokio::test]
    async fn test_multiple_tool_execution() {
        // Verify multiple rounds work
    }
}
```

### Task 5.2: Integration Testing
**Priority**: HIGH
**Estimated Time**: 4 hours

**Test Scenarios**:
1. Multi-step task requiring multiple tools
2. Permission denial (Wait) during tool execution
3. Mixed allow/deny permissions
4. Token limit handling
5. Error recovery
6. Synthesis after tool execution

### Task 5.3: Manual TUI Testing Checklist
**Priority**: CRITICAL
**Estimated Time**: 2 hours

- [ ] Start conversation with complex task
- [ ] Verify autonomous tool execution
- [ ] Test Option 3 (Wait) - stream stops immediately
- [ ] Verify interrupt message appears
- [ ] Test continuation after providing new instructions
- [ ] Verify no tools execute after abort
- [ ] Check message formatting in UI
- [ ] Verify tool results appear correctly

---

## Phase 6: Synthesis and Polish (Day 9)
**Goal**: Add synthesis and final polish

### Task 6.1: Implement Synthesis
**Priority**: MEDIUM
**File**: `src/ai/synthesis.rs` (NEW)
**Estimated Time**: 2 hours

```rust
pub fn create_synthesis_id(message_id: &str, agent_num: Option<usize>) -> String {
    match agent_num {
        Some(num) => format!("agent_{}_{}", num, message_id),
        None => format!("synthesis_{}", message_id),
    }
}

pub async fn perform_synthesis(
    tool_results: Vec<ToolResult>,
    message_id: &str,
) -> Result<String> {
    // Create synthesis of tool results
    let synthesis_id = create_synthesis_id(message_id, None);

    // Format results for presentation
    let mut synthesis = String::from("Based on the tool executions:\n");
    for result in tool_results {
        if !result.is_error.unwrap_or(false) {
            synthesis.push_str(&format!("- {}\n", result.content));
        }
    }

    Ok(synthesis)
}
```

### Task 6.2: Performance Optimization
**Priority**: LOW
**Estimated Time**: 2 hours

- Cache rendered messages
- Optimize streaming buffer sizes
- Reduce unnecessary clones
- Profile and optimize hot paths

---

## Implementation Schedule

### Week 1
- **Day 1-2**: Phase 1 - Foundation
- **Day 3**: Phase 2 - Permission System
- **Day 4-5**: Phase 3 - Conversation Loop

### Week 2
- **Day 6**: Phase 4 - Stream Integration
- **Day 7-8**: Phase 5 - Testing
- **Day 9**: Phase 6 - Synthesis and Polish
- **Day 10**: Final validation and deployment

---

## Risk Mitigation

### High-Risk Areas
1. **Conversation Loop Rewrite**
   - Mitigation: Create feature flag to toggle old/new behavior
   - Rollback plan: Keep old code commented

2. **Stream Cancellation**
   - Mitigation: Test thoroughly with mock streams
   - Fallback: Timeout-based cancellation

3. **Message Role Changes**
   - Mitigation: Add compatibility layer
   - Monitor: Log message roles for debugging

### Rollback Strategy
```rust
// Feature flags for gradual rollout
pub struct FeatureFlags {
    pub autonomous_loop: bool,
    pub stream_cancellation: bool,
    pub new_permission_system: bool,
}

impl Default for FeatureFlags {
    fn default() -> Self {
        Self {
            autonomous_loop: env::var("AUTONOMOUS_LOOP")
                .map(|v| v == "true")
                .unwrap_or(false),
            stream_cancellation: true,
            new_permission_system: true,
        }
    }
}
```

---

## Success Criteria

### Functional Requirements
- [x] Conversation continues autonomously until task complete
- [x] Option 3 (Wait) immediately stops all processing
- [x] Tool results sent as user messages
- [x] Interrupt message matches JavaScript format
- [x] Multiple tool executions work correctly
- [x] Synthesis occurs after tool execution

### Performance Requirements
- [ ] Response time < 100ms for permission dialog
- [ ] Stream cancellation < 50ms
- [ ] Memory usage stable over long conversations
- [ ] No UI freezing during processing

### Quality Requirements
- [ ] 90%+ test coverage for new code
- [ ] No regression in existing features
- [ ] Clean error handling throughout
- [ ] Comprehensive logging for debugging

---

## Team Assignments

### Developer 1: Core Infrastructure
- StreamController implementation
- Conversation loop rewrite
- Integration testing

### Developer 2: Permission and Tools
- Permission system updates
- Tool result formatting
- Interrupt message handling

### Developer 3: Streaming and UI
- SSE event processing
- UI updates for new flow
- Manual testing

---

## Communication Plan

### Daily Standup Topics
1. Progress on current phase
2. Blockers or issues encountered
3. Testing results
4. Next day's priorities

### Code Review Requirements
- All changes require review
- Focus on state management
- Verify error handling
- Check for race conditions

### Documentation Updates
- Update API documentation
- Add inline comments for complex logic
- Update user documentation
- Create troubleshooting guide

---

## Post-Implementation

### Monitoring
- Track conversation completion rates
- Monitor permission denial patterns
- Log stream cancellation frequency
- Measure average iterations per conversation

### Optimization Opportunities
1. Parallel tool execution
2. Response caching
3. Predictive permission rules
4. Smart continuation detection

### Future Enhancements
1. Sub-agent orchestration
2. Context window management
3. Advanced synthesis strategies
4. Tool execution priorities

---

## Appendix: Quick Reference

### File Modification Summary
```
src/
├── ai/
│   ├── stream_controller.rs (NEW) - Abort mechanism
│   ├── constants.rs (NEW) - Interrupt messages
│   ├── synthesis.rs (NEW) - Synthesis logic
│   ├── mod.rs (MODIFY) - StopReason enum
│   ├── tools.rs (MODIFY) - ToolResult structure
│   └── streaming.rs (MODIFY) - Abort integration
├── tui/
│   └── state.rs (MAJOR REFACTOR) - Conversation loop
└── permissions.rs (MODIFY) - Wait option

tests/
├── test_stream_controller.rs (NEW)
├── test_autonomous_loop.rs (NEW)
└── test_permission_flow.rs (NEW)
```

### Critical Code Sections
1. Lines 316-442 in `state.rs` - Main loop
2. Lines 600-780 in `state.rs` - Tool execution
3. Lines 790-860 in `state.rs` - Message handling

### Testing Commands
```bash
# Run all tests
cargo test

# Run specific test suite
cargo test test_autonomous_loop

# Run with logging
RUST_LOG=debug cargo test -- --nocapture

# Integration test
cargo run --example complex_conversation

# Manual TUI test
cargo run
```

---

## Document Control

- **Version**: 1.0
- **Status**: APPROVED FOR IMPLEMENTATION
- **Priority**: CRITICAL
- **Timeline**: 10 days
- **Resources**: 3 developers
- **Budget Impact**: None (internal resources)