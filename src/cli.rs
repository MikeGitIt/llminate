use clap::{Parser, Subcommand, Args, ValueEnum};
use crate::config::ConfigScope;
use crate::mcp::TransportType;
use crate::error::Result;
use colored::Colorize;
use std::path::PathBuf;

/// Claude Code - starts an interactive session by default, use -p/--print for non-interactive output
#[derive(Parser, Debug)]
#[command(
    name = "llminate",
    version = crate::VERSION,
    about = format!("{} - starts an interactive session by default, use -p/--print for non-interactive output", crate::PACKAGE_TITLE),
    long_about = None,
    version = crate::VERSION,
    after_help = format!(
        "{}: {}\n{}: {}",
        "Issues".dimmed(),
        crate::ISSUES_URL,
        "Documentation".dimmed(),
        crate::README_URL
    )
)]
pub struct Cli {
    /// Your prompt
    #[arg(value_name = "PROMPT")]
    pub prompt: Option<String>,

    /// Enable debug mode
    #[arg(short = 'd', long)]
    pub debug: bool,

    /// Override verbose mode setting from config
    #[arg(long)]
    pub verbose: bool,

    /// Print response and exit (useful for pipes)
    #[arg(short = 'p', long)]
    pub print: bool,

    /// Output format (only works with --print): "text" (default), "json" (single result), or "stream-json" (realtime streaming)
    #[arg(long, value_enum, default_value = "text")]
    pub output_format: OutputFormat,

    /// Input format (only works with --print): "text" (default), or "stream-json" (realtime streaming input)
    #[arg(long, value_enum, default_value = "text")]
    pub input_format: InputFormat,

    /// [DEPRECATED. Use --debug instead] Enable MCP debug mode (shows MCP server errors)
    #[arg(long)]
    pub mcp_debug: bool,

    /// Bypass all permission checks. Recommended only for sandboxes with no internet access
    #[arg(long)]
    pub dangerously_skip_permissions: bool,

    /// Maximum number of agentic turns in non-interactive mode. This will early exit the conversation after the specified number of turns. (only works with --print)
    #[arg(long, hide = true)]
    pub max_turns: Option<usize>,

    /// Comma or space-separated list of tool names to allow (e.g. "Bash(git:*) Edit")
    #[arg(long, value_delimiter = ' ')]
    pub allowed_tools: Vec<String>,

    /// Comma or space-separated list of tool names to deny (e.g. "Bash(git:*) Edit")
    #[arg(long, value_delimiter = ' ')]
    pub disallowed_tools: Vec<String>,

    /// Load MCP servers from a JSON file or string
    #[arg(long)]
    pub mcp_config: Option<String>,

    /// MCP tool to use for permission prompts (only works with --print)
    #[arg(long, hide = true)]
    pub permission_prompt_tool: Option<String>,

    /// System prompt to use for the session (only works with --print)
    #[arg(long, hide = true)]
    pub system_prompt: Option<String>,

    /// Append a system prompt to the default system prompt (only works with --print)
    #[arg(long, hide = true)]
    pub append_system_prompt: Option<String>,

    /// Permission mode to use for the session
    #[arg(long, value_enum, hide = true)]
    pub permission_mode: Option<PermissionMode>,

    /// Continue the most recent conversation
    #[arg(short = 'c', long)]
    pub continue_conversation: bool,

    /// Resume a conversation - provide a session ID or interactively select a conversation to resume
    #[arg(short = 'r', long, value_name = "SESSION_ID")]
    pub resume: Option<Option<String>>,

    /// Model for the current session. Provide an alias for the latest model (e.g. 'sonnet' or 'opus') or a model's full name (e.g. 'claude-sonnet-4-20250514')
    #[arg(long)]
    pub model: Option<String>,

    // Logging configuration flags
    /// Module-specific log levels (e.g. "llminate=debug,hyper=warn,tokio=info")
    #[arg(long)]
    pub log_modules: Option<String>,

    /// Enable JSON structured logging
    #[arg(long)]
    pub json_logs: bool,

    /// Enable stderr logging
    #[arg(long)]
    pub stderr_logs: bool,

    /// Log file path
    #[arg(long)]
    pub log_file: Option<String>,

    /// Log format style
    #[arg(long, value_enum)]
    pub log_format: Option<LogFormat>,

    /// Include timestamps in logs
    #[arg(long)]
    pub log_timestamps: bool,

    /// Include thread info in logs
    #[arg(long)]
    pub log_thread_info: bool,

    /// Include source location in logs
    #[arg(long)]
    pub log_source_location: bool,

    /// Enable automatic fallback to specified model when default model is overloaded (only works with --print)
    #[arg(long)]
    pub fallback_model: Option<String>,

    /// Additional directories to allow tool access to
    #[arg(long, value_delimiter = ' ')]
    pub add_dir: Vec<PathBuf>,

    /// MCP CLI mode - interact with MCP servers directly (servers, tools, info, grep, resources, read, call)
    /// This is used when the executable is called as mcp-cli (via shell alias)
    #[arg(long, hide = true)]
    pub mcp_cli: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Manage configuration (eg. llminate config set -g theme dark)
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },
    /// Configure and manage MCP servers
    Mcp {
        #[command(subcommand)]
        command: McpCommands,
    },
    /// Migrate from global npm installation to local installation
    MigrateInstaller,
    /// Check the health of your llminate auto-updater
    Doctor,
    /// Check for updates and install if available
    Update,
}

