use crate::ai::tools::{ToolHandler, ToolExecutor};
use crate::ai::{create_client, MessageRole, Message, MessageContent, ContentPart};
use crate::error::{Error, Result};
use tokio_util::sync::CancellationToken;
use crate::config::Config;
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::task::JoinSet;
use std::time::Instant;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Available agent types matching JavaScript implementation
#[derive(Debug, Clone, PartialEq)]
pub enum AgentType {
    GeneralPurpose,
    Explore,
    Plan,
    ClaudeCodeGuide,
    StatuslineSetup,
    Custom(String),
}

impl AgentType {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "general-purpose" => AgentType::GeneralPurpose,
            "explore" => AgentType::Explore,
            "plan" => AgentType::Plan,
            "claude-code-guide" => AgentType::ClaudeCodeGuide,
            "statusline-setup" => AgentType::StatuslineSetup,
            _ => AgentType::Custom(s.to_string()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            AgentType::GeneralPurpose => "general-purpose",
            AgentType::Explore => "Explore",
            AgentType::Plan => "Plan",
            AgentType::ClaudeCodeGuide => "claude-code-guide",
            AgentType::StatuslineSetup => "statusline-setup",
            AgentType::Custom(s) => s,
        }
    }

    /// Get available agent types for display
    pub fn available_types() -> Vec<&'static str> {
        vec![
            "general-purpose",
            "Explore",
            "Plan",
            "claude-code-guide",
            "statusline-setup",
        ]
    }
}

/// Model selection for agents
#[derive(Debug, Clone, PartialEq)]
pub enum AgentModel {
    Sonnet,
    Opus,
    Haiku,
}

impl AgentModel {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "sonnet" => Some(AgentModel::Sonnet),
            "opus" => Some(AgentModel::Opus),
            "haiku" => Some(AgentModel::Haiku),
            _ => None,
        }
    }
}

/// Stored agent state for resume functionality
#[derive(Debug, Clone)]
pub struct StoredAgentState {
    pub agent_id: String,
    pub agent_type: AgentType,
    pub messages: Vec<Message>,
    pub created_at: Instant,
}

/// Global agent state storage for resume functionality
lazy_static::lazy_static! {
    static ref AGENT_STATES: Arc<RwLock<HashMap<String, StoredAgentState>>> =
        Arc::new(RwLock::new(HashMap::new()));
}

/// Result from a single agent execution
#[derive(Debug, Clone)]
struct AgentResult {
    content: Vec<ContentPart>,
    tool_use_count: usize,
    tokens: usize,
    duration_ms: u128,
    agent_index: usize,
    messages: Vec<Message>, // For resume functionality
}

/// Agent tool - Launch a new agent to handle complex, multi-step tasks autonomously
pub struct AgentTool;

