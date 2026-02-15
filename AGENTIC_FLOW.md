# Agentic Flow Implementation Requirements

## JavaScript Discovery Notes

### Search Commands Used
```bash
# Find prompts in minified JS
for file in $(find ~/Code/rust_projects/paragen/output/ -type f -name "test-fixed.js");do gawk 'match($0,/[P-p]rompt:.*/,arr) {print substr(RSTART,RLENGTH) $0}' $file;done

# Find tool_use patterns
gawk '/tool_use/{print NR": "substr($0,match($0,/tool_use/)-50,150)}' test-fixed.js

# Find stop reasons
gawk '/stop_reason|stopReason|stop_sequence/{print NR": "substr($0,match($0,/stop_reason|stopReason|stop_sequence/)-50,150)}' test-fixed.js

# Find synthesis patterns
gawk '/synthesis|synthesize/{print NR": "substr($0,match($0,/synthesis|synthesize/)-100,250)}' test-fixed.js
```

### Key Findings

#### 1. System Prompt Construction (Line 368159)
```javascript
function func224() {
  return `You are ${str128}, Anthropic's official CLI for Claude.`;
}
```
- Full system prompt starts at line 368168
- Gets prepended when `prependCLISysprompt: true`
- Includes detailed instructions for autonomous behavior

#### 2. Stop Reasons (Line 255507)
```javascript
obj529 = {
  CONTENT_FILTERED: "content_filtered",
  END_TURN: "end_turn",
  GUARDRAIL_INTERVENED: "guardrail_intervened", 
  MAX_TOKENS: "max_tokens",
  STOP_SEQUENCE: "stop_sequence",
  TOOL_USE: "tool_use",
}
```

#### 3. Tool Execution Decision (Line 385545)
```javascript
function stringDecoder258(input20325) {
  return (
    input20325.type === "assistant" &&
    input20325.message.content.some(
      (config8199) => config8199.type === "tool_use",
    )
  );
}
```

#### 4. Synthesis Process (Line 418924)
- Synthesis happens after agent execution
- Tool use IDs like `synthesis_${messageId}` or `agent_${num}_${messageId}`
- Multiple agents can be orchestrated

#### 5. Continuation Logic (Line 393287)
```javascript
let input20347 = `This session is being continued from a previous conversation that ran out of context. The conversation is summarized below:
${summary}
Please continue the conversation from where we left it off without asking the user any further questions. Continue with the last task that you were asked to work on.`;
```

## Required Implementation Changes

### 1. Enhanced Conversation Loop
- ✅ Check `stop_reason` to determine continuation
- ❌ Add synthesis after tool execution
- ❌ Support multiple rounds of tool execution
- ❌ Implement proper error handling with continuation

### 2. System Prompt Management
- ✅ Basic system prompt from JavaScript
- ❌ Prepend system prompt to all conversations
- ❌ Context-aware prompt modifications
- ❌ Tool-specific instructions

### 3. Tool Execution Flow
```
User Input
    ↓
AI Analysis (with system prompt)
    ↓
Tool Selection (based on stop_reason: tool_use)
    ↓
Tool Execution
    ↓
Tool Results → Back to AI
    ↓
AI Synthesis
    ↓
Check if more work needed
    ↓
Either: Continue loop OR Present final response
```

### 4. Agent Orchestration
- ❌ Sub-agent support (Task tool)
- ❌ Agent progress tracking
- ❌ Synthesis of multiple agent results
- ❌ Agent-specific tool permissions

### 5. Conversation State
- ❌ Track conversation context
- ❌ Handle context overflow with summarization
- ❌ Resume with continuation prompt
- ❌ Maintain task state across sessions

## Implementation Priority

1. **P0 - Critical**: Fix conversation loop to properly check stop_reason
2. **P0 - Critical**: Ensure system prompt is always prepended
3. **P0 - Critical**: Implement proper tool → AI → synthesis flow
4. **P1 - Important**: Add continuation logic for long conversations
5. **P1 - Important**: Implement sub-agent orchestration
6. **P2 - Enhancement**: Add sophisticated prompt management

## Code Locations

### JavaScript
- System prompt: Line 368159-368180
- Stop reasons: Line 255507-255512
- Tool execution check: Line 385545-385550
- Synthesis: Line 418920-418950
- Continuation: Line 393287-393291

### Rust (Needs Update)
- Conversation loop: `src/tui/state.rs:316-442`
- System prompt: `src/ai/system_prompt.rs`
- Stop reason enum: `src/ai/mod.rs:254-259`
- Tool execution: `src/ai/tools.rs`

## Testing Checklist

- [ ] Agent autonomously executes multiple tools for complex tasks
- [ ] Agent synthesizes results after tool execution
- [ ] Agent continues until task is complete (not just one tool)
- [ ] Sub-agents can be spawned for complex tasks
- [ ] Conversation continues seamlessly after context overflow
- [ ] System behaves identically to JavaScript version