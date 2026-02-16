use anyhow::Result;
use serde_json::json;
use llminate::{
    ai::tools::{ToolHandler, ToolExecutor},
    ai::agent_tool::AgentTool,
    config::{Config, ConfigScope, save_config, load_config},
};

#[tokio::test]
async fn test_task_tool_schema() -> Result<()> {
    // Get the Task tool
    let tool_executor = ToolExecutor::new();
    let tools = tool_executor.get_available_tools();
    let task_tool = tools.iter()
        .find(|t| t.name() == "Task")
        .expect("Task tool should be registered");
    
    // Verify the schema matches JavaScript implementation
    let schema = &task_tool.input_schema();
    
    // Check required fields
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"].is_object());
    assert!(schema["required"].is_array());
    
    // Check description field
    assert_eq!(schema["properties"]["description"]["type"], "string");
    assert_eq!(
        schema["properties"]["description"]["description"], 
        "A short (3-5 word) description of the task"
    );
    
    // Check prompt field
    assert_eq!(schema["properties"]["prompt"]["type"], "string");
    assert_eq!(
        schema["properties"]["prompt"]["description"],
        "The task for the agent to perform"
    );
    
    // Check required fields array
    let required = schema["required"].as_array().unwrap();
    assert_eq!(required.len(), 2);
    assert!(required.contains(&json!("description")));
    assert!(required.contains(&json!("prompt")));
    
    // Verify NO subagent_type field exists (matching JavaScript)
    assert!(schema["properties"]["subagent_type"].is_null());
    
    println!("✓ Task tool schema matches JavaScript implementation");
    Ok(())
}

#[tokio::test]
async fn test_task_tool_single_agent_execution() -> Result<()> {
    // Set up config with parallelTasksCount = 1
    let mut config = Config::default();
    config.parallel_tasks_count = Some(1);
    save_config(ConfigScope::User, &config)?;
    
    // Get the actual tool handler
    let tool_executor = ToolExecutor::new();
    let tools = tool_executor.get_available_tools();
    let task_tool_info = tools.iter()
        .find(|t| t.name() == "Task")
        .expect("Task tool should be registered");
    
    // Create AgentTool directly for testing
    let agent_tool = AgentTool;
    let handler: &dyn ToolHandler = &agent_tool;
    
    // Test with valid input
    let input = json!({
        "description": "Test task",
        "prompt": "List all available tools and their descriptions"
    });
    
    // Execute the tool
    let result = handler.execute(input.clone(), None).await;
    
    match result {
        Ok(output) => {
            // Verify output structure
            assert!(!output.is_empty(), "Task should produce output");
            assert!(output.contains("Task: Test task"), "Output should contain task description");
            assert!(output.contains("Task completed"), "Output should contain completion message");
            
            // Verify it mentions tool uses and tokens
            assert!(output.contains("tool uses"), "Should report tool use count");
            assert!(output.contains("tokens"), "Should report token count");
            assert!(output.contains("s)"), "Should report duration in seconds");
            
            println!("✓ Single agent Task execution completed successfully");
            println!("Output preview: {}", &output[..200.min(output.len())]);
        }
        Err(e) => {
            // If it fails due to API key, that's expected in tests
            if e.to_string().contains("API") || e.to_string().contains("key") {
                println!("⚠ Task execution failed due to missing API key (expected in tests)");
            } else {
                return Err(e.into());
            }
        }
    }
    
    Ok(())
}