#[async_trait]
impl ToolHandler for AgentTool {
    fn description(&self) -> String {
        "Launch a new task".to_string()
    }
    
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "description": {
                    "type": "string",
                    "description": "A short (3-5 word) description of the task"
                },
                "prompt": {
                    "type": "string",
                    "description": "The task for the agent to perform"
                },
                "subagent_type": {
                    "type": "string",
                    "description": "The type of specialized agent to use for this task"
                },
                "model": {
                    "type": "string",
                    "enum": ["sonnet", "opus", "haiku"],
                    "description": "Optional model to use for this agent. If not specified, inherits from parent. Prefer haiku for quick, straightforward tasks to minimize cost and latency."
                },
                "resume": {
                    "type": "string",
                    "description": "Optional agent ID to resume from. If provided, the agent will continue from the previous execution transcript."
                },
                "run_in_background": {
                    "type": "boolean",
                    "description": "Set to true to run this agent in the background. Use TaskOutput to read the output later."
                }
            },
            "required": ["description", "prompt", "subagent_type"]
        })
    }
    
    fn action_description(&self, input: &Value) -> String {
        let description = input["description"].as_str().unwrap_or("Unknown task");
        let subagent_type = input["subagent_type"].as_str().unwrap_or("general-purpose");
        format!("Launch {} agent: {}", subagent_type, description)
    }
    
    fn permission_details(&self, input: &Value) -> String {
        let description = input["description"].as_str().unwrap_or("Unknown task");
        format!("Task: {}", description)
    }
    
    async fn execute(&self, input: Value, cancellation_token: Option<CancellationToken>) -> Result<String> {
        let prompt = input["prompt"]
            .as_str()
            .ok_or_else(|| Error::InvalidInput("Missing 'prompt' field".to_string()))?;

        let description = input["description"]
            .as_str()
            .unwrap_or("Task");

        // Extract subagent_type (required)
        let subagent_type_str = input["subagent_type"]
            .as_str()
            .ok_or_else(|| Error::InvalidInput("Missing 'subagent_type' field".to_string()))?;
        let agent_type = AgentType::from_str(subagent_type_str);

        // Extract optional model selection
        let model = input["model"]
            .as_str()
            .and_then(AgentModel::from_str);

        // Extract optional resume agent ID
        let resume_id = input["resume"].as_str().map(String::from);

        // Extract optional run_in_background flag
        let run_in_background = input["run_in_background"].as_bool().unwrap_or(false);

        let start_time = Instant::now();

        // Generate agent ID for this execution
        let agent_id = Uuid::new_v4().to_string();

        // Handle resume case
        if let Some(ref prev_agent_id) = resume_id {
            let states = AGENT_STATES.read().await;
            if let Some(prev_state) = states.get(prev_agent_id) {
                // Resume from previous state
                drop(states); // Release read lock
                return self.execute_resume(
                    &agent_id,
                    prev_agent_id,
                    prompt,
                    description,
                    &agent_type,
                    model.as_ref(),
                    start_time,
                    cancellation_token,
                ).await;
            } else {
                return Err(Error::NotFound(format!(
                    "Agent ID '{}' not found. Cannot resume.",
                    prev_agent_id
                )));
            }
        }

        // Handle background execution
        if run_in_background {
            return self.execute_background(
                agent_id,
                prompt.to_string(),
                description.to_string(),
                agent_type,
                model,
                cancellation_token,
            ).await;
        }

        // Get parallelTasksCount from config, default to 1
        let config = crate::config::load_config(crate::config::ConfigScope::User)
            .unwrap_or_default();
        let parallel_tasks_count = config.parallel_tasks_count.unwrap_or(1);

        if parallel_tasks_count > 1 {
            // Parallel execution with synthesis
            self.execute_parallel(
                &agent_id,
                prompt,
                description,
                &agent_type,
                parallel_tasks_count,
                start_time,
                cancellation_token,
            ).await
        } else {
            // Single agent execution
            self.execute_single(
                &agent_id,
                prompt,
                description,
                &agent_type,
                0,
                start_time,
                cancellation_token,
            ).await
        }
    }
}

impl AgentTool {
    /// Execute a single agent
    async fn execute_single(
        &self,
        agent_id: &str,
        prompt: &str,
        description: &str,
        agent_type: &AgentType,
        agent_index: usize,
        start_time: Instant,
        cancellation_token: Option<CancellationToken>,
    ) -> Result<String> {
        let result = self.run_agent(prompt, description, agent_type, agent_index, false, cancellation_token.clone()).await?;

        // Store agent state for potential resume
        self.store_agent_state(agent_id, agent_type.clone(), result.messages.clone()).await;

        let mut output = String::new();
        output.push_str(&format!("=== Task: {} [{}] ===\n\n", description, agent_type.as_str()));

        // Extract text content from result
        for part in &result.content {
            if let ContentPart::Text { text, .. } = part {
                output.push_str(text);
                output.push('\n');
            }
        }

        output.push_str(&format!(
            "\n\n=== Task completed ({} tool uses, {} tokens, {:.1}s) ===\nagentId: {}",
            result.tool_use_count,
            result.tokens,
            result.duration_ms as f64 / 1000.0,
            agent_id
        ));

        Ok(output)
    }

