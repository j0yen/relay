//! Unified error type for `relay-match`.

use thiserror::Error;

/// Errors from relay-match operations.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum MatchError {
    /// The need extractor failed (LLM unreachable, parse error, etc.).
    #[error("extractor failed: {0}")]
    ExtractorFailed(String),

    /// A directory store access error.
    #[error("directory error: {0}")]
    Directory(#[from] relay_directory::DirectoryError),

    /// An unexpected I/O error.
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
}
