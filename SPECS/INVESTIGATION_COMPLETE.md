# FULL STREAMING PIPELINE INVESTIGATION - COMPLETE FINDINGS

## PROBLEM STATEMENT
EnterPlanMode tool cannot be executed because it receives an empty input buffer that fails JSON parsing.

## ROOT CAUSE IDENTIFIED
The streaming pipeline has a critical bug in handling tools with **empty input schemas** (no properties).

### The Issue in Detail

**Anthropic API Behavior for Empty Input Tools:**
- When a tool has `properties: {}` in its schema, the API sends:
  1. `content_block_start` with `ToolUse { id, name, input: {} }`
  2. NO `input_json_delta` events (because input is already complete as `{}`)
  3. `content_block_stop`

**The Bug in streaming.rs:**
Both code paths (StreamingHandler and StreamProcessor) have identical bugs:

```rust
// LINE 110 (StreamingHandler::process_stream) or 396 (StreamProcessor::process)
ContentBlock::ToolUse { id, name, .. } => {
    // ↑ This ".." ignores the initial input field!
    current_tool_id = Some(id.clone());
    tool_input_buffer.clear();  // ← Clears to empty string ""
    // ...
}

// LINE 150 (StreamingHandler) or 437 (StreamProcessor)
match serde_json::from_str(&tool_input_buffer) {
    // ↑ tool_input_buffer is "", so this tries to parse empty string
    // ↓ This FAILS with: "EOF while parsing"
    Ok(input) => { /* send ToolUseComplete */ }
    Err(e) => {
        // ↓ WRONG PATH - sends Error instead of completion!
        let _ = tx.send(StreamingUpdate::Error(format!(
            "Failed to parse tool input: {}",  // ← This is what users see
            e
        )));
    }
}
```

### Impact on EnterPlanMode
1. EnterPlanMode has schema: `{ "type": "object", "properties": {}, "additionalProperties": false }`
2. API sends `input: {}`
3. Code ignores initial `input`, clears buffer to `""`
4. No InputJsonDelta arrives (input already complete as `{}`)
5. At ContentBlockStop, tries to parse `""`
6. Gets error: `"Failed to parse tool input: EOF while parsing"`
7. Sends `StreamingUpdate::Error` instead of `ToolUseComplete`
8. Tool handler never executes
9. Plan mode never enters

### Impact on Other Empty-Input Tools
Any tool with no required/optional input properties will have the same issue:
- EnterPlanMode (confirmed)
- Other tools with empty input schemas (if any)

### Example: How Bash Tool Works (For Comparison)
The Bash tool HAS input properties (command, timeout, etc.), so:
1. API sends `input: {}` (initial empty)
2. API sends multiple `input_json_delta` events with JSON chunks
3. Code accumulates chunks into `tool_input_buffer`
4. By ContentBlockStop, `tool_input_buffer = '{"command":"...", ...}'`
5. Successfully parses to JSON object
6. Sends `ToolUseComplete` with full input

---

## AFFECTED CODE LOCATIONS

### File 1: src/ai/streaming.rs

**StreamingHandler::process_stream (Lines 66-218)**
- Line 105-114: ContentBlockStart handling
  ```rust
  ContentBlock::ToolUse { id, name, .. } => {  // ← Ignores input
      current_tool_id = Some(id.clone());
      tool_input_buffer.clear();  // ← Bug here
      let _ = tx.send(StreamingUpdate::ToolUseStart { id, name });
  }
  ```
- Line 148-166: ContentBlockStop handling
  ```rust
  if let Some(id) = current_tool_id.take() {
      match serde_json::from_str(&tool_input_buffer) {  // ← Fails on empty buffer
          Ok(input) => { /* ... */ }
          Err(e) => {
              let _ = tx.send(StreamingUpdate::Error(format!(
                  "Failed to parse tool input: {}",  // ← User sees this
                  e
              )));
          }
      }
  }
  ```

**StreamProcessor::process (Lines 378-497)**
- Line 393-409: ContentBlockStart handling
  ```rust
  ContentBlock::ToolUse { id, name, .. } => {  // ← Ignores input
      StreamingUpdate::ToolUseStart { id, name }
  }
  ```
- Line 434-452: ContentBlockStop handling
  ```rust
  if let Some(index) = accumulator.current_tool_index {
      if let Some(tool) = accumulator.tool_uses.get_mut(index) {
          match serde_json::from_str(&tool.input_buffer) {  // ← Fails on empty
              Ok(input) => { /* ... */ }
              Err(e) => {
                  StreamingUpdate::Error(format!(
                      "Failed to parse tool input: {}",
                      e
                  ))
              }
          }
      }
  }
  ```

---

## VERIFICATION DATA

### src/ai/client.rs - ContentBlock Structure
```rust
#[serde(rename = "tool_use")]
ToolUse {
    id: String,
    name: String,
    input: serde_json::Value,  // ← This is the field being ignored!
},
```

The `input` field carries the complete or partially-accumulated input JSON.

### src/ai/enter_plan_mode_tool.rs - Tool Schema
```rust
fn input_schema(&self) -> Value {
    json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": {},  // ← No properties = empty input
        "additionalProperties": false
    })
}
```

### src/ai/tools.rs - Tool Registration
```rust
tools.insert("EnterPlanMode".to_string(), Box::new(EnterPlanModeTool));
```

