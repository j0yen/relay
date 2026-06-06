//! Rule-based PII redaction pass.
//!
//! Detects obvious direct identifiers in free text using regex patterns and
//! returns a list of [`Redaction`] spans.  This layer is independent of any
//! language model.
//!
//! Detected categories:
//! - `email` — RFC-5321-ish email addresses.
//! - `phone` — US/international phone numbers in common formats.
//! - `ssn`   — Social Security Number shaped strings (`NNN-NN-NNNN`).

use crate::record::Redaction;
use regex::Regex;
use std::sync::LazyLock;

// ─── Compiled patterns ────────────────────────────────────────────────────────

/// Email address pattern.
static EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
    // SAFETY: this is a compile-time-constant pattern; failure means a programmer error.
    // We cannot use `?` here because LazyLock::new takes a closure returning T, not Result<T>.
    // The pattern is reviewed and known-good; we allow the expect at this initialization site.
    #[allow(clippy::expect_used)]
    Regex::new(r"[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}")
        .expect("email regex is a compile-time constant and always valid")
});

/// Phone number pattern (common US formats).
static PHONE_RE: LazyLock<Regex> = LazyLock::new(|| {
    #[allow(clippy::expect_used)]
    Regex::new(r"(?:\+?1[-.\s]?)?\(?[2-9]\d{2}\)?[-.\s]?[2-9]\d{2}[-.\s]?\d{4}")
        .expect("phone regex is a compile-time constant and always valid")
});

/// SSN-shaped pattern (`NNN-NN-NNNN`).
static SSN_RE: LazyLock<Regex> = LazyLock::new(|| {
    #[allow(clippy::expect_used)]
    Regex::new(r"\b\d{3}-\d{2}-\d{4}\b")
        .expect("SSN regex is a compile-time constant and always valid")
});

// ─── Public API ───────────────────────────────────────────────────────────────

/// Scan `text` and return all detected PII spans as [`Redaction`] values.
///
/// Overlapping spans are not de-duplicated here; callers receive the full list
/// and can apply them via [`crate::record::CaseRecord::minimized`].
#[must_use]
pub fn detect(text: &str) -> Vec<Redaction> {
    let mut out: Vec<Redaction> = Vec::new();

    for m in EMAIL_RE.find_iter(text) {
        out.push(Redaction {
            kind: "email".to_owned(),
            original: m.as_str().to_owned(),
            offset: m.start(),
        });
    }

    for m in PHONE_RE.find_iter(text) {
        out.push(Redaction {
            kind: "phone".to_owned(),
            original: m.as_str().to_owned(),
            offset: m.start(),
        });
    }

    for m in SSN_RE.find_iter(text) {
        out.push(Redaction {
            kind: "ssn".to_owned(),
            original: m.as_str().to_owned(),
            offset: m.start(),
        });
    }

    // Sort by offset ascending for a predictable return order.
    out.sort_by_key(|r| r.offset);
    out
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_email() {
        let text = "Please call me or email alice@example.org about the referral.";
        let found = detect(text);
        assert_eq!(found.len(), 1);
        assert_eq!(found.first().map(|r| r.kind.as_str()), Some("email"));
        assert_eq!(
            found.first().map(|r| r.original.as_str()),
            Some("alice@example.org")
        );
    }

    #[test]
    fn detects_phone_dashes() {
        let text = "My number is 555-867-5309 thanks.";
        let found = detect(text);
        let phones: Vec<_> = found.iter().filter(|r| r.kind == "phone").collect();
        assert!(!phones.is_empty(), "expected at least one phone match");
        assert!(phones.iter().any(|r| r.original.contains("867-5309")));
    }

    #[test]
    fn detects_ssn() {
        let text = "SSN is 123-45-6789 please keep confidential.";
        let found = detect(text);
        let ssns: Vec<_> = found.iter().filter(|r| r.kind == "ssn").collect();
        assert_eq!(ssns.len(), 1);
        assert_eq!(ssns.first().map(|r| r.original.as_str()), Some("123-45-6789"));
    }

    #[test]
    fn detects_multiple_pii_types() {
        let text = "Email: bob@test.com, SSN 987-65-4321, phone 212-555-1234.";
        let found = detect(text);
        let kinds: Vec<&str> = found.iter().map(|r| r.kind.as_str()).collect();
        assert!(kinds.contains(&"email"), "email not found in {kinds:?}");
        assert!(kinds.contains(&"ssn"), "ssn not found in {kinds:?}");
        assert!(kinds.contains(&"phone"), "phone not found in {kinds:?}");
    }

    #[test]
    fn no_false_positives_on_clean_text() {
        let text = "Client needs help with food and housing. No contact details given.";
        let found = detect(text);
        assert!(found.is_empty(), "unexpected detections: {found:?}");
    }

    #[test]
    fn minimized_text_contains_no_detected_pii() {
        use crate::record::{CaseRecord, ConsentLevel, Demographics};

        let story = "Call john@pii.example at 800-555-1234 or SSN 000-11-2222.";
        let redactions = detect(story);
        assert!(!redactions.is_empty());

        let record = CaseRecord {
            summary: story.to_owned(),
            needs: vec![],
            risk_flags: vec![],
            demographics: Demographics::default(),
            barriers: vec![],
            timeline: vec![],
            redactions,
            consent: ConsentLevel::default(),
            partial: false,
            partial_notice: None,
        };
        let min = record.minimized(ConsentLevel::FullExport);

        // None of the literal identifiers should survive.
        assert!(
            !min.summary.contains("john@pii.example"),
            "email survived minimization"
        );
        assert!(
            !min.summary.contains("800-555-1234"),
            "phone survived minimization"
        );
        assert!(
            !min.summary.contains("000-11-2222"),
            "SSN survived minimization"
        );
    }
}
