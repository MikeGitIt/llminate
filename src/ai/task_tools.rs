use crate::ai::tools::ToolHandler;
use crate::error::{Error, Result};
use tokio_util::sync::CancellationToken;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use once_cell::sync::Lazy;

/// Task status matching JavaScript enum
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Open,
    Resolved,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskStatus::Open => write!(f, "open"),
            TaskStatus::Resolved => write!(f, "resolved"),
        }
    }
}

/// Task comment matching JavaScript schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskComment {
    pub author: String,
    pub content: String,
}

/// Task structure matching JavaScript schema exactly
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub subject: String,
    pub description: String,
    pub status: TaskStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    pub references: Vec<String>,
    pub blocks: Vec<String>,
    #[serde(rename = "blockedBy")]
    pub blocked_by: Vec<String>,
    pub comments: Vec<TaskComment>,
}

/// Task storage matching JavaScript implementation
/// Tasks are stored per team/session in JSON files
pub struct TaskStore {
    tasks: HashMap<String, Task>,
    counter: u32,
    team_name: String,
}

impl TaskStore {
    fn new(team_name: &str) -> Self {
        Self {
            tasks: HashMap::new(),
            counter: 0,
            team_name: team_name.to_string(),
        }
    }

    /// Get the tasks directory for a team
    fn get_tasks_dir(team_name: &str) -> Result<PathBuf> {
        // Check for custom directory from environment
        if let Ok(custom_dir) = std::env::var("TASK_DIR") {
            let tasks_dir = PathBuf::from(custom_dir);
            if !tasks_dir.exists() {
                fs::create_dir_all(&tasks_dir)
                    .map_err(Error::Io)?;
            }
            return Ok(tasks_dir);
        }

        // Default: ~/.claude/tasks/{team_name}
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map_err(|_| Error::Config("Cannot determine home directory".to_string()))?;

        let tasks_dir = PathBuf::from(home)
            .join(".claude")
            .join("tasks")
            .join(team_name);

        if !tasks_dir.exists() {
            fs::create_dir_all(&tasks_dir)
                .map_err(Error::Io)?;
        }

        Ok(tasks_dir)
    }

    /// Get the task file path for a specific task ID
    fn get_task_file(team_name: &str, task_id: &str) -> Result<PathBuf> {
        let tasks_dir = Self::get_tasks_dir(team_name)?;
        Ok(tasks_dir.join(format!("{}.json", task_id)))
    }

    /// Get the counter file path
    fn get_counter_file(team_name: &str) -> Result<PathBuf> {
        let tasks_dir = Self::get_tasks_dir(team_name)?;
        Ok(tasks_dir.join("_counter.json"))
    }

    /// Load counter from file
    fn load_counter(team_name: &str) -> u32 {
        if let Ok(counter_file) = Self::get_counter_file(team_name) {
            if counter_file.exists() {
                if let Ok(content) = fs::read_to_string(&counter_file) {
                    if let Ok(counter) = content.trim().parse::<u32>() {
                        return counter;
                    }
                }
            }
        }
        0
    }

    /// Save counter to file
    fn save_counter(team_name: &str, counter: u32) -> Result<()> {
        let counter_file = Self::get_counter_file(team_name)?;
        fs::write(&counter_file, counter.to_string())
            .map_err(Error::Io)?;
        Ok(())
    }

    /// Generate next task ID matching JavaScript variable31981
    fn generate_id(&mut self) -> Result<String> {
        // Load current counter
        self.counter = Self::load_counter(&self.team_name);
        self.counter += 1;

        // Save updated counter
        Self::save_counter(&self.team_name, self.counter)?;

        Ok(self.counter.to_string())
    }

    /// Create a new task matching JavaScript variable36588
    fn create_task(&mut self, subject: &str, description: &str) -> Result<Task> {
        let id = self.generate_id()?;

        let task = Task {
            id: id.clone(),
            subject: subject.to_string(),
            description: description.to_string(),
            status: TaskStatus::Open,
            owner: None,
            references: Vec::new(),
            blocks: Vec::new(),
            blocked_by: Vec::new(),
            comments: Vec::new(),
        };

        // Save to file
        let task_file = Self::get_task_file(&self.team_name, &id)?;
        let json_content = serde_json::to_string_pretty(&task)
            .map_err(Error::Serialization)?;
        fs::write(&task_file, json_content)
            .map_err(Error::Io)?;

        // Store in memory
        self.tasks.insert(id.clone(), task.clone());

        Ok(task)
    }