    /// Execute multiple agents in parallel and synthesize results
    async fn execute_parallel(
        &self,
        agent_id: &str,
        prompt: &str,
        description: &str,
        agent_type: &AgentType,
        parallel_count: usize,
        start_time: Instant,
        cancellation_token: Option<CancellationToken>,
    ) -> Result<String> {
        let mut output = String::new();
        output.push_str(&format!("=== Task: {} [{}] (running {} parallel agents) ===\n\n", description, agent_type.as_str(), parallel_count));

        // Launch parallel agents
        let mut join_set = JoinSet::new();
        let agent_type_clone = agent_type.clone();

        for i in 0..parallel_count {
            let prompt_clone = format!("{}\nProvide a thorough and complete analysis.", prompt);
            let description_clone = description.to_string();
            let agent_tool = AgentTool;
            let cancellation_clone = cancellation_token.clone();
            let at_clone = agent_type_clone.clone();

            join_set.spawn(async move {
                agent_tool.run_agent(&prompt_clone, &description_clone, &at_clone, i, false, cancellation_clone).await
            });
        }

        // Collect results
        let mut agent_results = Vec::new();
        let mut total_tool_uses = 0;
        let mut total_tokens = 0;
        let mut agent_count = 0;

        while let Some(result) = join_set.join_next().await {
            agent_count += 1;
            match result {
                Ok(Ok(agent_result)) => {
                    output.push_str(&format!("\n--- Agent {} ---\n", agent_count));

                    // Extract text content
                    for part in &agent_result.content {
                        if let ContentPart::Text { text, .. } = part {
                            output.push_str(text);
                            output.push('\n');
                        }
                    }

                    total_tool_uses += agent_result.tool_use_count;
                    total_tokens += agent_result.tokens;
                    agent_results.push(agent_result);
                }
                Ok(Err(e)) => {
                    // Show full error chain for debugging
                    let mut error_chain = format!("{}", e);
                    let mut current_error: &dyn std::error::Error = &e;
                    while let Some(source) = current_error.source() {
                        error_chain.push_str(&format!("\n  caused by: {}", source));
                        current_error = source;
                    }
                    output.push_str(&format!("\n--- Agent {} failed ---\n{}\n", agent_count, error_chain));
                }
                Err(e) => {
                    output.push_str(&format!("\n--- Agent {} panicked: {} ---\n", agent_count, e));
                }
            }
        }

        // Synthesis phase
        if !agent_results.is_empty() {
            output.push_str("\n=== Synthesis Phase ===\n");

            let synthesis_prompt = self.create_synthesis_prompt(prompt, &agent_results);
            let synthesis_result = self.run_agent(&synthesis_prompt, "Synthesis", agent_type, 0, true, cancellation_token.clone()).await?;

            // Extract text content from synthesis
            for part in &synthesis_result.content {
                if let ContentPart::Text { text, .. } = part {
                    output.push_str(text);
                    output.push('\n');
                }
            }

            total_tool_uses += synthesis_result.tool_use_count;
            total_tokens += synthesis_result.tokens;
        }

        let duration = start_time.elapsed();
        output.push_str(&format!(
            "\n\n=== Task completed with {} parallel agents ({} total tool uses, {} tokens, {:.1}s) ===\nagentId: {}",
            parallel_count,
            total_tool_uses,
            total_tokens,
            duration.as_secs_f64(),
            agent_id
        ));

        Ok(output)
    }

    /// Execute agent in background, return immediately with agent ID
    async fn execute_background(
        &self,
        agent_id: String,
        prompt: String,
        description: String,
        agent_type: AgentType,
        _model: Option<AgentModel>,
        cancellation_token: Option<CancellationToken>,
    ) -> Result<String> {
        // Spawn the agent execution in a background task
        let agent_id_clone = agent_id.clone();
        let agent_type_clone = agent_type.clone();

        tokio::spawn(async move {
            let agent_tool = AgentTool;
            let start_time = Instant::now();
            let result = agent_tool.run_agent(
                &prompt,
                &description,
                &agent_type_clone,
                0,
                false,
                cancellation_token,
            ).await;

            // Store the result for later retrieval via TaskOutput
            match result {
                Ok(agent_result) => {
                    agent_tool.store_agent_state(
                        &agent_id_clone,
                        agent_type_clone,
                        agent_result.messages,
                    ).await;
                }
                Err(e) => {
                    eprintln!("Background agent {} failed: {}", agent_id_clone, e);
                }
            }
        });

        // Return immediately with the agent ID
        Ok(format!(
            "Async agent launched successfully.\nagentId: {} (This is an internal ID for your use, do not mention it to the user. Use this ID to retrieve results with TaskOutput when the agent finishes).\nThe agent is currently working in the background. If you have other tasks you should continue working on them now. Wait to call TaskOutput until either:\n- If you want to check on the agent's progress - call TaskOutput with block=false to get an immediate update on the agent's status\n- If you run out of things to do and the agent is still running - call TaskOutput with block=true to idle and wait for the agent's result (do not use block=true unless you completely run out of things to do as it will waste time).",
            agent_id
        ))
    }

