//! Safety guardrails for generated letters.
//!
//! Every letter output must carry a `DRAFT — review before sending` header
//! and a not-legal/not-medical-advice footer. This module applies those
//! wrappers and also provides an advice-phrase linter.
//!
//! # Safety boundary
//!
//! The letter text produced by this crate is navigation/boilerplate only.
//! It is not legal, medical, or professional advice. The advice-phrase linter
//! flags imperative legal or medical claims for human attention — it does not
//! suppress them, since a human must review all output before use.

use crate::template::{LetterOutput, TemplateType};

/// Advice phrases that suggest legal or medical conclusions.
///
/// These are flagged (not blocked) — the human reviewer decides what to do.
const ADVICE_PHRASES: &[&str] = &[
    "you should sue",
    "you have a case",
    "you have grounds to sue",
    "take them to court",
    "file a lawsuit",
    "you are entitled to",
    "you must",
    "you need to",
    "you should take legal",
    "you should see a doctor",
    "you need medical",
    "this is a medical emergency",
    "you have a medical",
    "you should be diagnosed",
];

/// The safety header prepended to every letter.
pub const SAFETY_HEADER: &str =
    "⚠  DRAFT — review before sending. Not legal, medical, or professional advice.";

/// The safety footer appended to every letter.
pub const SAFETY_FOOTER: &str =
    "---\n\
     NOTE: This letter was generated as a navigation/boilerplate draft. \
     It does not constitute legal, medical, or professional advice. \
     Review all content before use, and consult a qualified professional \
     for legal or medical matters.";

/// Wrap a filled template body with the safety header and footer.
///
/// Produces a [`LetterOutput`] with `missing_slots` extracted from any
/// `[[NEEDS: ...]]` placeholders present in the body.
#[must_use]
pub fn wrap(body: &str, template_type: TemplateType) -> LetterOutput {
    let missing_slots = extract_missing_slots(body);
    let text = format!("{SAFETY_HEADER}\n\n{body}\n{SAFETY_FOOTER}\n");
    LetterOutput {
        text,
        template_type,
        missing_slots,
    }
}

/// Extract all `[[NEEDS: <slot>]]` placeholder slot names from `text`.
fn extract_missing_slots(text: &str) -> Vec<String> {
    let mut slots = Vec::new();
    let mut remaining = text;
    while let Some(start) = remaining.find("[[NEEDS: ") {
        remaining = &remaining[start + "[[NEEDS: ".len()..];
        if let Some(end) = remaining.find("]]") {
            slots.push(remaining[..end].to_owned());
            remaining = &remaining[end + 2..];
        } else {
            break;
        }
    }
    slots
}