    /// Get a task by ID matching JavaScript variable26607
    fn get_task(&self, task_id: &str) -> Result<Option<Task>> {
        // First check memory
        if let Some(task) = self.tasks.get(task_id) {
            return Ok(Some(task.clone()));
        }

        // Then check file
        let task_file = Self::get_task_file(&self.team_name, task_id)?;
        if !task_file.exists() {
            return Ok(None);
        }

        let json_content = fs::read_to_string(&task_file)
            .map_err(Error::Io)?;
        let task: Task = serde_json::from_str(&json_content)
            .map_err(Error::Serialization)?;

        Ok(Some(task))
    }

    /// Update a task matching JavaScript variable32356
    fn update_task(&mut self, task_id: &str, updates: TaskUpdates) -> Result<Option<Vec<String>>> {
        // First get the task
        let mut task = match self.get_task(task_id)? {
            Some(t) => t,
            None => return Ok(None),
        };

        let mut updated_fields = Vec::new();

        // Apply updates
        if let Some(subject) = updates.subject {
            task.subject = subject;
            updated_fields.push("subject".to_string());
        }

        if let Some(description) = updates.description {
            task.description = description;
            updated_fields.push("description".to_string());
        }

        if let Some(status) = updates.status {
            task.status = status;
            updated_fields.push("status".to_string());
        }

        if let Some(comment) = updates.add_comment {
            task.comments.push(comment);
            updated_fields.push("comments".to_string());
        }

        // Handle addReferences - bidirectional linking (matching JavaScript variable7712)
        if let Some(refs) = updates.add_references {
            for ref_id in refs {
                if !task.references.contains(&ref_id) {
                    task.references.push(ref_id.clone());
                    // Add bidirectional reference to the other task
                    if let Some(mut other_task) = self.get_task(&ref_id)? {
                        if !other_task.references.contains(&task_id.to_string()) {
                            other_task.references.push(task_id.to_string());
                            self.save_task(&other_task)?;
                            // Update memory cache for other task too
                            self.tasks.insert(ref_id.clone(), other_task);
                        }
                    }
                }
            }
            updated_fields.push("references".to_string());
        }

        // Handle addBlocks - this task blocks other tasks
        if let Some(blocks) = updates.add_blocks {
            for block_id in blocks {
                if !task.blocks.contains(&block_id) {
                    task.blocks.push(block_id.clone());
                    // Add blockedBy to the other task
                    if let Some(mut other_task) = self.get_task(&block_id)? {
                        if !other_task.blocked_by.contains(&task_id.to_string()) {
                            other_task.blocked_by.push(task_id.to_string());
                            self.save_task(&other_task)?;
                            // Update memory cache for other task too
                            self.tasks.insert(block_id.clone(), other_task);
                        }
                    }
                }
            }
            updated_fields.push("blocks".to_string());
        }

        // Handle addBlockedBy - other tasks block this task
        // Note: JavaScript line 494789-494791 adds blocks to the OTHER task
        if let Some(blocked_by) = updates.add_blocked_by {
            for blocker_id in blocked_by {
                if !task.blocked_by.contains(&blocker_id) {
                    task.blocked_by.push(blocker_id.clone());
                    // Add blocks to the other task
                    if let Some(mut other_task) = self.get_task(&blocker_id)? {
                        if !other_task.blocks.contains(&task_id.to_string()) {
                            other_task.blocks.push(task_id.to_string());
                            self.save_task(&other_task)?;
                            // Update memory cache for other task too
                            self.tasks.insert(blocker_id.clone(), other_task);
                        }
                    }
                }
            }
            updated_fields.push("blockedBy".to_string());
        }

        // Save updated task
        self.save_task(&task)?;

        // Update memory cache
        self.tasks.insert(task_id.to_string(), task);

        Ok(Some(updated_fields))
    }

    /// Save a task to file
    fn save_task(&self, task: &Task) -> Result<()> {
        let task_file = Self::get_task_file(&self.team_name, &task.id)?;
        let json_content = serde_json::to_string_pretty(task)
            .map_err(Error::Serialization)?;
        fs::write(&task_file, json_content)
            .map_err(Error::Io)?;
        Ok(())
    }