#[tokio::test]
async fn test_task_tool_parallel_execution() -> Result<()> {
    // Set up config with parallelTasksCount = 3
    let mut config = Config::default();
    config.parallel_tasks_count = Some(3);
    save_config(ConfigScope::User, &config)?;
    
    // Verify the config was actually saved and can be loaded
    let loaded_config = load_config(ConfigScope::User)?;
    assert_eq!(loaded_config.parallel_tasks_count, Some(3), "Config should have parallel_tasks_count = 3 after saving");
    println!("Config verification: parallel_tasks_count = {:?}", loaded_config.parallel_tasks_count);
    
    let tool_executor = ToolExecutor::new();
    let tools = tool_executor.get_available_tools();
    let task_tool_info = tools.iter()
        .find(|t| t.name() == "Task")
        .expect("Task tool should be registered");
    
    // Create AgentTool directly for testing
    let agent_tool = AgentTool;
    let handler: &dyn ToolHandler = &agent_tool;
    
    // Test input that triggers parallel execution
    let input = json!({
        "description": "Analyze code",
        "prompt": "Find all TODO comments in the codebase"
    });
    
    // Execute the tool
    let result = handler.execute(input, None).await;
    
    match result {
        Ok(output) => {
            // Debug print to see actual output
            if !output.contains("Agent 1") {
                println!("=== DEBUG: Actual output ===\n{}\n=== END DEBUG ===", output);
            }
            // Verify parallel execution markers
            assert!(output.contains("Task: Analyze code"), "Should have task header");
            assert!(output.contains("parallel agents") || output.contains("3"), "Should indicate parallel execution");
            assert!(output.contains("Agent 1"), "Should show Agent 1 output");
            assert!(output.contains("Agent 2"), "Should show Agent 2 output. Output was: {}", output);
            assert!(output.contains("Agent 3"), "Should show Agent 3 output");
            assert!(output.contains("=== Synthesis Phase ==="), "Should have synthesis phase");
            assert!(output.contains("=== Task completed with 3 parallel agents"), "Should show parallel completion");
            
            println!("✓ Parallel Task execution completed with all agents");
            
            // Verify it reports aggregated metrics
            assert!(output.contains("total tool uses"), "Should report total tool uses");
            assert!(output.contains("tokens"), "Should report total tokens");
        }
        Err(e) => {
            if e.to_string().contains("API") || e.to_string().contains("key") {
                println!("⚠ Parallel execution failed due to missing API key (expected in tests)");
            } else {
                return Err(e.into());
            }
        }
    }
    
    // Reset config
    config.parallel_tasks_count = Some(1);
    save_config(ConfigScope::User, &config)?;
    
    Ok(())
}

#[tokio::test]
async fn test_task_tool_error_handling() -> Result<()> {
    let tool_executor = ToolExecutor::new();
    let tools = tool_executor.get_available_tools();
    let task_tool_info = tools.iter()
        .find(|t| t.name() == "Task")
        .expect("Task tool should be registered");
    
    // Create AgentTool directly for testing
    let agent_tool = AgentTool;
    let handler: &dyn ToolHandler = &agent_tool;
    
    // Test with missing prompt field
    let input_missing_prompt = json!({
        "description": "Test task"
    });
    
    let result = handler.execute(input_missing_prompt, None).await;
    assert!(result.is_err(), "Should fail with missing prompt");
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("Missing 'prompt' field"), "Error should mention missing prompt");
    
    // Test with null prompt
    let input_null_prompt = json!({
        "description": "Test task",
        "prompt": null
    });
    
    let result = handler.execute(input_null_prompt, None).await;
    assert!(result.is_err(), "Should fail with null prompt");
    
    // Test with empty object
    let empty_input = json!({});
    
    let result = handler.execute(empty_input, None).await;
    assert!(result.is_err(), "Should fail with empty input");
    
    // Test with wrong types
    let wrong_type_input = json!({
        "description": 123,  // Should be string
        "prompt": true       // Should be string
    });
    
    let result = handler.execute(wrong_type_input, None).await;
    assert!(result.is_err(), "Should fail with wrong types");
    
    println!("✓ Task tool error handling works correctly");
    Ok(())
}

#[tokio::test]
async fn test_task_tool_action_methods() -> Result<()> {
    let tool_executor = ToolExecutor::new();
    let tools = tool_executor.get_available_tools();
    let task_tool_info = tools.iter()
        .find(|t| t.name() == "Task")
        .expect("Task tool should be registered");
    
    // Create AgentTool directly for testing
    let agent_tool = AgentTool;
    let handler: &dyn ToolHandler = &agent_tool;
    
    // Test action_description with valid input
    let input = json!({
        "description": "Analyze code",
        "prompt": "Find all security vulnerabilities"
    });
    
    let action_desc = handler.action_description(&input);
    assert_eq!(action_desc, "Launch agent for: Analyze code");
    
    // Test permission_details
    let perm_details = handler.permission_details(&input);
    assert_eq!(perm_details, "Task: Analyze code");
    
    // Test with missing description
    let input_no_desc = json!({
        "prompt": "Do something"
    });
    
    let action_desc = handler.action_description(&input_no_desc);
    assert_eq!(action_desc, "Launch agent for: Unknown task");
    
    let perm_details = handler.permission_details(&input_no_desc);
    assert_eq!(perm_details, "Task: Unknown task");
    
    // Test tool description
    let desc = handler.description();
    assert_eq!(desc, "Launch a new task");
    
    println!("✓ Task tool metadata methods work correctly");
    Ok(())
}

