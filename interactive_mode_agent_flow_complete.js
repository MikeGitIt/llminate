// COMPLETE INTERACTIVE MODE AGENT FLOW - Fully Verified from test-fixed.js
// This file documents the COMPLETE agentic/conversational flow from the JavaScript tool
// Based on thorough analysis of minified code with actual line number references

// ============================================================================
// VERIFIED COMPONENTS WITH LINE REFERENCES
// ============================================================================

/*
CONFIRMED FINDINGS FROM JAVASCRIPT:

1. SSE Event Types (Lines 370577-372019)
   - message_start, message_delta, message_stop
   - content_block_start, content_block_delta, content_block_stop

2. Permission Decision Handling (Lines 398117-398180, 398622-398652)
   - "yes" → Allow this time
   - "yes-dont-ask-again" → Always allow
   - "no" → Wait and tell Claude differently (THE CRITICAL OPTION 3!)
   - "never" → Never allow

3. Interrupt Message Construction (Lines 385298-385303, 385429-385435)
   - Yu = "[Request interrupted by user]"
   - XV = "[Request interrupted by user for tool use]"
   - str179 = "The user doesn't want to proceed with this tool use..."
   - objectBuilder19 creates tool_result with Yu and is_error: true

4. Abort Controller Integration (Lines 395021-395033)
   - When abortController.signal.aborted is true
   - Creates interrupt message via objectBuilder19
   - Sends tool_result with is_error: true

5. Synthesis Pattern (Lines 418924-418925)
   - synthesis_${messageId} for main synthesis
   - agent_${num}_${messageId} for sub-agents

6. Permission Dialog Structure (Line 398069 onward)
   - stringDecoder374 and stringDecoder383 handle permission UI
   - onReject callback triggers when "no" selected
*/

// ============================================================================
// 1. SYSTEM PROMPT (Line 368159)
// ============================================================================

const SYSTEM_PROMPT = `You are Claude Code, Anthropic's official CLI for Claude.
You are an interactive CLI tool that helps users with software engineering tasks.`;

function getSystemPrompt() {
  return {
    prependCLISysprompt: true,
    prompt: SYSTEM_PROMPT
  };
}

// ============================================================================
// 2. STOP REASONS (Line 255507)
// ============================================================================

const StopReasons = {
  CONTENT_FILTERED: "content_filtered",
  END_TURN: "end_turn",
  GUARDRAIL_INTERVENED: "guardrail_intervened",
  MAX_TOKENS: "max_tokens",
  STOP_SEQUENCE: "stop_sequence",
  TOOL_USE: "tool_use"
};

// ============================================================================
// 3. INTERRUPT MESSAGES (Lines 385298-385303)
// ============================================================================

const InterruptMessages = {
  Yu: "[Request interrupted by user]",  // Line 385432
  XV: "[Request interrupted by user for tool use]",
  str179: "The user doesn't want to proceed with this tool use. The tool use was rejected (eg. if it was a file edit, the new_string was NOT written to the file). STOP what you are doing and wait for the user to tell you how to proceed."
};

// ============================================================================
// 4. STREAMING EVENTS (Lines 370577-372019)
// ============================================================================

const StreamingEvents = {
  MESSAGE_START: "message_start",      // Line 370582
  MESSAGE_DELTA: "message_delta",      // Line 370583
  MESSAGE_STOP: "message_stop",        // Line 370584, 371997
  CONTENT_BLOCK_START: "content_block_start",  // Line 370585, 372009
  CONTENT_BLOCK_DELTA: "content_block_delta",  // Line 370586
  CONTENT_BLOCK_STOP: "content_block_stop"     // Line 370587, 372001
};

// Event handler from lines 370582-370597
async function* handleStreamEvents(stream) {
  for await (const chunk of stream) {
    const event = parseSSEEvent(chunk);

    if (event.event === StreamingEvents.MESSAGE_START ||
        event.event === StreamingEvents.MESSAGE_DELTA ||
        event.event === StreamingEvents.MESSAGE_STOP ||
        event.event === StreamingEvents.CONTENT_BLOCK_START ||
        event.event === StreamingEvents.CONTENT_BLOCK_DELTA ||
        event.event === StreamingEvents.CONTENT_BLOCK_STOP) {
      try {
        yield JSON.parse(event.data);  // Line 370590
      } catch (error) {
        console.error("Could not parse message into JSON:", event.data);  // Line 370594
        console.error("From chunk:", event.raw);  // Line 370596
        throw error;
      }
    }
  }
}

