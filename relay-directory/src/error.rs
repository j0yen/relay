//! Unified error type for `relay-directory`.

use thiserror::Error;

/// All errors that can arise in the directory crate.
#[derive(Debug, Error)]
pub enum DirectoryError {
    /// `SQLite` database error.
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    /// JSON (de)serialisation error.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// CSV parse error.
    #[error("csv error: {0}")]
    Csv(#[from] csv::Error),

    /// I/O error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// A required field was missing in the source record.
    #[error("missing required field: {field}")]
    MissingField {
        /// The name of the missing field.
        field: &'static str,
    },

    /// Invalid coordinate value.
    #[error("invalid coordinate: {0}")]
    InvalidCoordinate(String),
}
