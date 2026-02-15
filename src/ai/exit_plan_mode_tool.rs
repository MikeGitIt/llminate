use async_trait::async_trait;
use serde_json::{json, Value};
use crate::error::Result;
use tokio_util::sync::CancellationToken;

/// ExitPlanMode tool - matches JavaScript implementation
/// This tool is used when in plan mode and the plan has been written to a file.
/// It does NOT take the plan content as a parameter - it reads from the plan file.
pub struct ExitPlanModeTool;

#[async_trait]
impl crate::ai::tools::ToolHandler for ExitPlanModeTool {
    fn description(&self) -> String {
        "Prompts the user to exit plan mode and start coding".to_string()
    }

    fn input_schema(&self) -> Value {
        // JavaScript schema: strictObject with launchSwarm, teammateCount, passthrough
        // The plan is NOT passed as input - it's already written to a file
        json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {},
            "additionalProperties": true
        })
    }

    fn action_description(&self, _input: &Value) -> String {
        "Exit plan mode and start coding".to_string()
    }

    fn permission_details(&self, _input: &Value) -> String {
        "Exit plan mode".to_string()
    }

    async fn execute(&self, _input: Value, _cancellation_token: Option<CancellationToken>) -> Result<String> {
        // The plan is read from the plan file, not from input
        // This just signals readiness to exit plan mode
        let response = json!({
            "plan": null,
            "isAgent": false
        });

        Ok(response.to_string())
    }
}