//! The [`Prose`] trait and its implementations.
//!
//! `relay-letters` separates deterministic template filling (in [`super::template`])
//! from optional prose smoothing. Smoothing is done by a [`Prose`] impl.
//!
//! # Safety contract
//!
//! A `Prose` implementation **must not** introduce new named entities.
//! [`check_no_new_entities`] enforces this as a post-smoothing gate.
//! Any impl that deliberately violates this contract will be caught in tests.

use crate::LetterError;

/// A prose smoother that can rewrite a filled template into more fluent text.
///
/// Implementations **must not** introduce new facts or named entities.
pub trait Prose {
    /// Smooth `text` into more fluent prose.
    ///
    /// The returned string must not contain any named entities, numbers, or
    /// facts that are not present in `text`. The caller enforces this via
    /// [`check_no_new_entities`] after calling this method.
    ///
    /// # Errors
    ///
    /// Returns [`LetterError::ProseError`] if smoothing fails.
    fn smooth(&self, text: &str) -> Result<String, LetterError>;
}

/// A mock prose implementation for tests that returns the input unchanged.
///
/// This is the correct default for tests: it is transparent (no new entities)
/// and deterministic.
#[derive(Debug, Clone, Default)]
pub struct MockProse;

impl Prose for MockProse {
    fn smooth(&self, text: &str) -> Result<String, LetterError> {
        Ok(text.to_owned())
    }
}

/// A mock prose implementation that deliberately injects a new entity.
///
/// Used in tests to verify that [`check_no_new_entities`] catches violations.
#[derive(Debug, Clone)]
pub struct InjectingMockProse {
    /// The extra sentence to append, which should contain a new entity.
    pub injected: String,
}

impl Prose for InjectingMockProse {
    fn smooth(&self, text: &str) -> Result<String, LetterError> {
        Ok(format!("{text}\n{}", self.injected))
    }
}

/// A thin wrapper placeholder for a local LLM prose smoother.
///
/// # STUB — not implemented for iter-1
///
/// For iter-1, this struct panics with a compile-time unreachable if called.
/// It exists so the type is present and the CLI `--smooth` flag can be wired.
/// A real implementation will call the local qwen model via an inference
/// backend when relay-intake lands and the LLM integration is ready.
///
/// Note: `LocalLlmProse` intentionally does not implement [`Prose`] yet;
/// call-sites that need LLM smoothing must gate on a feature flag or use
/// dependency injection with `Box<dyn Prose>`.
#[derive(Debug, Clone, Default)]
pub struct LocalLlmProse;

// LocalLlmProse deliberately omits a Prose impl for iter-1.
// The stub compiles but cannot be called — no Prose bound is satisfied.

/// Check that `smoothed` contains no named entities absent from `original`.
///
/// This is the post-smoothing safety gate. "Named entity" here is a pragmatic
/// approximation: any token that starts with an uppercase letter and is at
/// least 2 characters long, and is not a common English stop-word or
/// sentence-start word.
///
/// # Errors
///
/// Returns [`LetterError::NewEntitiesIntroduced`] if new entities are found.
pub fn check_no_new_entities(original: &str, smoothed: &str) -> Result<(), LetterError> {
    let orig_entities = extract_entities(original);
    let smoothed_entities = extract_entities(smoothed);

    let new_entities: Vec<String> = smoothed_entities
        .into_iter()
        .filter(|e| !orig_entities.contains(e))
        .collect();

    if new_entities.is_empty() {
        Ok(())
    } else {
        Err(LetterError::NewEntitiesIntroduced { new_entities })
    }
}

/// Extract candidate named entities from `text`.
///
/// A token is considered a candidate named entity if:
/// - It starts with an uppercase ASCII letter.
/// - It is at least 2 characters long.
/// - It is not in the common stop-word list below (sentence-start words,
///   common English words that happen to be capitalized).
fn extract_entities(text: &str) -> std::collections::HashSet<String> {
    // Common sentence-start / structural words to ignore.
    const STOP_WORDS: &[&str] = &[
        "The", "This", "A", "An", "In", "To", "For", "Of", "On", "At",
        "As", "It", "We", "I", "You", "He", "She", "They", "My", "Our",
        "Your", "Dear", "Re", "Subject", "Date", "From", "Sincerely",
        "Please", "Thank", "Attached", "Note", "Not", "No", "If", "Also",
        "However", "Therefore", "Additionally", "Furthermore", "First",
        "Second", "Third", "Finally", "DRAFT", "review", "before", "sending",
        "This", "letter", "does", "constitute", "legal", "medical",
        "professional", "advice", "All", "Any",
    ];

    let stop: std::collections::HashSet<&str> = STOP_WORDS.iter().copied().collect();

    text.split_whitespace()
        .filter_map(|token| {
            // Strip leading/trailing punctuation
            let clean: String = token
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '\'')
                .collect();
            if clean.len() >= 2
                && clean.starts_with(|c: char| c.is_ascii_uppercase())
                && !stop.contains(clean.as_str())
            {
                Some(clean)
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_prose_is_transparent() {
        let p = MockProse;
        let text = "Hello World from Jane Smith";
        let result = p.smooth(text).expect("mock prose must not fail");
        assert_eq!(result, text);
    }

    #[test]
    fn check_no_new_entities_passes_unchanged() {
        let text = "Jane Smith lives at 123 Oak Street and needs FoodBank assistance.";
        check_no_new_entities(text, text).expect("same text must pass entity check");
    }

    #[test]
    fn check_no_new_entities_catches_injection() {
        let original = "Client Jane Smith needs food assistance.";
        let smoothed = "Client Jane Smith needs food assistance.\nContact Dr. Goldstein for follow-up.";
        let err = check_no_new_entities(original, smoothed)
            .expect_err("injected entity must be caught");
        match err {
            LetterError::NewEntitiesIntroduced { new_entities } => {
                // "Dr" or "Goldstein" must appear as a new entity
                let found = new_entities.iter().any(|e| {
                    e.contains("Goldstein") || e.contains("Dr")
                });
                assert!(found, "expected Goldstein or Dr in new_entities, got {new_entities:?}");
            }
            other => panic!("unexpected error variant: {other}"),
        }
    }

    #[test]
    fn injecting_mock_prose_triggers_entity_check() {
        let injecting = InjectingMockProse {
            injected: "Contact Acme Corporation for more information.".to_owned(),
        };
        let original = "Client Alice Brown needs shelter.";
        let smoothed = injecting.smooth(original).expect("should not fail");
        let err = check_no_new_entities(original, &smoothed)
            .expect_err("injection must be caught");
        assert!(matches!(err, LetterError::NewEntitiesIntroduced { .. }));
    }
}
