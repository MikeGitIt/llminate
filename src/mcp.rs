use crate::config::{self, ConfigScope, McpServerConfig};
use crate::error::{Error, Result};
use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot};

#[derive(Debug, Clone, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum TransportType {
    Stdio,
    Sse,
    Http,
}

impl std::fmt::Display for TransportType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportType::Stdio => write!(f, "stdio"),
            TransportType::Sse => write!(f, "sse"),
            TransportType::Http => write!(f, "http"),
        }
    }
}

#[derive(Debug)]
pub struct McpClient {
    name: String,
    transport: TransportType,
    process: Option<Child>,
    sender: mpsc::UnboundedSender<McpRequest>,
    receiver: mpsc::UnboundedReceiver<McpResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpRequest {
    pub id: String,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResponse {
    pub id: String,
    pub result: Option<Value>,
    pub error: Option<McpError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpError {
    pub code: i32,
    pub message: String,
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpCommand {
    pub name: String,
    pub description: String,
    pub args: Vec<String>,
}

/// Start MCP server
pub async fn serve(debug: bool, verbose: bool) -> Result<()> {
    let cwd = std::env::current_dir()?;
    
    if !cwd.exists() {
        eprintln!("Error: Directory {} does not exist", cwd.display());
        std::process::exit(1);
    }
    
    println!("Starting MCP server in {}...", cwd.display());
    
    // In a real implementation, this would start the MCP server
    // For now, we'll simulate it
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
}

/// Add an MCP server
pub async fn add_server(
    name: &str,
    command_or_url: &str,
    args: Vec<String>,
    scope: ConfigScope,
    transport: TransportType,
    env: Vec<String>,
    headers: Vec<String>,
) -> Result<()> {
    let mut config = config::load_config(scope)?;
    
    if config.mcp_servers.is_none() {
        config.mcp_servers = Some(HashMap::new());
    }
    
    let servers = config.mcp_servers.as_mut().unwrap();
    
    let server_config = match transport {
        TransportType::Stdio => {
            let env_map = parse_env_vars(&env)?;
            
            McpServerConfig {
                transport_type: Some("stdio".to_string()),
                command: Some(command_or_url.to_string()),
                args: if args.is_empty() { None } else { Some(args) },
                url: None,
                headers: None,
                env: if env_map.is_empty() { None } else { Some(env_map) },
            }
        }
        TransportType::Sse | TransportType::Http => {
            let headers_map = parse_headers(&headers)?;
            
            McpServerConfig {
                transport_type: Some(transport.to_string()),
                command: None,
                args: None,
                url: Some(command_or_url.to_string()),
                headers: if headers_map.is_empty() { None } else { Some(headers_map) },
                env: None,
            }
        }
    };
    
    servers.insert(name.to_string(), server_config);
    config::save_config(scope, &config)?;
    
    println!(
        "Added {} MCP server {} to {} config",
        transport, name, scope
    );
    
    Ok(())
}

/// Remove an MCP server
pub async fn remove_server(name: &str, scope: Option<ConfigScope>) -> Result<()> {
    if let Some(scope) = scope {
        // Remove from specific scope
        let mut config = config::load_config(scope)?;
        
        if let Some(ref mut servers) = config.mcp_servers {
            if servers.remove(name).is_some() {
                config::save_config(scope, &config)?;
                println!("Removed MCP server {} from {} config", name, scope);
                return Ok(());
            }
        }
        
        return Err(Error::Config(format!(
            "No MCP server found with name: \"{}\"",
            name
        )));
    }
    
    // Try to find and remove from any scope
    let mut found_scopes = Vec::new();
    
    for scope in [ConfigScope::Local, ConfigScope::Project, ConfigScope::User] {
        if let Ok(config) = config::load_config(scope) {
            if let Some(servers) = &config.mcp_servers {
                if servers.contains_key(name) {
                    found_scopes.push(scope);
                }
            }
        }
    }
    
    match found_scopes.len() {
        0 => Err(Error::Config(format!(
            "No MCP server found with name: \"{}\"",
            name
        ))),
        1 => {
            let scope = found_scopes[0];
            let mut config = config::load_config(scope)?;
            config.mcp_servers.as_mut().unwrap().remove(name);
            config::save_config(scope, &config)?;
            println!("Removed MCP server \"{}\" from {} config", name, scope);
            Ok(())
        }
        _ => {
            eprintln!("MCP server \"{}\" exists in multiple scopes:", name);
            for scope in &found_scopes {
                eprintln!("  - {}", scope);
            }
            eprintln!("\nTo remove from a specific scope, use:");
            for scope in &found_scopes {
                eprintln!("  llminate mcp remove \"{}\" -s {}", name, scope);
            }
            std::process::exit(1);
        }
    }
}

/// List all MCP servers
pub async fn list_servers() -> Result<()> {
    let servers = config::get_all_mcp_servers()?;
    
    if servers.is_empty() {
        println!("No MCP servers configured. Use `llminate mcp add` to add a server.");
    } else {
        for (name, config) in servers {
            match config.transport_type.as_deref() {
                Some("sse") => {
                    println!("{}: {} (SSE)", name, config.url.unwrap_or_default());
                }
                Some("http") => {
                    println!("{}: {} (HTTP)", name, config.url.unwrap_or_default());
                }
                _ => {
                    let args = config.args.unwrap_or_default();
                    println!(
                        "{}: {} {}",
                        name,
                        config.command.unwrap_or_default(),
                        args.join(" ")
                    );
                }
            }
        }
    }
    
    Ok(())
}

/// Get details about an MCP server
pub async fn get_server(name: &str) -> Result<()> {
    let servers = config::get_all_mcp_servers()?;
    
    let server = servers
        .get(name)
        .ok_or_else(|| Error::Config(format!("No MCP server found with name: {}", name)))?;
    
    println!("{}:", name);
    
    match server.transport_type.as_deref() {
        Some("sse") => {
            println!("  Type: sse");
            println!("  URL: {}", server.url.as_deref().unwrap_or(""));
            if let Some(headers) = &server.headers {
                println!("  Headers:");
                for (k, v) in headers {
                    println!("    {}: {}", k, v);
                }
            }
        }
        Some("http") => {
            println!("  Type: http");
            println!("  URL: {}", server.url.as_deref().unwrap_or(""));
            if let Some(headers) = &server.headers {
                println!("  Headers:");
                for (k, v) in headers {
                    println!("    {}: {}", k, v);
                }
            }
        }
        _ => {
            println!("  Type: stdio");
            println!("  Command: {}", server.command.as_deref().unwrap_or(""));
            if let Some(args) = &server.args {
                println!("  Args: {}", args.join(" "));
            }
            if let Some(env) = &server.env {
                println!("  Environment:");
                for (k, v) in env {
                    println!("    {}={}", k, v);
                }
            }
        }
    }
    
    Ok(())
}

/// Add server from JSON
pub async fn add_server_json(name: &str, json: &str, scope: ConfigScope) -> Result<()> {
    let server_config: McpServerConfig = serde_json::from_str(json)
        .context("Invalid JSON for MCP server configuration")?;
    
    let mut config = config::load_config(scope)?;
    
    if config.mcp_servers.is_none() {
        config.mcp_servers = Some(HashMap::new());
    }
    
    config.mcp_servers.as_mut().unwrap().insert(name.to_string(), server_config);
    config::save_config(scope, &config)?;
    
    println!("Added MCP server {} to {} config", name, scope);
    
    Ok(())
}

/// Import servers from Claude Desktop configuration
pub async fn add_from_claude_desktop(scope: ConfigScope) -> Result<()> {
    let claude_config_path = get_claude_desktop_config_path()?;
    
    if !claude_config_path.exists() {
        println!("No Claude Desktop configuration found.");
        return Ok(());
    }
    
    let content = std::fs::read_to_string(&claude_config_path)?;
    let claude_config: Value = serde_json::from_str(&content)?;
    
    if let Some(mcp_servers) = claude_config.get("mcpServers").and_then(|v| v.as_object()) {
        let mut config = config::load_config(scope)?;
        
        if config.mcp_servers.is_none() {
            config.mcp_servers = Some(HashMap::new());
        }
        
        let servers = config.mcp_servers.as_mut().unwrap();
        let mut imported = 0;
        
        for (name, value) in mcp_servers {
            if let Ok(server_config) = serde_json::from_value::<McpServerConfig>(value.clone()) {
                servers.insert(name.clone(), server_config);
                imported += 1;
            }
        }
        
        if imported > 0 {
            config::save_config(scope, &config)?;
            println!(
                "Successfully imported {} MCP server{} to {} config.",
                imported,
                if imported != 1 { "s" } else { "" },
                scope
            );
        } else {
            println!("No servers were imported.");
        }
    } else {
        println!("No MCP servers found in Claude Desktop configuration.");
    }
    
    Ok(())
}

/// Reset project MCP choices
pub async fn reset_project_choices() -> Result<()> {
    let mut config = config::load_config(ConfigScope::Local)?;
    
    config.enabled_mcpjson_servers = Some(Vec::new());
    config.disabled_mcpjson_servers = Some(Vec::new());
    config.enable_all_project_mcp_servers = Some(false);
    
    config::save_config(ConfigScope::Local, &config)?;
    
    println!("All project-scoped (.mcp.json) server approvals and rejections have been reset.");
    println!("You will be prompted for approval next time you start llminate.");
    
    Ok(())
}

/// Parse MCP configuration from string or file
pub fn parse_config(config_str: &str) -> Result<HashMap<String, McpServerConfig>> {
    // Try to parse as JSON first
    if let Ok(json_config) = serde_json::from_str::<HashMap<String, McpServerConfig>>(config_str) {
        return Ok(json_config);
    }
    
    // Try to load as file
    if let Ok(content) = std::fs::read_to_string(config_str) {
        let file_config: HashMap<String, McpServerConfig> = serde_json::from_str(&content)
            .context("Invalid MCP configuration file")?;
        return Ok(file_config);
    }
    
    Err(Error::Config("Invalid MCP configuration".to_string()))
}

/// Parse environment variables
fn parse_env_vars(env: &[String]) -> Result<HashMap<String, String>> {
    let mut map = HashMap::new();
    
    for var in env {
        let parts: Vec<&str> = var.splitn(2, '=').collect();
        if parts.len() != 2 {
            return Err(Error::InvalidInput(format!(
                "Invalid environment variable format: {}",
                var
            )));
        }
        map.insert(parts[0].to_string(), parts[1].to_string());
    }
    
    Ok(map)
}

/// Parse HTTP headers
fn parse_headers(headers: &[String]) -> Result<HashMap<String, String>> {
    let mut map = HashMap::new();
    
    for header in headers {
        let parts: Vec<&str> = header.splitn(2, ':').collect();
        if parts.len() != 2 {
            return Err(Error::InvalidInput(format!(
                "Invalid header format: {}",
                header
            )));
        }
        map.insert(
            parts[0].trim().to_string(),
            parts[1].trim().to_string(),
        );
    }
    
    Ok(map)
}

/// Get Claude Desktop config path
fn get_claude_desktop_config_path() -> Result<std::path::PathBuf> {
    #[cfg(target_os = "macos")]
    {
        Ok(dirs::home_dir()
            .ok_or_else(|| Error::Config("Could not determine home directory".to_string()))?
            .join("Library")
            .join("Application Support")
            .join("Claude")
            .join("claude_desktop_config.json"))
    }
    
    #[cfg(target_os = "windows")]
    {
        Ok(dirs::config_dir()
            .ok_or_else(|| Error::Config("Could not determine config directory".to_string()))?
            .join("Claude")
            .join("claude_desktop_config.json"))
    }
    
    #[cfg(target_os = "linux")]
    {
        Ok(dirs::config_dir()
            .ok_or_else(|| Error::Config("Could not determine config directory".to_string()))?
            .join("Claude")
            .join("claude_desktop_config.json"))
    }
}

/// Start an MCP client
pub async fn start_client(name: String, config: McpServerConfig) -> Result<McpClient> {
    match config.transport_type.as_deref() {
        Some("stdio") | None => start_stdio_client(name, config).await,
        Some("sse") => start_sse_client(name, config).await,
        Some("http") => start_http_client(name, config).await,
        Some(t) => Err(Error::Config(format!("Unknown transport type: {}", t))),
    }
}

/// Start stdio MCP client
async fn start_stdio_client(name: String, config: McpServerConfig) -> Result<McpClient> {
    let command = config.command
        .ok_or_else(|| Error::Config("Missing command for stdio transport".to_string()))?;
    
    let mut cmd = Command::new(&command);
    
    if let Some(args) = &config.args {
        cmd.args(args);
    }
    
    if let Some(env) = &config.env {
        cmd.envs(env);
    }
    
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    
    let mut process = cmd.spawn()
        .with_context(|| format!("Failed to start MCP server: {}", command))?;
    
    let stdin = process.stdin.take().unwrap();
    let stdout = process.stdout.take().unwrap();
    
    let (tx, rx) = mpsc::unbounded_channel();
    let (response_tx, response_rx) = mpsc::unbounded_channel();
    
    // Spawn task to handle communication
    tokio::spawn(async move {
        handle_stdio_communication(stdin, stdout, rx, response_tx).await;
    });
    
    Ok(McpClient {
        name,
        transport: TransportType::Stdio,
        process: Some(process),
        sender: tx,
        receiver: response_rx,
    })
}

/// Start SSE MCP client
/// SSE transport uses Server-Sent Events for receiving messages and HTTP POST for sending
async fn start_sse_client(name: String, config: McpServerConfig) -> Result<McpClient> {
    let url = config.url
        .ok_or_else(|| Error::Config("Missing URL for SSE transport".to_string()))?;

    let (tx, rx) = mpsc::unbounded_channel();
    let (response_tx, response_rx) = mpsc::unbounded_channel();

    // Build headers
    let mut headers = reqwest::header::HeaderMap::new();
    if let Some(config_headers) = &config.headers {
        for (key, value) in config_headers {
            if let (Ok(name), Ok(val)) = (
                reqwest::header::HeaderName::from_bytes(key.as_bytes()),
                reqwest::header::HeaderValue::from_str(value)
            ) {
                headers.insert(name, val);
            }
        }
    }

    let url_clone = url.clone();
    let headers_clone = headers.clone();

    // Spawn SSE handler task
    tokio::spawn(async move {
        handle_sse_communication(url_clone, headers_clone, rx, response_tx).await;
    });

    Ok(McpClient {
        name,
        transport: TransportType::Sse,
        process: None,
        sender: tx,
        receiver: response_rx,
    })
}

/// Start HTTP MCP client
/// HTTP transport uses HTTP POST for both sending and receiving (request/response)
async fn start_http_client(name: String, config: McpServerConfig) -> Result<McpClient> {
    let url = config.url
        .ok_or_else(|| Error::Config("Missing URL for HTTP transport".to_string()))?;

    let (tx, rx) = mpsc::unbounded_channel();
    let (response_tx, response_rx) = mpsc::unbounded_channel();

    // Build headers
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        reqwest::header::HeaderValue::from_static("application/json")
    );
    if let Some(config_headers) = &config.headers {
        for (key, value) in config_headers {
            if let (Ok(name), Ok(val)) = (
                reqwest::header::HeaderName::from_bytes(key.as_bytes()),
                reqwest::header::HeaderValue::from_str(value)
            ) {
                headers.insert(name, val);
            }
        }
    }

    // Spawn HTTP handler task
    tokio::spawn(async move {
        handle_http_communication(url, headers, rx, response_tx).await;
    });

    Ok(McpClient {
        name,
        transport: TransportType::Http,
        process: None,
        sender: tx,
        receiver: response_rx,
    })
}

/// Handle SSE communication
async fn handle_sse_communication(
    url: String,
    headers: reqwest::header::HeaderMap,
    mut request_rx: mpsc::UnboundedReceiver<McpRequest>,
    response_tx: mpsc::UnboundedSender<McpResponse>,
) {
    let client = reqwest::Client::new();

    // Connect to SSE endpoint to get the POST endpoint
    let mut post_endpoint: Option<String> = None;

    // First, establish SSE connection
    let sse_response = match client.get(&url)
        .headers(headers.clone())
        .header("Accept", "text/event-stream")
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            eprintln!("Failed to connect to SSE endpoint: {}", e);
            return;
        }
    };

    if !sse_response.status().is_success() {
        eprintln!("SSE connection failed with status: {}", sse_response.status());
        return;
    }

    // Parse SSE stream for endpoint and messages
    let mut stream = sse_response.bytes_stream();
    use futures::StreamExt;

    let mut buffer = String::new();
    let headers_for_post = headers.clone();

    loop {
        tokio::select! {
            Some(request) = request_rx.recv() => {
                // Send request via HTTP POST to the endpoint
                if let Some(ref endpoint) = post_endpoint {
                    let json_rpc = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": request.id,
                        "method": request.method,
                        "params": request.params
                    });

                    match client.post(endpoint)
                        .headers(headers_for_post.clone())
                        .header("Content-Type", "application/json")
                        .json(&json_rpc)
                        .send()
                        .await
                    {
                        Ok(resp) => {
                            if !resp.status().is_success() {
                                eprintln!("POST request failed: {}", resp.status());
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to send POST request: {}", e);
                        }
                    }
                } else {
                    eprintln!("No POST endpoint available yet");
                }
            }
            Some(chunk_result) = stream.next() => {
                match chunk_result {
                    Ok(chunk) => {
                        buffer.push_str(&String::from_utf8_lossy(&chunk));

                        // Parse SSE events from buffer
                        while let Some(event_end) = buffer.find("\n\n") {
                            let event_str = buffer[..event_end].to_string();
                            buffer = buffer[event_end + 2..].to_string();

                            // Parse SSE event
                            for line in event_str.lines() {
                                if line.starts_with("data: ") {
                                    let data = &line[6..];
                                    if let Ok(json) = serde_json::from_str::<Value>(data) {
                                        // Check if this is an endpoint message
                                        if let Some(endpoint) = json.get("endpoint").and_then(|e| e.as_str()) {
                                            post_endpoint = Some(endpoint.to_string());
                                        } else {
                                            // Regular JSON-RPC response
                                            let response = McpResponse {
                                                id: json.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                                                result: json.get("result").cloned(),
                                                error: json.get("error").and_then(|e| serde_json::from_value(e.clone()).ok()),
                                            };
                                            let _ = response_tx.send(response);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("SSE stream error: {}", e);
                        break;
                    }
                }
            }
            else => break
        }
    }
}

/// Handle HTTP communication
async fn handle_http_communication(
    url: String,
    headers: reqwest::header::HeaderMap,
    mut request_rx: mpsc::UnboundedReceiver<McpRequest>,
    response_tx: mpsc::UnboundedSender<McpResponse>,
) {
    let client = reqwest::Client::new();

    while let Some(request) = request_rx.recv().await {
        let json_rpc = serde_json::json!({
            "jsonrpc": "2.0",
            "id": request.id,
            "method": request.method,
            "params": request.params
        });

        match client.post(&url)
            .headers(headers.clone())
            .json(&json_rpc)
            .send()
            .await
        {
            Ok(resp) => {
                if resp.status().is_success() {
                    match resp.json::<Value>().await {
                        Ok(json) => {
                            let response = McpResponse {
                                id: json.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                                result: json.get("result").cloned(),
                                error: json.get("error").and_then(|e| serde_json::from_value(e.clone()).ok()),
                            };
                            let _ = response_tx.send(response);
                        }
                        Err(e) => {
                            eprintln!("Failed to parse HTTP response: {}", e);
                        }
                    }
                } else {
                    eprintln!("HTTP request failed: {}", resp.status());
                }
            }
            Err(e) => {
                eprintln!("Failed to send HTTP request: {}", e);
            }
        }
    }
}

/// Handle stdio communication
async fn handle_stdio_communication(
    mut stdin: tokio::process::ChildStdin,
    stdout: tokio::process::ChildStdout,
    mut request_rx: mpsc::UnboundedReceiver<McpRequest>,
    response_tx: mpsc::UnboundedSender<McpResponse>,
) {
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();

    loop {
        tokio::select! {
            Some(request) = request_rx.recv() => {
                // Send request as JSON-RPC 2.0
                let json_rpc = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": request.id,
                    "method": request.method,
                    "params": request.params
                });
                let request_str = serde_json::to_string(&json_rpc).unwrap();
                if let Err(e) = stdin.write_all(request_str.as_bytes()).await {
                    eprintln!("Failed to write to stdin: {}", e);
                    break;
                }
                if let Err(e) = stdin.write_all(b"\n").await {
                    eprintln!("Failed to write newline: {}", e);
                    break;
                }
                if let Err(e) = stdin.flush().await {
                    eprintln!("Failed to flush stdin: {}", e);
                    break;
                }
            }
            result = reader.read_line(&mut line) => {
                match result {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        // Parse JSON-RPC response
                        if let Ok(json) = serde_json::from_str::<Value>(&line) {
                            let response = McpResponse {
                                id: json.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                                result: json.get("result").cloned(),
                                error: json.get("error").and_then(|e| serde_json::from_value(e.clone()).ok()),
                            };
                            let _ = response_tx.send(response);
                        }
                        line.clear();
                    }
                    Err(e) => {
                        eprintln!("Failed to read from stdout: {}", e);
                        break;
                    }
                }
            }
        }
    }
}

impl McpClient {
    /// Send a request and wait for response
    pub async fn request(&mut self, method: &str, params: Option<Value>) -> Result<Value> {
        let id = uuid::Uuid::new_v4().to_string();
        let request = McpRequest {
            id: id.clone(),
            method: method.to_string(),
            params,
        };

        self.sender.send(request)
            .map_err(|e| Error::Other(format!("Failed to send request: {}", e)))?;

        // Wait for response with timeout
        let timeout = tokio::time::Duration::from_secs(30);
        let start = tokio::time::Instant::now();

        loop {
            if start.elapsed() > timeout {
                return Err(Error::Timeout("MCP request timed out".to_string()));
            }

            match tokio::time::timeout(tokio::time::Duration::from_millis(100), self.receiver.recv()).await {
                Ok(Some(response)) => {
                    if response.id == id {
                        if let Some(error) = response.error {
                            return Err(Error::Other(format!("MCP error: {} (code: {})", error.message, error.code)));
                        }
                        return Ok(response.result.unwrap_or(Value::Null));
                    }
                    // Not our response, continue waiting
                }
                Ok(None) => {
                    return Err(Error::Other("MCP channel closed".to_string()));
                }
                Err(_) => {
                    // Timeout on receive, continue loop
                    continue;
                }
            }
        }
    }

    /// Initialize the MCP connection
    pub async fn initialize(&mut self) -> Result<Value> {
        let params = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "roots": { "listChanged": true },
                "sampling": {}
            },
            "clientInfo": {
                "name": "llminate",
                "version": env!("CARGO_PKG_VERSION")
            }
        });