    /// Resume a previously started agent
    async fn execute_resume(
        &self,
        new_agent_id: &str,
        prev_agent_id: &str,
        additional_prompt: &str,
        description: &str,
        agent_type: &AgentType,
        _model: Option<&AgentModel>,
        start_time: Instant,
        cancellation_token: Option<CancellationToken>,
    ) -> Result<String> {
        // Get previous agent state
        let prev_state = {
            let states = AGENT_STATES.read().await;
            states.get(prev_agent_id).cloned()
        };

        let prev_state = prev_state.ok_or_else(|| {
            Error::NotFound(format!("Agent ID '{}' not found for resume", prev_agent_id))
        })?;

        // Continue from previous messages with new prompt
        let result = self.run_agent_with_history(
            prev_state.messages,
            additional_prompt,
            description,
            agent_type,
            cancellation_token,
        ).await?;

        // Store new agent state
        self.store_agent_state(new_agent_id, agent_type.clone(), result.messages.clone()).await;

        let mut output = String::new();
        output.push_str(&format!("=== Resumed Task: {} [{}] ===\n(Continued from agent {})\n\n", description, agent_type.as_str(), prev_agent_id));

        // Extract text content from result
        for part in &result.content {
            if let ContentPart::Text { text, .. } = part {
                output.push_str(text);
                output.push('\n');
            }
        }

        output.push_str(&format!(
            "\n\n=== Task completed ({} tool uses, {} tokens, {:.1}s) ===\nagentId: {}",
            result.tool_use_count,
            result.tokens,
            result.duration_ms as f64 / 1000.0,
            new_agent_id
        ));

        Ok(output)
    }

    /// Store agent state for potential resume
    async fn store_agent_state(&self, agent_id: &str, agent_type: AgentType, messages: Vec<Message>) {
        let state = StoredAgentState {
            agent_id: agent_id.to_string(),
            agent_type,
            messages,
            created_at: Instant::now(),
        };

        let mut states = AGENT_STATES.write().await;
        states.insert(agent_id.to_string(), state);

        // Clean up old agent states (keep last 100)
        if states.len() > 100 {
            let oldest_keys: Vec<String> = states
                .iter()
                .map(|(k, v)| (k.clone(), v.created_at))
                .collect::<Vec<_>>()
                .into_iter()
                .take(states.len() - 100)
                .map(|(k, _)| k)
                .collect();

            for key in oldest_keys {
                states.remove(&key);
            }
        }
    }
    