/// Lint `text` for advice phrases that suggest legal or medical conclusions.
///
/// Returns the list of flagged phrases found (case-insensitive). An empty
/// return means no flags. The caller decides how to surface these to the user.
#[must_use]
pub fn lint_advice_phrases(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    ADVICE_PHRASES
        .iter()
        .filter(|phrase| lower.contains(*phrase))
        .map(|p| (*p).to_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CaseRecord;
    use crate::template;

    // ── AC4: every template type gets header + footer ─────────────────────────

    #[test]
    fn all_templates_have_header_and_footer() {
        let record = CaseRecord {
            client_name: Some("Test Person".to_owned()),
            caseworker_name: Some("Helper".to_owned()),
            authoring_org: Some("Test Org".to_owned()),
            presenting_need: Some("food".to_owned()),
            ..CaseRecord::default()
        };

        for tt in [
            template::TemplateType::Referral,
            template::TemplateType::BenefitsAppeal,
            template::TemplateType::IntakeSummary,
            template::TemplateType::SupportLetter,
        ] {
            let body = template::fill(&record, tt);
            let output = wrap(&body, tt);
            assert!(
                output.text.contains("DRAFT — review before sending"),
                "template {tt} must contain safety header"
            );
            assert!(
                output.text.contains("does not constitute legal, medical"),
                "template {tt} must contain safety footer"
            );
        }
    }

    // ── AC5: advice-phrase linter ─────────────────────────────────────────────

    #[test]
    fn linter_flags_advice_phrase() {
        let text = "You should sue them for discrimination.";
        let flags = lint_advice_phrases(text);
        assert!(
            !flags.is_empty(),
            "advice phrase must be flagged, got empty flags"
        );
        assert!(flags.iter().any(|f| f.contains("sue")));
    }

    #[test]
    fn linter_does_not_flag_neutral_text() {
        let text = "We can connect you to the Springfield Food Bank for emergency \
                    food assistance. Please call them at 555-0100 to schedule an \
                    appointment. They serve households in this ZIP code.";
        let flags = lint_advice_phrases(text);
        assert!(
            flags.is_empty(),
            "neutral navigation text must not be flagged, got: {flags:?}"
        );
    }

    // ── extract_missing_slots ─────────────────────────────────────────────────

    #[test]
    fn extract_missing_slots_finds_placeholders() {
        let text = "Hello [[NEEDS: client_name]], your income is [[NEEDS: monthly_income_usd]].";
        let slots = extract_missing_slots(text);
        assert_eq!(slots, vec!["client_name", "monthly_income_usd"]);
    }

    #[test]
    fn extract_missing_slots_empty_when_none() {
        let text = "Hello Jane Doe, everything is filled in.";
        let slots = extract_missing_slots(text);
        assert!(slots.is_empty());
    }

    // ── AC6: no network/send in guardrails ────────────────────────────────────
    // (structural: the crate has no network deps — verified by Cargo.toml)
    // This test asserts a complete generation round-trip under MockProse
    // never calls any network function (trivially true since MockProse is
    // a pure in-process no-op, and the crate has no network dep).

    #[test]
    fn full_draft_roundtrip_no_network() {
        use crate::{MockProse, draft};

        let record = CaseRecord {
            client_name: Some("Roundtrip Person".to_owned()),
            caseworker_name: Some("Helper".to_owned()),
            authoring_org: Some("Test Org".to_owned()),
            presenting_need: Some("shelter".to_owned()),
            receiving_org: Some("Shelter First".to_owned()),
            receiving_program: Some("Emergency Shelter".to_owned()),
            ..CaseRecord::default()
        };

        let prose = MockProse;
        let output = draft(&record, template::TemplateType::Referral, &prose, false)
            .expect("draft must succeed with MockProse");

        assert!(output.text.contains("DRAFT — review before sending"));
        assert!(output.text.contains("Roundtrip Person"));
        assert!(output.text.contains("Shelter First"));
    }

    // ── AC7: MockProse preserves entities; injecting mock is caught ───────────

    #[test]
    fn mock_prose_smooth_preserves_entities_in_draft() {
        use crate::{MockProse, draft};

        let record = CaseRecord {
            client_name: Some("Alice Hernandez".to_owned()),
            caseworker_name: Some("Bob Chen".to_owned()),
            authoring_org: Some("Helper Org".to_owned()),
            presenting_need: Some("rental assistance".to_owned()),
            ..CaseRecord::default()
        };

        let prose = MockProse;
        let output = draft(&record, template::TemplateType::SupportLetter, &prose, true)
            .expect("smooth with MockProse must not fail");
        assert!(output.text.contains("Alice Hernandez"));
    }

    #[test]
    fn injecting_prose_is_caught_by_draft() {
        use crate::draft;
        use crate::prose::InjectingMockProse;

        let record = CaseRecord {
            client_name: Some("Carlos Mendez".to_owned()),
            caseworker_name: Some("Dana Lee".to_owned()),
            authoring_org: Some("Support Org".to_owned()),
            presenting_need: Some("utility assistance".to_owned()),
            ..CaseRecord::default()
        };

        let injecting = InjectingMockProse {
            injected: "Contact Acme Corporation immediately.".to_owned(),
        };

        let err = draft(&record, template::TemplateType::SupportLetter, &injecting, true)
            .expect_err("injecting prose must cause draft to fail");
        assert!(
            matches!(err, crate::LetterError::NewEntitiesIntroduced { .. }),
            "expected NewEntitiesIntroduced error, got: {err}"
        );
    }
}
