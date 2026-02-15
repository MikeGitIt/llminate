use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("API error: {0}")]
    Api(String),
    
    #[error("LLM error: {0}")]
    Llm(String),
    
    #[error("Protocol error: {0}")]
    Protocol(String),
    
    #[error("TUI error: {0}")]
    Tui(String),
    
    #[error("Configuration error: {0}")]
    Config(String),
    
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    #[error("HTTP error: {0}")]
    Http(#[from] hyper::Error),
    
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    
    #[error("Other error: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<anyhow::Error> for Error {
    fn from(err: anyhow::Error) -> Self {
        Error::Other(err.to_string())
    }
}