// INTERACTIVE MODE AGENT FLOW - Complete Extraction from test-fixed.js
// This file documents the complete agentic/conversational flow from the JavaScript tool
// Based on analysis of minified code, AGENTIC_FLOW.md, and deep code inspection

// ============================================================================
// CORE FLOW ARCHITECTURE
// ============================================================================

/*
Main Conversation Loop:
1. User Input → System Prompt Injection → AI Processing
2. AI Response with stop_reason check
3. If stop_reason === "tool_use" → Execute Tools → Collect Results
4. Send Tool Results back to AI → AI Synthesis
5. Check if more work needed → Loop or Complete

CRITICAL DISCOVERY: The conversation must continue AUTONOMOUSLY until the task
is complete, not just execute one tool and stop. The AI should keep working
until it has fully addressed the user's request.
*/

// ============================================================================
// 1. SYSTEM PROMPT CONSTRUCTION (Line 368159)
// ============================================================================

const SYSTEM_PROMPT = `You are Claude Code, Anthropic's official CLI for Claude.
You are an interactive CLI tool that helps users with software engineering tasks.`;

function getSystemPrompt() {
  // Line 368159-368180 in test-fixed.js
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
  TOOL_USE: "tool_use"  // <-- CRITICAL: Triggers tool execution
};

// ============================================================================
// 3. PERMISSION CONSTANTS (Line 385298-385303)
// ============================================================================

const InterruptMessages = {
  USER_INTERRUPT: "[Request interrupted by user]",  // Du
  TOOL_INTERRUPT: "[Request interrupted by user for tool use]",  // XV
  ACTION_INTERRUPT: "The user doesn't want to take this action right now. STOP what you are doing and wait for the user to tell you how to proceed.",  // Yu
  TOOL_REJECT: "The user doesn't want to proceed with this tool use. The tool use was rejected (eg. if it was a file edit, the new_string was NOT written to the file). STOP what you are doing and wait for the user to tell you how to proceed."  // str179
};

// ============================================================================
// 4. STREAMING EVENT HANDLING (Lines 370577-372019)
// ============================================================================

const StreamingEvents = {
  MESSAGE_START: "message_start",
  MESSAGE_DELTA: "message_delta",
  MESSAGE_STOP: "message_stop",
  CONTENT_BLOCK_START: "content_block_start",
  CONTENT_BLOCK_DELTA: "content_block_delta",
  CONTENT_BLOCK_STOP: "content_block_stop"
};

async function* handleStream(stream) {
  // Lines 370577-372019: Process SSE events
  for await (const chunk of stream) {
    const event = parseSSEEvent(chunk);

    // Lines 370582-370587: Filter relevant events
    if (event.event === StreamingEvents.MESSAGE_START ||
        event.event === StreamingEvents.MESSAGE_DELTA ||
        event.event === StreamingEvents.MESSAGE_STOP ||
        event.event === StreamingEvents.CONTENT_BLOCK_START ||
        event.event === StreamingEvents.CONTENT_BLOCK_DELTA ||
        event.event === StreamingEvents.CONTENT_BLOCK_STOP) {

      try {
        yield JSON.parse(event.data);
      } catch (error) {
        console.error("Could not parse message into JSON:", event.data);
        console.error("From chunk:", event.raw);
        throw error;
      }
    }
  }
}

// ============================================================================
// 5. PERMISSION DECISION HANDLING (Lines 398117-398180, 398622-398652)
// ============================================================================

const PermissionDecisions = {
  YES: "yes",  // Allow this time
  YES_DONT_ASK_AGAIN: "yes-dont-ask-again",  // Always allow
  YES_DONT_ASK_AGAIN_PREFIX: "yes-dont-ask-again-prefix",  // Allow with prefix rule
  NO: "no",  // Wait and tell Claude differently
  NEVER: "never"  // Never allow
};

