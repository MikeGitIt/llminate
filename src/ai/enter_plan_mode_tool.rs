//! EnterPlanMode tool implementation
//!
//! This tool allows Claude to request permission to enter plan mode for complex tasks
//! requiring exploration and design.
//!
//! Matches JavaScript implementation from cli-jsdef-fixed.js (around line 443203)

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::ai::tools::ToolHandler;
use crate::error::{Error, Result};

/// Output for EnterPlanMode tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnterPlanModeOutput {
    /// Confirmation that plan mode was entered
    pub message: String,
}

/// EnterPlanMode tool - matches JavaScript implementation
///
/// Requests permission to enter plan mode for complex tasks requiring exploration and design.
///
/// In plan mode, the assistant should:
/// 1. Thoroughly explore the codebase to understand existing patterns
/// 2. Identify similar features and architectural approaches
/// 3. Consider multiple approaches and their trade-offs
/// 4. Use AskUserQuestion if clarification on approach is needed
/// 5. Design a concrete implementation strategy
/// 6. When ready, use ExitPlanMode to present the plan for approval
///
/// IMPORTANT: The assistant should NOT write or edit any files while in plan mode.
/// This is a read-only exploration and planning phase.
pub struct EnterPlanModeTool;

/// The prompt description for EnterPlanMode tool (matches JavaScript exactly)
const ENTER_PLAN_MODE_PROMPT: &str = r#"Use this tool to request permission to enter plan mode for complex tasks requiring exploration and design.

## When to Use This Tool
Use this tool when you encounter a complex task that would benefit from a planning phase before implementation. This includes:
- Large refactoring efforts
- Architecture decisions
- Multi-file changes
- Complex feature implementations

## What Happens in Plan Mode
In plan mode, you should:
1. Thoroughly explore the codebase to understand existing patterns
2. Identify similar features and architectural approaches
3. Consider multiple approaches and their trade-offs
4. Use AskUserQuestion if you need to clarify the approach
5. Design a concrete implementation strategy
6. When ready, use ExitPlanMode to present your plan for approval

Remember: DO NOT write or edit any files yet. This is a read-only exploration and planning phase.

## Handling Ambiguity
Before proceeding with implementation after plan mode:
1. Use the AskUserQuestion tool to clarify with the user
2. Ask about specific implementation choices (e.g., architectural patterns, which library to use)
3. Clarify any assumptions that could affect the implementation
4. Only proceed with implementation after resolving ambiguities"#;

#[async_trait]
impl ToolHandler for EnterPlanModeTool {
    fn description(&self) -> String {
        "Requests permission to enter plan mode for complex tasks requiring exploration and design".to_string()
    }

    fn input_schema(&self) -> Value {
        // JavaScript schema: strictObject with no properties
        json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })
    }

    fn action_description(&self, _input: &Value) -> String {
        "Enter plan mode".to_string()
    }

    fn permission_details(&self, _input: &Value) -> String {
        "Enter plan mode?".to_string()
    }

    async fn execute(&self, _input: Value, _cancellation_token: Option<CancellationToken>) -> Result<String> {
        // Matching JavaScript behavior: return confirmation message
        // The actual mode switching is handled by the UI/state management
        let output = EnterPlanModeOutput {
            message: "Entered plan mode. You should now focus on exploring the codebase and designing an implementation approach.".to_string(),
        };

        let result = serde_json::to_string(&output)
            .map_err(|e| Error::Serialization(e))?;

        Ok(result)
    }
}

/// Format the tool result for the model (matches JavaScript mapToolResultToToolResultBlockParam)
pub fn format_tool_result(message: &str, tool_use_id: &str) -> Value {
    let full_message = format!(
        r#"{}

In plan mode, you should:
1. Thoroughly explore the codebase to understand existing patterns
2. Identify similar features and architectural approaches
3. Consider multiple approaches and their trade-offs
4. Use AskUserQuestion if you need to clarify the approach
5. Design a concrete implementation strategy
6. When ready, use ExitPlanMode to present your plan for approval

Remember: DO NOT write or edit any files yet. This is a read-only exploration and planning phase."#,
        message
    );

    json!({
        "type": "tool_result",
        "content": full_message,
        "tool_use_id": tool_use_id
    })
}

/// Check if EnterPlanMode can be used in the current context
/// (cannot be used in agent contexts - matches JavaScript)
pub fn check_context(agent_id: Option<&str>) -> Result<()> {
    if agent_id.is_some() {
        return Err(Error::ToolExecution(
            "EnterPlanMode tool cannot be used in agent contexts".to_string()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_enter_plan_mode_basic() {
        let tool = EnterPlanModeTool;

        let input = json!({});
        let result = tool.execute(input, None).await;

        assert!(result.is_ok());

        let output: EnterPlanModeOutput = serde_json::from_str(&result.unwrap()).unwrap();
        assert!(output.message.contains("Entered plan mode"));
    }

    #[tokio::test]
    async fn test_enter_plan_mode_description() {
        let tool = EnterPlanModeTool;
        let desc = tool.description();

        assert!(desc.contains("plan mode"));
        assert!(desc.contains("exploration"));
    }

    #[tokio::test]
    async fn test_enter_plan_mode_schema() {
        let tool = EnterPlanModeTool;
        let schema = tool.input_schema();

        assert_eq!(schema["type"], "object");
        assert_eq!(schema["additionalProperties"], false);
        // Empty properties object
        assert!(schema["properties"].as_object().map(|o| o.is_empty()).unwrap_or(false));
    }

    #[test]
    fn test_format_tool_result() {
        let result = format_tool_result("Test message", "test-id");

        assert_eq!(result["type"], "tool_result");
        assert_eq!(result["tool_use_id"], "test-id");

        let content = result["content"].as_str().unwrap();
        assert!(content.contains("Test message"));
        assert!(content.contains("In plan mode"));
        assert!(content.contains("ExitPlanMode"));
    }

    #[test]
    fn test_check_context_no_agent() {
        let result = check_context(None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_context_with_agent() {
        let result = check_context(Some("agent-123"));
        assert!(result.is_err());
    }
}