    /// Get all tasks matching JavaScript variable1822
    fn get_all_tasks(&self) -> Result<Vec<Task>> {
        let tasks_dir = Self::get_tasks_dir(&self.team_name)?;
        let mut tasks = Vec::new();

        // Read all .json files in the tasks directory (except _counter.json)
        if tasks_dir.exists() {
            let entries = fs::read_dir(&tasks_dir)
                .map_err(Error::Io)?;

            for entry in entries {
                let entry = entry.map_err(Error::Io)?;
                let path = entry.path();

                // Skip counter file and non-json files
                if let Some(file_name) = path.file_name() {
                    let name = file_name.to_string_lossy();
                    if name == "_counter.json" || !name.ends_with(".json") {
                        continue;
                    }
                }

                let json_content = fs::read_to_string(&path)
                    .map_err(Error::Io)?;
                let task: Task = serde_json::from_str(&json_content)
                    .map_err(Error::Serialization)?;
                tasks.push(task);
            }
        }

        // Sort by ID for consistent ordering
        tasks.sort_by(|a, b| {
            a.id.parse::<u32>().unwrap_or(0)
                .cmp(&b.id.parse::<u32>().unwrap_or(0))
        });

        Ok(tasks)
    }
}

/// Task updates structure for TaskUpdate tool
pub struct TaskUpdates {
    pub subject: Option<String>,
    pub description: Option<String>,
    pub status: Option<TaskStatus>,
    pub add_comment: Option<TaskComment>,
    pub add_references: Option<Vec<String>>,
    pub add_blocks: Option<Vec<String>>,
    pub add_blocked_by: Option<Vec<String>>,
}

/// Get the team name from environment (matching JavaScript variable22734)
fn get_team_name() -> String {
    std::env::var("CLAUDE_CODE_TEAM_NAME")
        .unwrap_or_else(|_| "default".to_string())
}

/// Global task store manager
static TASK_STORES: Lazy<Arc<Mutex<HashMap<String, TaskStore>>>> =
    Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

/// Get or create task store for a team
async fn get_task_store(team_name: &str) -> Arc<Mutex<HashMap<String, TaskStore>>> {
    TASK_STORES.clone()
}

/// TaskCreate tool - Create a new task for tracking
/// Matches JavaScript implementation at line 494338
pub struct TaskCreateTool;

#[async_trait]
impl ToolHandler for TaskCreateTool {
    fn description(&self) -> String {
        "Create a new task for tracking team work".to_string()
    }

    fn input_schema(&self) -> Value {
        // Schema matches JavaScript: subject, description
        json!({
            "type": "object",
            "properties": {
                "subject": {
                    "type": "string",
                    "description": "A brief title for the task"
                },
                "description": {
                    "type": "string",
                    "description": "A detailed description of what needs to be done"
                }
            },
            "required": ["subject", "description"],
            "additionalProperties": false
        })
    }

    fn action_description(&self, input: &Value) -> String {
        let subject = input["subject"].as_str().unwrap_or("<unknown>");
        format!("Create task: {}", subject)
    }

    fn permission_details(&self, input: &Value) -> String {
        let subject = input["subject"].as_str().unwrap_or("<unknown>");
        format!("Task: {}", subject)
    }

    async fn execute(&self, input: Value, _cancellation_token: Option<CancellationToken>) -> Result<String> {
        // Get required parameters matching JavaScript schema
        let subject = input["subject"]
            .as_str()
            .ok_or_else(|| Error::InvalidInput("Missing 'subject' field".to_string()))?;

        let description = input["description"]
            .as_str()
            .ok_or_else(|| Error::InvalidInput("Missing 'description' field".to_string()))?;

        // Get team name
        let team_name = get_team_name();

        // Get or create task store
        let stores = get_task_store(&team_name).await;
        let mut stores_guard = stores.lock().await;

        let store = stores_guard
            .entry(team_name.clone())
            .or_insert_with(|| TaskStore::new(&team_name));

        // Create the task
        let task = store.create_task(subject, description)?;

        // Format response matching JavaScript mapToolResultToToolResultBlockParam
        // "Task #${variable29010.id} created successfully: ${variable29010.subject}"
        Ok(format!("Task #{} created successfully: {}", task.id, task.subject))
    }
}

