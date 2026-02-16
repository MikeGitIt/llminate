use llminate::ai::tools::{EditFileTool, ToolHandler};
use serde_json::json;
use std::fs;
use tempfile::NamedTempFile;
use tokio;

#[tokio::test]
async fn test_file_edit_basic_replacement() {
    // Create a temporary file
    let mut temp_file = NamedTempFile::new().unwrap();
    let temp_path = temp_file.path().to_str().unwrap();
    
    // Write initial content
    let initial_content = "Hello world!\nThis is a test file.\nWe will edit this line.\nEnd of file.";
    fs::write(temp_path, initial_content).unwrap();
    
    // Create the tool
    let edit_tool = EditFileTool;
    
    // Test input
    let input = json!({
        "file_path": temp_path,
        "old_string": "We will edit this line.",
        "new_string": "This line has been edited!",
        "replace_all": false
    });
    
    // Execute the tool
    let result = edit_tool.execute(input, None).await;
    assert!(result.is_ok(), "FileEdit should succeed");
    
    // Verify the content changed
    let new_content = fs::read_to_string(temp_path).unwrap();
    let expected_content = "Hello world!\nThis is a test file.\nThis line has been edited!\nEnd of file.";
    assert_eq!(new_content, expected_content, "File content should be updated correctly");
    
    println!("✓ Basic replacement test passed");
}

#[tokio::test]
async fn test_file_edit_replace_all() {
    // Create a temporary file
    let mut temp_file = NamedTempFile::new().unwrap();
    let temp_path = temp_file.path().to_str().unwrap();
    
    // Write initial content with repeated text
    let initial_content = "test test test\nmore test content\ntest again";
    fs::write(temp_path, initial_content).unwrap();
    
    // Create the tool
    let edit_tool = EditFileTool;
    
    // Test input with replace_all = true
    let input = json!({
        "file_path": temp_path,
        "old_string": "test",
        "new_string": "replaced",
        "replace_all": true
    });
    
    // Execute the tool
    let result = edit_tool.execute(input, None).await;
    assert!(result.is_ok(), "FileEdit with replace_all should succeed");
    
    // Verify all occurrences were replaced
    let new_content = fs::read_to_string(temp_path).unwrap();
    let expected_content = "replaced replaced replaced\nmore replaced content\nreplaced again";
    assert_eq!(new_content, expected_content, "All occurrences should be replaced");
    
    println!("✓ Replace all test passed");
}

#[tokio::test]
async fn test_file_edit_string_not_found() {
    // Create a temporary file
    let mut temp_file = NamedTempFile::new().unwrap();
    let temp_path = temp_file.path().to_str().unwrap();
    
    // Write initial content
    let initial_content = "Hello world!\nThis is a test file.";
    fs::write(temp_path, initial_content).unwrap();
    
    // Create the tool
    let edit_tool = EditFileTool;
    
    // Test input with non-existent string
    let input = json!({
        "file_path": temp_path,
        "old_string": "This string does not exist",
        "new_string": "replacement",
        "replace_all": false
    });
    
    // Execute the tool
    let result = edit_tool.execute(input, None).await;
    assert!(result.is_err(), "FileEdit should fail when string not found");
    
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("String not found in file"), "Error should indicate string not found");
    
    println!("✓ String not found test passed");
}

#[tokio::test]
async fn test_file_edit_same_strings() {
    // Create a temporary file
    let mut temp_file = NamedTempFile::new().unwrap();
    let temp_path = temp_file.path().to_str().unwrap();
    
    // Write initial content
    let initial_content = "Hello world!";
    fs::write(temp_path, initial_content).unwrap();
    
    // Create the tool
    let edit_tool = EditFileTool;
    
    // Test input with same old_string and new_string
    let input = json!({
        "file_path": temp_path,
        "old_string": "Hello",
        "new_string": "Hello",
        "replace_all": false
    });
    
    // Execute the tool
    let result = edit_tool.execute(input, None).await;
    assert!(result.is_err(), "FileEdit should fail when old_string equals new_string");
    
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("exactly the same"), "Error should indicate strings are the same");
    
    println!("✓ Same strings test passed");
}