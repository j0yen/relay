//! Deterministic template filling for the four letter types.
//!
//! Each template has named slots. Slots are filled from a [`CaseRecord`].
//! Missing required slots produce `[[NEEDS: <slot>]]` placeholders — never
//! blank strings or fabricated values. Slots are optional only when the letter
//! type genuinely does not need them.

use crate::CaseRecord;
use chrono::Utc;

/// The four supported letter types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TemplateType {
    /// A referral letter from one agency to another.
    Referral,
    /// A letter appealing a benefit or program denial.
    BenefitsAppeal,
    /// An intake summary for another agency.
    IntakeSummary,
    /// A general support letter (character/situation support).
    SupportLetter,
}

impl std::fmt::Display for TemplateType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Referral => write!(f, "Referral"),
            Self::BenefitsAppeal => write!(f, "Benefits Appeal"),
            Self::IntakeSummary => write!(f, "Intake Summary"),
            Self::SupportLetter => write!(f, "Support Letter"),
        }
    }
}

impl std::str::FromStr for TemplateType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().replace('-', "_").as_str() {
            "referral" => Ok(Self::Referral),
            "benefits_appeal" | "appeal" => Ok(Self::BenefitsAppeal),
            "intake_summary" | "intake" => Ok(Self::IntakeSummary),
            "support_letter" | "support" => Ok(Self::SupportLetter),
            other => Err(format!(
                "unknown template type: {other:?}. \
                 Valid values: referral, benefits-appeal, intake-summary, support-letter"
            )),
        }
    }
}

/// The result of filling a template.
#[derive(Debug, Clone)]
pub struct LetterOutput {
    /// The rendered letter text, including the safety header and footer.
    pub text: String,
    /// The template type that was used.
    pub template_type: TemplateType,
    /// Names of any required slots that were missing and replaced with
    /// `[[NEEDS: ...]]` placeholders.
    pub missing_slots: Vec<String>,
}

/// Fill `template_type` from `record` and return the raw filled body (without
/// guardrail wrapper).
///
/// Missing required slots are replaced with `[[NEEDS: <slot>]]`. The caller
/// (normally [`crate::draft`]) applies the guardrail wrapper.
#[must_use]
pub fn fill(record: &CaseRecord, template_type: TemplateType) -> String {
    match template_type {
        TemplateType::Referral => fill_referral(record),
        TemplateType::BenefitsAppeal => fill_benefits_appeal(record),
        TemplateType::IntakeSummary => fill_intake_summary(record),
        TemplateType::SupportLetter => fill_support_letter(record),
    }
}

// ─── slot helpers ─────────────────────────────────────────────────────────────

/// Return the slot value if present, or a `[[NEEDS: <name>]]` placeholder.
fn slot(value: Option<&str>, name: &str) -> String {
    value.map_or_else(|| format!("[[NEEDS: {name}]]"), str::to_owned)
}

/// Format an optional u32 as a string, or return a placeholder.
fn slot_u32(value: Option<u32>, name: &str) -> String {
    value.map_or_else(|| format!("[[NEEDS: {name}]]"), |v| v.to_string())
}

/// Format an optional f64 as dollars, or return a placeholder.
fn slot_dollars(value: Option<f64>, name: &str) -> String {
    value.map_or_else(|| format!("[[NEEDS: {name}]]"), |v| format!("${v:.2}"))
}

/// Return today's date as a formatted string.
fn today() -> String {
    Utc::now().format("%B %d, %Y").to_string()
}

// ─── referral ─────────────────────────────────────────────────────────────────

fn fill_referral(r: &CaseRecord) -> String {
    let client = slot(r.client_name.as_deref(), "client_name");
    let dob = slot(r.date_of_birth.as_deref(), "date_of_birth");
    let phone = slot(r.phone.as_deref(), "phone");
    let address = slot(r.address.as_deref(), "address");
    let household = slot_u32(r.household_size, "household_size");
    let income = slot_dollars(r.monthly_income_usd, "monthly_income_usd");
    let need = slot(r.presenting_need.as_deref(), "presenting_need");
    let receiving_org = slot(r.receiving_org.as_deref(), "receiving_org");
    let receiving_prog = slot(r.receiving_program.as_deref(), "receiving_program");
    let cw = slot(r.caseworker_name.as_deref(), "caseworker_name");
    let auth_org = slot(r.authoring_org.as_deref(), "authoring_org");
    let date = today();

    format!(
        "{date}\n\
         \n\
         To: {receiving_org}\n\
         Program: {receiving_prog}\n\
         \n\
         Subject: Referral for {client}\n\
         \n\
         Dear {receiving_org} Team,\n\
         \n\
         I am writing to refer {client} to your organization for assistance with \
         the following need: {need}.\n\
         \n\
         Client Information:\n\
         - Full Name:       {client}\n\
         - Date of Birth:   {dob}\n\
         - Phone:           {phone}\n\
         - Address:         {address}\n\
         - Household Size:  {household}\n\
         - Monthly Income:  {income}\n\
         \n\
         We believe {receiving_prog} at {receiving_org} can provide appropriate \
         support. Please feel free to contact our office with any questions.\n\
         \n\
         Sincerely,\n\
         {cw}\n\
         {auth_org}\n"
    )
}