async function handlePermissionDecision(decision, toolUseConfirm, callbacks) {
  // Based on lines 398117-398180 (Edit tool) and 398622-398652 (Bash tool)
  switch (decision) {
    case PermissionDecisions.YES:
      // Line 398622-398625
      callbacks.onAllow("temporary", toolUseConfirm.input);
      callbacks.onDone();
      break;

    case PermissionDecisions.YES_DONT_ASK_AGAIN:
      // Lines 398145-398164
      // Store permission permanently
      addToAllowList(toolUseConfirm.toolName, toolUseConfirm.input);
      callbacks.onAllow("permanent", toolUseConfirm.input);
      callbacks.onDone();
      break;

    case PermissionDecisions.NO:
      // Lines 398166-398178 - THIS IS THE WAIT OPTION
      // User wants to provide feedback
      callbacks.onReject();  // This sends the interrupt message
      callbacks.onDone();
      // CRITICAL: The stream must be cancelled here
      return { interrupt: true };

    case PermissionDecisions.NEVER:
      // Add to permanent deny list
      addToDenyList(toolUseConfirm.toolName, toolUseConfirm.input);
      callbacks.onReject();
      callbacks.onDone();
      break;
  }
  return { interrupt: false };
}

// ============================================================================
// 6. TOOL RESULT CONSTRUCTION (Lines 385429-385435, 394386-394401)
// ============================================================================

function createToolResult(toolUseId, content, isError = false) {
  // Lines 385429-385435: Error result format
  // Lines 394386-394401: Success result format
  return {
    type: "tool_result",
    tool_use_id: toolUseId,
    content: content,
    is_error: isError  // Critical: Must be true for interrupts/errors
  };
}

function createInterruptResult(toolUseId) {
  // Line 385432: Uses Yu (ACTION_INTERRUPT) for content
  // Line 385433: Sets is_error: true
  return {
    type: "tool_result",
    tool_use_id: toolUseId,
    content: InterruptMessages.TOOL_INTERRUPT + "\n\n" + InterruptMessages.TOOL_REJECT,
    is_error: true
  };
}

// ============================================================================
// 7. TOOL EXECUTION DECISION (Line 385545)
// ============================================================================

function hasToolUse(message) {
  // Line 385545-385550 in test-fixed.js
  return (
    message.type === "assistant" &&
    message.content.some(part => part.type === "tool_use")
  );
}

// ============================================================================
// 8. CONVERSATION CONTINUATION (Line 393287)
// ============================================================================

function createContinuationPrompt(summary) {
  // Lines 393287-393291
  return `This session is being continued from a previous conversation that ran out of context.
The conversation is summarized below:
${summary}
Please continue the conversation from where we left it off without asking the user any further questions.
Continue with the last task that you were asked to work on.`;
}

// ============================================================================
// 9. SYNTHESIS PROCESS (Line 418924)
// ============================================================================

function createSynthesisToolUse(agentResults, messageId) {
  // After agents/tools complete, AI synthesizes results
  return {
    type: "tool_use",
    id: `synthesis_${messageId}`,  // or `agent_${num}_${messageId}`
    name: "synthesis",
    input: {
      results: agentResults,
      instruction: "Synthesize the results and provide a coherent response"
    }
  };
}

// ============================================================================
// 10. MAIN CONVERSATION LOOP IMPLEMENTATION
// ============================================================================

class AgenticConversationFlow {
  constructor() {
    this.messages = [];
    this.toolExecutor = new ToolExecutor();
    this.permissionManager = new PermissionManager();
    this.streamCanceller = null;
    this.wasInterrupted = false;
  }

