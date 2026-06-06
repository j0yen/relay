//! [`NeedExtractor`] trait and built-in implementations.
//!
//! The trait decouples the matcher from any specific LLM or keyword approach.
//! Tests inject [`MockExtractor`]; production uses [`LocalLlmExtractor`].

use crate::{error::MatchError, needs::Needs};

// ─── Trait ───────────────────────────────────────────────────────────────────

/// Extract structured [`Needs`] from a free-text situation description.
///
/// All implementations must be **pure** with respect to external I/O under
/// test (no disk writes, no outbound connections under `MockExtractor`).
pub trait NeedExtractor: Send + Sync {
    /// Extract needs from `situation_text`.
    ///
    /// Implementations that cannot reach their model (network/process unavailable)
    /// should return [`MatchError::ExtractorFailed`] so callers can fall back to
    /// keyword extraction.
    ///
    /// # Errors
    /// Returns [`MatchError::ExtractorFailed`] on LLM or parse failure.
    fn extract(&self, situation_text: &str) -> Result<Needs, MatchError>;
}

// ─── MockExtractor ───────────────────────────────────────────────────────────

/// A test-only extractor that returns a fixed, pre-configured [`Needs`] value.
///
/// Satisfies AC5 (the matcher depends only on [`NeedExtractor`]; swapping
/// `MockExtractor` ↔ `LocalLlmExtractor` changes no matcher code).
/// Satisfies AC7 (no network connection is ever made by this impl).
#[derive(Debug, Clone)]
pub struct MockExtractor {
    /// The `Needs` value that will always be returned by [`extract`].
    pub needs: Needs,
}

impl MockExtractor {
    /// Create a `MockExtractor` that always returns `needs`.
    #[must_use]
    pub fn new(needs: Needs) -> Self {
        Self { needs }
    }
}

impl NeedExtractor for MockExtractor {
    fn extract(&self, _situation_text: &str) -> Result<Needs, MatchError> {
        Ok(self.needs.clone())
    }
}

// ─── FailingExtractor ────────────────────────────────────────────────────────

/// A test-only extractor that always fails (simulates an unreachable model).
///
/// Used to test AC6 (graceful degrade).
#[derive(Debug, Clone, Default)]
pub struct FailingExtractor;

impl NeedExtractor for FailingExtractor {
    fn extract(&self, _situation_text: &str) -> Result<Needs, MatchError> {
        Err(MatchError::ExtractorFailed(
            "local model unreachable (test stub)".to_owned(),
        ))
    }
}

// ─── KeywordExtractor ────────────────────────────────────────────────────────

/// A simple keyword-based fallback extractor.
///
/// Used when [`LocalLlmExtractor`] is unavailable (AC6 graceful degrade).
/// Maps common keywords → [`ServiceType`] values without any LLM or network.
#[derive(Debug, Clone, Default)]
pub struct KeywordExtractor;

impl KeywordExtractor {
    /// Create a new `KeywordExtractor`.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Extract needs from `situation_text` using keyword heuristics.
    ///
    /// Public so callers can invoke it directly for graceful-degrade fallback.
    #[must_use]
    pub fn extract_from_text(&self, situation_text: &str) -> Needs {
        use relay_directory::schema::ServiceType;
        let lower = situation_text.to_lowercase();

        let mut service_types = Vec::new();

        // Food keywords
        if lower.contains("food")
            || lower.contains("hungry")
            || lower.contains("eat")
            || lower.contains("meal")
            || lower.contains("pantry")
            || lower.contains("snap")
        {
            service_types.push(ServiceType::Food);
        }
        // Shelter keywords
        if lower.contains("shelter")
            || lower.contains("housing")
            || lower.contains("homeless")
            || lower.contains("couch")
            || lower.contains("evict")
            || lower.contains("sleep")
        {
            service_types.push(ServiceType::Shelter);
        }
        // Benefits keywords
        if lower.contains("benefit")
            || lower.contains("welfare")
            || lower.contains("assistance")
            || lower.contains("cut off")
        {
            service_types.push(ServiceType::Benefits);
        }
        // Health keywords
        if lower.contains("health")
            || lower.contains("medical")
            || lower.contains("doctor")
            || lower.contains("mental")
            || lower.contains("dental")
        {
            service_types.push(ServiceType::Health);
        }
        // Legal keywords
        if lower.contains("legal")
            || lower.contains("lawyer")
            || lower.contains("court")
            || lower.contains("custody")
        {
            service_types.push(ServiceType::Legal);
        }

        Needs {
            service_types,
            ..Needs::default()
        }
    }
}

impl NeedExtractor for KeywordExtractor {
    fn extract(&self, situation_text: &str) -> Result<Needs, MatchError> {
        Ok(self.extract_from_text(situation_text))
    }
}

// ─── LocalLlmExtractor ───────────────────────────────────────────────────────

/// Stub for the local LLM-backed extractor (qwen2.5:3b via ollama).
///
/// This is a **stub** — it always returns [`MatchError::ExtractorFailed`], which
/// triggers the graceful-degrade path in [`crate::matcher::Matcher::match_situation`].
/// The real implementation is deferred to AC8 (manual verification).
///
/// # Privacy
/// When fully implemented, situation text will only be sent to the local ollama
/// endpoint (127.0.0.1); it will never leave the device.
#[derive(Debug, Clone)]
pub struct LocalLlmExtractor {
    /// Ollama base URL (default: `http://127.0.0.1:11434`).
    pub base_url: String,
    /// Model name (default: `qwen2.5:3b`).
    pub model: String,
}

impl Default for LocalLlmExtractor {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:11434".to_owned(),
            model: "qwen2.5:3b".to_owned(),
        }
    }
}

impl LocalLlmExtractor {
    /// Create a new extractor with custom endpoint and model.
    #[must_use]
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            model: model.into(),
        }
    }
}

impl NeedExtractor for LocalLlmExtractor {
    /// Extract needs using the local LLM.
    ///
    /// # Errors
    /// Currently always returns [`MatchError::ExtractorFailed`] (stub;
    /// real implementation is AC8 / deferred).
    fn extract(&self, _situation_text: &str) -> Result<Needs, MatchError> {
        // STUB — deferred to AC8 (live model, manual verification).
        // When the real implementation ships, it will:
        //   1. POST to {base_url}/api/generate with a structured prompt.
        //   2. Parse the JSON response into a `Needs` value.
        //   3. Never write situation_text to disk.
        Err(MatchError::ExtractorFailed(
            "local LLM extractor not yet implemented (AC8 deferred)".to_owned(),
        ))
    }
}
