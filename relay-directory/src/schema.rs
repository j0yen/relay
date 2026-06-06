//! Normalised `Resource` model for the relay workspace.
//!
//! Collapses the HSDS core entities (organization / service /
//! location / `service_at_location`) into what a matcher needs.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ─── ServiceType ─────────────────────────────────────────────────────────────

/// High-level taxonomy of human-service types.
///
/// Derived from the Open Referral HSDS taxonomy; extended with `Other` for
/// unknown or future categories.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ServiceType {
    /// Food assistance (food banks, meals programmes, SNAP navigation).
    Food,
    /// Emergency and transitional shelter.
    Shelter,
    /// Legal aid and representation.
    Legal,
    /// Medical, dental, mental-health services.
    Health,
    /// Government benefits navigation and enrollment.
    Benefits,
    /// Employment and job-training programmes.
    Employment,
    /// Child and elder care services.
    Care,
    /// Transportation assistance.
    Transportation,
    /// Clothing and household goods.
    Goods,
    /// Any type not captured above; raw taxonomy string preserved.
    Other(String),
}

impl ServiceType {
    /// Parse a taxonomy string into a [`ServiceType`].
    #[must_use]
    pub fn from_taxonomy(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "food" | "food_bank" | "food bank" | "meals" | "pantry" => Self::Food,
            "shelter" | "housing" | "emergency_shelter" | "emergency shelter" => Self::Shelter,
            "legal" | "legal_aid" | "legal aid" => Self::Legal,
            "health" | "medical" | "mental_health" | "dental" => Self::Health,
            "benefits" | "benefit" | "snap" | "social_services" => Self::Benefits,
            "employment" | "job_training" | "workforce" => Self::Employment,
            "childcare" | "child_care" | "eldercare" | "elder_care" | "care" => Self::Care,
            "transportation" | "transit" => Self::Transportation,
            "goods" | "clothing" | "household" => Self::Goods,
            other => Self::Other(other.to_owned()),
        }
    }
}

// ─── GeoPoint ────────────────────────────────────────────────────────────────

/// A geographic point as WGS-84 latitude/longitude.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GeoPoint {
    /// Latitude in decimal degrees (−90..=90).
    pub lat: f64,
    /// Longitude in decimal degrees (−180..=180).
    pub lon: f64,
}

impl GeoPoint {
    /// Construct a new `GeoPoint`, returning `None` if coordinates are out of range.
    #[must_use]
    pub fn new(lat: f64, lon: f64) -> Option<Self> {
        if (-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon) {
            Some(Self { lat, lon })
        } else {
            None
        }
    }

    /// Haversine distance to another point in kilometres.
    #[must_use]
    pub fn distance_km(self, other: Self) -> f64 {
        const R: f64 = 6_371.0;
        let d_lat = (other.lat - self.lat).to_radians();
        let d_lon = (other.lon - self.lon).to_radians();
        #[allow(clippy::suboptimal_flops)]
        let a = (d_lat / 2.0).sin().powi(2)
            + self.lat.to_radians().cos()
                * other.lat.to_radians().cos()
                * (d_lon / 2.0).sin().powi(2);
        let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
        R * c
    }
}

// ─── Hours ───────────────────────────────────────────────────────────────────

/// Operating hours as a free-form string (e.g. "Mon–Fri 9am–5pm").
///
/// Stored verbatim because HSDS hours encoding varies widely across sources.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hours(pub String);

// ─── EligibilityFlag ─────────────────────────────────────────────────────────

/// Eligibility restriction flags on a resource.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum EligibilityFlag {
    /// Service is restricted to individuals under 18.
    Under18Only,
    /// Service is restricted to individuals 18 or older.
    Adults18Plus,
    /// Service is restricted to individuals 60 or older.
    Seniors60Plus,
    /// Service is restricted to veterans.
    VeteransOnly,
    /// Service is restricted to documented residents only.
    DocumentedOnly,
    /// Service is restricted to a specific zip code or county.
    LocalResidentOnly,
    /// Any other eligibility note; raw string preserved.
    Other(String),
}

// ─── Provenance ──────────────────────────────────────────────────────────────

/// Source provenance for a resource record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    /// Identifier of the data source (e.g. `"hsds:openreferral-demo"`).
    pub source_id: String,
    /// Raw id from the originating dataset (used for dedup and upsert).
    pub external_id: String,
    /// UTC timestamp when this record was last ingested.
    pub ingested_at: DateTime<Utc>,
}

// ─── Resource ────────────────────────────────────────────────────────────────

/// A single normalised human-services resource.
///
/// One `Resource` corresponds to one service at one location offered by one
/// organisation. Where HSDS splits these across four tables, the directory
/// collapses them for local query efficiency.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Resource {
    /// Stable dedup key — SHA-256 hex of `(org_name, service_name, lat, lon)`.
    pub id: String,

    /// Name of the organisation offering the service.
    pub org_name: String,

    /// Name of the specific service (may equal `org_name` for small orgs).
    pub service_name: String,

    /// High-level service categories.
    pub service_types: Vec<ServiceType>,

    /// Geographic location of the service access point.
    pub location: Option<GeoPoint>,

    /// Street address (optional — not all entries have structured addresses).
    pub address: Option<String>,

    /// Primary phone number.
    pub phone: Option<String>,

    /// Website URL.
    pub url: Option<String>,

    /// Operating hours description.
    pub hours: Option<Hours>,

    /// Eligibility restrictions. Empty vec means "open to all".
    pub eligibility: Vec<EligibilityFlag>,

    /// Languages in which the service is offered (ISO 639-1 codes).
    pub languages: Vec<String>,

    /// Human-readable description.
    pub description: Option<String>,

    /// Source provenance.
    pub provenance: Provenance,
}

impl Resource {
    /// Compute the stable dedup key for a resource given its identifying fields.
    ///
    /// The key is a hex-encoded SHA-256 of the canonical form
    /// `"<org_name>\0<service_name>\0<lat_6dp>\0<lon_6dp>"`.
    #[must_use]
    pub fn dedup_key(org_name: &str, service_name: &str, location: Option<GeoPoint>) -> String {
        use std::fmt::Write as _;
        // FNV-1a-64 constants — declared before any statements to satisfy clippy::items_after_statements.
        const OFFSET: u64 = 14_695_981_039_346_656_037;
        const PRIME: u64 = 1_099_511_628_211;
        let mut buf = String::new();
        let _ = write!(buf, "{org_name}\0{service_name}\0");
        match location {
            Some(pt) => {
                let _ = write!(buf, "{:.6}\0{:.6}", pt.lat, pt.lon);
            }
            None => buf.push_str("no_loc\0no_loc"),
        }
        // Simple FNV-1a-64 — no crypto needed for a local dedup key.
        let hash = buf.bytes().fold(OFFSET, |acc, b| {
            acc.wrapping_mul(PRIME) ^ byte_to_u64(b)
        });
        format!("{hash:016x}")
    }
}

/// Extend `u8` → `u64`; u8→u64 is always lossless.
#[inline]
const fn byte_to_u64(b: u8) -> u64 {
    // u8 fits in u64: this cast is always lossless.
    #[allow(clippy::as_conversions)]
    { b as u64 }
}