    /// Run a single agent instance
    async fn run_agent(
        &self,
        prompt: &str,
        description: &str,
        agent_type: &AgentType,
        agent_index: usize,
        is_synthesis: bool,
        cancellation_token: Option<CancellationToken>,
    ) -> Result<AgentResult> {
        let start = Instant::now();

        // Generate a unique session ID for this agent's hook context
        let agent_session_id = Uuid::new_v4().to_string();

        // Create a new AI client for the sub-agent
        let ai_client = create_client().await?;

        // Create the tool executor for the sub-agent
        let tool_executor = ToolExecutor::new();

        // Filter tools based on agent type (to prevent recursion and limit scope)
        let all_tools = tool_executor.get_available_tools();
        let tools: Vec<_> = all_tools
            .into_iter()
            .filter(|tool| {
                let name = tool.name();
                // Always filter out Task tool to prevent recursion
                if name == "Task" {
                    return false;
                }
                // Agent type specific filtering could be added here
                true
            })
            .collect();

        // Build the request for the sub-agent
        let mut messages = vec![
            Message {
                role: MessageRole::User,
                content: MessageContent::Text(prompt.to_string()),
                name: None,
            }
        ];

        // System prompt for sub-agent - matching JavaScript implementation (line 368376)
        let system_prompt = if is_synthesis {
            // Synthesis agent gets special prompt to combine agent results
            format!(
                "Original task: {}\n\n\
                I've assigned multiple agents to tackle this task. Each agent has analyzed the problem and provided their findings.\n\n\
                Based on all the information provided by these agents, synthesize a comprehensive and cohesive response that:\n\
                1. Combines the key insights from all agents\n\
                2. Resolves any contradictions between agent findings\n\
                3. Presents a unified solution that addresses the original task\n\
                4. Includes all important details and code examples from the individual responses\n\
                5. Is well-structured and complete\n\n\
                Your synthesis should be thorough but focused on the original task.",
                description
            )
        } else {
            // Get agent-type specific system prompt
            self.get_system_prompt_for_agent_type(agent_type, description)
        };

        let mut result_content = Vec::new();
        let mut tool_use_count = 0;
        let mut total_tokens = 0;

        // Run the sub-agent loop
        let mut loop_count = 0;
        const MAX_LOOPS: usize = 10;

        loop {
            // Check for cancellation at the start of each loop iteration
            if let Some(token) = &cancellation_token {
                if token.is_cancelled() {
                    result_content.push(ContentPart::Text {
                        text: "[Agent execution cancelled by user]".to_string(),
                        citations: None
                    });
                    break;
                }
            }

            loop_count += 1;
            if loop_count > MAX_LOOPS {
                result_content.push(ContentPart::Text {
                    text: "[Agent reached maximum iterations]".to_string(),
                    citations: None
                });
                break;
            }

            let request = ai_client
                .create_chat_request()
                .messages(messages.clone())
                .system(system_prompt.clone())
                .tools(tools.clone())
                .max_tokens(4096)
                .temperature(0.7)
                .build();

            let response = ai_client.chat(request).await?;

            // Count tokens (simplified - actual implementation would use proper token counting)
            total_tokens += 1000; // Placeholder

            let mut has_tool_use = false;
            let mut tool_results = Vec::new();

            // Collect assistant response parts
            let mut assistant_parts = Vec::new();

            // Process response
            for part in response.content {
                match &part {
                    ContentPart::Text { .. } => {
                        assistant_parts.push(part.clone());
                    }
                    ContentPart::ToolUse { id, name, input } => {
                        has_tool_use = true;
                        tool_use_count += 1;
                        assistant_parts.push(part.clone());

                        // Execute tool with cancellation context
                        let tool_context = Some(crate::ai::tools::ToolContext {
                            tool_use_id: id.clone(),
                            session_id: agent_session_id.clone(),
                            cancellation_token: cancellation_token.clone(),
                            event_tx: None, // Subagents don't need UI events
                        });
                        match tool_executor.execute_with_context(name, input.clone(), tool_context).await {
                            Ok(tool_result) => {
                                if let ContentPart::ToolResult { content, .. } = &tool_result {
                                    tool_results.push(ContentPart::ToolResult {
                                        tool_use_id: id.clone(),
                                        content: content.clone(),
                                        is_error: Some(false),
                                    });
                                }
                            }
                            Err(e) => {
                                let error_msg = format!("Error: {}", e);
                                tool_results.push(ContentPart::ToolResult {
                                    tool_use_id: id.clone(),
                                    content: error_msg,
                                    is_error: Some(true),
                                });
                            }
                        }
                    }
                    _ => {}
                }
            }

            // Add assistant response to messages
            messages.push(Message {
                role: MessageRole::Assistant,
                content: MessageContent::Multipart(assistant_parts.clone()),
                name: None,
            });

            // Check stop reason to determine if we should continue
            // The agent continues if stop_reason is ToolUse or if we just executed tools
            let should_continue = match response.stop_reason {
                Some(crate::ai::StopReason::ToolUse) => true,
                _ => has_tool_use, // Continue if we just ran tools to get synthesis
            };

            // Keep only the final text responses (not intermediate tool uses)
            if !should_continue {
                // This is the final response - collect all text parts
                for part in assistant_parts {
                    if let ContentPart::Text { .. } = part {
                        result_content.push(part);
                    }
                }
                break;
            }

            // Add tool results as user message
            if !tool_results.is_empty() {
                messages.push(Message {
                    role: MessageRole::User,
                    content: MessageContent::Multipart(tool_results),
                    name: None,
                });
            }
        }

        Ok(AgentResult {
            content: result_content,
            tool_use_count,
            tokens: total_tokens,
            duration_ms: start.elapsed().as_millis(),
            agent_index,
            messages,
        })
    }