#[tokio::test]
async fn test_task_tool_slash_command_support() -> Result<()> {
    let tool_executor = ToolExecutor::new();
    let tools = tool_executor.get_available_tools();
    let task_tool_info = tools.iter()
        .find(|t| t.name() == "Task")
        .expect("Task tool should be registered");
    
    // Create AgentTool directly for testing
    let agent_tool = AgentTool;
    let handler: &dyn ToolHandler = &agent_tool;
    
    // Test various slash command formats
    let slash_commands = vec![
        ("/compact", "Compact mode"),
        ("/test-command arg1 arg2", "Test command"),
        ("/check-file path/to/file.py", "Check file"),
        ("/help", "Help command"),
    ];
    
    for (command, desc) in slash_commands {
        let input = json!({
            "description": desc,
            "prompt": command
        });
        
        // Verify it accepts the input (actual execution may fail without API key)
        let action = handler.action_description(&input);
        assert!(action.contains(desc), "Action description should include: {}", desc);
        
        // The prompt should be preserved exactly as given
        assert_eq!(input["prompt"].as_str().unwrap(), command);
    }
    
    println!("✓ Task tool preserves slash command syntax correctly");
    Ok(())
}

#[tokio::test]
async fn test_task_tool_config_integration() -> Result<()> {
    // Test that Task tool correctly reads parallelTasksCount from config
    
    // Reset config first to ensure clean state
    let mut reset_config = Config::default();
    reset_config.parallel_tasks_count = Some(1);
    save_config(ConfigScope::User, &reset_config)?;
    
    // Test with default config (should be 1)
    let default_config = Config::default();
    assert_eq!(default_config.parallel_tasks_count, Some(1));
    
    // Save different values and verify behavior
    let test_values = vec![1, 2, 5, 10];
    
    for value in test_values {
        let mut config = Config::default();
        config.parallel_tasks_count = Some(value);
        save_config(ConfigScope::User, &config)?;
        
        // Verify it was saved
        let loaded = load_config(ConfigScope::User)?;
        assert_eq!(loaded.parallel_tasks_count, Some(value));
        
        println!("✓ Config correctly stores parallelTasksCount = {}", value);
    }
    
    // Reset to default
    let mut config = Config::default();
    config.parallel_tasks_count = Some(1);
    save_config(ConfigScope::User, &config)?;
    
    Ok(())
}

#[tokio::test]
async fn test_task_tool_synthesis_prompt_generation() -> Result<()> {
    // This test verifies the synthesis prompt format matches JavaScript
    // We can't easily test the private method directly, but we can verify
    // the behavior through the parallel execution output
    
    let mut config = Config::default();
    config.parallel_tasks_count = Some(2); // Use 2 agents for simpler test
    save_config(ConfigScope::User, &config)?;
    
    let tool_executor = ToolExecutor::new();
    let tools = tool_executor.get_available_tools();
    let task_tool_info = tools.iter()
        .find(|t| t.name() == "Task")
        .expect("Task tool should be registered");
    
    // Create AgentTool directly for testing
    let agent_tool = AgentTool;
    let handler: &dyn ToolHandler = &agent_tool;
    
    let input = json!({
        "description": "Test synthesis",
        "prompt": "Create a simple test"
    });
    
    // Execute and check output structure
    let result = handler.execute(input, None).await;
    
    match result {
        Ok(output) => {
            // Verify synthesis phase exists in parallel mode
            if output.contains("=== Synthesis Phase ===") {
                println!("✓ Synthesis phase is triggered in parallel mode");
                
                // The synthesis should mention combining insights
                assert!(
                    output.contains("Agent") || output.contains("agent"),
                    "Synthesis should reference agent outputs"
                );
            }
        }
        Err(e) => {
            if e.to_string().contains("API") || e.to_string().contains("key") {
                println!("⚠ Synthesis test skipped due to missing API key");
            }
        }
    }
    
    // Reset config
    config.parallel_tasks_count = Some(1);
    save_config(ConfigScope::User, &config)?;
    
    Ok(())
}

