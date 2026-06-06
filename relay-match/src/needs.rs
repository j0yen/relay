//! [`Needs`] — structured representation of a person's service needs, extracted
//! from free-text situation descriptions.

use relay_directory::schema::{EligibilityFlag, GeoPoint, ServiceType};
use serde::{Deserialize, Serialize};

/// How urgent the need is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Urgency {
    /// Informational; no immediate action required.
    Low,
    /// Should be addressed within days.
    Medium,
    /// Immediate need (shelter tonight, safety, medical emergency).
    High,
}

impl Default for Urgency {
    fn default() -> Self {
        Self::Medium
    }
}

/// Structured needs extracted from a free-text situation description.
///
/// This is the bridge between the prose a helper provides and the structured
/// directory query the matcher runs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Needs {
    /// Service types identified as relevant to the situation.
    ///
    /// An empty vec means no specific type was identified; the matcher will
    /// return all resources sorted by proximity.
    pub service_types: Vec<ServiceType>,

    /// Eligibility signals extracted from the situation.
    ///
    /// Used to filter out incompatible resources.  Empty means "unknown /
    /// no restriction implied."
    pub eligibility_signals: Vec<EligibilityFlag>,

    /// Geographic hint extracted from the situation (e.g. a neighbourhood name
    /// resolved to a [`GeoPoint`] by the caller).
    pub location_hint: Option<GeoPoint>,

    /// Languages spoken by the person (ISO 639-1 codes).
    pub languages: Vec<String>,

    /// How urgent the need is.
    pub urgency: Urgency,
}
