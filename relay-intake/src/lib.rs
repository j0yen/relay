//! `relay-intake` — structured case-record intake with PII redaction, consent gating,
//! and next-action generation.
//!
//! # Overview
//!
//! Given a helper's spoken-style intake story, this crate produces:
//! - A [`CaseRecord`] capturing structured facts about the person's situation.
//! - A `Vec<`[`NextAction`]`>` of follow-up steps, each optionally linked to a
//!   directory resource.
//!
//! The structuring is done by a [`Structurer`] implementation.  Two are provided:
//! - [`MockStructurer`] — deterministic, no I/O, used in tests and CLI default.
//! - [`LocalLlmStructurer`] — drives the local qwen ladder; gracefully degrades
//!   when the model is unreachable.
//!
//! A rule-based [`redact`] pass runs independently of any model and is always
//! applied when `--minimized` output is requested.

#![allow(clippy::module_name_repetitions)]

pub mod error;
pub mod record;
pub mod redact;
pub mod structurer;

pub use error::IntakeError;
pub use record::{CaseRecord, ConsentLevel, NextAction, ResourceId, TimelineEntry};
pub use structurer::{LocalLlmStructurer, MockStructurer, Structurer};
