//! `relay-letters` — deterministic letter drafting for the relay workspace.
//!
//! # Overview
//!
//! Drafts navigation/boilerplate letters from a [`CaseRecord`] + a typed
//! template. The pipeline is:
//!
//! 1. A [`Template`] fills named slots from the case record; missing required
//!    slots produce `[[NEEDS: <slot>]]` placeholders — never fabricated values.
//! 2. The optional [`Prose`] layer can smooth the filled template into fluent
//!    text; it **must not** introduce new facts (enforced by an entity-diff
//!    post-check in [`prose::check_no_new_entities`]).
//! 3. Every output is wrapped with a safety header and footer by [`guardrails`]
//!    and scanned for advice phrases by [`guardrails::lint_advice_phrases`].
//!
//! # Safety boundary
//!
//! This crate contains **no network or send capability**. It only produces
//! `String` values for the caller to review and act on. The generated text is
//! navigation/boilerplate only — not legal, medical, or professional advice.

#![allow(clippy::print_stdout)] // tests use println!

pub mod case_record;
pub mod error;
pub mod guardrails;
pub mod prose;
pub mod template;

pub use case_record::CaseRecord;
pub use error::LetterError;
pub use prose::{MockProse, Prose};
pub use template::{LetterOutput, TemplateType};

/// Draft a letter from a case record.
///
/// Fills the template, optionally smooths prose, applies guardrails, and
/// returns a [`LetterOutput`].
///
/// # Errors
///
/// Returns [`LetterError`] if the prose smoother reports a fact-addition
/// violation or any other internal error.
pub fn draft<P: Prose>(
    record: &CaseRecord,
    template_type: TemplateType,
    prose: &P,
    smooth: bool,
) -> Result<LetterOutput, LetterError> {
    let filled = template::fill(record, template_type);
    let body = if smooth {
        let smoothed = prose.smooth(&filled)?;
        prose::check_no_new_entities(&filled, &smoothed)?;
        smoothed
    } else {
        filled
    };
    let output = guardrails::wrap(&body, template_type);
    Ok(output)
}