  async processUserInput(input) {
    // Add system prompt if first message
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

    // Main conversation loop - continues autonomously
    let iterationCount = 0;
    const MAX_ITERATIONS = 25;  // Prevent infinite loops

    while (iterationCount < MAX_ITERATIONS) {
      iterationCount++;
      this.wasInterrupted = false;

      // Create stream with cancellation support
      const { stream, canceller } = await this.createCancellableStream(this.messages);
      this.streamCanceller = canceller;

      // Process streaming response
      const response = await this.processStream(stream);

      // Clear stream canceller
      this.streamCanceller = null;

      // Check if we were interrupted
      if (this.wasInterrupted) {
        // User interrupted with "Wait and tell Claude differently"
        // Stop processing and wait for new user input
        break;
      }

      // Add assistant message to conversation
      if (response.content && response.content.length > 0) {
        this.messages.push({
          role: "assistant",
          content: response.content
        });
      }

      // Check stop reason
      if (response.stop_reason === StopReasons.TOOL_USE) {
        // Execute tools with permission checking
        const toolResults = await this.executeToolsWithPermissions(response.tool_uses);

        // Check if user interrupted (Wait decision)
        if (this.checkForInterrupt(toolResults)) {
          // Add interrupt message to conversation
          this.messages.push({
            role: "user",
            content: toolResults
          });
          this.wasInterrupted = true;
          break;  // Stop processing
        }

        // Add tool results as user message for AI to process
        if (toolResults.length > 0) {
          this.messages.push({
            role: "user",
            content: toolResults
          });
          // Continue loop for AI to synthesize results
          continue;
        }
      }

      // Check if AI indicates more work is needed
      if (this.needsContinuation(response)) {
        continue;
      }

      // Task complete
      break;
    }
  }

  async processStream(stream) {
    const response = {
      content: [],
      stop_reason: null,
      tool_uses: []
    };

    let currentText = "";
    let currentToolUse = null;

    for await (const event of handleStream(stream)) {
      // Check if we should cancel
      if (this.wasInterrupted) {
        break;
      }

      switch (event.type) {
        case StreamingEvents.MESSAGE_START:
          // Initialize message
          break;

        case StreamingEvents.CONTENT_BLOCK_START:
          if (event.content_block?.type === "tool_use") {
            currentToolUse = {
              id: event.content_block.id,
              name: event.content_block.name,
              input: {}
            };
          }
          break;

        case StreamingEvents.CONTENT_BLOCK_DELTA:
          if (event.delta?.type === "text_delta") {
            currentText += event.delta.text;
          } else if (event.delta?.type === "input_json_delta") {
            // Accumulate tool input
            if (currentToolUse) {
              Object.assign(currentToolUse.input, JSON.parse(event.delta.partial_json));
            }
          }
          break;

        case StreamingEvents.CONTENT_BLOCK_STOP:
          if (currentToolUse) {
            response.tool_uses.push(currentToolUse);
            currentToolUse = null;
          }
          break;

        case StreamingEvents.MESSAGE_STOP:
          response.stop_reason = event.stop_reason;
          if (currentText) {
            response.content.push({
              type: "text",
              text: currentText
            });
          }
          break;
      }
    }

    return response;
  }

  async executeToolsWithPermissions(toolUses) {
    const results = [];

    for (const toolUse of toolUses) {
      // Check if tool requires permission
      if (this.requiresPermission(toolUse.name)) {
        const decision = await this.permissionManager.requestPermission(
          toolUse.name,
          toolUse.input
        );

        if (decision === PermissionDecisions.NO) {
          // User chose "Wait and tell Claude differently"
          const interruptResult = createInterruptResult(toolUse.id);
          results.push(interruptResult);

          // CRITICAL: Cancel the stream immediately
          if (this.streamCanceller) {
            this.streamCanceller.cancel();
          }

          // Set flag to stop processing
          this.wasInterrupted = true;

          // Don't process any more tools
          break;
        }

        if (decision === PermissionDecisions.NEVER) {
          // Never allow this tool
          results.push(createToolResult(
            toolUse.id,
            `Permission to use ${toolUse.name} has been permanently denied.`,
            true
          ));
          continue;
        }

        // Handle YES and YES_DONT_ASK_AGAIN
        if (decision === PermissionDecisions.YES_DONT_ASK_AGAIN) {
          this.permissionManager.addToAllowList(toolUse.name, toolUse.input);
        }
      }

      // Execute the tool
      try {
        const result = await this.toolExecutor.execute(toolUse);
        results.push(createToolResult(toolUse.id, result, false));
      } catch (error) {
        results.push(createToolResult(
          toolUse.id,
          `Error executing ${toolUse.name}: ${error.message}`,
          true
        ));
      }
    }

    return results;
  }

  checkForInterrupt(toolResults) {
    // Check if any result contains the interrupt message
    return toolResults.some(result =>
      result.content?.includes(InterruptMessages.TOOL_INTERRUPT)
    );
  }

