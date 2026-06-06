//! Unified error type for `relay-intake`.

use thiserror::Error;

/// All errors that can arise during intake processing.
#[derive(Debug, Error)]
pub enum IntakeError {
    /// The story text could not be structured (parse/rule failure).
    #[error("structuring failed: {0}")]
    StructureFailed(String),

    /// A [`crate::NextAction`] references a resource id that is not in the
    /// known directory.  The unknown id is included for diagnostics.
    #[error("unknown resource id in next-action: {0}")]
    UnknownResource(String),

    /// The local LLM is unreachable; a partial record was produced instead.
    #[error("LLM unavailable: {0}")]
    LlmUnavailable(String),

    /// An I/O error occurred while reading the story or writing output.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialisation/deserialisation failed.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}
