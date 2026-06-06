//! Error types for `relay-letters`.

use thiserror::Error;

/// Errors produced by the `relay-letters` crate.
#[derive(Debug, Error)]
pub enum LetterError {
    /// The prose smoother introduced new named entities not present in the
    /// source template. This is a safety violation — the output would contain
    /// fabricated facts.
    #[error("prose smoother introduced new entities: {new_entities:?}")]
    NewEntitiesIntroduced {
        /// The entities found in the smoothed output that were not in the filled
        /// template.
        new_entities: Vec<String>,
    },

    /// The prose smoother returned an error.
    #[error("prose smoother error: {message}")]
    ProseError {
        /// Underlying error message.
        message: String,
    },
}
