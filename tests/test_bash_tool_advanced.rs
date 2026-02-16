use llminate::ai::tools::{BashTool, ToolHandler};
use serde_json::json;
use tokio;
use std::path::PathBuf;

#[tokio::test]
async fn test_bash_tool_basic_command() {
    let bash_tool = BashTool;
    
    let input = json!({
        "command": "echo 'Hello World'"
    });
    
    let result = bash_tool.execute(input, None).await;
    assert!(result.is_ok(), "Basic bash command should succeed");
    
    let output = result.unwrap();
    assert!(output.contains("Hello World"), "Output should contain command result");
    assert!(output.contains("Executed in"), "Output should contain execution time");
    
    println!("✓ Basic bash command test passed");
}

#[tokio::test]
async fn test_bash_tool_persistent_session_basic_mode() {
    let bash_tool = BashTool;
    
    // Test that basic mode (default) only persists working directory, not variables
    // This matches JavaScript behavior
    
    // First command - set a variable
    let input1 = json!({
        "command": "TEST_VAR=hello_world",
        "shell_id": "test_session"
    });
    
    let result1 = bash_tool.execute(input1, None).await;
    assert!(result1.is_ok(), "First command should succeed");
    
    // Second command - variable should NOT persist in basic mode
    let input2 = json!({
        "command": "echo \"TEST_VAR=$TEST_VAR\"",
        "shell_id": "test_session"
    });
    
    let result2 = bash_tool.execute(input2, None).await;
    assert!(result2.is_ok(), "Second command should succeed");
    
    let output2 = result2.unwrap();
    assert!(output2.contains("TEST_VAR="), "Command should execute");
    assert!(!output2.contains("hello_world"), "Variables should NOT persist in basic mode");
    
    println!("✓ Basic mode persistence test passed (variables don't persist)");
}

#[tokio::test]
async fn test_bash_tool_persistent_session_advanced_mode() {
    let bash_tool = BashTool;
    
    // Test advanced persistence mode where variables DO persist
    
    // First command - set a variable with advanced persistence enabled
    // Use export to ensure the variable is available
    let input1 = json!({
        "command": "export TEST_VAR=hello_world && echo \"TEST_VAR set to: $TEST_VAR\"",
        "shell_id": "test_session_advanced",
        "advanced_persistence": true
    });
    
    let result1 = bash_tool.execute(input1, None).await;
    assert!(result1.is_ok(), "First command should succeed");
    println!("First command output: {}", result1.unwrap());
    
    // Second command - use the variable (should work with advanced persistence)
    let input2 = json!({
        "command": "set | grep TEST_VAR || echo 'TEST_VAR not found'; echo \"TEST_VAR is: $TEST_VAR\"",
        "shell_id": "test_session_advanced",
        "advanced_persistence": true
    });
    
    let result2 = bash_tool.execute(input2, None).await;
    assert!(result2.is_ok(), "Second command should succeed");
    
    let output2 = result2.unwrap();
    println!("Second command full output: {}", output2);
    // Check that the output contains the expected value
    assert!(output2.contains("hello_world"), 
            "Advanced persistence should maintain variables. Output was: {}", output2);
    
    println!("✓ Advanced persistence test passed");
}

#[tokio::test]
async fn test_bash_tool_working_directory() {
    let bash_tool = BashTool;
    
    // Create a temporary directory for testing
    let temp_dir = std::env::temp_dir().join("bash_tool_test");
    std::fs::create_dir_all(&temp_dir).unwrap();
    
    let input = json!({
        "command": "pwd",
        "working_dir": temp_dir.to_str().unwrap()
    });
    
    let result = bash_tool.execute(input, None).await;
    assert!(result.is_ok(), "Command with working directory should succeed");
    
    let output = result.unwrap();
    assert!(output.contains(temp_dir.to_str().unwrap()), "Should be in specified working directory");
    
    // Cleanup
    let _ = std::fs::remove_dir_all(&temp_dir);
    
    println!("✓ Working directory test passed");
}

#[tokio::test]
async fn test_bash_tool_environment_variables() {
    let bash_tool = BashTool;
    
    let input = json!({
        "command": "echo $CUSTOM_VAR",
        "env": {
            "CUSTOM_VAR": "test_value_123"
        },
        "shell_id": "env_test_session"
    });
    
    let result = bash_tool.execute(input, None).await;
    assert!(result.is_ok(), "Command with environment variables should succeed");
    
    let output = result.unwrap();
    assert!(output.contains("test_value_123"), "Environment variable should be available");
    
    println!("✓ Environment variables test passed");
}

#[tokio::test]
async fn test_bash_tool_timeout() {
    let bash_tool = BashTool;
    
    let input = json!({
        "command": "sleep 3",
        "timeout": 1000, // 1 second timeout
        "shell_id": "timeout_test_session"
    });
    
    let result = bash_tool.execute(input, None).await;
    // Should timeout and return an error
    assert!(result.is_err(), "Command should timeout");
    
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("timeout") || error_msg.contains("timed out"), 
           "Error should indicate timeout: {}", error_msg);
    
    println!("✓ Timeout test passed");
}

