# EXACT CODE LOCATIONS AND REQUIRED CHANGES

## File: /Users/mickillah/Code/rust_projects/llminate/src/ai/streaming.rs

### CHANGE 1: StreamingHandler::process_stream - ContentBlockStart (Lines 105-114)

**CURRENT CODE (BUGGY):**
```rust
105                            StreamEvent::ContentBlockStart { content_block, .. } => {
106                                match content_block {
107                                    ContentBlock::Text { text } => {
108                                        let _ = tx.send(StreamingUpdate::TextChunk(text));
109                                    }
110                                    ContentBlock::ToolUse { id, name, .. } => {  // ← IGNORES input
111                                        current_tool_id = Some(id.clone());
112                                        tool_input_buffer.clear();  // ← CLEARS to empty
113                                        let _ = tx.send(StreamingUpdate::ToolUseStart { id, name });
114                                    }
```

**REQUIRED FIX:**
```rust
105                            StreamEvent::ContentBlockStart { content_block, .. } => {
106                                match content_block {
107                                    ContentBlock::Text { text } => {
108                                        let _ = tx.send(StreamingUpdate::TextChunk(text));
109                                    }
110                                    ContentBlock::ToolUse { id, name, input } => {  // ← CAPTURE input
111                                        current_tool_id = Some(id.clone());
112                                        // Initialize buffer with initial input from API
113                                        tool_input_buffer = serde_json::to_string(&input)
114                                            .unwrap_or_else(|_| "{}".to_string());
115                                        let _ = tx.send(StreamingUpdate::ToolUseStart { id, name });
116                                    }
```

---

### CHANGE 2: StreamingHandler::process_stream - ContentBlockStop (Lines 148-166)

**CURRENT CODE (BUGGY):**
```rust
148                            StreamEvent::ContentBlockStop { .. } => {
149                                if let Some(id) = current_tool_id.take() {
150                                    match serde_json::from_str(&tool_input_buffer) {  // ← Parses ""
151                                        Ok(input) => {
152                                            let _ = tx.send(StreamingUpdate::ToolUseComplete {
153                                                id,
154                                                input,
155                                            });
156                                        }
157                                        Err(e) => {
158                                            let _ = tx.send(StreamingUpdate::Error(format!(
159                                                "Failed to parse tool input: {}",
160                                                e
161                                            )));
162                                        }
163                                    }
164                                    tool_input_buffer.clear();
165                                }
166                            }
```

**REQUIRED FIX:**
```rust
148                            StreamEvent::ContentBlockStop { .. } => {
149                                if let Some(id) = current_tool_id.take() {
150                                    // Handle empty buffer - use fallback for tools with no input
151                                    if tool_input_buffer.is_empty() {
152                                        tool_input_buffer = "{}".to_string();
153                                    }
154                                    match serde_json::from_str(&tool_input_buffer) {
155                                        Ok(input) => {
156                                            let _ = tx.send(StreamingUpdate::ToolUseComplete {
157                                                id,
158                                                input,
159                                            });
160                                        }
161                                        Err(e) => {
162                                            let _ = tx.send(StreamingUpdate::Error(format!(
163                                                "Failed to parse tool input: {}",
164                                                e
165                                            )));
166                                        }
167                                    }
168                                    tool_input_buffer.clear();
169                                }
170                            }
```

---

### CHANGE 3: StreamProcessor::process - ContentBlockStart (Lines 393-409)

**CURRENT CODE (BUGGY):**
```rust
393                        StreamEvent::ContentBlockStart { content_block, .. } => {
394                            match content_block {
395                                ContentBlock::Text { text } => StreamingUpdate::TextChunk(text),
396                                ContentBlock::ToolUse { id, name, .. } => {  // ← IGNORES input
397                                    StreamingUpdate::ToolUseStart { id, name }
398                                }
399                                ContentBlock::Thinking { thinking, .. } => {
400                                    if thinking.is_empty() {
401                                        StreamingUpdate::ThinkingStart
402                                    } else {
403                                        StreamingUpdate::ThinkingChunk(thinking)
404                                    }
405                                }
406                                ContentBlock::RedactedThinking { .. } => {
407                                    continue; // Redacted thinking not shown to user
408                                }
409                            }
```