/// TaskGet tool - Get task details by ID
/// Matches JavaScript implementation at line 494472
pub struct TaskGetTool;

#[async_trait]
impl ToolHandler for TaskGetTool {
    fn description(&self) -> String {
        "Get a task by ID from the task list".to_string()
    }

    fn input_schema(&self) -> Value {
        // Schema matches JavaScript: taskId
        json!({
            "type": "object",
            "properties": {
                "taskId": {
                    "type": "string",
                    "description": "The ID of the task to retrieve"
                }
            },
            "required": ["taskId"],
            "additionalProperties": false
        })
    }

    fn action_description(&self, input: &Value) -> String {
        let task_id = input["taskId"].as_str().unwrap_or("<unknown>");
        format!("Get task #{}", task_id)
    }

    fn permission_details(&self, input: &Value) -> String {
        let task_id = input["taskId"].as_str().unwrap_or("<unknown>");
        format!("Task ID: {}", task_id)
    }

    async fn execute(&self, input: Value, _cancellation_token: Option<CancellationToken>) -> Result<String> {
        // Get required parameter matching JavaScript schema
        let task_id = input["taskId"]
            .as_str()
            .ok_or_else(|| Error::InvalidInput("Missing 'taskId' field".to_string()))?;

        // Get team name
        let team_name = get_team_name();

        // Get or create task store
        let stores = get_task_store(&team_name).await;
        let mut stores_guard = stores.lock().await;

        let store = stores_guard
            .entry(team_name.clone())
            .or_insert_with(|| TaskStore::new(&team_name));

        // Get the task
        let task = store.get_task(task_id)?;

        // Format response matching JavaScript mapToolResultToToolResultBlockParam
        match task {
            None => {
                // JavaScript returns is_error: true for not found
                Err(Error::NotFound("Task not found".to_string()))
            }
            Some(task) => {
                // Build output matching JavaScript format exactly:
                // `Task #${variable29010.id}: ${variable29010.subject}`
                // `Status: ${variable29010.status}`
                // `Description: ${variable29010.description}`
                // etc.
                let mut lines = vec![
                    format!("Task #{}: {}", task.id, task.subject),
                    format!("Status: {}", task.status),
                    format!("Description: {}", task.description),
                ];

                // Add blocked_by if present
                if !task.blocked_by.is_empty() {
                    let blocked_by_str = task.blocked_by
                        .iter()
                        .map(|id| format!("#{}", id))
                        .collect::<Vec<_>>()
                        .join(", ");
                    lines.push(format!("Blocked by: {}", blocked_by_str));
                }

                // Add blocks if present
                if !task.blocks.is_empty() {
                    let blocks_str = task.blocks
                        .iter()
                        .map(|id| format!("#{}", id))
                        .collect::<Vec<_>>()
                        .join(", ");
                    lines.push(format!("Blocks: {}", blocks_str));
                }

                // Add references if present
                if !task.references.is_empty() {
                    let refs_str = task.references
                        .iter()
                        .map(|id| format!("#{}", id))
                        .collect::<Vec<_>>()
                        .join(", ");
                    lines.push(format!("References: {}", refs_str));
                }

                // Add comments if present
                if !task.comments.is_empty() {
                    lines.push("Comments:".to_string());
                    for comment in &task.comments {
                        lines.push(format!("  [{}]: {}", comment.author, comment.content));
                    }
                }

                Ok(lines.join("\n"))
            }
        }
    }
}

/// TaskUpdate tool - Update a task in the task list
/// Matches JavaScript implementation at line 494655
pub struct TaskUpdateTool;

#[async_trait]
impl ToolHandler for TaskUpdateTool {
    fn description(&self) -> String {
        "Update a task in the task list".to_string()
    }