// ============================================================================
// 5. TOOL RESULT CREATION (Lines 385429-385435)
// ============================================================================

// Line 385429: objectBuilder19 creates interrupt tool result
function objectBuilder19(toolUseId) {
  return {
    type: "tool_result",           // Line 385431
    content: InterruptMessages.Yu,  // Line 385432 - Uses Yu constant
    is_error: true,                 // Line 385433
    tool_use_id: toolUseId          // Line 385434
  };
}

// ============================================================================
// 6. ABORT CONTROLLER USAGE (Lines 395021-395033)
// ============================================================================

async function* handleToolExecution(toolUse, abortController) {
  // Line 395021: Check if aborted
  if (abortController.signal.aborted) {
    // Line 395022-395026: Log cancellation
    logEvent("tengu_tool_use_cancelled", {
      toolName: toolUse.name,
      toolUseID: toolUse.id,
      isMcp: toolUse.isMcp ?? false
    });

    // Line 395027: Create interrupt result
    let interruptResult = objectBuilder19(toolUse.id);

    // Lines 395028-395032: Yield the interrupt
    yield {
      content: [interruptResult],
      toolUseResult: InterruptMessages.Yu
    };

    return;  // Line 395033
  }

  // Continue with normal tool execution if not aborted
  // Lines 395035-395043
}

// ============================================================================
// 7. PERMISSION DECISION HANDLING (Lines 398117-398652)
// ============================================================================

const PermissionDecisions = {
  YES: "yes",
  YES_DONT_ASK_AGAIN: "yes-dont-ask-again",
  YES_DONT_ASK_AGAIN_PREFIX: "yes-dont-ask-again-prefix",
  NO: "no",  // THE WAIT OPTION!
  NEVER: "never"
};

// Based on lines 398117-398180 (Edit tool) and 398622-398652 (Bash tool)
async function handlePermissionDecision(decision, toolUseConfirm) {
  switch (decision) {
    case PermissionDecisions.YES:  // Line 398127, 398622
      toolUseConfirm.onAllow("temporary", toolUseConfirm.input);
      break;

    case PermissionDecisions.YES_DONT_ASK_AGAIN:  // Line 398145
      addToAllowList(toolUseConfirm.toolName, toolUseConfirm.input);
      toolUseConfirm.onAllow("permanent", toolUseConfirm.input);
      break;

    case PermissionDecisions.NO:  // Lines 398166, 398649 - WAIT OPTION!
      // THIS IS WHERE THE MAGIC HAPPENS!
      // onReject() triggers the abort and sends interrupt message
      toolUseConfirm.onReject();
      // The abort controller will be triggered, causing:
      // 1. Stream cancellation
      // 2. Interrupt message sent to LLM
      // 3. Processing stops completely
      return { shouldAbort: true };

    case PermissionDecisions.NEVER:
      addToDenyList(toolUseConfirm.toolName, toolUseConfirm.input);
      toolUseConfirm.onReject();
      break;
  }

  return { shouldAbort: false };
}

// ============================================================================
// 8. SYNTHESIS PROCESS (Lines 418919-418934)
// ============================================================================

async function* generateSynthesisProgress(message, isMainSynthesis, agentNumber) {
  // Lines 418924-418925: Create synthesis tool use ID
  const toolUseID = isMainSynthesis
    ? `synthesis_${message.id}`
    : `agent_${agentNumber}_${message.id}`;

  yield {
    type: "progress",
    toolUseID: toolUseID,
    data: {
      message: message,
      type: "agent_progress"
    }
  };
}

// ============================================================================
// 9. CONTINUATION PROMPT (Line 393287)
// ============================================================================

function createContinuationPrompt(summary) {
  return `This session is being continued from a previous conversation that ran out of context.
The conversation is summarized below:
${summary}
Please continue the conversation from where we left it off without asking the user any further questions.
Continue with the last task that you were asked to work on.`;
}

// ============================================================================
// 10. COMPLETE AGENTIC FLOW IMPLEMENTATION
// ============================================================================

class CompleteAgenticFlow {
  constructor() {
    this.messages = [];
    this.abortController = null;
    this.wasInterrupted = false;
  }

