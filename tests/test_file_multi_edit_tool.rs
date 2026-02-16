use llminate::ai::tools::{FileMultiEditTool, ToolHandler};
use serde_json::json;
use std::fs;
use tempfile::NamedTempFile;
use tokio;

#[tokio::test]
async fn test_file_multi_edit_basic() {
    // Create a temporary file
    let mut temp_file = NamedTempFile::new().unwrap();
    let temp_path = temp_file.path().to_str().unwrap();
    
    // Write initial content
    let initial_content = "Hello world!\nThis is line 2.\nThis is line 3.\nEnd of file.";
    fs::write(temp_path, initial_content).unwrap();
    
    // Create the tool
    let multi_edit_tool = FileMultiEditTool;
    
    // Test input with multiple edits
    let input = json!({
        "file_path": temp_path,
        "edits": [
            {
                "old_string": "Hello world!",
                "new_string": "Greetings universe!",
                "replace_all": false
            },
            {
                "old_string": "line 2",
                "new_string": "second line",
                "replace_all": false
            },
            {
                "old_string": "line 3",
                "new_string": "third line",
                "replace_all": false
            }
        ]
    });
    
    // Execute the tool
    let result = multi_edit_tool.execute(input, None).await;
    assert!(result.is_ok(), "FileMultiEdit should succeed");
    
    // Verify the content changed
    let new_content = fs::read_to_string(temp_path).unwrap();
    let expected_content = "Greetings universe!\nThis is second line.\nThis is third line.\nEnd of file.";
    assert_eq!(new_content, expected_content, "File content should be updated correctly");
    
    println!("✓ Basic multi-edit test passed");
}

#[tokio::test]
async fn test_file_multi_edit_with_replace_all() {
    // Create a temporary file
    let mut temp_file = NamedTempFile::new().unwrap();
    let temp_path = temp_file.path().to_str().unwrap();
    
    // Write initial content with repeated text
    let initial_content = "test content\nmore test here\ntest again\nfinal test line";
    fs::write(temp_path, initial_content).unwrap();
    
    // Create the tool
    let multi_edit_tool = FileMultiEditTool;
    
    // Test input with replace_all
    let input = json!({
        "file_path": temp_path,
        "edits": [
            {
                "old_string": "test",
                "new_string": "REPLACED",
                "replace_all": true
            },
            {
                "old_string": "content",
                "new_string": "data",
                "replace_all": false
            }
        ]
    });
    
    // Execute the tool
    let result = multi_edit_tool.execute(input, None).await;
    assert!(result.is_ok(), "FileMultiEdit with replace_all should succeed");
    
    // Verify all occurrences were replaced
    let new_content = fs::read_to_string(temp_path).unwrap();
    let expected_content = "REPLACED data\nmore REPLACED here\nREPLACED again\nfinal REPLACED line";
    assert_eq!(new_content, expected_content, "All occurrences should be replaced");
    
    println!("✓ Multi-edit with replace_all test passed");
}

#[tokio::test]
async fn test_file_multi_edit_sequential_edits() {
    // Create a temporary file
    let mut temp_file = NamedTempFile::new().unwrap();
    let temp_path = temp_file.path().to_str().unwrap();
    
    // Write initial content
    let initial_content = "First line\nSecond line\nThird line";
    fs::write(temp_path, initial_content).unwrap();
    
    // Create the tool
    let multi_edit_tool = FileMultiEditTool;
    
    // Test sequential edits where later edits depend on earlier ones
    let input = json!({
        "file_path": temp_path,
        "edits": [
            {
                "old_string": "First",
                "new_string": "1st",
                "replace_all": false
            },
            {
                "old_string": "1st line",
                "new_string": "FIRST LINE",
                "replace_all": false
            }
        ]
    });
    
    // Execute the tool
    let result = multi_edit_tool.execute(input, None).await;
    assert!(result.is_ok(), "Sequential edits should succeed");
    
    // Verify sequential processing
    let new_content = fs::read_to_string(temp_path).unwrap();
    let expected_content = "FIRST LINE\nSecond line\nThird line";
    assert_eq!(new_content, expected_content, "Sequential edits should work correctly");
    
    println!("✓ Sequential edits test passed");
}

#[tokio::test]
async fn test_file_multi_edit_string_not_found() {
    // Create a temporary file
    let mut temp_file = NamedTempFile::new().unwrap();
    let temp_path = temp_file.path().to_str().unwrap();
    
    // Write initial content
    let initial_content = "Hello world!\nThis is a test.";
    fs::write(temp_path, initial_content).unwrap();
    
    // Create the tool
    let multi_edit_tool = FileMultiEditTool;
    
    // Test input with non-existent string
    let input = json!({
        "file_path": temp_path,
        "edits": [
            {
                "old_string": "Hello",
                "new_string": "Hi",
                "replace_all": false
            },
            {
                "old_string": "This string does not exist",
                "new_string": "replacement",
                "replace_all": false
            }
        ]
    });
    
    // Execute the tool - should still succeed with partial edits
    let result = multi_edit_tool.execute(input, None).await;
    assert!(result.is_ok(), "FileMultiEdit should succeed with partial edits");
    
    // Verify only valid edit was applied
    let new_content = fs::read_to_string(temp_path).unwrap();
    let expected_content = "Hi world!\nThis is a test.";
    assert_eq!(new_content, expected_content, "Only valid edit should be applied");
    
    println!("✓ String not found test passed");
}

#[tokio::test]
async fn test_file_multi_edit_no_valid_edits() {
    // Create a temporary file
    let mut temp_file = NamedTempFile::new().unwrap();
    let temp_path = temp_file.path().to_str().unwrap();
    
    // Write initial content
    let initial_content = "Hello world!";
    fs::write(temp_path, initial_content).unwrap();
    
    // Create the tool
    let multi_edit_tool = FileMultiEditTool;
    
    // Test input with all non-existent strings
    let input = json!({
        "file_path": temp_path,
        "edits": [
            {
                "old_string": "This does not exist",
                "new_string": "replacement1",
                "replace_all": false
            },
            {
                "old_string": "Neither does this",
                "new_string": "replacement2",
                "replace_all": false
            }
        ]
    });
    
    // Execute the tool
    let result = multi_edit_tool.execute(input, None).await;
    assert!(result.is_err(), "FileMultiEdit should fail when no edits can be applied");
    
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("No edits could be applied"), "Error should indicate no edits applied");
    
    println!("✓ No valid edits test passed");
}