    fn input_schema(&self) -> Value {
        // Schema matches JavaScript at line 494701-494715
        json!({
            "type": "object",
            "properties": {
                "taskId": {
                    "type": "string",
                    "description": "The ID of the task to update"
                },
                "subject": {
                    "type": "string",
                    "description": "New subject for the task"
                },
                "description": {
                    "type": "string",
                    "description": "New description for the task"
                },
                "status": {
                    "type": "string",
                    "enum": ["open", "resolved"],
                    "description": "New status for the task"
                },
                "addComment": {
                    "type": "object",
                    "properties": {
                        "author": {
                            "type": "string",
                            "description": "Author of the comment"
                        },
                        "content": {
                            "type": "string",
                            "description": "Content of the comment"
                        }
                    },
                    "required": ["author", "content"],
                    "description": "Add a comment to the task"
                },
                "addReferences": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Task IDs to add as references"
                },
                "addBlocks": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Task IDs that this task blocks"
                },
                "addBlockedBy": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Task IDs that block this task"
                }
            },
            "required": ["taskId"],
            "additionalProperties": false
        })
    }

    fn action_description(&self, input: &Value) -> String {
        let task_id = input["taskId"].as_str().unwrap_or("<unknown>");
        format!("Update task #{}", task_id)
    }

    fn permission_details(&self, input: &Value) -> String {
        let task_id = input["taskId"].as_str().unwrap_or("<unknown>");
        format!("Task ID: {}", task_id)
    }

    async fn execute(&self, input: Value, _cancellation_token: Option<CancellationToken>) -> Result<String> {
        // Get required parameter
        let task_id = input["taskId"]
            .as_str()
            .ok_or_else(|| Error::InvalidInput("Missing 'taskId' field".to_string()))?;

        // Build updates from input
        let updates = TaskUpdates {
            subject: input["subject"].as_str().map(|s| s.to_string()),
            description: input["description"].as_str().map(|s| s.to_string()),
            status: input["status"].as_str().and_then(|s| match s {
                "open" => Some(TaskStatus::Open),
                "resolved" => Some(TaskStatus::Resolved),
                _ => None,
            }),
            add_comment: if input["addComment"].is_object() {
                let author = input["addComment"]["author"].as_str();
                let content = input["addComment"]["content"].as_str();
                match (author, content) {
                    (Some(a), Some(c)) => Some(TaskComment {
                        author: a.to_string(),
                        content: c.to_string(),
                    }),
                    _ => None,
                }
            } else {
                None
            },
            add_references: input["addReferences"].as_array().map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            }),
            add_blocks: input["addBlocks"].as_array().map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            }),
            add_blocked_by: input["addBlockedBy"].as_array().map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            }),
        };

        // Track if status was set to resolved for special message
        let was_resolved = updates.status == Some(TaskStatus::Resolved);

        // Get team name
        let team_name = get_team_name();

        // Get or create task store
        let stores = get_task_store(&team_name).await;
        let mut stores_guard = stores.lock().await;

        let store = stores_guard
            .entry(team_name.clone())
            .or_insert_with(|| TaskStore::new(&team_name));

        // Update the task
        match store.update_task(task_id, updates)? {
            None => {
                // Task not found - matching JavaScript behavior (line 494757-494763)
                Err(Error::NotFound(format!("Task #{} not found", task_id)))
            }
            Some(updated_fields) => {
                // Format response matching JavaScript (line 494810-494817)
                // `Updated task #${variable21016} ${variable26452.join(", ")}`
                let fields_str = if updated_fields.is_empty() {
                    "no fields".to_string()
                } else {
                    updated_fields.join(", ")
                };

                let mut result = format!("Updated task #{} {}", task_id, fields_str);

                // If resolved and running as an agent, add prompt to find next task
                // Matching JavaScript line 494811-494813
                if was_resolved {
                    if std::env::var("CLAUDE_CODE_AGENT_ID").is_ok() {
                        result.push_str("\n\nTask completed. Call TaskList now to find your next available task or see if your work unblocked others.");
                    }
                }

                Ok(result)
            }
        }
    }
}

/// TaskList tool - List all tasks in the task list
/// Matches JavaScript implementation at line 494851
pub struct TaskListTool;

#[async_trait]
impl ToolHandler for TaskListTool {
    fn description(&self) -> String {
        "List all tasks in the task list".to_string()
    }