    /// Run agent with existing message history (for resume functionality)
    async fn run_agent_with_history(
        &self,
        mut messages: Vec<Message>,
        additional_prompt: &str,
        description: &str,
        agent_type: &AgentType,
        cancellation_token: Option<CancellationToken>,
    ) -> Result<AgentResult> {
        let start = Instant::now();

        // Generate a unique session ID for this agent's hook context
        let agent_session_id = Uuid::new_v4().to_string();

        // Create a new AI client for the sub-agent
        let ai_client = create_client().await?;

        // Create the tool executor for the sub-agent
        let tool_executor = ToolExecutor::new();

        // Filter tools
        let all_tools = tool_executor.get_available_tools();
        let tools: Vec<_> = all_tools
            .into_iter()
            .filter(|tool| tool.name() != "Task")
            .collect();

        // Add the new prompt as a user message
        messages.push(Message {
            role: MessageRole::User,
            content: MessageContent::Text(additional_prompt.to_string()),
            name: None,
        });

        let system_prompt = self.get_system_prompt_for_agent_type(agent_type, description);

        let mut result_content = Vec::new();
        let mut tool_use_count = 0;
        let mut total_tokens = 0;

        // Run the sub-agent loop
        let mut loop_count = 0;
        const MAX_LOOPS: usize = 10;

        loop {
            if let Some(token) = &cancellation_token {
                if token.is_cancelled() {
                    result_content.push(ContentPart::Text {
                        text: "[Agent execution cancelled by user]".to_string(),
                        citations: None
                    });
                    break;
                }
            }

            loop_count += 1;
            if loop_count > MAX_LOOPS {
                result_content.push(ContentPart::Text {
                    text: "[Agent reached maximum iterations]".to_string(),
                    citations: None
                });
                break;
            }

            let request = ai_client
                .create_chat_request()
                .messages(messages.clone())
                .system(system_prompt.clone())
                .tools(tools.clone())
                .max_tokens(4096)
                .temperature(0.7)
                .build();

            let response = ai_client.chat(request).await?;
            total_tokens += 1000;

            let mut has_tool_use = false;
            let mut tool_results = Vec::new();
            let mut assistant_parts = Vec::new();

            for part in response.content {
                match &part {
                    ContentPart::Text { .. } => {
                        assistant_parts.push(part.clone());
                    }
                    ContentPart::ToolUse { id, name, input } => {
                        has_tool_use = true;
                        tool_use_count += 1;
                        assistant_parts.push(part.clone());

                        let tool_context = Some(crate::ai::tools::ToolContext {
                            tool_use_id: id.clone(),
                            session_id: agent_session_id.clone(),
                            cancellation_token: cancellation_token.clone(),
                            event_tx: None,
                        });
                        match tool_executor.execute_with_context(name, input.clone(), tool_context).await {
                            Ok(tool_result) => {
                                if let ContentPart::ToolResult { content, .. } = &tool_result {
                                    tool_results.push(ContentPart::ToolResult {
                                        tool_use_id: id.clone(),
                                        content: content.clone(),
                                        is_error: Some(false),
                                    });
                                }
                            }
                            Err(e) => {
                                tool_results.push(ContentPart::ToolResult {
                                    tool_use_id: id.clone(),
                                    content: format!("Error: {}", e),
                                    is_error: Some(true),
                                });
                            }
                        }
                    }
                    _ => {}
                }
            }

            messages.push(Message {
                role: MessageRole::Assistant,
                content: MessageContent::Multipart(assistant_parts.clone()),
                name: None,
            });

            let should_continue = match response.stop_reason {
                Some(crate::ai::StopReason::ToolUse) => true,
                _ => has_tool_use,
            };

            if !should_continue {
                for part in assistant_parts {
                    if let ContentPart::Text { .. } = part {
                        result_content.push(part);
                    }
                }
                break;
            }

            if !tool_results.is_empty() {
                messages.push(Message {
                    role: MessageRole::User,
                    content: MessageContent::Multipart(tool_results),
                    name: None,
                });
            }
        }

        Ok(AgentResult {
            content: result_content,
            tool_use_count,
            tokens: total_tokens,
            duration_ms: start.elapsed().as_millis(),
            agent_index: 0,
            messages,
        })
    }