EnterPlanMode IS registered and ready to execute - it just never gets the chance because the streaming pipeline fails.

### src/tui/state.rs - Tool Execution
```rust
match tool_executor.execute_with_context(&tool_name, input.clone(), Some(tool_context)).await {
    Ok(result) => {
        if tool_name == "EnterPlanMode" {
            // Signal plan mode enabled
            if let Some(tx) = &event_tx {
                let _ = tx.send(crate::tui::TuiEvent::PlanModeChanged { enabled: true });
            }
        }
        // ...
    }
}
```

This code EXISTS to handle EnterPlanMode, but it never executes because the streaming error prevents ToolUseComplete.

---

## ERROR VISIBILITY

### How Users See the Error
1. User types in chat and mentions plan mode
2. Claude Code API decides to use EnterPlanMode tool
3. Streaming starts, error occurs
4. At line 1775-1779 of state.rs:
   ```rust
   StreamingUpdate::Error(e) => {
       if let Some(tx) = &event_tx_inner {
           let _ = tx.send(crate::tui::TuiEvent::Error(e));
       }
       break;
   }
   ```
5. Error is sent to TUI and displayed to user
6. User sees: `"Failed to parse tool input: EOF while parsing"`

---

## THE COMPLETE FIX

### Solution Overview
Both code paths need to:
1. **Capture** the initial `input` value from `ContentBlockStart`
2. **Initialize** the input buffer with the initial value
3. **Accumulate** InputJsonDelta chunks (if any)
4. **Parse** the final accumulated input at ContentBlockStop

### For Empty Input Case
When API sends `input: {}`:
- Initialize buffer with `"{}"` (or capture the Value and serialize)
- No InputJsonDelta arrives
- ContentBlockStop immediately tries to parse `"{}"` → Success!

### For Tools with Input Properties
When API sends `input: {}` then chunks:
- Initialize buffer with `"{}"`
- Accumulate chunk deltas: `"{` → `{"command": "ls"`
- Final buffer: `{"command": "ls"}`
- Parse succeeds

### Specific Changes Needed

**In StreamingHandler::process_stream (lines 105-114):**
```rust
ContentBlock::ToolUse { id, name, input } => {  // ← Capture input
    current_tool_id = Some(id.clone());
    // Initialize buffer with serialized input (or just "{}" for empty)
    tool_input_buffer = serde_json::to_string(&input)
        .unwrap_or_else(|_| "{}".to_string());
    let _ = tx.send(StreamingUpdate::ToolUseStart { id, name });
}
```

**In StreamingHandler::process_stream (lines 148-166):**
```rust
if let Some(id) = current_tool_id.take() {
    // Handle empty or populated buffer
    if tool_input_buffer.is_empty() {
        tool_input_buffer = "{}".to_string();
    }
    match serde_json::from_str(&tool_input_buffer) {
        Ok(input) => {
            let _ = tx.send(StreamingUpdate::ToolUseComplete { id, input });
        }
        Err(e) => {
            let _ = tx.send(StreamingUpdate::Error(format!(
                "Failed to parse tool input: {}",
                e
            )));
        }
    }
    tool_input_buffer.clear();
}
```

**Same changes for StreamProcessor::process (lines 393-452):**
- Capture `input` in ContentBlockStart
- Initialize `tool.input_buffer` with serialized input
- Handle empty buffer at ContentBlockStop

---

## SUMMARY TABLE

| Component | Issue | Location | Impact |
|-----------|-------|----------|--------|
| StreamingHandler::process_stream | Ignores initial input, clears to "" | Line 110-112 | Empty input tools fail |
| StreamingHandler::process_stream | Parses empty string "" | Line 150 | JSON parse error |
| StreamProcessor::process | Ignores initial input | Line 396 | Empty input tools fail |
| StreamProcessor::process | Parses empty buffer | Line 437 | JSON parse error |
| ContentBlockStart (client.rs) | Has unused `input: Value` field | Line 497 | Field not extracted |
| EnterPlanMode tool schema | Empty properties object | enter_plan_mode_tool.rs | Triggers the bug |

---

## NEXT STEPS (FOR THE USER)

1. **Modify src/ai/streaming.rs - StreamingHandler::process_stream:**
   - Change line 110: Capture `input` instead of ignoring with `..`
   - Change line 112: Initialize buffer with serialized input
   - Change line 150: Add fallback to "{}" if buffer empty

2. **Modify src/ai/streaming.rs - StreamProcessor::process:**
   - Change line 396: Capture `input` instead of ignoring with `..`
   - Change accumulator to store initial input value
   - Change line 437: Add fallback to initial input if buffer empty

3. **Test:**
   - Run tool tests to ensure Bash tool still works
   - Run EnterPlanMode in TUI and verify plan mode activates
   - Check other empty-input tools (if any)

---

## WHY THIS WASN'T OBVIOUS

1. **Two identical code paths** - Same bug in two places, easy to miss
2. **Pattern matching with `..`** - Silently ignores the input field
3. **Only affects empty-input tools** - Most tools have input properties
4. **Error message is generic** - "EOF while parsing" doesn't immediately suggest empty input
5. **No comments** - Code flow wasn't documented
6. **Tool registration works** - Confuses debugging ("tool is registered but doesn't execute")

