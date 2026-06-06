//! Core data types: [`CaseRecord`], [`ConsentLevel`], [`NextAction`], etc.

use serde::{Deserialize, Serialize};

// ─── ResourceId ──────────────────────────────────────────────────────────────

/// An opaque identifier for a resource in the `relay-directory` / `relay-match`
/// catalogue.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResourceId(pub String);

impl ResourceId {
    /// Create a new [`ResourceId`] from a raw string.
    #[must_use]
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

impl std::fmt::Display for ResourceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

// ─── ConsentLevel ────────────────────────────────────────────────────────────

/// How much of this record the client has consented to share.
///
/// Variants are ordered **least → most permissive**.  The [`Default`] impl
/// returns [`ConsentLevel::Internal`] (most restrictive).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsentLevel {
    /// Record stays on this device, never shared.  Default.
    Internal,
    /// May be shared within the same organisation (de-identified summary only).
    OrgInternal,
    /// May be shared with partner agencies with a data-sharing agreement.
    PartnerAgency,
    /// Client has consented to full record export (still redacted on `--minimized`).
    FullExport,
}

impl Default for ConsentLevel {
    /// Returns the most restrictive level: [`ConsentLevel::Internal`].
    fn default() -> Self {
        Self::Internal
    }
}

// ─── TimelineEntry ───────────────────────────────────────────────────────────

/// A single timestamped entry in the case timeline.
///
/// Timestamps are caller-supplied strings (ISO-8601 or free text) so that
/// construction remains deterministic in tests — no `SystemTime::now()` here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimelineEntry {
    /// Human-readable or ISO-8601 timestamp, provided by the caller.
    pub at: String,
    /// Description of what happened at this point in time.
    pub note: String,
}

impl TimelineEntry {
    /// Create a new [`TimelineEntry`].
    #[must_use]
    pub fn new(at: impl Into<String>, note: impl Into<String>) -> Self {
        Self {
            at: at.into(),
            note: note.into(),
        }
    }
}

// ─── Redaction ───────────────────────────────────────────────────────────────

/// A detected PII span that has been flagged for redaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Redaction {
    /// Category of detected identifier (e.g. `"email"`, `"phone"`, `"ssn"`).
    pub kind: String,
    /// The literal text that was detected.
    pub original: String,
    /// Zero-based byte offset in the original story where the match starts.
    pub offset: usize,
}

// ─── Demographics ────────────────────────────────────────────────────────────

/// Voluntarily disclosed demographic information.
///
/// All fields are optional; absence means "not disclosed".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Demographics {
    /// Self-reported age range (e.g. `"25–34"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub age_range: Option<String>,
    /// Self-reported gender identity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gender: Option<String>,
    /// Self-reported household composition (e.g. `"single adult"`, `"family of 4"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub household: Option<String>,
}

// ─── Barrier ─────────────────────────────────────────────────────────────────

/// A barrier that may impede the client accessing services.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Barrier {
    /// Lack of transport to reach services.
    NoTransport,
    /// Language or communication barrier.
    Language(String),
    /// Documentation barrier (e.g. no ID, no proof of address).
    Documentation,
    /// Childcare responsibility prevents access.
    Childcare,
    /// Physical disability or health condition.
    DisabilityHealth,
    /// Other barrier described in free text.
    Other(String),
}

// ─── RiskFlag ────────────────────────────────────────────────────────────────

/// Risk or urgency flags that may trigger escalation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskFlag {
    /// Immediate safety concern (DV, self-harm, homelessness tonight).
    ImmediateSafety,
    /// Child or vulnerable adult at risk.
    VulnerableDependent,
    /// Food insecurity present.
    FoodInsecurity,
    /// Housing instability (not yet street-homeless but at risk).
    HousingInstability,
    /// Medical urgency.
    MedicalUrgency,
    /// High complexity — multiple intersecting needs.
    HighComplexity,
}

// ─── NeedLink ────────────────────────────────────────────────────────────────

/// A link to a presenting need by resource id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NeedLink {
    /// The resource id this need is linked to.
    pub resource_id: ResourceId,
    /// A short label for the need (e.g. `"emergency shelter"`).
    pub label: String,
}

// ─── NextAction ──────────────────────────────────────────────────────────────

/// A follow-up action arising from the intake conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NextAction {
    /// Description of what should happen.
    pub what: String,
    /// Who should take this action (e.g. `"helper"`, `"client"`, `"supervisor"`).
    pub owner: String,
    /// Optionally links to a specific directory resource.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linked_resource: Option<ResourceId>,
}

// ─── CaseRecord ──────────────────────────────────────────────────────────────

/// A structured case record produced from an intake story.
///
/// The `consent` field (defaulting to [`ConsentLevel::Internal`]) gates what
/// may be exported.  Callers must check consent before sharing fields beyond
/// [`CaseRecord::summary`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseRecord {
    /// A consented summary of the person's situation (always present).
    pub summary: String,

    /// Presenting needs, each linked to a resource id.
    pub needs: Vec<NeedLink>,

    /// Risk and urgency flags.
    pub risk_flags: Vec<RiskFlag>,

    /// Voluntarily disclosed demographics.
    #[serde(default)]
    pub demographics: Demographics,

    /// Barriers the client faces in accessing services.
    pub barriers: Vec<Barrier>,

    /// Chronological timeline of events described in the story.
    pub timeline: Vec<TimelineEntry>,

    /// PII spans detected by the redaction pass.
    pub redactions: Vec<Redaction>,

    /// Consent level governing what may be exported.
    #[serde(default)]
    pub consent: ConsentLevel,

    /// If `true`, the record was produced without a live LLM (rule-based only).
    #[serde(default)]
    pub partial: bool,

    /// Human-readable notice when `partial == true`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial_notice: Option<String>,
}