        let result = self.request("initialize", Some(params)).await?;

        // Send initialized notification (no response expected)
        let notification = McpRequest {
            id: String::new(),
            method: "notifications/initialized".to_string(),
            params: None,
        };
        let _ = self.sender.send(notification);

        Ok(result)
    }

    /// List available tools from the server
    pub async fn list_tools(&mut self) -> Result<Vec<McpTool>> {
        let result = self.request("tools/list", None).await?;

        let tools = result.get("tools")
            .and_then(|t| t.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| {
                        Some(McpTool {
                            name: v.get("name")?.as_str()?.to_string(),
                            description: v.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string(),
                            input_schema: v.get("inputSchema").cloned().unwrap_or(Value::Null),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(tools)
    }

    /// Call a tool
    pub async fn call_tool(&mut self, name: &str, arguments: Value) -> Result<Value> {
        let params = serde_json::json!({
            "name": name,
            "arguments": arguments
        });

        let result = self.request("tools/call", Some(params)).await?;
        Ok(result)
    }

    /// List available resources from the server
    pub async fn list_resources(&mut self) -> Result<Vec<McpResource>> {
        let result = self.request("resources/list", None).await?;

        let resources = result.get("resources")
            .and_then(|r| r.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| {
                        Some(McpResource {
                            uri: v.get("uri")?.as_str()?.to_string(),
                            name: v.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string(),
                            description: v.get("description").and_then(|d| d.as_str()).map(|s| s.to_string()),
                            mime_type: v.get("mimeType").and_then(|m| m.as_str()).map(|s| s.to_string()),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(resources)
    }

    /// Read a resource
    pub async fn read_resource(&mut self, uri: &str) -> Result<String> {
        let params = serde_json::json!({
            "uri": uri
        });

        let result = self.request("resources/read", Some(params)).await?;

        // Extract content from response
        let contents = result.get("contents")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|content| {
                // Try text first, then blob
                content.get("text").and_then(|t| t.as_str())
                    .or_else(|| content.get("blob").and_then(|b| b.as_str()))
            })
            .unwrap_or("");

        Ok(contents.to_string())
    }

    /// Get server name
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// MCP Resource definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResource {
    pub uri: String,
    pub name: String,
    pub description: Option<String>,
    pub mime_type: Option<String>,
}

/// Connect to an MCP server and initialize it
pub async fn connect_and_initialize(name: &str, config: &McpServerConfig) -> Result<McpClient> {
    let mut client = start_client(name.to_string(), config.clone()).await?;

    // Initialize the connection
    client.initialize().await?;

    Ok(client)
}