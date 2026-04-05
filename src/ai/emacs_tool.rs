use crate::ai::tools::ToolHandler;
use crate::error::{Error, Result};
use async_trait::async_trait;
use once_cell::sync::Lazy;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};
use tokio_util::sync::CancellationToken;

/// Shared map of pending Emacs eval requests.
/// Key: request_id, Value: oneshot sender to deliver the EmacsEvalResult.
pub type PendingEmacsRequests = Arc<Mutex<HashMap<String, oneshot::Sender<Value>>>>;

/// Global pending requests map — accessed by both EmacsCommandTool::execute()
/// (to register a pending request) and the keep-alive stdin reader (to deliver results).
pub static PENDING_EMACS_REQUESTS: Lazy<PendingEmacsRequests> =
    Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

/// EmacsCommand tool — allows the AI to invoke Emacs functions in the user's live session.
///
/// When running in keep-alive mode inside Emacs, this tool emits an `EmacsEval` event
/// to stdout and blocks until the Emacs bridge sends back an `EmacsEvalResult` via stdin.
pub struct EmacsCommandTool;

#[async_trait]
impl ToolHandler for EmacsCommandTool {
    fn description(&self) -> String {
        "Execute an Emacs command or function. Use this to interact with the editor directly — \
         open files in buffers, stage changes with magit, navigate to definitions with eglot, \
         run compilation, manage windows, and more. The command is executed in the user's \
         live Emacs session."
            .to_string()
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The Emacs function to call (e.g. 'find-file', 'magit-stage-file', 'eglot-rename')"
                },
                "args": {
                    "type": "array",
                    "description": "Arguments to pass to the function",
                    "items": {}
                },
                "description": {
                    "type": "string",
                    "description": "Human-readable description of what this command does"
                }
            },
            "required": ["command"]
        })
    }

    fn action_description(&self, input: &Value) -> String {
        let command = input["command"].as_str().unwrap_or("unknown");
        let args_desc = input["args"]
            .as_array()
            .filter(|a| !a.is_empty())
            .map(|a| {
                let arg_strs: Vec<String> = a.iter().map(|v| v.to_string()).collect();
                format!(" with args [{}]", arg_strs.join(", "))
            })
            .unwrap_or_default();
        format!("Execute Emacs command: {}{}", command, args_desc)
    }

    fn permission_details(&self, input: &Value) -> String {
        let command = input["command"].as_str().unwrap_or("unknown");
        format!("Emacs: {}", command)
    }

    async fn execute(
        &self,
        input: Value,
        cancellation_token: Option<CancellationToken>,
    ) -> Result<String> {
        // EmacsCommand only works in keep-alive mode where the Emacs bridge
        // reads stdout JSON events and sends results back via stdin.
        // In TUI mode, there's no bridge — stdout is the terminal and no
        // stdin reader dispatches EmacsEvalResult events. Fail fast instead
        // of hanging for 60s waiting for a response that will never come.
        if !crate::tui::print_mode::EMACS_BRIDGE_ACTIVE
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return Err(Error::ToolExecution(
                "EmacsCommand is only available when running via the Emacs bridge \
                 (--keep-alive mode). In TUI mode, use the built-in file tools \
                 (Read, Write, Edit, Bash) instead."
                    .to_string(),
            ));
        }

        let command = input["command"]
            .as_str()
            .ok_or_else(|| Error::InvalidInput("Missing 'command' parameter".to_string()))?
            .to_string();

        let args = if input["args"].is_null() || input["args"].is_object() {
            json!([])
        } else {
            input["args"].clone()
        };

        let description = input["description"].as_str().map(|s| s.to_string());

        // Generate unique request ID
        let request_id = uuid::Uuid::new_v4().to_string();

        // Create oneshot channel for the result
        let (tx, rx) = oneshot::channel::<Value>();

        // Register the pending request
        {
            let mut pending = PENDING_EMACS_REQUESTS.lock().await;
            pending.insert(request_id.clone(), tx);
        }

        // Build and emit EmacsEval event to stdout
        let eval_event = crate::tui::print_mode::StreamEvent::EmacsEval {
            command: command.clone(),
            args: args.clone(),
            request_id: request_id.clone(),
        };

        let event_json = serde_json::to_string(&eval_event)
            .map_err(|e| Error::ToolExecution(format!("Failed to serialize EmacsEval: {}", e)))?;

        // Write to stdout (the bridge reads this)
        {
            use tokio::io::AsyncWriteExt;
            let mut stdout = tokio::io::stdout();
            stdout
                .write_all(event_json.as_bytes())
                .await
                .map_err(|e| Error::ToolExecution(format!("Failed to write to stdout: {}", e)))?;
            stdout
                .write_all(b"\n")
                .await
                .map_err(|e| Error::ToolExecution(format!("Failed to write newline: {}", e)))?;
            stdout
                .flush()
                .await
                .map_err(|e| Error::ToolExecution(format!("Failed to flush stdout: {}", e)))?;
        }

        // Wait for result with timeout and cancellation support
        let timeout_duration = tokio::time::Duration::from_secs(60);

        let result = if let Some(token) = cancellation_token {
            tokio::select! {
                biased;
                _ = token.cancelled() => {
                    let mut pending = PENDING_EMACS_REQUESTS.lock().await;
                    pending.remove(&request_id);
                    return Err(Error::Cancelled("EmacsCommand execution cancelled".to_string()));
                }
                result = rx => {
                    result.map_err(|_| Error::ToolExecution(
                        "EmacsEvalResult channel closed unexpectedly".to_string()
                    ))?
                }
                _ = tokio::time::sleep(timeout_duration) => {
                    let mut pending = PENDING_EMACS_REQUESTS.lock().await;
                    pending.remove(&request_id);
                    return Err(Error::Timeout(format!(
                        "EmacsCommand '{}' timed out after {}s",
                        command,
                        timeout_duration.as_secs()
                    )));
                }
            }
        } else {
            tokio::select! {
                result = rx => {
                    result.map_err(|_| Error::ToolExecution(
                        "EmacsEvalResult channel closed unexpectedly".to_string()
                    ))?
                }
                _ = tokio::time::sleep(timeout_duration) => {
                    let mut pending = PENDING_EMACS_REQUESTS.lock().await;
                    pending.remove(&request_id);
                    return Err(Error::Timeout(format!(
                        "EmacsCommand '{}' timed out after {}s",
                        command,
                        timeout_duration.as_secs()
                    )));
                }
            }
        };

        // Process the result from Emacs
        let success = result["success"].as_bool().unwrap_or(false);
        let result_value = &result["result"];

        if success {
            let desc = description
                .map(|d| format!(" ({})", d))
                .unwrap_or_default();
            Ok(format!(
                "Emacs command '{}'{} executed successfully.\nResult: {}",
                command,
                desc,
                serde_json::to_string_pretty(result_value)
                    .unwrap_or_else(|_| result_value.to_string())
            ))
        } else {
            let error_msg = result_value.as_str().unwrap_or("Unknown error");
            Ok(format!(
                "Emacs command '{}' failed: {}",
                command, error_msg
            ))
        }
    }
}

/// Deliver an EmacsEvalResult to the pending request.
/// Called by the keep-alive stdin reader when it receives an EmacsEvalResult event.
/// Returns true if the result was delivered, false if no pending request was found.
pub async fn deliver_emacs_eval_result(
    request_id: &str,
    success: bool,
    result: Value,
) -> bool {
    let mut pending = PENDING_EMACS_REQUESTS.lock().await;
    if let Some(tx) = pending.remove(request_id) {
        let payload = json!({
            "success": success,
            "result": result,
        });
        tx.send(payload).is_ok()
    } else {
        false
    }
}