**REQUIRED FIX:**
```rust
393                        StreamEvent::ContentBlockStart { content_block, .. } => {
394                            match content_block {
395                                ContentBlock::Text { text } => StreamingUpdate::TextChunk(text),
396                                ContentBlock::ToolUse { id, name, input } => {  // ← CAPTURE input
397                                    // Store initial input for tool use
398                                    if let Some(index) = accumulator.tool_uses.len() {
399                                        if let Some(tool) = accumulator.tool_uses.get_mut(index) {
400                                            tool.input_buffer = serde_json::to_string(&input)
401                                                .unwrap_or_else(|_| "{}".to_string());
402                                        }
403                                    }
404                                    StreamingUpdate::ToolUseStart { id, name }
405                                }
406                                ContentBlock::Thinking { thinking, .. } => {
407                                    if thinking.is_empty() {
408                                        StreamingUpdate::ThinkingStart
409                                    } else {
410                                        StreamingUpdate::ThinkingChunk(thinking)
411                                    }
412                                }
413                                ContentBlock::RedactedThinking { .. } => {
414                                    continue; // Redacted thinking not shown to user
415                                }
416                            }
```

NOTE: This is more complex because StreamProcessor uses an accumulator. Alternative approach:
Store the input value in a local variable and set it when ToolUseStart event creates the AccumulatedToolUse.

---

### CHANGE 4: StreamProcessor::process - ContentBlockStop (Lines 434-452)

**CURRENT CODE (BUGGY):**
```rust
434                        StreamEvent::ContentBlockStop { .. } => {
435                            if let Some(index) = accumulator.current_tool_index {
436                                if let Some(tool) = accumulator.tool_uses.get_mut(index) {
437                                    match serde_json::from_str(&tool.input_buffer) {  // ← Parses empty
438                                        Ok(input) => StreamingUpdate::ToolUseComplete {
439                                            id: tool.id.clone(),
440                                            input,
441                                        },
442                                        Err(e) => StreamingUpdate::Error(format!(
443                                            "Failed to parse tool input: {}",
444                                            e
445                                        )),
446                                    }
447                                } else {
447                                    continue;
448                                }
449                            } else {
450                                continue;
451                            }
452                        }
```

**REQUIRED FIX:**
```rust
434                        StreamEvent::ContentBlockStop { .. } => {
435                            if let Some(index) = accumulator.current_tool_index {
436                                if let Some(tool) = accumulator.tool_uses.get_mut(index) {
437                                    // Handle empty buffer - use fallback for tools with no input
438                                    if tool.input_buffer.is_empty() {
439                                        tool.input_buffer = "{}".to_string();
439                                    }
440                                    match serde_json::from_str(&tool.input_buffer) {
441                                        Ok(input) => StreamingUpdate::ToolUseComplete {
442                                            id: tool.id.clone(),
443                                            input,
444                                        },
445                                        Err(e) => StreamingUpdate::Error(format!(
446                                            "Failed to parse tool input: {}",
447                                            e
448                                        )),
449                                    }
450                                } else {
451                                    continue;
452                                }
453                            } else {
454                                continue;
455                            }
456                        }
```

---

## Summary of Changes

| Location | Change | Reason |
|----------|--------|--------|
| Line 110 | Change `..` to capture `input` | Capture initial input from API |
| Lines 112-114 | Initialize buffer with input | Use API's input value |
| Lines 150-153 | Add empty buffer fallback | Handle tools with no input |
| Line 396 | Change `..` to capture `input` | Capture initial input from API |
| Lines 398-403 | Store initial input in accumulator | Initialize tool's input_buffer |
| Lines 437-439 | Add empty buffer fallback | Handle tools with no input |

---

## Testing After Fix

```bash
# 1. Verify compilation
cargo build

# 2. Run streaming-related tests
cargo test streaming

# 3. Run tool tests to ensure no regression
cargo test bash
cargo test read
cargo test write

# 4. Manual TUI test:
#    - In TUI, ask Claude to enter plan mode
#    - Expected: Plan mode activates (not error message)
#    - Check for "PlanModeChanged { enabled: true }" in logs
```

---

## Why This Fix Works

1. **For empty-input tools (EnterPlanMode):**
   - ContentBlockStart has `input: {}`
   - Code captures and serializes to `"{}"` 
   - No InputJsonDelta arrives
   - ContentBlockStop: buffer = `"{}"` 
   - Parse succeeds: `{}`
   - ToolUseComplete sent with correct input

2. **For tools with input (Bash, Read, etc.):**
   - ContentBlockStart has `input: {}`
   - Code initializes buffer to `"{}"`
   - InputJsonDelta events add chunks: `"{"`, `"command": "ls"`
   - ContentBlockStop: buffer = complete JSON
   - Parse succeeds with full input
   - ToolUseComplete sent with correct input

3. **For malformed input:**
   - Fallback to `"{}"` only if buffer is empty
   - Parsing still fails for actual malformed JSON
   - Error event still sent (correct behavior)

---

## Risk Assessment

**Low Risk:**
- Changes are isolated to two code paths
- Existing tests should still pass
- Tools with input properties get same behavior
- Only affects tools with empty input (none tested before)

**Verification Needed:**
- Ensure Bash tool still works (has actual input)
- Ensure Read/Write tools still work
- Test EnterPlanMode specifically

