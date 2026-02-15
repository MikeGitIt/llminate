use crate::ai::tools::ToolHandler;
use crate::error::{Error, Result};
use tokio_util::sync::CancellationToken;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

/// Todo item matching JavaScript schema
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Todo {
    pub content: String,
    pub status: TodoStatus,
    #[serde(rename = "activeForm")]
    pub active_form: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
}

/// TodoWrite tool - Update the todo list for the current session
pub struct TodoWriteTool;

#[async_trait]
impl ToolHandler for TodoWriteTool {
    fn description(&self) -> String {
        "Update the todo list for the current session. To be used proactively and often to track progress and pending tasks.".to_string()
    }
    
    fn input_schema(&self) -> Value {
        // Schema matches JavaScript: content, status, activeForm
        json!({
            "type": "object",
            "properties": {
                "todos": {
                    "type": "array",
                    "description": "The updated todo list",
                    "items": {
                        "type": "object",
                        "properties": {
                            "content": {
                                "type": "string",
                                "minLength": 1
                            },
                            "status": {
                                "type": "string",
                                "enum": ["pending", "in_progress", "completed"]
                            },
                            "activeForm": {
                                "type": "string",
                                "minLength": 1
                            }
                        },
                        "required": ["content", "status", "activeForm"],
                        "additionalProperties": false
                    }
                }
            },
            "required": ["todos"]
        })
    }
    
    async fn execute(&self, input: Value, _cancellation_token: Option<CancellationToken>) -> Result<String> {
        // Parse the todos from input
        let todos = input["todos"]
            .as_array()
            .ok_or_else(|| Error::InvalidInput("Missing or invalid 'todos' field".to_string()))?;

        // Validate and convert todos
        let mut todo_list: Vec<Todo> = Vec::new();
        for todo_value in todos {
            let todo: Todo = serde_json::from_value(todo_value.clone())
                .map_err(|e| Error::InvalidInput(format!("Invalid todo format: {}", e)))?;
            todo_list.push(todo);
        }

        // Get the todos directory
        let todos_dir = get_todos_dir()?;

        // Get the agent ID from environment or use default
        let agent_id = std::env::var("AGENT_ID").unwrap_or_else(|_| "default".to_string());
        let todo_file = todos_dir.join(format!("claude-agent-{}.json", agent_id));

        // Save the todos to file
        let json_content = serde_json::to_string_pretty(&todo_list)
            .map_err(|e| Error::Serialization(e))?;

        fs::write(&todo_file, json_content)
            .map_err(|e| Error::Io(e))?;

        // Format response matching JavaScript output
        let mut response = String::from("Todos have been modified successfully. Ensure that you continue to use the todo list to track your progress. Please proceed with the current tasks if applicable");

        Ok(response)
    }
    
    fn action_description(&self, input: &Value) -> String {
        let count = input["todos"]
            .as_array()
            .map(|todos| todos.len())
            .unwrap_or(0);
        format!("Update todo list ({} items)", count)
    }
    
    fn permission_details(&self, _input: &Value) -> String {
        "Update session todo list".to_string()
    }
}

/// TodoRead tool - Read the current todo list
pub struct TodoReadTool;

#[async_trait]
impl ToolHandler for TodoReadTool {
    fn description(&self) -> String {
        "Read the current todo list for the session".to_string()
    }

    fn input_schema(&self) -> Value {
        // TodoRead takes no input
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false,
            "description": "No input is required, leave this field blank. NOTE that we do not require a dummy object, placeholder string or a key like \"input\" or \"empty\". LEAVE IT BLANK."
        })
    }

    async fn execute(&self, _input: Value, _cancellation_token: Option<CancellationToken>) -> Result<String> {
        // Get the todos directory
        let todos_dir = get_todos_dir()?;

        // Get the agent ID from environment or use default
        let agent_id = std::env::var("AGENT_ID").unwrap_or_else(|_| "default".to_string());
        let todo_file = todos_dir.join(format!("claude-agent-{}.json", agent_id));

        // Read the todos from file if it exists
        if todo_file.exists() {
            let json_content = fs::read_to_string(&todo_file)
                .map_err(|e| Error::Io(e))?;

            let todos: Vec<Todo> = serde_json::from_str(&json_content)
                .unwrap_or_else(|_| Vec::new());

            if todos.is_empty() {
                return Ok("(Todo list is empty)".to_string());
            }

            // Format the todos for display - matching JavaScript format
            let mut output = String::new();

            // Sort todos by status (in_progress first, then pending, then completed)
            let mut sorted_todos = todos.clone();
            sorted_todos.sort_by(|a, b| {
                let status_order = |s: &TodoStatus| match s {
                    TodoStatus::InProgress => 0,
                    TodoStatus::Pending => 1,
                    TodoStatus::Completed => 2,
                };
                status_order(&a.status).cmp(&status_order(&b.status))
            });

            for (i, todo) in sorted_todos.iter().enumerate() {
                let status_str = match todo.status {
                    TodoStatus::Completed => "completed",
                    TodoStatus::InProgress => "in_progress",
                    TodoStatus::Pending => "pending",
                };

                output.push_str(&format!(
                    "{}. [{}] {}\n",
                    i + 1,
                    status_str,
                    todo.content
                ));
            }

            Ok(output)
        } else {
            Ok("(Todo list is empty)".to_string())
        }
    }

    fn action_description(&self, _input: &Value) -> String {
        "Read todo list".to_string()
    }

    fn permission_details(&self, _input: &Value) -> String {
        "Read session todo list".to_string()
    }
}

/// Get the todos directory, creating it if necessary
fn get_todos_dir() -> Result<PathBuf> {
    // Check if TODO_DIR environment variable is set (for testing or custom locations)
    if let Ok(custom_dir) = std::env::var("TODO_DIR") {
        let todos_dir = PathBuf::from(custom_dir);
        if !todos_dir.exists() {
            fs::create_dir_all(&todos_dir)
                .map_err(|e| Error::Io(e))?;
        }
        return Ok(todos_dir);
    }
    
    // Default behavior - use ~/.claude/todos
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| Error::Config("Cannot determine home directory".to_string()))?;
    
    let todos_dir = PathBuf::from(home).join(".claude").join("todos");
    
    if !todos_dir.exists() {
        fs::create_dir_all(&todos_dir)
            .map_err(|e| Error::Io(e))?;
    }
    
    Ok(todos_dir)
}