  async processUserInput(input) {
    // Add system prompt if needed
    if (this.messages.length === 0) {
      const systemPrompt = getSystemPrompt();
      if (systemPrompt.prependCLISysprompt) {
        this.messages.push({
          role: "system",
          content: systemPrompt.prompt
        });
      }
    }

    // Add user message
    this.messages.push({
      role: "user",
      content: input
    });

    // Create abort controller for this conversation
    // Line 36275-36276: AbortController creation
    this.abortController = typeof AbortController !== "undefined"
      ? new AbortController()
      : null;

    // MAIN CONVERSATION LOOP - AUTONOMOUS CONTINUATION
    let iterationCount = 0;
    const MAX_ITERATIONS = 25;

    while (iterationCount < MAX_ITERATIONS) {
      iterationCount++;

      // Check if aborted
      if (this.abortController?.signal.aborted) {
        break;
      }

      // Stream response from AI
      const response = await this.streamResponse(this.messages);

      // Add assistant response
      if (response.content?.length > 0) {
        this.messages.push({
          role: "assistant",
          content: response.content
        });
      }

      // Check stop reason (critical for autonomous behavior!)
      if (response.stop_reason === StopReasons.TOOL_USE) {
        // Execute tools with permission checking
        const toolResults = await this.executeToolsWithPermissions(response.tool_uses);

        // Check if interrupted
        if (this.wasInterrupted) {
          // User chose "Wait and tell Claude differently"
          // Stream has been cancelled, stop everything
          break;
        }

        // Add tool results as USER message (not assistant!)
        if (toolResults.length > 0) {
          this.messages.push({
            role: "user",  // CRITICAL: Tool results go as user messages!
            content: toolResults
          });

          // CONTINUE LOOP - This is the autonomous behavior!
          continue;
        }
      }

      // Check other stop reasons
      if (response.stop_reason === StopReasons.END_TURN ||
          response.stop_reason === StopReasons.MAX_TOKENS) {
        // Task might be complete, or hit token limit
        break;
      }

      // If no explicit stop reason but has tool uses, continue
      if (this.hasMoreWork(response)) {
        continue;
      }

      // Otherwise, we're done
      break;
    }
  }

  async executeToolsWithPermissions(toolUses) {
    const results = [];

    for (const toolUse of toolUses) {
      // Check if already aborted (Line 395021)
      if (this.abortController?.signal.aborted) {
        // Create interrupt result (Line 395027)
        const interruptResult = objectBuilder19(toolUse.id);
        results.push(interruptResult);
        this.wasInterrupted = true;
        break;
      }

      // Check if permission required
      if (this.requiresPermission(toolUse.name)) {
        const decision = await this.showPermissionDialog(toolUse);

        // Handle permission decision (Lines 398117-398652)
        if (decision === PermissionDecisions.NO) {
          // User chose "Wait and tell Claude differently"
          // Trigger abort (this is what onReject() does)
          this.abortController?.abort();

          // Create interrupt result
          const interruptResult = {
            type: "tool_result",
            tool_use_id: toolUse.id,
            content: InterruptMessages.XV + "\n\n" + InterruptMessages.str179,
            is_error: true
          };
          results.push(interruptResult);
          this.wasInterrupted = true;
          break;
        }

        if (decision === PermissionDecisions.NEVER) {
          results.push({
            type: "tool_result",
            tool_use_id: toolUse.id,
            content: `Permission to use ${toolUse.name} has been permanently denied.`,
            is_error: true
          });
          continue;
        }

        // YES or YES_DONT_ASK_AGAIN - continue with execution
      }

      // Execute tool
      try {
        const result = await this.executeTool(toolUse);
        results.push({
          type: "tool_result",
          tool_use_id: toolUse.id,
          content: result,
          is_error: false
        });
      } catch (error) {
        results.push({
          type: "tool_result",
          tool_use_id: toolUse.id,
          content: `Error: ${error.message}`,
          is_error: true
        });
      }
    }

    return results;
  }

  async streamResponse(messages) {
    const response = {
      content: [],
      stop_reason: null,
      tool_uses: []
    };

    // Process streaming events (Lines 370577-372019)
    const stream = await this.createStream(messages);

    for await (const event of handleStreamEvents(stream)) {
      // Check abort status
      if (this.abortController?.signal.aborted) {
        break;
      }

      // Handle events based on type (Lines 371997-372012)
      switch (event.type) {
        case StreamingEvents.MESSAGE_START:
          // Line 372006
          break;

        case StreamingEvents.CONTENT_BLOCK_START:
          // Line 372009
          if (event.content_block?.type === "tool_use") {
            response.tool_uses.push(event.content_block);
          }
          break;

        case StreamingEvents.CONTENT_BLOCK_STOP:
          // Line 372001
          break;

        case StreamingEvents.MESSAGE_STOP:
          // Line 371997
          response.stop_reason = event.stop_reason;
          break;
      }
    }

    return response;
  }

