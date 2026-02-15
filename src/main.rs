use anyhow::Result;
use llminate::{cli::Cli, error, config::LoggingConfig};
use tracing_subscriber::{prelude::*, fmt, EnvFilter};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env file if it exists
    dotenv::dotenv().ok();
    
    // Parse CLI arguments first to get debug flag
    let cli = Cli::parse_args();
    
    // Initialize tracing/logging with configurable system
    let logging_config = cli.to_logging_config();
    init_tracing(logging_config, cli.print).await?;
    
    tracing::debug!("Starting llminate with debug logging enabled");
    tracing::debug!("CLI args: debug={}, print={}, prompt={:?}", 
        cli.debug, cli.print, cli.prompt);
    
    // Initialize error tracking
    let _sentry = error::init_sentry();
    
    // Set up panic handler
    error::create_panic_handler();
    
    // Execute CLI command
    cli.execute().await?;
    
    Ok(())
}

/// Initialize tracing subscriber with configurable logging system
async fn init_tracing(config: LoggingConfig, is_print_mode: bool) -> Result<()> {
    use std::io;
    use std::sync::Arc;
    
    // Build EnvFilter with module-specific levels
    let default_level = config.default_level.as_deref().unwrap_or("info");
    let mut filter_string = format!("llminate={},tokio=info,hyper=info,reqwest=info", default_level);
    
    // Add module-specific overrides
    if let Some(module_levels) = &config.module_levels {
        for (module, level) in module_levels {
            filter_string.push_str(&format!(",{}={}", module, level));
        }
    }
    
    // Try environment variable first, then our constructed filter
    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(&filter_string))?;
    
    // Determine what layers to enable
    let enable_stdout = config.enable_stderr_logging.unwrap_or(is_print_mode);
    let enable_file = config.enable_file_logging.unwrap_or(!is_print_mode);
    let enable_json = config.enable_json_logging.unwrap_or(false);
    
    // Format settings
    let format_style = config.format_style.as_deref().unwrap_or("compact");
    let include_thread_info = config.include_thread_info.unwrap_or(false);
    let include_source = config.include_source_location.unwrap_or(false);
    
    // Build registry
    let registry = tracing_subscriber::registry().with(env_filter);
    
    // Handle each combination explicitly with correct types
    if enable_stdout && enable_file && enable_json {
        // All three layers - stdout + file + json
        let stdout_layer = fmt::layer()
            .with_writer(io::stdout)
            .with_target(!is_print_mode)
            .with_thread_ids(include_thread_info)
            .with_thread_names(false)
            .with_file(include_source)
            .with_line_number(include_source)
            .with_ansi(!is_print_mode)
            .compact();
        
        let log_file_path = config.log_file_path.as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(get_debug_log_path);
        if let Some(parent) = log_file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let log_file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&log_file_path)?;
        let file_layer = fmt::layer()
            .with_writer(Arc::new(log_file))
            .with_target(true)
            .with_thread_ids(include_thread_info)
            .with_thread_names(false)
            .with_file(include_source)
            .with_line_number(include_source)
            .with_ansi(false);
        
        let json_file_path = config.log_file_path.as_deref()
            .map(|p| {
                let path = PathBuf::from(p);
                path.with_extension("json")
            })
            .unwrap_or_else(|| {
                if cfg!(target_os = "windows") {
                    std::env::temp_dir().join("llminate-debug.json")
                } else {
                    PathBuf::from("/tmp/llminate-debug.json")
                }
            });
        if let Some(parent) = json_file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json_file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&json_file_path)?;
        let json_layer = fmt::layer()
            .with_writer(Arc::new(json_file))
            .with_target(true)
            .with_thread_ids(include_thread_info)
            .with_file(include_source)
            .with_line_number(include_source)
            .json();
        
        registry.with(stdout_layer).with(file_layer).with(json_layer).init();
        
    } else if enable_stdout && enable_file {
        // Stdout + file
        let stdout_layer = fmt::layer()
            .with_writer(io::stdout)
            .with_target(!is_print_mode)
            .with_thread_ids(include_thread_info)
            .with_thread_names(false)
            .with_file(include_source)
            .with_line_number(include_source)
            .with_ansi(!is_print_mode)
            .compact();
        
        let log_file_path = config.log_file_path.as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(get_debug_log_path);
        if let Some(parent) = log_file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let log_file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&log_file_path)?;
        let file_layer = fmt::layer()
            .with_writer(Arc::new(log_file))
            .with_target(true)
            .with_thread_ids(include_thread_info)
            .with_thread_names(false)
            .with_file(include_source)
            .with_line_number(include_source)
            .with_ansi(false);
        
        registry.with(stdout_layer).with(file_layer).init();
        
    } else if enable_stdout && enable_json {
        // Stdout + JSON
        let stdout_layer = fmt::layer()
            .with_writer(io::stdout)
            .with_target(!is_print_mode)
            .with_thread_ids(include_thread_info)
            .with_thread_names(false)
            .with_file(include_source)
            .with_line_number(include_source)
            .with_ansi(!is_print_mode)
            .compact();
        
        let json_file_path = config.log_file_path.as_deref()
            .map(|p| {
                let path = PathBuf::from(p);
                path.with_extension("json")
            })
            .unwrap_or_else(|| {
                if cfg!(target_os = "windows") {
                    std::env::temp_dir().join("llminate-debug.json")
                } else {
                    PathBuf::from("/tmp/llminate-debug.json")
                }
            });
        if let Some(parent) = json_file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json_file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&json_file_path)?;
        let json_layer = fmt::layer()
            .with_writer(Arc::new(json_file))
            .with_target(true)
            .with_thread_ids(include_thread_info)
            .with_file(include_source)
            .with_line_number(include_source)
            .json();
        
        registry.with(stdout_layer).with(json_layer).init();
        
    } else if enable_file && enable_json {
        // File + JSON
        let log_file_path = config.log_file_path.as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(get_debug_log_path);
        if let Some(parent) = log_file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let log_file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&log_file_path)?;
        let file_layer = fmt::layer()
            .with_writer(Arc::new(log_file))
            .with_target(true)
            .with_thread_ids(include_thread_info)
            .with_thread_names(false)
            .with_file(include_source)
            .with_line_number(include_source)
            .with_ansi(false);
        
        let json_file_path = config.log_file_path.as_deref()
            .map(|p| {
                let path = PathBuf::from(p);
                path.with_extension("json")
            })
            .unwrap_or_else(|| {
                if cfg!(target_os = "windows") {
                    std::env::temp_dir().join("llminate-debug.json")
                } else {
                    PathBuf::from("/tmp/llminate-debug.json")
                }
            });
        if let Some(parent) = json_file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json_file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&json_file_path)?;
        let json_layer = fmt::layer()
            .with_writer(Arc::new(json_file))
            .with_target(true)
            .with_thread_ids(include_thread_info)
            .with_file(include_source)
            .with_line_number(include_source)
            .json();
        
        registry.with(file_layer).with(json_layer).init();
        
    } else if enable_stdout {
        // Only stdout - handle each format style separately due to type system
        match format_style {
            "pretty" => {
                let stdout_layer = fmt::layer()
                    .with_writer(io::stderr)
                    .with_target(!is_print_mode)
                    .with_thread_ids(include_thread_info)
                    .with_thread_names(false)
                    .with_file(include_source)
                    .with_line_number(include_source)
                    .with_ansi(!is_print_mode)
                    .pretty();
                registry.with(stdout_layer).init();
            },
            "compact" => {
                let stdout_layer = fmt::layer()
                    .with_writer(io::stderr)
                    .with_target(!is_print_mode)
                    .with_thread_ids(include_thread_info)
                    .with_thread_names(false)
                    .with_file(include_source)
                    .with_line_number(include_source)
                    .with_ansi(!is_print_mode)
                    .compact();
                registry.with(stdout_layer).init();
            },
            _ => {
                let stdout_layer = fmt::layer()
                    .with_writer(io::stderr)
                    .with_target(!is_print_mode)
                    .with_thread_ids(include_thread_info)
                    .with_thread_names(false)
                    .with_file(include_source)
                    .with_line_number(include_source)
                    .with_ansi(!is_print_mode);
                registry.with(stdout_layer).init();
            }
        }
        
    } else if enable_file {
        // Only file
        let log_file_path = config.log_file_path.as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(get_debug_log_path);
        if let Some(parent) = log_file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let log_file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&log_file_path)?;
        let file_layer = fmt::layer()
            .with_writer(Arc::new(log_file))
            .with_target(true)
            .with_thread_ids(include_thread_info)
            .with_thread_names(false)
            .with_file(include_source)
            .with_line_number(include_source)
            .with_ansi(false);
        
        registry.with(file_layer).init();
        
    } else if enable_json {
        // Only JSON
        let json_file_path = config.log_file_path.as_deref()
            .map(|p| {
                let path = PathBuf::from(p);
                path.with_extension("json")
            })
            .unwrap_or_else(|| {
                if cfg!(target_os = "windows") {
                    std::env::temp_dir().join("llminate-debug.json")
                } else {
                    PathBuf::from("/tmp/llminate-debug.json")
                }
            });
        if let Some(parent) = json_file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json_file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&json_file_path)?;
        let json_layer = fmt::layer()
            .with_writer(Arc::new(json_file))
            .with_target(true)
            .with_thread_ids(include_thread_info)
            .with_file(include_source)
            .with_line_number(include_source)
            .json();
        
        registry.with(json_layer).init();
        
    } else {
        // Fallback to basic stdout logging
        registry.with(fmt::layer().with_writer(io::stdout).compact()).init();
    }
    
    tracing::debug!("Configurable logging initialized");
    tracing::debug!("Config: default_level={:?}, modules={:?}, stdout={}, file={}, json={}", 
        config.default_level, 
        config.module_levels,
        enable_stdout,
        enable_file,
        enable_json
    );
    
    if !is_print_mode {
        tracing::info!("llminate debug logging started - session beginning");
    }
    
    Ok(())
}

/// Get the path for the debug log file
fn get_debug_log_path() -> PathBuf {
    if cfg!(target_os = "windows") {
        std::env::temp_dir().join("llminate-debug.log")
    } else {
        PathBuf::from("/tmp/llminate-debug.log")
    }
}