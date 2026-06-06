//! `relay-match` — free-text need extraction and deterministic resource ranking.
//!
//! Turns a prose situation description (e.g. "she's couch-surfing with two kids
//! and her benefits got cut off") into structured [`Needs`] via a
//! [`NeedExtractor`] impl, then ranks directory resources by (service-type
//! overlap, eligibility compatibility, proximity, hours, language match),
//! returning a ranked [`Vec<Match>`] with per-factor scores and a plain-language
//! explanation.
//!
//! # Privacy
//! Situation text is **never written to disk** and is only passed to the
//! extractor in memory.  Under [`MockExtractor`] no network connection is made.
//! [`LocalLlmExtractor`] sends text to a local ollama endpoint only.
//!
//! # Modules
//! - [`extractor`] — [`NeedExtractor`] trait, [`MockExtractor`], [`LocalLlmExtractor`] stub.
//! - [`needs`] — [`Needs`] structured type.
//! - [`matcher`] — deterministic scorer and [`Match`] result type.
//! - [`error`] — unified error type.

pub mod error;
pub mod extractor;
pub mod matcher;
pub mod needs;

pub use error::MatchError;
pub use extractor::{LocalLlmExtractor, MockExtractor, NeedExtractor};
pub use matcher::{Match, Matcher, MatcherConfig};
pub use needs::{Needs, Urgency};

#[cfg(test)]
mod tests;