    fn input_schema(&self) -> Value {
        // TaskList takes no input - matching JavaScript (line 494916)
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })
    }

    fn action_description(&self, _input: &Value) -> String {
        "List all tasks".to_string()
    }

    fn permission_details(&self, _input: &Value) -> String {
        "List task list".to_string()
    }

    async fn execute(&self, _input: Value, _cancellation_token: Option<CancellationToken>) -> Result<String> {
        // Get team name
        let team_name = get_team_name();

        // Get or create task store
        let stores = get_task_store(&team_name).await;
        let mut stores_guard = stores.lock().await;

        let store = stores_guard
            .entry(team_name.clone())
            .or_insert_with(|| TaskStore::new(&team_name));

        // Get all tasks
        let tasks = store.get_all_tasks()?;

        // Format response matching JavaScript (line 494976-494989)
        if tasks.is_empty() {
            return Ok("No tasks found".to_string());
        }

        // Get resolved task IDs for filtering blockedBy (matching JavaScript line 494961)
        let resolved_ids: std::collections::HashSet<String> = tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Resolved)
            .map(|t| t.id.clone())
            .collect();

        // Format each task (matching JavaScript line 494981-494984)
        // `#${variable26452.id} [${variable26452.status}] ${variable26452.subject}${variable27524}${variable1729}`
        let task_lines: Vec<String> = tasks
            .iter()
            .map(|task| {
                let status_str = match task.status {
                    TaskStatus::Open => "open",
                    TaskStatus::Resolved => "resolved",
                };

                // Add owner if present
                let owner_str = task.owner
                    .as_ref()
                    .map(|o| format!(" ({})", o))
                    .unwrap_or_default();

                // Filter out resolved tasks from blockedBy (matching JavaScript line 494969)
                let active_blockers: Vec<&String> = task.blocked_by
                    .iter()
                    .filter(|id| !resolved_ids.contains(*id))
                    .collect();

                // Format blocked by string
                let blocked_str = if !active_blockers.is_empty() {
                    format!(
                        " [blocked by {}]",
                        active_blockers.iter().map(|id| format!("#{}", id)).collect::<Vec<_>>().join(", ")
                    )
                } else {
                    String::new()
                };

                format!(
                    "#{} [{}] {}{}{}",
                    task.id,
                    status_str,
                    task.subject,
                    owner_str,
                    blocked_str
                )
            })
            .collect();

        Ok(task_lines.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::sync::atomic::{AtomicU64, Ordering};

    // Counter for unique team names in tests
    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    /// Helper to set up test environment with unique team name
    fn setup_test_env() -> TempDir {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let test_id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        std::env::set_var("TASK_DIR", temp_dir.path().to_str().unwrap());
        std::env::set_var("CLAUDE_CODE_TEAM_NAME", format!("test-team-{}", test_id));
        temp_dir
    }

    #[tokio::test]
    async fn test_task_create() {
        let _temp_dir = setup_test_env();

        let tool = TaskCreateTool;
        let input = json!({
            "subject": "Test task",
            "description": "This is a test task description"
        });

        let result = tool.execute(input, None).await;
        assert!(result.is_ok());

        let output = result.expect("Should succeed");
        assert!(output.contains("created successfully:"));
        assert!(output.contains("Test task"));
    }

    #[tokio::test]
    async fn test_task_create_missing_subject() {
        let _temp_dir = setup_test_env();

        let tool = TaskCreateTool;
        let input = json!({
            "description": "This is a test task description"
        });

        let result = tool.execute(input, None).await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.to_string().contains("Missing 'subject' field"));
    }

    #[tokio::test]
    async fn test_task_get() {
        let _temp_dir = setup_test_env();

        // First create a task
        let create_tool = TaskCreateTool;
        let create_input = json!({
            "subject": "Test task for get",
            "description": "Description for get test"
        });

        let create_result = create_tool.execute(create_input, None).await;
        assert!(create_result.is_ok());

        // Now get the task
        let get_tool = TaskGetTool;
        let get_input = json!({
            "taskId": "1"
        });

        let result = get_tool.execute(get_input, None).await;
        assert!(result.is_ok());

        let output = result.expect("Should succeed");
        assert!(output.contains("Task #1:"));
        assert!(output.contains("Test task for get"));
        assert!(output.contains("Status: open"));
        assert!(output.contains("Description: Description for get test"));
    }

    #[tokio::test]
    async fn test_task_get_not_found() {
        let _temp_dir = setup_test_env();

        let tool = TaskGetTool;
        let input = json!({
            "taskId": "999"
        });

        let result = tool.execute(input, None).await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.to_string().contains("Task not found"));
    }

    #[tokio::test]
    async fn test_task_get_missing_task_id() {
        let _temp_dir = setup_test_env();

        let tool = TaskGetTool;
        let input = json!({});

        let result = tool.execute(input, None).await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.to_string().contains("Missing 'taskId' field"));
    }

    #[tokio::test]
    async fn test_multiple_task_creation() {
        let _temp_dir = setup_test_env();

        let tool = TaskCreateTool;

        // Create first task
        let input1 = json!({
            "subject": "First task",
            "description": "First description"
        });
        let result1 = tool.execute(input1, None).await;
        assert!(result1.is_ok());
        let output1 = result1.expect("Should succeed");
        assert!(output1.contains("created successfully"));
        assert!(output1.contains("First task"));

        // Create second task
        let input2 = json!({
            "subject": "Second task",
            "description": "Second description"
        });
        let result2 = tool.execute(input2, None).await;
        assert!(result2.is_ok());
        let output2 = result2.expect("Should succeed");
        assert!(output2.contains("created successfully"));
        assert!(output2.contains("Second task"));
    }

    #[test]
    fn test_task_status_display() {
        assert_eq!(TaskStatus::Open.to_string(), "open");
        assert_eq!(TaskStatus::Resolved.to_string(), "resolved");
    }

    #[test]
    fn test_task_serialization() {
        let task = Task {
            id: "1".to_string(),
            subject: "Test".to_string(),
            description: "Test description".to_string(),
            status: TaskStatus::Open,
            owner: None,
            references: vec!["2".to_string()],
            blocks: vec!["3".to_string()],
            blocked_by: vec!["4".to_string()],
            comments: vec![TaskComment {
                author: "agent-1".to_string(),
                content: "Working on it".to_string(),
            }],
        };

        let json_str = serde_json::to_string(&task).expect("Should serialize");
        assert!(json_str.contains("\"blockedBy\""));  // Verify camelCase

        let deserialized: Task = serde_json::from_str(&json_str).expect("Should deserialize");
        assert_eq!(deserialized.id, "1");
        assert_eq!(deserialized.blocked_by, vec!["4".to_string()]);
    }

    // ============================
    // TaskUpdate tests
    // ============================

    #[tokio::test]
    async fn test_task_update_status() {
        let _temp_dir = setup_test_env();

        // First create a task
        let create_tool = TaskCreateTool;
        let create_input = json!({
            "subject": "Task to update",
            "description": "Task for update test"
        });
        create_tool.execute(create_input, None).await.expect("Should create task");

        // Update the status
        let update_tool = TaskUpdateTool;
        let update_input = json!({
            "taskId": "1",
            "status": "resolved"
        });

        let result = update_tool.execute(update_input, None).await;
        assert!(result.is_ok());

        let output = result.expect("Should succeed");
        assert!(output.contains("Updated task #1"));
        assert!(output.contains("status"));

        // Verify the update by getting the task
        let get_tool = TaskGetTool;
        let get_result = get_tool.execute(json!({"taskId": "1"}), None).await;
        assert!(get_result.is_ok());
        assert!(get_result.expect("Should get task").contains("Status: resolved"));
    }

    #[tokio::test]
    async fn test_task_update_add_comment() {
        let _temp_dir = setup_test_env();

        // Create a task
        let create_tool = TaskCreateTool;
        create_tool.execute(json!({
            "subject": "Task for comments",
            "description": "Testing comments"
        }), None).await.expect("Should create task");

        // Add a comment
        let update_tool = TaskUpdateTool;
        let update_input = json!({
            "taskId": "1",
            "addComment": {
                "author": "test-agent",
                "content": "Working on this now"
            }
        });

        let result = update_tool.execute(update_input, None).await;
        assert!(result.is_ok());
        assert!(result.expect("Should succeed").contains("comments"));

        // Verify comment was added
        let get_tool = TaskGetTool;
        let get_result = get_tool.execute(json!({"taskId": "1"}), None).await;
        assert!(get_result.is_ok());
        let output = get_result.expect("Should get task");
        assert!(output.contains("[test-agent]: Working on this now"));
    }

    #[tokio::test]
    async fn test_task_update_not_found() {
        let _temp_dir = setup_test_env();

        let update_tool = TaskUpdateTool;
        let update_input = json!({
            "taskId": "999",
            "status": "resolved"
        });

        let result = update_tool.execute(update_input, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_task_update_dependencies() {
        let _temp_dir = setup_test_env();

        // Create two tasks
        let create_tool = TaskCreateTool;
        let result1 = create_tool.execute(json!({
            "subject": "Task 1",
            "description": "First task"
        }), None).await.expect("Should create task 1");

        let task1_id = result1.split('#').nth(1)
            .and_then(|s| s.split(' ').next())
            .expect("Should have task ID");

        let result2 = create_tool.execute(json!({
            "subject": "Task 2",
            "description": "Second task"
        }), None).await.expect("Should create task 2");

        let task2_id = result2.split('#').nth(1)
            .and_then(|s| s.split(' ').next())
            .expect("Should have task ID");

        // Make task 2 blocked by task 1
        let update_tool = TaskUpdateTool;
        let result = update_tool.execute(json!({
            "taskId": task2_id,
            "addBlockedBy": [task1_id]
        }), None).await;

        assert!(result.is_ok());
        assert!(result.expect("Should succeed").contains("blockedBy"));

        // Verify task 2 shows blocked by
        let get_tool = TaskGetTool;
        let task2 = get_tool.execute(json!({"taskId": task2_id}), None).await.expect("Should get task 2");
        assert!(task2.contains("Blocked by:"));

        // Verify task 1 shows blocks
        let task1 = get_tool.execute(json!({"taskId": task1_id}), None).await.expect("Should get task 1");
        assert!(task1.contains("Blocks:"));
    }

    // ============================
    // TaskList tests
    // ============================

    #[tokio::test]
    async fn test_task_list_empty() {
        let _temp_dir = setup_test_env();

        let list_tool = TaskListTool;
        let result = list_tool.execute(json!({}), None).await;
        assert!(result.is_ok());
        assert_eq!(result.expect("Should succeed"), "No tasks found");
    }

    #[tokio::test]
    async fn test_task_list_with_tasks() {
        let _temp_dir = setup_test_env();

        // Create a couple of tasks
        let create_tool = TaskCreateTool;
        create_tool.execute(json!({
            "subject": "First task",
            "description": "Description 1"
        }), None).await.expect("Should create task 1");

        create_tool.execute(json!({
            "subject": "Second task",
            "description": "Description 2"
        }), None).await.expect("Should create task 2");

        // List tasks
        let list_tool = TaskListTool;
        let result = list_tool.execute(json!({}), None).await;
        assert!(result.is_ok());

        let output = result.expect("Should succeed");
        // Check for open tasks (don't depend on specific IDs)
        assert!(output.contains("[open] First task"));
        assert!(output.contains("[open] Second task"));
    }

    #[tokio::test]
    async fn test_task_list_filters_resolved_blockers() {
        let _temp_dir = setup_test_env();

        // Create two tasks where task 2 is blocked by task 1
        let create_tool = TaskCreateTool;
        let result1 = create_tool.execute(json!({
            "subject": "Blocking task",
            "description": "This blocks task 2"
        }), None).await.expect("Should create task 1");

        // Extract task ID from "Task #N created successfully"
        let task1_id = result1.split('#').nth(1)
            .and_then(|s| s.split(' ').next())
            .expect("Should have task ID");

        let result2 = create_tool.execute(json!({
            "subject": "Blocked task",
            "description": "This is blocked by task 1"
        }), None).await.expect("Should create task 2");

        let task2_id = result2.split('#').nth(1)
            .and_then(|s| s.split(' ').next())
            .expect("Should have task ID");

        // Add dependency
        let update_tool = TaskUpdateTool;
        update_tool.execute(json!({
            "taskId": task2_id,
            "addBlockedBy": [task1_id]
        }), None).await.expect("Should update dependency");

        // List should show task 2 as blocked
        let list_tool = TaskListTool;
        let list_result1 = list_tool.execute(json!({}), None).await.expect("Should list");
        assert!(list_result1.contains("[blocked by"));

        // Resolve task 1
        update_tool.execute(json!({
            "taskId": task1_id,
            "status": "resolved"
        }), None).await.expect("Should resolve task 1");

        // List should no longer show task 2 as blocked (since blocker is resolved)
        let list_result2 = list_tool.execute(json!({}), None).await.expect("Should list");
        assert!(!list_result2.contains("[blocked by"));
    }
}
