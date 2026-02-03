//! Error types for DATUM module

use thiserror::Error;

/// DATUM module errors
#[derive(Error, Debug)]
pub enum DatumError {
    #[error("Protocol error: {0}")]
    ProtocolError(String),

    #[error("Template error: {0}")]
    TemplateError(String),

    #[error("Pool connection error: {0}")]
    PoolConnectionError(String),

    #[error("Stratum error: {0}")]
    StratumError(String),

    #[error("Encryption error: {0}")]
    EncryptionError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Node API error: {0}")]
    NodeApiError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    SerializationError(String),
}

impl From<blvm_node::module::traits::ModuleError> for DatumError {
    fn from(e: blvm_node::module::traits::ModuleError) -> Self {
        DatumError::NodeApiError(e.to_string())
    }
}