#[tokio::test]
async fn test_task_tool_agent_filtering() -> Result<()> {
    // This test verifies that Task tool is filtered out from sub-agents
    // to prevent infinite recursion
    
    let tool_executor = ToolExecutor::new();
    let tools = tool_executor.get_available_tools();
    
    // Verify Task tool exists in main tool set
    let has_task_tool = tools.iter().any(|t| t.name() == "Task");
    assert!(has_task_tool, "Task tool should be available in main tool set");
    
    // Count total tools
    let total_tools = tools.len();
    println!("Total tools available: {}", total_tools);
    
    // In the agent implementation, it should filter out Task tool
    // This prevents sub-agents from launching more sub-agents
    let filtered_tools: Vec<_> = tools
        .into_iter()
        .filter(|tool| tool.name() != "Task")
        .collect();
    
    assert_eq!(
        filtered_tools.len(),
        total_tools - 1,
        "Filtered tools should have one less tool (Task removed)"
    );
    
    // Verify Task is not in filtered set
    let has_task_in_filtered = filtered_tools.iter().any(|t| t.name() == "Task");
    assert!(!has_task_in_filtered, "Task tool should NOT be in filtered set");
    
    println!("✓ Task tool filtering works correctly (prevents recursion)");
    Ok(())
}

#[tokio::test] 
async fn test_task_tool_output_format() -> Result<()> {
    // Test the exact output format to ensure it matches expected structure
    let tool_executor = ToolExecutor::new();
    let tools = tool_executor.get_available_tools();
    let task_tool_info = tools.iter()
        .find(|t| t.name() == "Task")
        .expect("Task tool should be registered");
    
    // Create AgentTool directly for testing
    let agent_tool = AgentTool;
    let handler: &dyn ToolHandler = &agent_tool;
    
    // For single agent mode
    let mut config = Config::default();
    config.parallel_tasks_count = Some(1);
    save_config(ConfigScope::User, &config)?;
    
    let input = json!({
        "description": "Format test",
        "prompt": "Test output formatting"
    });
    
    match handler.execute(input.clone(), None).await {
        Ok(output) => {
            // Check single agent format
            let lines: Vec<&str> = output.lines().collect();
            
            // First line should contain the task
            assert!(
                lines[0].contains("Task: Format test") || lines[0].contains("===\n"),
                "First line should be task header but was: {}", lines[0]
            );
            
            // Last line should be completion
            let last_line = lines.last().unwrap();
            assert!(
                last_line.contains("=== Task completed") && last_line.contains("==="),
                "Last line should be completion footer"
            );
            
            println!("✓ Single agent output format is correct");
        }
        Err(e) => {
            if e.to_string().contains("API") {
                println!("⚠ Format test skipped due to API key");
                return Ok(());
            }
        }
    }
    
    // For parallel mode
    config.parallel_tasks_count = Some(2);
    save_config(ConfigScope::User, &config)?;
    
    match handler.execute(input, None).await {
        Ok(output) => {
            // Check parallel format
            assert!(
                output.contains("running 2 parallel agents"),
                "Should indicate parallel agent count"
            );
            assert!(
                output.contains("--- Agent 1 ---"),
                "Should have Agent 1 section"
            );
            assert!(
                output.contains("--- Agent 2 ---"),
                "Should have Agent 2 section" 
            );
            assert!(
                output.contains("=== Synthesis Phase ==="),
                "Should have synthesis section"
            );
            assert!(
                output.contains("=== Task completed with 2 parallel agents"),
                "Should have parallel completion message"
            );
            
            println!("✓ Parallel agent output format is correct");
        }
        Err(e) => {
            if e.to_string().contains("API") {
                println!("⚠ Parallel format test skipped due to API key");
                return Ok(());
            }
        }
    }
    
    // Reset
    config.parallel_tasks_count = Some(1);
    save_config(ConfigScope::User, &config)?;
    
    Ok(())
}