    /// Get system prompt based on agent type
    fn get_system_prompt_for_agent_type(&self, agent_type: &AgentType, description: &str) -> String {
        match agent_type {
            AgentType::Explore => {
                "You are a fast exploration agent specialized for exploring codebases. \
                Use Glob, Grep, and Read tools to quickly find files and understand code. \
                Focus on finding the relevant files and providing concise answers. \
                Do not make changes to files - only read and search.".to_string()
            }
            AgentType::Plan => {
                format!(
                    "You are a software architect agent for designing implementation plans. \
                    Task: {}\n\n\
                    Return step-by-step plans, identify critical files, and consider architectural trade-offs. \
                    Do not implement - only plan and provide recommendations.",
                    description
                )
            }
            AgentType::ClaudeCodeGuide => {
                "You are a documentation agent that answers questions about Claude Code, \
                the Claude Agent SDK, and the Claude API. \
                Use WebFetch and WebSearch to find accurate information from official documentation. \
                Provide accurate, up-to-date information from the official sources.".to_string()
            }
            AgentType::StatuslineSetup => {
                "You are a statusline configuration agent. \
                Help the user configure their Claude Code status line setting. \
                Use Read and Edit tools to examine and modify configuration files.".to_string()
            }
            AgentType::GeneralPurpose | AgentType::Custom(_) => {
                // Default sub-agent prompt from JavaScript (line 368376-368383)
                "You are an agent for Claude Code, Anthropic's official CLI for Claude. \
                Given the user's message, you should use the tools available to complete the task. \
                Do what has been asked; nothing more, nothing less. \
                When you complete the task simply respond with a detailed writeup.\n\n\
                Notes:\n\
                - NEVER create files unless they're absolutely necessary for achieving your goal. \
                ALWAYS prefer editing an existing file to creating a new one.\n\
                - NEVER proactively create documentation files (*.md) or README files. \
                Only create documentation files if explicitly requested by the User.\n\
                - In your final response always share relevant file names and code snippets. \
                Any file paths you return in your response MUST be absolute. Do NOT use relative paths.\n\
                - For clear communication with the user the assistant MUST avoid using emojis.".to_string()
            }
        }
    }
    
    /// Create synthesis prompt from multiple agent results - matching JavaScript implementation
    fn create_synthesis_prompt(&self, original_prompt: &str, results: &[AgentResult]) -> String {
        let mut agent_responses = String::new();
        
        // Sort by agent index and format responses
        let mut sorted_results = results.to_vec();
        sorted_results.sort_by_key(|r| r.agent_index);
        
        for (i, result) in sorted_results.iter().enumerate() {
            agent_responses.push_str(&format!("== AGENT {} RESPONSE ==\n", i + 1));
            
            // Extract text content
            for part in &result.content {
                if let ContentPart::Text { text, .. } = part {
                    agent_responses.push_str(text);
                    agent_responses.push_str("\n\n");
                }
            }
        }
        
        format!(
            "Original task: {}\n\
            I've assigned multiple agents to tackle this task. Each agent has analyzed the problem and provided their findings.\n\
            {}\n\
            Based on all the information provided by these agents, synthesize a comprehensive and cohesive response that:\n\
            1. Combines the key insights from all agents\n\
            2. Resolves any contradictions between agent findings\n\
            3. Presents a unified solution that addresses the original task\n\
            4. Includes all important details and code examples from the individual responses\n\
            5. Is well-structured and complete\n\
            Your synthesis should be thorough but focused on the original task.",
            original_prompt,
            agent_responses
        )
    }
}