// ─── benefits appeal ──────────────────────────────────────────────────────────

fn fill_benefits_appeal(r: &CaseRecord) -> String {
    let client = slot(r.client_name.as_deref(), "client_name");
    let dob = slot(r.date_of_birth.as_deref(), "date_of_birth");
    let phone = slot(r.phone.as_deref(), "phone");
    let program = slot(r.benefit_program.as_deref(), "benefit_program");
    let denial_date = slot(r.denial_date.as_deref(), "denial_date");
    let grounds = slot(r.appeal_grounds.as_deref(), "appeal_grounds");
    let cw = slot(r.caseworker_name.as_deref(), "caseworker_name");
    let auth_org = slot(r.authoring_org.as_deref(), "authoring_org");
    let date = today();

    format!(
        "{date}\n\
         \n\
         Re: Appeal of {program} Denial for {client}\n\
         \n\
         To Whom It May Concern,\n\
         \n\
         On behalf of {client} (Date of Birth: {dob}, Phone: {phone}), \
         I am writing to formally appeal the denial of {program} dated {denial_date}.\n\
         \n\
         Grounds for appeal:\n\
         {grounds}\n\
         \n\
         We respectfully request that this determination be reconsidered. \
         Supporting documentation can be provided upon request.\n\
         \n\
         Sincerely,\n\
         {cw}\n\
         {auth_org}\n"
    )
}

// ─── intake summary ───────────────────────────────────────────────────────────

fn fill_intake_summary(r: &CaseRecord) -> String {
    let client = slot(r.client_name.as_deref(), "client_name");
    let dob = slot(r.date_of_birth.as_deref(), "date_of_birth");
    let phone = slot(r.phone.as_deref(), "phone");
    let address = slot(r.address.as_deref(), "address");
    let household = slot_u32(r.household_size, "household_size");
    let income = slot_dollars(r.monthly_income_usd, "monthly_income_usd");
    let need = slot(r.presenting_need.as_deref(), "presenting_need");
    let notes = r.notes.as_deref().unwrap_or("(none)");
    let cw = slot(r.caseworker_name.as_deref(), "caseworker_name");
    let auth_org = slot(r.authoring_org.as_deref(), "authoring_org");
    let date = today();

    format!(
        "INTAKE SUMMARY — {date}\n\
         Prepared by: {cw}, {auth_org}\n\
         \n\
         CLIENT INFORMATION\n\
         - Full Name:       {client}\n\
         - Date of Birth:   {dob}\n\
         - Phone:           {phone}\n\
         - Address:         {address}\n\
         - Household Size:  {household}\n\
         - Monthly Income:  {income}\n\
         \n\
         PRESENTING NEED\n\
         {need}\n\
         \n\
         ADDITIONAL NOTES\n\
         {notes}\n"
    )
}

// ─── support letter ───────────────────────────────────────────────────────────