#[derive(Subcommand, Debug)]
pub enum ConfigCommands {
    /// Get a config value
    Get {
        /// Config key
        key: String,
        /// Use global config
        #[arg(short = 'g', long)]
        global: bool,
    },
    /// Set a config value
    Set {
        /// Config key
        key: String,
        /// Config value
        value: String,
        /// Use global config
        #[arg(short = 'g', long)]
        global: bool,
    },
    /// Remove a config value or items from a config array
    #[command(alias = "rm")]
    Remove {
        /// Config key
        key: String,
        /// Values to remove (for arrays)
        values: Vec<String>,
        /// Use global config
        #[arg(short = 'g', long)]
        global: bool,
    },
    /// List all config values
    #[command(alias = "ls")]
    List {
        /// Use global config
        #[arg(short = 'g', long)]
        global: bool,
    },
    /// Add items to a config array (space or comma separated)
    Add {
        /// Config key
        key: String,
        /// Values to add
        values: Vec<String>,
        /// Use global config
        #[arg(short = 'g', long)]
        global: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum McpCommands {
    /// Start the llminate MCP server
    Serve {
        /// Enable debug mode
        #[arg(short = 'd', long)]
        debug: bool,
        /// Override verbose mode setting from config
        #[arg(long)]
        verbose: bool,
    },
    /// Add a server
    Add {
        /// Server name
        name: String,
        /// Command or URL
        command_or_url: String,
        /// Arguments
        args: Vec<String>,
        /// Configuration scope (local, user, or project)
        #[arg(short = 's', long, default_value = "local")]
        scope: ConfigScope,
        /// Transport type (stdio, sse, http)
        #[arg(short = 't', long, default_value = "stdio")]
        transport: TransportType,
        /// Set environment variables (e.g. -e KEY=value)
        #[arg(short = 'e', long)]
        env: Vec<String>,
        /// Set HTTP headers for SSE and HTTP transports (e.g. -H "X-Api-Key: abc123" -H "X-Custom: value")
        #[arg(short = 'H', long)]
        header: Vec<String>,
    },
    /// Remove an MCP server
    Remove {
        /// Server name
        name: String,
        /// Configuration scope (local, user, or project) - if not specified, removes from whichever scope it exists in
        #[arg(short = 's', long)]
        scope: Option<ConfigScope>,
    },
    /// List configured MCP servers
    List,
    /// Get details about an MCP server
    Get {
        /// Server name
        name: String,
    },
    /// Add an MCP server (stdio or SSE) with a JSON string
    AddJson {
        /// Server name
        name: String,
        /// JSON configuration
        json: String,
        /// Configuration scope (local, user, or project)
        #[arg(short = 's', long, default_value = "local")]
        scope: ConfigScope,
    },
    /// Import MCP servers from Claude Desktop (Mac and WSL only)
    AddFromClaudeDesktop {
        /// Configuration scope (local, user, or project)
        #[arg(short = 's', long, default_value = "local")]
        scope: ConfigScope,
    },
    /// Reset all approved and rejected project-scoped (.mcp.json) servers within this project
    ResetProjectChoices,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    /// Plain text output
    Text,
    /// JSON output (single result)
    Json,
    /// Streaming JSON output
    #[value(name = "stream-json")]
    StreamJson,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum InputFormat {
    /// Plain text input
    Text,
    /// Streaming JSON input
    #[value(name = "stream-json")]
    StreamJson,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum LogFormat {
    /// Compact format for production
    Compact,
    /// Full context information
    Full,
    /// Human-readable development format
    Pretty,
    /// JSON structured format
    Json,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum PermissionMode {
    /// Ask for permission
    Ask,
    /// Allow all operations
    Allow,
    /// Deny all operations
    Deny,
}

impl Cli {
    /// Parse CLI arguments
    pub fn parse_args() -> Self {
        Self::parse()
    }

    /// Convert CLI logging flags to LoggingConfig
    pub fn to_logging_config(&self) -> crate::config::LoggingConfig {
        use crate::config::LoggingConfig;
        use std::collections::HashMap;
        
        let mut module_levels = HashMap::new();
        
        // Parse module levels from --log-modules "llminate=debug,hyper=warn"
        if let Some(modules_str) = &self.log_modules {
            for module_spec in modules_str.split(',') {
                if let Some((module, level)) = module_spec.split_once('=') {
                    module_levels.insert(module.trim().to_string(), level.trim().to_string());
                }
            }
        }
        
        LoggingConfig {
            default_level: if self.debug { Some("debug".to_string()) } else { Some("info".to_string()) },
            module_levels: if module_levels.is_empty() { None } else { Some(module_levels) },
            enable_file_logging: Some(!self.print), // File logging in TUI mode, not in print mode
            enable_stderr_logging: Some(self.stderr_logs), // Only enable stderr if explicitly requested
            enable_json_logging: Some(self.json_logs),
            format_style: self.log_format.as_ref().map(|f| match f {
                LogFormat::Compact => "compact".to_string(),
                LogFormat::Full => "full".to_string(),
                LogFormat::Pretty => "pretty".to_string(),
                LogFormat::Json => "json".to_string(),
            }),
            include_timestamps: Some(self.log_timestamps),
            include_thread_info: Some(self.log_thread_info),
            include_source_location: Some(self.log_source_location),
            log_file_path: self.log_file.clone(),
            max_file_size_mb: Some(10), // Default 10MB
            enable_rotation: Some(true),
        }
    }

    /// Execute the CLI command
    pub async fn execute(self) -> Result<()> {
        // Handle deprecated options
        let debug = self.debug || self.mcp_debug;
        
        if self.mcp_debug {
            eprintln!("Warning: --mcp-debug is deprecated. Please use --debug instead.");
        }

        // Initialize telemetry
        crate::telemetry::init().await;

        // Handle --mcp-cli mode (when invoked via mcp-cli shell alias)
        if self.mcp_cli {
            return handle_mcp_cli_mode(self.prompt).await;
        }

        // Handle subcommands
        match self.command {
            Some(Commands::Config { command }) => {
                handle_config_command(command).await?;
            }
            Some(Commands::Mcp { command }) => {
                handle_mcp_command(command, debug).await?;
            }
            Some(Commands::MigrateInstaller) => {
                handle_migrate_installer().await?;
            }
            Some(Commands::Doctor) => {
                handle_doctor().await?;
            }
            Some(Commands::Update) => {
                handle_update().await?;
            }
            None => {
                // Check authentication before main command
                if let Err(_) = crate::auth::get_or_prompt_auth().await {
                    // No valid authentication found - run setup wizard
                    run_authentication_wizard().await?;
                }
                
                // Main interactive session or print mode
                handle_main_command(self, debug).await?;
            }
        }

        Ok(())
    }
}

/// Handle config subcommands
async fn handle_config_command(command: ConfigCommands) -> Result<()> {
    use crate::config;
    
    match command {
        ConfigCommands::Get { key, global } => {
            let scope = if global { ConfigScope::User } else { ConfigScope::Local };
            let value = config::get_config_value(&key, scope)?;
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
        ConfigCommands::Set { key, value, global } => {
            let scope = if global { ConfigScope::User } else { ConfigScope::Local };
            config::set_config_value(&key, &value, scope)?;
            println!("Set {} to {}", key, value);
        }
        ConfigCommands::Remove { key, values, global } => {
            let scope = if global { ConfigScope::User } else { ConfigScope::Local };
            if values.is_empty() {
                config::remove_config_value(&key, scope)?;
                println!("Removed {}", key);
            } else {
                config::remove_from_array(&key, &values, scope == ConfigScope::User)?;
                println!("Removed from {} in {} config: {}", key, scope, values.join(", "));
            }
        }
        ConfigCommands::List { global } => {
            let scope = if global { ConfigScope::User } else { ConfigScope::Local };
            let config = config::load_config(scope)?;
            println!("{}", serde_json::to_string_pretty(&config)?);
        }
        ConfigCommands::Add { key, values, global } => {
            let scope = if global { ConfigScope::User } else { ConfigScope::Local };
            let parsed_values = parse_array_values(values);
            config::add_to_array(&key, &parsed_values, scope == ConfigScope::User)?;
            println!("Added to {} in {} config: {}", key, scope, parsed_values.join(", "));
        }
    }
    
    Ok(())
}

/// Handle MCP subcommands
async fn handle_mcp_command(command: McpCommands, debug: bool) -> Result<()> {
    use crate::mcp;
    
    match command {
        McpCommands::Serve { debug, verbose } => {
            mcp::serve(debug, verbose).await?;
        }
        McpCommands::Add {
            name,
            command_or_url,
            args,
            scope,
            transport,
            env,
            header,
        } => {
            mcp::add_server(&name, &command_or_url, args, scope, transport, env, header).await?;
        }
        McpCommands::Remove { name, scope } => {
            mcp::remove_server(&name, scope).await?;
        }
        McpCommands::List => {
            mcp::list_servers().await?;
        }
        McpCommands::Get { name } => {
            mcp::get_server(&name).await?;
        }
        McpCommands::AddJson { name, json, scope } => {
            mcp::add_server_json(&name, &json, scope).await?;
        }
        McpCommands::AddFromClaudeDesktop { scope } => {
            mcp::add_from_claude_desktop(scope).await?;
        }
        McpCommands::ResetProjectChoices => {
            mcp::reset_project_choices().await?;
        }
    }
    
    Ok(())
}

/// Handle migrate installer command
async fn handle_migrate_installer() -> Result<()> {
    use crate::updater;
    
    if updater::is_running_from_local() {
        println!("Already running from local installation. No migration needed.");
        return Ok(());
    }
    
    // Track telemetry event
    crate::telemetry::track("tengu_migrate_installer_command", None::<serde_json::Value>).await;
    
    // Interactive migration process
    println!("This will migrate llminate from a global npm installation to a local installation.");
    println!();
    println!("Benefits of local installation:");
    println!("  - Automatic updates without admin permissions");
    println!("  - Faster startup times");
    println!("  - Isolated from system npm packages");
    println!();
    println!("The migration will:");
    println!("  1. Install llminate to ~/.claude/local/");
    println!("  2. Set up a shell alias");
    println!("  3. Optionally uninstall the global npm package");
    println!();
    
    // Confirm migration
    if !confirm_action("Do you want to proceed with the migration?")? {
        println!("Migration cancelled.");
        return Ok(());
    }
    
    // Perform migration
    updater::migrate_to_local().await?;
    
    println!();
    println!("{}", "Migration completed successfully!".green());
    println!("Please restart your terminal or run the source command shown above.");
    
    Ok(())
}

/// Handle doctor command
async fn handle_doctor() -> Result<()> {
    use crate::updater;
    
    // Track telemetry event
    crate::telemetry::track("tengu_doctor_command", None::<serde_json::Value>).await;
    
    println!("{}", "llminate Doctor".bold());
    println!("{}", "================".dimmed());
    println!();
    
    // Check version
    println!("Version: {}", crate::VERSION);
    
    // Check installation type
    if updater::is_running_from_local() {
        println!("Installation: {} ({})", "Local".green(), updater::get_local_installation_path().display());
    } else {
        println!("Installation: {} (npm global)", "Global".yellow());
    }
    
    // Check for updates
    match updater::check_for_updates().await {
        Ok(status) => {
            if status.update_available {
                println!("Update available: {} -> {}", status.current_version, status.latest_version.green());
            } else {
                println!("Up to date: {}", status.current_version.green());
            }
        }
        Err(e) => {
            println!("Update check: {} ({})", "Failed".red(), e);
        }
    }
    
    // Check permissions
    match updater::can_update() {
        Ok(true) => println!("Update permissions: {}", "OK".green()),
        Ok(false) => println!("Update permissions: {} (may need sudo)", "Limited".yellow()),
        Err(e) => println!("Update permissions: {} ({})", "Error".red(), e),
    }
    
    // Check MCP servers
    match crate::config::get_all_mcp_servers() {
        Ok(servers) => {
            if servers.is_empty() {
                println!("MCP servers: {}", "None configured".dimmed());
            } else {
                println!("MCP servers: {} configured", servers.len());
            }
        }
        Err(e) => {
            println!("MCP servers: {} ({})", "Error".red(), e);
        }
    }
    
    // Check config files
    for scope in [ConfigScope::User, ConfigScope::Local, ConfigScope::Project] {
        match crate::config::load_config(scope) {
            Ok(_) => println!("{} config: {}", scope, "OK".green()),
            Err(_) => println!("{} config: {}", scope, "Not found".dimmed()),
        }
    }
    
    println!();
    println!("{}", "Diagnostics complete.".green());
    
    Ok(())
}

/// Handle update command
async fn handle_update() -> Result<()> {
    use crate::updater::{self, UpdateResult};
    
    println!("Checking for updates...");
    
    match updater::check_and_update().await? {
        UpdateResult::Updated(version) => {
            println!("{}", format!("Successfully updated to version {}", version).green());
            println!("Please restart llminate to use the new version.");
        }
        UpdateResult::AlreadyLatest => {
            println!("You are already running the latest version ({}).", crate::VERSION);
        }
        UpdateResult::UpdateAvailable(version) => {
            println!("Update available: {} -> {}", crate::VERSION, version.yellow());
            println!();
            
            if updater::is_running_from_local() {
                println!("To update, run: {}", "llminate update".cyan());
            } else {
                println!("To update, run: {}", "npm install -g llminate@latest".cyan());
                println!();
                println!("Note: You may need to use sudo if you get permission errors.");
                println!("Alternatively, migrate to a local installation with: {}", "llminate migrate-installer".cyan());
            }
        }
    }
    
    Ok(())
}

/// Handle main command (interactive or print mode)
async fn handle_main_command(cli: Cli, debug: bool) -> Result<()> {
    // Track telemetry
    if let Some(prompt) = &cli.prompt {
        if !prompt.trim().is_empty() {
            crate::telemetry::track("tengu_main_command_with_prompt", None::<serde_json::Value>).await;
        }
    }
    
    if cli.print {
        // Non-interactive print mode
        handle_print_mode(cli, debug).await?;
    } else {
        // Interactive TUI mode
        handle_interactive_mode(cli, debug).await?;
    }
    
    Ok(())
}

/// Handle print mode (non-interactive)
async fn handle_print_mode(cli: Cli, debug: bool) -> Result<()> {
    use crate::tui::print_mode;
    
    let options = print_mode::PrintOptions {
        prompt: cli.prompt,
        output_format: match cli.output_format {
            OutputFormat::Text => print_mode::OutputFormat::Text,
            OutputFormat::Json => print_mode::OutputFormat::Json,
            OutputFormat::StreamJson => print_mode::OutputFormat::StreamJson,
        },
        input_format: match cli.input_format {
            InputFormat::Text => print_mode::InputFormat::Text,
            InputFormat::StreamJson => print_mode::InputFormat::StreamJson,
        },
        debug,
        verbose: cli.verbose,
        max_turns: cli.max_turns,
        allowed_tools: cli.allowed_tools,
        disallowed_tools: cli.disallowed_tools,
        system_prompt: cli.system_prompt,
        append_system_prompt: cli.append_system_prompt,
        permission_mode: cli.permission_mode.map(|m| match m {
            PermissionMode::Ask => print_mode::PermissionMode::Ask,
            PermissionMode::Allow => print_mode::PermissionMode::Allow,
            PermissionMode::Deny => print_mode::PermissionMode::Deny,
        }),
        model: cli.model,
        fallback_model: cli.fallback_model,
        add_dirs: cli.add_dir,
        continue_conversation: cli.continue_conversation,
        resume_session_id: cli.resume.and_then(|r| r),
        mcp_config: cli.mcp_config,
        permission_prompt_tool: cli.permission_prompt_tool,
        dangerously_skip_permissions: cli.dangerously_skip_permissions,
    };
    
    print_mode::run(options).await
}

/// Handle interactive mode (TUI)
async fn handle_interactive_mode(cli: Cli, debug: bool) -> Result<()> {
    use crate::tui::interactive_mode;
    
    let options = interactive_mode::InteractiveOptions {
        initial_prompt: cli.prompt,
        debug,
        verbose: cli.verbose,
        allowed_tools: cli.allowed_tools,
        disallowed_tools: cli.disallowed_tools,
        model: cli.model,
        add_dirs: cli.add_dir,
        continue_conversation: cli.continue_conversation,
        resume_session_id: cli.resume.and_then(|r| r),
        mcp_config: cli.mcp_config,
        dangerously_skip_permissions: cli.dangerously_skip_permissions,
    };
    
    interactive_mode::run(options).await
}

/// Parse array values from command line (handle comma separation)
fn parse_array_values(values: Vec<String>) -> Vec<String> {
    values
        .iter()
        .flat_map(|v| v.split(','))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Confirm an action with the user
fn confirm_action(message: &str) -> Result<bool> {
    use std::io::{self, Write};
    
    print!("{} [y/N] ", message);
    io::stdout().flush()?;
    
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    
    Ok(matches!(input.trim().to_lowercase().as_str(), "y" | "yes"))
}

/// Run authentication setup wizard matching JavaScript tool behavior
async fn run_authentication_wizard() -> Result<()> {
    use std::io::{self, Write};
    
    println!("🚀 Welcome to Claude Code!");
    println!();
    println!("It looks like this is your first time using Claude Code.");
    println!("Let's set up your authentication so you can start chatting with Claude.");
    println!();
    
    // Check if Claude Desktop is available
    let mut auth_manager = crate::auth::AuthManager::new()?;
    let desktop_available = auth_manager.is_desktop_available().await;
    
    if desktop_available {
        println!("🎉 I found Claude Desktop on your system!");
        println!();
        println!("You have two authentication options:");
        println!("1. Use Claude Desktop (recommended for Claude Pro/Max subscribers)");
        println!("   - Uses your existing Claude subscription");
        println!("   - No additional API charges");
        println!("   - Seamless integration with Claude Desktop");
        println!();
        println!("2. Use an Anthropic API key");
        println!("   - Requires an Anthropic API account");
        println!("   - Charges based on usage");
        println!("   - Good for developers with API credits");
        println!();
        
        loop {
            print!("Which option would you like to use? [1/2]: ");
            io::stdout().flush()?;
            
            let mut choice = String::new();
            io::stdin().read_line(&mut choice)?;
            
            match choice.trim() {
                "1" => {
                    println!();
                    println!("Starting OAuth authentication...");
                    println!();

                    // Start OAuth flow
                    match setup_oauth_auth(&mut auth_manager).await {
                        Ok(_) => {
                            println!("✅ OAuth authentication set up successfully!");
                            println!("You're ready to start using Claude Code with your subscription.");
                        }
                        Err(e) => {
                            println!("❌ OAuth authentication failed: {}", e);
                            println!();
                            println!("Let's try setting up an API key instead...");
                            setup_api_key_auth(&mut auth_manager).await?;
                        }
                    }
                    break;
                }
                "2" => {
                    println!();
                    setup_api_key_auth(&mut auth_manager).await?;
                    break;
                }
                _ => {
                    println!("Please enter 1 or 2.");
                    continue;
                }
            }
        }
    } else {
        println!("I didn't find Claude Desktop on your system.");
        println!("Let's set up an Anthropic API key for authentication.");
        println!();
        setup_api_key_auth(&mut auth_manager).await?;
    }
    
    println!();
    println!("🎉 Authentication setup complete!");
    println!("You're now ready to use Claude Code. Enjoy!");
    println!();
    
    Ok(())
}

/// Set up API key authentication
async fn setup_api_key_auth(auth_manager: &mut crate::auth::AuthManager) -> Result<()> {
    use std::io::{self, Write};
    
    println!("To use an Anthropic API key, you'll need to:");
    println!("1. Sign up for an Anthropic account at https://console.anthropic.com");
    println!("2. Generate an API key in your account settings");
    println!("3. Add credits to your account for usage");
    println!();
    
    loop {
        print!("Please enter your Anthropic API key: ");
        io::stdout().flush()?;
        
        let mut api_key = String::new();
        io::stdin().read_line(&mut api_key)?;
        let api_key = api_key.trim().to_string();
        
        if api_key.is_empty() {
            println!("API key cannot be empty. Please try again.");
            continue;
        }
        
        if !api_key.starts_with("sk-ant-") {
            println!("⚠️  Warning: Your API key doesn't look like a standard Anthropic API key.");
            println!("   Anthropic API keys typically start with 'sk-ant-'");
            println!();
            
            if !confirm_action("Do you want to continue with this key anyway?")? {
                continue;
            }
        }
        
        println!();
        println!("Testing your API key...");
        
        // Test the API key
        let auth_method = crate::auth::AuthMethod::ApiKey(api_key.clone());
        auth_manager.set_auth(auth_method);
        
        match auth_manager.verify_auth().await {
            Ok(true) => {
                // Save the working API key
                auth_manager.save_auth().await?;
                println!("✅ API key verified and saved successfully!");
                break;
            }
            Ok(false) | Err(_) => {
                println!("❌ API key verification failed.");
                println!("   Please check that your key is correct and you have credits available.");
                println!();

                if !confirm_action("Would you like to try a different API key?")? {
                    return Err(crate::error::Error::Authentication(
                        "API key setup cancelled by user".to_string()
                    ));
                }
            }
        }
    }

    Ok(())
}

/// Set up OAuth authentication (Claude Desktop / Claude.ai)
async fn setup_oauth_auth(auth_manager: &mut crate::auth::AuthManager) -> Result<()> {
    use crate::oauth::{OAuthManager, OAuthCredential};

    println!("Starting OAuth authentication...");
    println!();

    // Create OAuth manager
    let mut oauth_manager = OAuthManager::new();

    // Generate auth URL (use console.anthropic.com for CLI, not claude.ai)
    let auth_url = oauth_manager.start_oauth_flow(false).await
        .map_err(|e| crate::error::Error::Authentication(format!("Failed to start OAuth flow: {}", e)))?;

    println!("Starting callback server and opening browser...");
    println!();

    // Start callback server, open browser, and wait for callback
    // This matches JavaScript's automatic flow (cli-jsdef-fixed.js lines 393485-393507)
    let (code, _state) = oauth_manager.start_callback_server(Some(&auth_url)).await
        .map_err(|e| crate::error::Error::Authentication(format!("OAuth callback failed: {}", e)))?;

    if code.is_empty() {
        return Err(crate::error::Error::Authentication("OAuth authentication failed - no code received".to_string()));
    }

    println!("Authorization received! Processing...");
    println!();

    // Exchange code for credential (API key or OAuth token)
    // JavaScript (cli-jsdef-fixed.js lines 400798-400821):
    // - If token has 'user:inference' scope, use OAuth token directly (Claude Max)
    // - Otherwise, create an API key (Console login)
    let credential = oauth_manager.exchange_code_for_credential(&code).await
        .map_err(|e| crate::error::Error::Authentication(format!("Failed to exchange code: {}", e)))?;

    match credential {
        OAuthCredential::ApiKey(api_key) => {
            // Console login path - save as API key
            auth_manager.save_api_key_from_oauth(&api_key).await
                .map_err(|e| crate::error::Error::Authentication(format!("Failed to save API key: {}", e)))?;

            // Also set in environment for current session
            std::env::set_var("ANTHROPIC_API_KEY", &api_key);

            println!("✅ API key created and saved successfully!");
        }
        OAuthCredential::OAuthToken { access_token, refresh_token, expires_in, scopes, account_uuid } => {
            // Claude Max path - save as OAuth token with accountUuid
            // The token is used directly with Bearer auth
            // accountUuid is CRITICAL for metadata user_id construction
            auth_manager.save_oauth_token(&access_token, &refresh_token, expires_in, &scopes, account_uuid.as_deref()).await
                .map_err(|e| crate::error::Error::Authentication(format!("Failed to save OAuth token: {}", e)))?;

            // Set the token in environment for current session
            // Note: This uses a different env var than API key
            std::env::set_var("ANTHROPIC_AUTH_TOKEN", &access_token);

            // Also set account UUID in environment for metadata
            if let Some(ref uuid) = account_uuid {
                std::env::set_var("CLAUDE_CODE_ACCOUNT_UUID", uuid);
            }

            println!("✅ OAuth token saved successfully!");
            println!("   Using Claude Max subscription with direct OAuth authentication.");
            if account_uuid.is_some() {
                println!("   Account UUID stored for API authentication.");
            }
        }
    }

    Ok(())
}

/// Handle mcp-cli mode - direct interaction with MCP servers
/// JavaScript: mcp-cli is a shell alias that invokes the main executable with --mcp-cli flag
/// Commands: servers, tools, info, grep, resources, read, call
async fn handle_mcp_cli_mode(prompt: Option<String>) -> Result<()> {
    let command = prompt.unwrap_or_default();
    let parts: Vec<&str> = command.split_whitespace().collect();

    if parts.is_empty() {
        print_mcp_cli_help();
        return Ok(());
    }

    match parts[0] {
        "servers" => {
            handle_mcp_cli_servers().await
        }
        "tools" => {
            let server = parts.get(1).map(|s| s.to_string());
            handle_mcp_cli_tools(server).await
        }
        "info" => {
            if parts.len() < 2 {
                eprintln!("Error: Usage: mcp-cli info <server>/<tool>");
                eprintln!("Example: mcp-cli info myserver/my_tool");
                return Err(crate::error::Error::InvalidInput("Missing tool path".to_string()));
            }
            handle_mcp_cli_info(parts[1]).await
        }
        "grep" => {
            if parts.len() < 2 {
                eprintln!("Error: Usage: mcp-cli grep <pattern>");
                return Err(crate::error::Error::InvalidInput("Missing search pattern".to_string()));
            }
            let pattern = parts[1..].join(" ");
            handle_mcp_cli_grep(&pattern).await
        }
        "resources" => {
            let server = parts.get(1).map(|s| s.to_string());
            handle_mcp_cli_resources(server).await
        }
        "read" => {
            if parts.len() < 2 {
                eprintln!("Error: Usage: mcp-cli read <server>/<resource>");
                return Err(crate::error::Error::InvalidInput("Missing resource path".to_string()));
            }
            handle_mcp_cli_read(parts[1]).await
        }
        "call" => {
            if parts.len() < 2 {
                eprintln!("Error: Usage: mcp-cli call <server>/<tool> '<json>'");
                return Err(crate::error::Error::InvalidInput("Missing tool path".to_string()));
            }
            let tool_path = parts[1].to_string();
            // The args can be either:
            // - Inline JSON: mcp-cli call server/tool '{"key": "value"}'
            // - Stdin: mcp-cli call server/tool -
            let args = if parts.len() > 2 {
                if parts[2] == "-" {
                    // Read JSON from stdin
                    let mut input = String::new();
                    std::io::stdin().read_line(&mut input)?;
                    Some(input.trim().to_string())
                } else {
                    // Inline JSON (join remaining parts in case of spaces)
                    Some(parts[2..].join(" "))
                }
            } else {
                None
            };
            handle_mcp_cli_call(&tool_path, args).await
        }
        "help" | "--help" | "-h" => {
            print_mcp_cli_help();
            Ok(())
        }
        unknown => {
            eprintln!("Unknown mcp-cli command: {}", unknown);
            print_mcp_cli_help();
            Err(crate::error::Error::InvalidInput(format!("Unknown command: {}", unknown)))
        }
    }
}

/// Print mcp-cli help message
fn print_mcp_cli_help() {
    println!("MCP CLI - Model Context Protocol command-line interface");
    println!();
    println!("IMPORTANT: Always run 'mcp-cli info <server>/<tool>' BEFORE 'mcp-cli call'");
    println!("           to check the required input schema.");
    println!();
    println!("Commands:");
    println!("  mcp-cli servers                        List all connected MCP servers");
    println!("  mcp-cli tools [server]                 List available tools (optionally filter by server)");
    println!("  mcp-cli info <server>/<tool>           View JSON schema for input and output");
    println!("  mcp-cli grep <pattern>                 Search tool names and descriptions");
    println!("  mcp-cli resources [server]             List MCP resources");
    println!("  mcp-cli read <server>/<resource>       Read an MCP resource");
    println!("  mcp-cli call <server>/<tool> '<json>'  Call a tool with JSON input");
    println!("  mcp-cli call <server>/<tool> -         Call a tool with JSON from stdin");
    println!();
    println!("Examples:");
    println!("  mcp-cli servers");
    println!("  mcp-cli tools filesystem");
    println!("  mcp-cli info filesystem/read_file");
    println!("  mcp-cli call filesystem/read_file '{{\"path\": \"/tmp/file.txt\"}}'");
}

/// List MCP servers
async fn handle_mcp_cli_servers() -> Result<()> {
    let servers = crate::config::get_all_mcp_servers()?;

    if servers.is_empty() {
        println!("No MCP servers configured.");
        println!();
        println!("To add an MCP server, run:");
        println!("  llminate mcp add <name> <command> [args...]");
        return Ok(());
    }

    println!("Connected MCP servers:");
    for (name, config) in servers {
        let command = config.command.clone().unwrap_or_default();
        println!("  {} - {}", name, command);
    }

    Ok(())
}

/// List tools from MCP servers
async fn handle_mcp_cli_tools(server_filter: Option<String>) -> Result<()> {
    let servers = crate::config::get_all_mcp_servers()?;

    if servers.is_empty() {
        println!("No MCP servers configured.");
        return Ok(());
    }

    // Connect to servers and query tools
    for (name, config) in servers {
        // Filter by server if specified
        if let Some(ref filter) = server_filter {
            if &name != filter {
                continue;
            }
        }

        println!("Server: {}", name);

        // Try to connect and get tools
        match connect_and_get_tools(&name, &config).await {
            Ok(tools) => {
                if tools.is_empty() {
                    println!("  (no tools available)");
                } else {
                    for tool in tools {
                        println!("  {} - {}", tool.name, tool.description);
                    }
                }
            }
            Err(e) => {
                println!("  (error: {})", e);
            }
        }
        println!();
    }

    Ok(())
}

/// Get tool info
async fn handle_mcp_cli_info(tool_path: &str) -> Result<()> {
    let (server_name, tool_name) = parse_tool_path(tool_path)?;

    let servers = crate::config::get_all_mcp_servers()?;
    let config = servers.get(&server_name)
        .ok_or_else(|| crate::error::Error::NotFound(format!("Server not found: {}", server_name)))?;

    match connect_and_get_tool_info(&server_name, config, &tool_name).await {
        Ok(tool) => {
            println!("Tool: {}/{}", server_name, tool_name);
            println!();
            println!("Description: {}", tool.description);
            println!();
            println!("Input Schema:");
            println!("{}", serde_json::to_string_pretty(&tool.input_schema)?);
        }
        Err(e) => {
            eprintln!("Error getting tool info: {}", e);
            return Err(e);
        }
    }

    Ok(())
}

/// Search tools by pattern
async fn handle_mcp_cli_grep(pattern: &str) -> Result<()> {
    let servers = crate::config::get_all_mcp_servers()?;
    let pattern_lower = pattern.to_lowercase();
    let mut found = false;

    for (name, config) in servers {
        match connect_and_get_tools(&name, &config).await {
            Ok(tools) => {
                for tool in tools {
                    let matches = tool.name.to_lowercase().contains(&pattern_lower)
                        || tool.description.to_lowercase().contains(&pattern_lower);

                    if matches {
                        found = true;
                        println!("{}/{} - {}", name, tool.name, tool.description);
                    }
                }
            }
            Err(_) => continue,
        }
    }

    if !found {
        println!("No tools found matching: {}", pattern);
    }

    Ok(())
}

/// List MCP resources
async fn handle_mcp_cli_resources(server_filter: Option<String>) -> Result<()> {
    let servers = crate::config::get_all_mcp_servers()?;

    for (name, config) in servers {
        if let Some(ref filter) = server_filter {
            if &name != filter {
                continue;
            }
        }

        println!("Server: {}", name);

        match connect_and_get_resources(&name, &config).await {
            Ok(resources) => {
                if resources.is_empty() {
                    println!("  (no resources available)");
                } else {
                    for resource in resources {
                        let desc = resource.description.as_deref().unwrap_or("");
                        println!("  {} - {}", resource.uri, desc);
                    }
                }
            }
            Err(e) => {
                println!("  (error: {})", e);
            }
        }
        println!();
    }

    Ok(())
}

/// Read an MCP resource
async fn handle_mcp_cli_read(resource_path: &str) -> Result<()> {
    let (server_name, resource_uri) = parse_tool_path(resource_path)?;

    let servers = crate::config::get_all_mcp_servers()?;
    let config = servers.get(&server_name)
        .ok_or_else(|| crate::error::Error::NotFound(format!("Server not found: {}", server_name)))?;

    match connect_and_read_resource(&server_name, config, &resource_uri).await {
        Ok(content) => {
            println!("{}", content);
        }
        Err(e) => {
            eprintln!("Error reading resource: {}", e);
            return Err(e);
        }
    }

    Ok(())
}

/// Call an MCP tool
async fn handle_mcp_cli_call(tool_path: &str, args: Option<String>) -> Result<()> {
    let (server_name, tool_name) = parse_tool_path(tool_path)?;

    let servers = crate::config::get_all_mcp_servers()?;
    let config = servers.get(&server_name)
        .ok_or_else(|| crate::error::Error::NotFound(format!("Server not found: {}", server_name)))?;

    // Parse JSON args
    let input: serde_json::Value = if let Some(json_str) = args {
        serde_json::from_str(&json_str)
            .map_err(|e| crate::error::Error::InvalidInput(format!("Invalid JSON: {}", e)))?
    } else {
        serde_json::json!({})
    };

    match connect_and_call_tool(&server_name, config, &tool_name, input).await {
        Ok(result) => {
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        Err(e) => {
            eprintln!("Error calling tool: {}", e);
            return Err(e);
        }
    }

    Ok(())
}

/// Parse a tool path like "server/tool" into (server, tool)
fn parse_tool_path(path: &str) -> Result<(String, String)> {
    let parts: Vec<&str> = path.splitn(2, '/').collect();
    if parts.len() != 2 {
        return Err(crate::error::Error::InvalidInput(
            format!("Invalid path format: {}. Expected format: <server>/<tool>", path)
        ));
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

/// Connect to an MCP server and get available tools
async fn connect_and_get_tools(
    server_name: &str,
    config: &crate::config::McpServerConfig,
) -> Result<Vec<crate::mcp::McpTool>> {
    let mut client = crate::mcp::connect_and_initialize(server_name, config).await?;
    client.list_tools().await
}

/// Connect to an MCP server and get tool info
async fn connect_and_get_tool_info(
    server_name: &str,
    config: &crate::config::McpServerConfig,
    tool_name: &str,
) -> Result<crate::mcp::McpTool> {
    let mut client = crate::mcp::connect_and_initialize(server_name, config).await?;
    let tools = client.list_tools().await?;

    tools.into_iter()
        .find(|t| t.name == tool_name)
        .ok_or_else(|| crate::error::Error::NotFound(format!("Tool not found: {}", tool_name)))
}

/// Connect to an MCP server and get resources
async fn connect_and_get_resources(
    server_name: &str,
    config: &crate::config::McpServerConfig,
) -> Result<Vec<crate::mcp::McpResource>> {
    let mut client = crate::mcp::connect_and_initialize(server_name, config).await?;
    client.list_resources().await
}

/// Connect to an MCP server and read a resource
async fn connect_and_read_resource(
    server_name: &str,
    config: &crate::config::McpServerConfig,
    resource_uri: &str,
) -> Result<String> {
    let mut client = crate::mcp::connect_and_initialize(server_name, config).await?;
    client.read_resource(resource_uri).await
}

/// Connect to an MCP server and call a tool
async fn connect_and_call_tool(
    server_name: &str,
    config: &crate::config::McpServerConfig,
    tool_name: &str,
    input: serde_json::Value,
) -> Result<serde_json::Value> {
    let mut client = crate::mcp::connect_and_initialize(server_name, config).await?;
    client.call_tool(tool_name, input).await
}