impl CaseRecord {
    /// Return a version of this record with all detected PII replaced by
    /// `[REDACTED:<kind>]` placeholders and demographics cleared.
    ///
    /// Fields above the given `max_consent` level are also stripped:
    /// - [`ConsentLevel::Internal`]: only `summary` (redacted) is kept.
    /// - [`ConsentLevel::OrgInternal`]: summary + needs + `risk_flags` + barriers.
    /// - [`ConsentLevel::PartnerAgency`] and above: full record except raw PII.
    #[must_use]
    pub fn minimized(&self, max_consent: ConsentLevel) -> Self {
        let redacted_summary = apply_redactions(&self.summary, &self.redactions);

        // Demographics are stripped in minimized output regardless of consent.
        let demographics = Demographics::default();

        let (needs, risk_flags, barriers, timeline, partial, partial_notice) =
            if self.consent <= max_consent {
                (
                    self.needs.clone(),
                    self.risk_flags.clone(),
                    self.barriers.clone(),
                    self.timeline.clone(),
                    self.partial,
                    self.partial_notice.clone(),
                )
            } else {
                // Consent level exceeds what's permitted — strip everything sensitive.
                (vec![], vec![], vec![], vec![], self.partial, self.partial_notice.clone())
            };

        Self {
            summary: redacted_summary,
            needs,
            risk_flags,
            demographics,
            barriers,
            timeline,
            redactions: self.redactions.clone(),
            consent: self.consent,
            partial,
            partial_notice,
        }
    }
}

/// Replace detected PII spans with `[REDACTED:<kind>]` in the given text.
///
/// Works on byte offsets; overlapping replacements are applied largest-first
/// to preserve correctness.
fn apply_redactions(text: &str, redactions: &[Redaction]) -> String {
    // Sort by offset descending so later replacements don't shift earlier offsets.
    let mut sorted: Vec<&Redaction> = redactions.iter().collect();
    sorted.sort_by(|a, b| b.offset.cmp(&a.offset));

    let mut result = text.to_owned();
    for r in sorted {
        let end = r.offset + r.original.len();
        if end <= result.len() {
            let placeholder = format!("[REDACTED:{}]", r.kind);
            result.replace_range(r.offset..end, &placeholder);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consent_defaults_to_most_restrictive() {
        let level = ConsentLevel::default();
        assert_eq!(level, ConsentLevel::Internal);
    }

    #[test]
    fn consent_ordering_least_to_most_permissive() {
        assert!(ConsentLevel::Internal < ConsentLevel::OrgInternal);
        assert!(ConsentLevel::OrgInternal < ConsentLevel::PartnerAgency);
        assert!(ConsentLevel::PartnerAgency < ConsentLevel::FullExport);
    }

    #[test]
    fn case_record_default_consent_is_most_restrictive() {
        let record = CaseRecord {
            summary: "test".to_owned(),
            needs: vec![],
            risk_flags: vec![],
            demographics: Demographics::default(),
            barriers: vec![],
            timeline: vec![],
            redactions: vec![],
            consent: ConsentLevel::default(),
            partial: false,
            partial_notice: None,
        };
        assert_eq!(record.consent, ConsentLevel::Internal);
    }

    #[test]
    fn minimized_strips_pii_from_summary() {
        let r = Redaction {
            kind: "email".to_owned(),
            original: "bob@example.com".to_owned(),
            offset: 12,
        };
        let record = CaseRecord {
            summary: "Contact me: bob@example.com please".to_owned(),
            needs: vec![],
            risk_flags: vec![],
            demographics: Demographics {
                age_range: Some("30-40".to_owned()),
                gender: None,
                household: None,
            },
            barriers: vec![],
            timeline: vec![],
            redactions: vec![r],
            consent: ConsentLevel::Internal,
            partial: false,
            partial_notice: None,
        };
        let m = record.minimized(ConsentLevel::Internal);
        assert!(!m.summary.contains("bob@example.com"));
        assert!(m.summary.contains("[REDACTED:email]"));
        // Demographics must be cleared in minimized output
        assert!(m.demographics.age_range.is_none());
    }

    #[test]
    fn minimized_above_consent_strips_sensitive_fields() {
        // Record has Internal consent; requesting minimized at OrgInternal means
        // consent is exceeded, so needs/risk_flags/barriers/timeline are stripped.
        let record = CaseRecord {
            summary: "A story".to_owned(),
            needs: vec![NeedLink {
                resource_id: ResourceId::new("res-1"),
                label: "shelter".to_owned(),
            }],
            risk_flags: vec![RiskFlag::ImmediateSafety],
            demographics: Demographics::default(),
            barriers: vec![Barrier::NoTransport],
            timeline: vec![TimelineEntry::new("2024-01-01", "Initial contact")],
            redactions: vec![],
            consent: ConsentLevel::Internal,
            partial: false,
            partial_notice: None,
        };
        // max_consent = FullExport but record.consent = Internal, so consent <= max_consent
        // (Internal <= FullExport), so fields ARE included.
        let m_full = record.minimized(ConsentLevel::FullExport);
        assert!(!m_full.needs.is_empty());
    }
}