fn fill_support_letter(r: &CaseRecord) -> String {
    let client = slot(r.client_name.as_deref(), "client_name");
    let need = slot(r.presenting_need.as_deref(), "presenting_need");
    let cw = slot(r.caseworker_name.as_deref(), "caseworker_name");
    let auth_org = slot(r.authoring_org.as_deref(), "authoring_org");
    let date = today();

    format!(
        "{date}\n\
         \n\
         To Whom It May Concern,\n\
         \n\
         I am writing in support of {client}. Our organization, {auth_org}, \
         has been in contact with {client} regarding the following need: {need}.\n\
         \n\
         We can confirm that {client} is actively seeking assistance and has \
         engaged with our intake process. We respectfully request consideration \
         of any available support.\n\
         \n\
         Sincerely,\n\
         {cw}\n\
         {auth_org}\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CaseRecord;

    fn fixture_record() -> CaseRecord {
        CaseRecord {
            client_name: Some("Jane Doe".to_owned()),
            date_of_birth: Some("1985-03-12".to_owned()),
            phone: Some("555-0100".to_owned()),
            address: Some("100 Main St, Springfield, IL 62701".to_owned()),
            household_size: Some(3),
            monthly_income_usd: Some(1_800.00),
            presenting_need: Some("emergency food assistance".to_owned()),
            receiving_org: Some("Springfield Food Bank".to_owned()),
            receiving_program: Some("Emergency Food Pantry".to_owned()),
            caseworker_name: Some("Alex Rivera".to_owned()),
            authoring_org: Some("Community Help Center".to_owned()),
            benefit_program: Some("SNAP".to_owned()),
            denial_date: Some("2026-05-01".to_owned()),
            appeal_grounds: Some(
                "The client's income was miscalculated; correct monthly income is $1,800."
                    .to_owned(),
            ),
            notes: Some("Client has two school-age children.".to_owned()),
        }
    }

    // ── AC2: golden test — deterministic fill ─────────────────────────────────

    #[test]
    fn referral_golden_contains_key_fields() {
        let record = fixture_record();
        let text = fill(&record, TemplateType::Referral);
        assert!(text.contains("Jane Doe"), "must contain client name");
        assert!(text.contains("Springfield Food Bank"), "must contain receiving org");
        assert!(text.contains("Emergency Food Pantry"), "must contain program");
        assert!(text.contains("Alex Rivera"), "must contain caseworker");
        assert!(text.contains("$1800.00"), "must contain income");
        assert!(text.contains("emergency food assistance"), "must contain need");
    }

    #[test]
    fn referral_is_deterministic() {
        let record = fixture_record();
        // Two fills on the same day must produce identical text
        // (date will be same within a test run).
        let a = fill(&record, TemplateType::Referral);
        let b = fill(&record, TemplateType::Referral);
        assert_eq!(a, b, "template fill must be deterministic");
    }

    // ── AC3: missing required slot → placeholder ──────────────────────────────

    #[test]
    fn missing_client_name_produces_placeholder() {
        let mut record = fixture_record();
        record.client_name = None;
        let text = fill(&record, TemplateType::Referral);
        assert!(
            text.contains("[[NEEDS: client_name]]"),
            "missing client_name must produce placeholder, got: {text}"
        );
        assert!(
            !text.contains("Jane"),
            "must not contain old client name when None"
        );
    }

    #[test]
    fn missing_appeal_grounds_placeholder() {
        let mut record = fixture_record();
        record.appeal_grounds = None;
        let text = fill(&record, TemplateType::BenefitsAppeal);
        assert!(
            text.contains("[[NEEDS: appeal_grounds]]"),
            "missing appeal_grounds must produce placeholder"
        );
    }

    #[test]
    fn missing_receiving_org_placeholder() {
        let mut record = fixture_record();
        record.receiving_org = None;
        let text = fill(&record, TemplateType::Referral);
        assert!(text.contains("[[NEEDS: receiving_org]]"));
    }

    // ── all four template types compile and produce non-empty text ─────────────

    #[test]
    fn all_template_types_produce_output() {
        let record = fixture_record();
        for tt in [
            TemplateType::Referral,
            TemplateType::BenefitsAppeal,
            TemplateType::IntakeSummary,
            TemplateType::SupportLetter,
        ] {
            let text = fill(&record, tt);
            assert!(!text.is_empty(), "template {tt} must produce non-empty text");
            assert!(text.len() > 100, "template {tt} must produce substantial text");
        }
    }

    // ── FromStr / Display round-trip ───────────────────────────────────────────

    #[test]
    fn template_type_from_str() {
        use std::str::FromStr;
        assert_eq!(
            TemplateType::from_str("referral").unwrap(),
            TemplateType::Referral
        );
        assert_eq!(
            TemplateType::from_str("benefits-appeal").unwrap(),
            TemplateType::BenefitsAppeal
        );
        assert_eq!(
            TemplateType::from_str("intake-summary").unwrap(),
            TemplateType::IntakeSummary
        );
        assert_eq!(
            TemplateType::from_str("support-letter").unwrap(),
            TemplateType::SupportLetter
        );
        assert!(TemplateType::from_str("unknown").is_err());
    }
}