  needsContinuation(response) {
    // AI indicates it needs to continue if:
    // 1. Stop reason is "tool_use" (handled separately)
    // 2. Response indicates incomplete task
    // 3. Response asks to continue
    return response.stop_reason === StopReasons.TOOL_USE ||
           this.checkResponseForContinuation(response.content);
  }

  checkResponseForContinuation(content) {
    // Check if AI's response indicates more work is needed
    // This would check for phrases like "Let me...", "I'll now...", etc.
    if (!content || content.length === 0) return false;

    const text = content.map(c => c.text || "").join("");
    const continuationPhrases = [
      "let me",
      "i'll now",
      "i will now",
      "next, i",
      "continuing with",
      "moving on to"
    ];

    const lowerText = text.toLowerCase();
    return continuationPhrases.some(phrase => lowerText.includes(phrase));
  }

  requiresPermission(toolName) {
    // Tools that require permission
    const permissionRequiredTools = [
      "Edit", "MultiEdit", "Write", "NotebookEdit",
      "Bash", "Delete", "Move", "Copy"
    ];
    return permissionRequiredTools.includes(toolName);
  }

  getPermissionDenialMessage(toolUse) {
    // Generate descriptive denial messages
    const { name, input } = toolUse;

    if (name === "Edit" || name === "MultiEdit" || name === "Write") {
      const filePath = input.file_path || input.notebook_path || "<unknown file>";
      return `Permission to edit ${filePath} has been denied.`;
    }

    if (name === "Bash") {
      const command = input.command || "<unknown command>";
      return `Permission to use Bash with command '${command}' has been denied.`;
    }

    if (name === "Read") {
      const filePath = input.file_path || "<unknown file>";
      return `Permission to read ${filePath} has been denied.`;
    }

    return `Permission to use ${name} has been denied.`;
  }
}

// ============================================================================
// CRITICAL IMPLEMENTATION NOTES FOR RUST
// ============================================================================

/*
1. CONVERSATION LOOP MUST BE AUTONOMOUS
   - Continue until task is complete, not just one tool execution
   - Check stop_reason to determine if more work is needed
   - Allow multiple rounds of tool execution and synthesis

2. PERMISSION SYSTEM IS COMPLEX
   - "No" option (Option 3) means "Wait and tell Claude differently"
   - This sends an interrupt message and STOPS all processing
   - Stream must be cancelled immediately when Wait is chosen
   - No further tools should be executed after interrupt

3. MESSAGE FLOW
   - System prompt prepended to conversation
   - User messages contain user input and tool results
   - Assistant messages contain AI responses and tool uses
   - Tool results are sent as USER messages, not assistant

4. STREAMING HANDLING
   - Must handle all 6 event types properly
   - Accumulate tool uses during streaming
   - Execute tools only after streaming completes
   - Support stream cancellation for interrupts

5. ERROR HANDLING
   - Tool errors have is_error: true
   - Permission denials have is_error: true
   - Interrupts have is_error: true with specific message

6. SYNTHESIS
   - After tools execute, AI synthesizes results
   - May spawn sub-agents for complex tasks
   - Tracks progress with synthesis IDs

7. CONTINUATION
   - Handle context overflow with summarization
   - Resume conversations with continuation prompt
   - Maintain task state across sessions

8. BACKGROUND SHELLS
   - Need BashOutput tool to read output
   - Need KillShell tool to terminate
   - Track shell IDs for management

IMPLEMENTATION CHECKLIST:
[ ] Autonomous conversation loop
[ ] Proper stop_reason checking
[ ] Complete permission system with Wait
[ ] Stream cancellation on interrupt
[ ] Tool result formatting with is_error
[ ] Message role management
[ ] Synthesis after tool execution
[ ] Context overflow handling
[ ] Background shell management
[ ] Descriptive error messages
*/

module.exports = {
  StopReasons,
  StreamingEvents,
  PermissionDecisions,
  InterruptMessages,
  AgenticConversationFlow,
  createToolResult,
  createInterruptResult,
  createContinuationPrompt,
  createSynthesisToolUse,
  getSystemPrompt,
  handlePermissionDecision
};