//! Minimal local mirror of `relay-intake`'s `CaseRecord`.
//!
//! # TEMPORARY SEAM — replace on relay-intake land
//!
//! `relay-intake` is not yet in master (it lives on an unlanded `build/*`
//! branch). This struct is a local mirror with the fields that the letter
//! templates need. Once `relay-intake` lands and becomes a workspace member,
//! replace this file with:
//!
//! ```ignore
//! pub use relay_intake::CaseRecord;
//! ```
//!
//! and remove the `case_record` module from `lib.rs`. The field names and types
//! below are intentionally kept minimal so the swap is mechanical.

use serde::{Deserialize, Serialize};

/// A structured case record produced by relay-intake (local mirror).
///
/// All fields are `Option<String>` unless the intake form guarantees a value,
/// so missing data is surfaced as `[[NEEDS: ...]]` placeholders rather than
/// panics or empty strings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub struct CaseRecord {
    /// Full name of the person being helped.
    pub client_name: Option<String>,
    /// Date of birth, ISO 8601 (YYYY-MM-DD).
    pub date_of_birth: Option<String>,
    /// Primary phone number.
    pub phone: Option<String>,
    /// Mailing address (free text).
    pub address: Option<String>,
    /// Household size (number of people).
    pub household_size: Option<u32>,
    /// Monthly household income in USD.
    pub monthly_income_usd: Option<f64>,
    /// Primary presenting need (free-text summary).
    pub presenting_need: Option<String>,
    /// Receiving organization name (for referrals).
    pub receiving_org: Option<String>,
    /// Program or service being referred to.
    pub receiving_program: Option<String>,
    /// Name of the caseworker/helper authoring the letter.
    pub caseworker_name: Option<String>,
    /// Name of the organization authoring the letter.
    pub authoring_org: Option<String>,
    /// Benefit or program being appealed (for benefits-appeal letters).
    pub benefit_program: Option<String>,
    /// Date of denial, ISO 8601.
    pub denial_date: Option<String>,
    /// Grounds for the appeal (free text; written by helper, not generated).
    pub appeal_grounds: Option<String>,
    /// Any additional context notes (free text).
    pub notes: Option<String>,
}

impl CaseRecord {
    /// Create a new empty [`CaseRecord`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}
