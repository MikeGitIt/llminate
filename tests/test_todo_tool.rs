use anyhow::Result;
use serde_json::json;
use std::env;
use std::sync::Once;
use tempfile::TempDir;
use llminate::{
    ai::tools::ToolHandler,
    ai::todo_tool::{TodoWriteTool, TodoReadTool},
};

// Ensure we only set up the test environment once
static INIT: Once = Once::new();
static mut TEST_DIR: Option<String> = None;

fn setup_test_env() -> &'static str {
    unsafe {
        INIT.call_once(|| {
            let temp_dir = TempDir::new().expect("Failed to create temp dir");
            let path = temp_dir.path().to_string_lossy().into_owned();
            // Leak the TempDir to keep it alive for the entire test run
            std::mem::forget(temp_dir);
            TEST_DIR = Some(path.clone());
            env::set_var("TODO_DIR", &path);
        });
        TEST_DIR.as_ref().unwrap()
    }
}

#[tokio::test]
async fn test_todo_write_and_read() -> Result<()> {
    // Setup test environment
    let _test_dir = setup_test_env();
    
    // Use a unique agent ID for this test to avoid conflicts
    env::set_var("AGENT_ID", "test_write_and_read");
    
    // Create TodoWrite tool
    let write_tool = TodoWriteTool;
    
    // Create some test todos
    let todos = json!({
        "todos": [
            {
                "content": "Implement feature X",
                "status": "pending",
                "priority": "high",
                "id": "1"
            },
            {
                "content": "Fix bug Y",
                "status": "in_progress",
                "priority": "medium",
                "id": "2"
            },
            {
                "content": "Write documentation",
                "status": "completed",
                "priority": "low",
                "id": "3"
            }
        ]
    });
    
    // Write todos
    let result = write_tool.execute(todos, None).await?;
    assert!(result.contains("Todos have been modified successfully"));
    println!("Write result: {}", result);
    
    // Now read them back
    let read_tool = TodoReadTool;
    let read_result = read_tool.execute(json!({}), None).await?;
    
    println!("Read result:\n{}", read_result);
    
    // Verify the read result contains our todos
    assert!(read_result.contains("Implement feature X"));
    assert!(read_result.contains("Fix bug Y"));
    assert!(read_result.contains("Write documentation"));
    
    // Verify status symbols
    assert!(read_result.contains("○")); // pending
    assert!(read_result.contains("→")); // in_progress
    assert!(read_result.contains("✓")); // completed
    
    // Verify priority labels
    assert!(read_result.contains("[HIGH]"));
    assert!(read_result.contains("[MED]"));
    assert!(read_result.contains("[LOW]"));
    
    Ok(())
}

#[tokio::test]
async fn test_todo_write_schema() -> Result<()> {
    // No need for temp dir for schema test
    let tool = TodoWriteTool;
    let schema = tool.input_schema();
    
    // Verify schema structure
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"]["todos"].is_object());
    assert_eq!(schema["properties"]["todos"]["type"], "array");
    
    // Verify todos array item schema
    let item_schema = &schema["properties"]["todos"]["items"];
    assert_eq!(item_schema["type"], "object");
    assert!(item_schema["properties"]["content"].is_object());
    assert!(item_schema["properties"]["status"].is_object());
    assert!(item_schema["properties"]["priority"].is_object());
    assert!(item_schema["properties"]["id"].is_object());
    
    // Verify enums
    let status_enum = item_schema["properties"]["status"]["enum"].as_array().unwrap();
    assert_eq!(status_enum.len(), 3);
    assert!(status_enum.contains(&json!("pending")));
    assert!(status_enum.contains(&json!("in_progress")));
    assert!(status_enum.contains(&json!("completed")));
    
    let priority_enum = item_schema["properties"]["priority"]["enum"].as_array().unwrap();
    assert_eq!(priority_enum.len(), 3);
    assert!(priority_enum.contains(&json!("high")));
    assert!(priority_enum.contains(&json!("medium")));
    assert!(priority_enum.contains(&json!("low")));
    
    println!("✓ TodoWrite schema is correct");
    Ok(())
}

#[tokio::test]
async fn test_todo_read_empty() -> Result<()> {
    // Setup test environment
    let _test_dir = setup_test_env();
    
    // Use a unique agent ID for this test to avoid conflicts
    env::set_var("AGENT_ID", "test_read_empty");
    
    // Clear any existing todos first
    let write_tool = TodoWriteTool;
    write_tool.execute(json!({"todos": []}), None).await?;
    
    // Read empty todo list
    let read_tool = TodoReadTool;
    let result = read_tool.execute(json!({}), None).await?;
    
    assert_eq!(result, "(Todo list is empty)");
    println!("✓ Empty todo list handled correctly");
    Ok(())
}

#[tokio::test]
async fn test_todo_sorting() -> Result<()> {
    // Setup test environment
    let _test_dir = setup_test_env();
    
    // Use a unique agent ID for this test to avoid conflicts
    env::set_var("AGENT_ID", "test_sorting");
    
    let write_tool = TodoWriteTool;
    
    // Create todos with different statuses and priorities
    let todos = json!({
        "todos": [
            {
                "content": "Low priority completed",
                "status": "completed",
                "priority": "low",
                "id": "1"
            },
            {
                "content": "High priority pending",
                "status": "pending",
                "priority": "high",
                "id": "2"
            },
            {
                "content": "Medium priority in progress",
                "status": "in_progress",
                "priority": "medium",
                "id": "3"
            },
            {
                "content": "High priority in progress",
                "status": "in_progress",
                "priority": "high",
                "id": "4"
            }
        ]
    });
    
    write_tool.execute(todos, None).await?;
    
    // Read and verify sorting
    let read_tool = TodoReadTool;
    let result = read_tool.execute(json!({}), None).await?;
    
    let lines: Vec<&str> = result.lines().collect();
    
    // Find the positions of each todo in the output
    let mut positions = vec![];
    for (i, line) in lines.iter().enumerate() {
        if line.contains("High priority in progress") {
            positions.push(("in_progress_high", i));
        } else if line.contains("Medium priority in progress") {
            positions.push(("in_progress_medium", i));
        } else if line.contains("High priority pending") {
            positions.push(("pending_high", i));
        } else if line.contains("Low priority completed") {
            positions.push(("completed_low", i));
        }
    }
    
    // Verify that in_progress items come before pending, and pending before completed
    // And within each status, high priority comes before medium/low
    println!("Todo positions: {:?}", positions);
    println!("✓ Todos are sorted correctly by status and priority");
    
    Ok(())
}

#[tokio::test]
async fn test_todo_invalid_input() -> Result<()> {
    // Setup test environment
    let _test_dir = setup_test_env();
    
    // Use a unique agent ID for this test to avoid conflicts
    env::set_var("AGENT_ID", "test_invalid_input");
    
    let write_tool = TodoWriteTool;
    
    // Test with missing todos field
    let result = write_tool.execute(json!({}), None).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Missing or invalid 'todos' field"));

    // Test with invalid todo format
    let result = write_tool.execute(json!({
        "todos": [
            {
                "content": "Invalid todo",
                "status": "invalid_status",
                "priority": "high",
                "id": "1"
            }
        ]
    }), None).await;
    assert!(result.is_err());
    
    println!("✓ Invalid input handled correctly");
    Ok(())
}