  hasMoreWork(response) {
    // Check if AI indicates more work needed
    const text = response.content?.map(c => c.text || "").join("");
    const continuationPhrases = [
      "let me", "i'll now", "i will now",
      "next, i", "continuing with", "moving on to"
    ];
    return continuationPhrases.some(phrase =>
      text.toLowerCase().includes(phrase)
    );
  }

  requiresPermission(toolName) {
    const permissionRequiredTools = [
      "Edit", "MultiEdit", "Write", "NotebookEdit",
      "Bash", "Delete", "Move", "Copy"
    ];
    return permissionRequiredTools.includes(toolName);
  }

  async showPermissionDialog(toolUse) {
    // This would show the actual dialog
    // Options from permission dialog construction
    const options = [
      { label: "Yes", value: "yes" },
      { label: "Yes, and don't ask again", value: "yes-dont-ask-again" },
      { label: "No, and tell Claude what to do differently", value: "no" },
      { label: "Never allow", value: "never" }
    ];

    // Return user's choice
    return await getUserChoice(options);
  }
}

// ============================================================================
// CRITICAL INSIGHTS FOR RUST IMPLEMENTATION - FULLY VERIFIED
// ============================================================================

/*
VERIFIED CRITICAL REQUIREMENTS:

1. AUTONOMOUS CONVERSATION LOOP
   - Loop continues while stop_reason === "tool_use"
   - Multiple rounds of tool execution until task complete
   - Not just one tool and stop!

2. PERMISSION "NO" (OPTION 3) = WAIT
   - Line 398649: case "no" calls onReject()
   - onReject() triggers abort controller
   - Line 395021-395033: Abort creates interrupt message
   - Stream cancellation stops ALL processing

3. ABORT CONTROLLER INTEGRATION
   - Line 36275-36276: Created for each conversation
   - Line 395021: Checked before tool execution
   - Line 395027: Creates interrupt via objectBuilder19
   - Cancels stream and stops loop

4. MESSAGE ROLES
   - System prompts: role = "system"
   - User input: role = "user"
   - Tool results: role = "user" (NOT assistant!)
   - AI responses: role = "assistant"

5. TOOL RESULT FORMAT
   - type: "tool_result"
   - tool_use_id: matches tool use
   - content: result or error message
   - is_error: true for errors/interrupts

6. STREAMING EVENTS (Lines 370577-372019)
   - Six event types must be handled
   - Parse JSON data from events
   - Build response incrementally

7. SYNTHESIS (Lines 418924-418925)
   - synthesis_${messageId} for main
   - agent_${num}_${messageId} for sub-agents
   - Happens after tool execution

8. CONTINUATION LOGIC
   - Check stop_reason for "tool_use"
   - Check response text for continuation phrases
   - Loop until task complete or interrupted

9. INTERRUPT MESSAGE FORMAT
   - Yu + str179 for full interrupt
   - XV for tool-specific interrupt
   - is_error: true required

10. PERMISSION FLOW
    - Show dialog with 4 options
    - Option 3 ("no") = Wait and interrupt
    - Abort controller cancels everything
    - No further tools executed after interrupt

RUST IMPLEMENTATION MUST:
[ ] Implement autonomous loop with stop_reason checking
[ ] Create AbortController equivalent for stream cancellation
[ ] Send tool results as USER messages
[ ] Handle all 6 streaming event types
[ ] Implement proper Wait option with stream cancellation
[ ] Use correct interrupt message format
[ ] Support synthesis after tool execution
[ ] Check for continuation needs
[ ] Stop immediately on abort
[ ] Format tool results with is_error field
*/

module.exports = {
  CompleteAgenticFlow,
  StopReasons,
  StreamingEvents,
  PermissionDecisions,
  InterruptMessages,
  objectBuilder19,
  handlePermissionDecision,
  handleStreamEvents,
  createContinuationPrompt,
  generateSynthesisProgress
};