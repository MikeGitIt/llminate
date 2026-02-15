use sentry::protocol::{Event, Level};
use std::fmt;
use std::sync::Arc;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("API error: {error_type} (status {status}): {message}")]
    Api {
        status: u16,
        error_type: String,
        message: String,
    },

    #[error("Authentication error: {0}")]
    Auth(String),

    #[error("Authentication failed: {0}")]
    Authentication(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("MCP error: {0}")]
    Mcp(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Tool execution error: {0}")]
    ToolExecution(String),

    #[error("Update error: {0}")]
    Update(String),

    #[error("TUI error: {0}")]
    Tui(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] hyper::Error),

    #[error("Request error: {0}")]
    Request(#[from] reqwest::Error),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Rate limit exceeded: {0}")]
    RateLimit(String),

    #[error("Session error: {0}")]
    Session(String),

    #[error("File watch error: {0}")]
    FileWatch(#[from] notify::Error),

    #[error("Process error: {0}")]
    Process(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    #[error("Tool not allowed: {0}")]
    ToolNotAllowed(String),
    
    #[error("Permission required: {0}")]
    PermissionRequired(String),

    #[error("Shell parsing error: {0}")]
    ShellParse(#[from] shell_words::ParseError),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Regex error: {0}")]
    Regex(#[from] regex::Error),

    #[error("Glob pattern error: {0}")]
    GlobPattern(#[from] glob::PatternError),
    
    #[error("Cancelled: {0}")]
    Cancelled(String),

    #[error("Other error: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<anyhow::Error> for Error {
    fn from(err: anyhow::Error) -> Self {
        Error::Other(err.to_string())
    }
}

impl Error {
    /// Convert error to Sentry event level
    pub fn sentry_level(&self) -> Level {
        match self {
            Error::Auth(_) | Error::PermissionDenied(_) => Level::Warning,
            Error::Api { .. } | Error::Http(_) | Error::Request(_) => Level::Error,
            Error::RateLimit(_) => Level::Info,
            Error::Config(_) | Error::InvalidInput(_) => Level::Warning,
            Error::NotFound(_) => Level::Info,
            Error::ToolNotFound(_) | Error::ToolNotAllowed(_) => Level::Warning,
            _ => Level::Error,
        }
    }

    /// Check if error is recoverable
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Error::RateLimit(_) | Error::Http(_) | Error::Request(_) | Error::Process(_)
        )
    }

    /// Check if error should trigger retry
    pub fn should_retry(&self) -> bool {
        matches!(self, Error::RateLimit(_) | Error::Http(_) | Error::Request(_))
    }

    /// Get retry delay in milliseconds
    pub fn retry_delay_ms(&self) -> Option<u64> {
        match self {
            Error::RateLimit(_) => Some(60000), // 1 minute
            Error::Http(_) | Error::Request(_) => Some(5000), // 5 seconds
            _ => None,
        }
    }
}

/// Initialize Sentry error tracking
pub fn init_sentry() -> sentry::ClientInitGuard {
    let dsn = std::env::var("SENTRY_DSN").ok();
    let environment = if cfg!(debug_assertions) {
        "development"
    } else {
        "production"
    };

    sentry::init((
        dsn,
        sentry::ClientOptions {
            release: Some(format!("llminate@{}", crate::VERSION).into()),
            environment: Some(environment.into()),
            sample_rate: 1.0,
            traces_sample_rate: 0.1,
            attach_stacktrace: true,
            send_default_pii: false,
            before_send: Some(Arc::new(|mut event: sentry::protocol::Event| {
                // Filter out sensitive data
                if let Some(ref mut request) = event.request {
                    // Remove authorization headers from URL if present
                    if let Some(ref mut url) = request.url {
                        url.set_password(None);
                    }
                }
                Some(event)
            })),
            ..Default::default()
        },
    ))
}

/// Capture an error and send to Sentry
pub fn capture_error(error: &Error) {
    let mut event = Event::new();
    event.level = error.sentry_level();
    event.message = Some(error.to_string());
    
    // Add error type as tag
    event.tags.insert(
        "error_type".to_string(),
        format!("{:?}", std::mem::discriminant(error)),
    );
    
    // Add additional context
    event.extra.insert(
        "is_recoverable".to_string(),
        sentry::protocol::Value::Bool(error.is_recoverable()),
    );
    
    sentry::capture_event(event);
}

/// Capture an error with additional context
pub fn capture_error_with_context<C: fmt::Display>(error: &Error, context: C) {
    let mut event = Event::new();
    event.level = error.sentry_level();
    event.message = Some(format!("{}: {}", context, error));
    
    event.tags.insert(
        "error_type".to_string(),
        format!("{:?}", std::mem::discriminant(error)),
    );
    
    event.extra.insert(
        "context".to_string(),
        sentry::protocol::Value::String(context.to_string()),
    );
    
    sentry::capture_event(event);
}

/// Add breadcrumb for tracking
pub fn add_breadcrumb(message: impl Into<String>, category: impl Into<String>) {
    sentry::add_breadcrumb(sentry::Breadcrumb {
        message: Some(message.into()),
        category: Some(category.into()),
        level: Level::Info,
        ..Default::default()
    });
}

/// Set user context for Sentry
pub fn set_user_context(user_id: Option<String>) {
    sentry::configure_scope(|scope| {
        if let Some(id) = user_id {
            scope.set_user(Some(sentry::User {
                id: Some(id),
                ..Default::default()
            }));
        } else {
            scope.set_user(None);
        }
    });
}

/// Set additional tags for context
pub fn set_tags(tags: Vec<(&str, String)>) {
    sentry::configure_scope(|scope| {
        for (key, value) in tags {
            scope.set_tag(key, value);
        }
    });
}

/// Extension trait for Result types to capture errors
pub trait ResultExt<T> {
    fn capture_err(self) -> Result<T>;
    fn capture_err_with_context(self, context: impl fmt::Display) -> Result<T>;
}

impl<T> ResultExt<T> for Result<T> {
    fn capture_err(self) -> Result<T> {
        if let Err(ref e) = self {
            capture_error(e);
        }
        self
    }

    fn capture_err_with_context(self, context: impl fmt::Display) -> Result<T> {
        if let Err(ref e) = self {
            capture_error_with_context(e, context);
        }
        self
    }
}

/// Create an error event for panics
pub fn create_panic_handler() {
    let default_panic = std::panic::take_hook();
    
    std::panic::set_hook(Box::new(move |panic_info| {
        let payload = panic_info.payload();
        let message = if let Some(s) = payload.downcast_ref::<&str>() {
            s
        } else if let Some(s) = payload.downcast_ref::<String>() {
            s
        } else {
            "Unknown panic"
        };

        let location = panic_info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown".to_string());

        sentry::capture_message(
            &format!("Panic at {}: {}", location, message),
            Level::Fatal,
        );

        // Call the default panic handler
        default_panic(panic_info);
    }));
}