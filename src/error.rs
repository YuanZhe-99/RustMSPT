use thiserror::Error;

#[derive(Debug, Error)]
pub enum RustMsptError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("YAML parse error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("Invalid config: {0}")]
    InvalidConfig(String),

    #[error("Invalid mesh: {0}")]
    InvalidMesh(String),

    #[error("Algorithm not available yet: {0}")]
    NotAvailable(String),
}

pub type Result<T> = std::result::Result<T, RustMsptError>;