#[tokio::test]
async fn test_bash_tool_error_handling() {
    let bash_tool = BashTool;
    
    let input = json!({
        "command": "ls /nonexistent_directory_12345",
        "shell_id": "error_test_session"
    });
    
    let result = bash_tool.execute(input, None).await;
    assert!(result.is_ok(), "Tool should handle command errors gracefully");
    
    let output = result.unwrap();
    assert!(output.contains("Exit code:") || output.contains("STDERR:"), 
           "Output should indicate error");
    
    println!("✓ Error handling test passed");
}

#[tokio::test]
async fn test_bash_tool_shell_executable() {
    let bash_tool = BashTool;
    
    // Test with explicit shell path
    let input = json!({
        "command": "echo $0", // Shows the shell being used
        "shellExecutable": "/bin/bash",
        "shell_id": "shell_exec_test"
    });
    
    let result = bash_tool.execute(input, None).await;
    assert!(result.is_ok(), "Command with shell executable should succeed");
    
    let output = result.unwrap();
    assert!(output.contains("bash"), "Should use specified shell executable");
    
    println!("✓ Shell executable test passed");
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn test_bash_tool_sandbox_mode() {
    let bash_tool = BashTool;
    
    let input = json!({
        "command": "echo 'sandbox test'",
        "sandbox": true,
        "shell_id": "sandbox_test_session"
    });
    
    let result = bash_tool.execute(input, None).await;
    // On macOS, sandbox should work
    assert!(result.is_ok(), "Sandbox mode should work on macOS");
    
    let output = result.unwrap();
    assert!(output.contains("sandbox test"), "Sandboxed command should execute");
    
    println!("✓ Sandbox mode test passed (macOS)");
}

#[cfg(not(target_os = "macos"))]
#[tokio::test]
async fn test_bash_tool_sandbox_mode_unsupported() {
    let bash_tool = BashTool;
    
    let input = json!({
        "command": "echo 'sandbox test'",
        "sandbox": true,
        "shell_id": "sandbox_fail_test"
    });
    
    let result = bash_tool.execute(input, None).await;
    // On non-macOS systems, sandbox should fail
    assert!(result.is_err(), "Sandbox mode should fail on non-macOS systems");
    
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("not available on this system"), 
           "Error should indicate sandbox unavailable");
    
    println!("✓ Sandbox mode unsupported test passed (non-macOS)");
}

#[tokio::test]
async fn test_bash_tool_multiple_sessions_with_advanced_mode() {
    let bash_tool = BashTool;
    
    // Test multiple sessions with advanced persistence mode
    
    // Set variable in session 1 with advanced persistence
    let input1 = json!({
        "command": "SESSION_VAR=session1_value",
        "shell_id": "session1",
        "advanced_persistence": true
    });
    
    let result1 = bash_tool.execute(input1, None).await;
    assert!(result1.is_ok(), "Session 1 setup should succeed");
    
    // Set different variable in session 2 with advanced persistence
    let input2 = json!({
        "command": "SESSION_VAR=session2_value",
        "shell_id": "session2",
        "advanced_persistence": true
    });
    
    let result2 = bash_tool.execute(input2, None).await;
    assert!(result2.is_ok(), "Session 2 setup should succeed");
    
    // Check session 1 still has its value
    let input3 = json!({
        "command": "echo $SESSION_VAR",
        "shell_id": "session1",
        "advanced_persistence": true
    });
    
    let result3 = bash_tool.execute(input3, None).await;
    assert!(result3.is_ok(), "Session 1 check should succeed");
    
    let output3 = result3.unwrap();
    println!("Session 1 check output: {}", output3);
    assert!(output3.contains("session1_value"), "Session 1 should maintain its state. Output was: {}", output3);
    
    // Check session 2 has its value
    let input4 = json!({
        "command": "echo $SESSION_VAR",
        "shell_id": "session2",
        "advanced_persistence": true
    });
    
    let result4 = bash_tool.execute(input4, None).await;
    assert!(result4.is_ok(), "Session 2 check should succeed");
    
    let output4 = result4.unwrap();
    assert!(output4.contains("session2_value"), "Session 2 should maintain its state");
    
    println!("✓ Multiple sessions with advanced persistence test passed");
}

#[tokio::test]
async fn test_bash_tool_working_directory_persistence() {
    let bash_tool = BashTool;
    
    // Test that working directory DOES persist in basic mode (default)
    // This is the key difference - directories persist, variables don't
    
    let temp_dir = std::env::temp_dir().join("bash_tool_wd_test");
    std::fs::create_dir_all(&temp_dir).unwrap();
    
    // Change directory in session
    let input1 = json!({
        "command": format!("cd {}", temp_dir.to_str().unwrap()),
        "shell_id": "wd_session"
    });
    
    let result1 = bash_tool.execute(input1, None).await;
    assert!(result1.is_ok(), "First command should succeed");
    
    // Check if we're still in that directory
    let input2 = json!({
        "command": "pwd",
        "shell_id": "wd_session"
    });
    
    let result2 = bash_tool.execute(input2, None).await;
    assert!(result2.is_ok(), "Second command should succeed");
    
    let output2 = result2.unwrap();
    assert!(output2.contains(temp_dir.to_str().unwrap()), "Working directory should persist in basic mode");
    
    // Cleanup
    let _ = std::fs::remove_dir_all(&temp_dir);
    
    println!("✓ Working directory persistence test passed");
}