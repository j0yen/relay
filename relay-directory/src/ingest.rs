//! Ingest layer: the [`Ingestor`] trait, HSDS-JSON ingestor, and CSV ingestor.
//!
//! All ingestors return `Vec<Resource>` from a byte slice so that callers can
//! feed them any data source (file, HTTP body, stdin) without the ingestor
//! touching I/O itself.

use crate::{
    error::DirectoryError,
    schema::{EligibilityFlag, GeoPoint, Hours, Provenance, Resource, ServiceType},
};
use chrono::Utc;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use tracing::warn;

// ─── Ingestor trait ───────────────────────────────────────────────────────────

/// Converts a raw byte payload into normalised [`Resource`] records.
///
/// The trait is synchronous; network fetching is the caller's concern.
pub trait Ingestor {
    /// Parse `data` into a list of resources.
    ///
    /// # Errors
    /// Returns [`DirectoryError`] on unrecoverable parse failure.  Individual
    /// malformed rows are skipped with a warning (counted by the implementation).
    fn ingest(&self, data: &[u8]) -> Result<IngestReport, DirectoryError>;
}

/// Summary returned by every [`Ingestor`] call.
#[derive(Debug, Clone)]
pub struct IngestReport {
    /// Successfully parsed resources.
    pub resources: Vec<Resource>,
    /// Number of rows/records that were skipped due to errors.
    pub skipped: usize,
}

// ─── Dedup ────────────────────────────────────────────────────────────────────

/// Deduplicate a list of resources by their stable dedup key.
///
/// When two records share the same key (same org+service+location), the later
/// record in the slice is discarded.  The output order matches the first
/// occurrence of each key.
#[must_use]
pub fn dedup(resources: Vec<Resource>) -> Vec<Resource> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::with_capacity(resources.len());
    for r in resources {
        if seen.insert(r.id.clone()) {
            out.push(r);
        }
    }
    out
}

// ─── HSDS-JSON ingestor ───────────────────────────────────────────────────────

/// Ingests an HSDS 2.x JSON export (as produced by many 211 APIs).
///
/// The export is expected to be a JSON object with top-level arrays for the
/// HSDS core tables.  This ingestor handles both the full multi-table form and
/// the simpler "services" flat array common in HSDS 2.x bulk exports.
pub struct HsdsJsonIngestor {
    /// Source identifier embedded in [`Provenance`].
    pub source_id: String,
}

// ─── Internal HSDS wire types (serde-only, not exposed) ──────────────────────

#[derive(Debug, Deserialize)]
struct HsdsExport {
    #[serde(default)]
    organizations: Vec<HsdsOrg>,
    #[serde(default)]
    services: Vec<HsdsService>,
    #[serde(default)]
    locations: Vec<HsdsLocation>,
    #[serde(default)]
    services_at_location: Vec<HsdsSal>,
}

#[derive(Debug, Deserialize)]
struct HsdsOrg {
    id: String,
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct HsdsService {
    id: String,
    organization_id: String,
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    taxonomy_ids: Option<String>,
    #[serde(default)]
    eligibility: Option<String>,
    #[serde(default)]
    languages: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    phones: Vec<HsdsPhone>,
}

#[derive(Debug, Deserialize)]
struct HsdsPhone {
    #[serde(default)]
    number: Option<String>,
}

#[derive(Debug, Deserialize)]
struct HsdsLocation {
    id: String,
    #[serde(default)]
    latitude: Option<f64>,
    #[serde(default)]
    longitude: Option<f64>,
    #[serde(default)]
    address_1: Option<String>,
    #[serde(default)]
    city: Option<String>,
    #[serde(default)]
    state_province: Option<String>,
    #[serde(default)]
    postal_code: Option<String>,
}

#[derive(Debug, Deserialize)]
struct HsdsSal {
    service_id: String,
    location_id: String,
}

impl HsdsJsonIngestor {
    /// Construct a new HSDS-JSON ingestor with the given `source_id`.
    #[must_use]
    pub fn new(source_id: impl Into<String>) -> Self {
        Self {
            source_id: source_id.into(),
        }
    }

    fn build_resource(
        &self,
        svc: &HsdsService,
        org: &HsdsOrg,
        loc: Option<&HsdsLocation>,
    ) -> Resource {
        let location = loc.and_then(|l| {
            let lat_val = l.latitude?;
            let lon_val = l.longitude?;
            GeoPoint::new(lat_val, lon_val)
        });

        let address = loc.and_then(|l| {
            let parts: Vec<&str> = [
                l.address_1.as_deref(),
                l.city.as_deref(),
                l.state_province.as_deref(),
                l.postal_code.as_deref(),
            ]
            .into_iter()
            .flatten()
            .collect();
            if parts.is_empty() {
                None
            } else {
                Some(parts.join(", "))
            }
        });

        let service_types: Vec<ServiceType> = svc
            .taxonomy_ids
            .as_deref()
            .unwrap_or("")
            .split([',', ';'])
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ServiceType::from_taxonomy)
            .collect();

        let languages: Vec<String> = svc
            .languages
            .as_deref()
            .unwrap_or("")
            .split([',', ';'])
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();

        let eligibility = parse_eligibility(svc.eligibility.as_deref().unwrap_or(""));

        let phone = svc.phones.first().and_then(|p| p.number.clone());

        let id = Resource::dedup_key(&org.name, &svc.name, location);
        let description = svc.description.clone().or_else(|| org.description.clone());
        let url = svc.url.clone().or_else(|| org.url.clone());

        Resource {
            id,
            org_name: org.name.clone(),
            service_name: svc.name.clone(),
            service_types,
            location,
            address,
            phone,
            url,
            hours: None, // HSDS hours are nested; left to a post-processor
            eligibility,
            languages,
            description,
            provenance: Provenance {
                source_id: self.source_id.clone(),
                external_id: svc.id.clone(),
                ingested_at: Utc::now(),
            },
        }
    }
}

impl Ingestor for HsdsJsonIngestor {
    fn ingest(&self, data: &[u8]) -> Result<IngestReport, DirectoryError> {
        let export: HsdsExport = serde_json::from_slice(data)?;

        // Build lookup maps for O(1) joins.
        let org_map: HashMap<&str, &HsdsOrg> =
            export.organizations.iter().map(|o| (o.id.as_str(), o)).collect();
        let loc_map: HashMap<&str, &HsdsLocation> =
            export.locations.iter().map(|l| (l.id.as_str(), l)).collect();

        // service → location mapping via services_at_location
        let sal_map: HashMap<&str, &str> = export
            .services_at_location
            .iter()
            .map(|sal| (sal.service_id.as_str(), sal.location_id.as_str()))
            .collect();

        let mut resources = Vec::new();
        let mut skipped: usize = 0;

        for svc in &export.services {
            // Skip inactive services silently.
            if svc
                .status
                .as_deref()
                .is_some_and(|s| s.eq_ignore_ascii_case("inactive"))
            {
                skipped = skipped.saturating_add(1);
                continue;
            }

            let Some(org) = org_map.get(svc.organization_id.as_str()) else {
                warn!(
                    service_id = %svc.id,
                    org_id = %svc.organization_id,
                    "skipping service: unknown organization_id"
                );
                skipped = skipped.saturating_add(1);
                continue;
            };

            let loc = sal_map
                .get(svc.id.as_str())
                .and_then(|lid| loc_map.get(*lid).copied());

            resources.push(self.build_resource(svc, org, loc));
        }

        let resources = dedup(resources);
        Ok(IngestReport { resources, skipped })
    }
}

// ─── CSV ingestor ─────────────────────────────────────────────────────────────

/// Ingests a 211-style CSV sheet.
///
/// Expected columns (header row required, case-insensitive):
/// `org_name`, `service_name`, `taxonomy`, `latitude`, `longitude`, `address`,
/// `phone`, `url`, `hours`, `eligibility`, `languages`, `description`,
/// `external_id`.
///
/// Unknown columns are ignored.  Missing optional columns default to empty.
/// Rows missing `org_name` or `service_name` are skipped with a warning.
pub struct CsvIngestor {
    /// Source identifier embedded in [`Provenance`].
    pub source_id: String,
}

impl CsvIngestor {
    /// Construct a new CSV ingestor with the given `source_id`.
    #[must_use]
    pub fn new(source_id: impl Into<String>) -> Self {
        Self {
            source_id: source_id.into(),
        }
    }
}

impl Ingestor for CsvIngestor {
    #[allow(clippy::too_many_lines)]
    fn ingest(&self, data: &[u8]) -> Result<IngestReport, DirectoryError> {
        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(true)
            .trim(csv::Trim::All)
            .from_reader(data);

        let headers: Vec<String> = {
            let h = rdr.headers()?;
            h.iter().map(str::to_lowercase).collect()
        };

        let col = |name: &str| -> Option<usize> { headers.iter().position(|h| h == name) };

        let org_col = col("org_name");
        let svc_col = col("service_name");
        let tax_col = col("taxonomy");
        let lat_col = col("latitude");
        let lon_col = col("longitude");
        let addr_col = col("address");
        let phone_col = col("phone");
        let url_col = col("url");
        let hours_col = col("hours");
        let elig_col = col("eligibility");
        let lang_col = col("languages");
        let desc_col = col("description");
        let ext_col = col("external_id");

        let mut resources = Vec::new();
        let mut skipped: usize = 0;

        for result in rdr.records() {
            let record = match result {
                Ok(r) => r,
                Err(e) => {
                    warn!("skipping malformed csv row: {e}");
                    skipped = skipped.saturating_add(1);
                    continue;
                }
            };

            let get = |idx: Option<usize>| -> &str {
                idx.and_then(|i| record.get(i)).unwrap_or("").trim()
            };

            let org_name = get(org_col);
            let service_name = get(svc_col);

            if org_name.is_empty() || service_name.is_empty() {
                warn!("skipping csv row: missing org_name or service_name");
                skipped = skipped.saturating_add(1);
                continue;
            }

            let lat: Option<f64> = get(lat_col).parse().ok();
            let lon: Option<f64> = get(lon_col).parse().ok();
            let location = lat.zip(lon).and_then(|(la, lo)| GeoPoint::new(la, lo));

            let service_types: Vec<ServiceType> = get(tax_col)
                .split([',', ';'])
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ServiceType::from_taxonomy)
                .collect();

            let languages: Vec<String> = get(lang_col)
                .split([',', ';'])
                .map(|s| s.trim().to_lowercase())
                .filter(|s| !s.is_empty())
                .collect();

            let eligibility = parse_eligibility(get(elig_col));

            let address = {
                let a = get(addr_col);
                if a.is_empty() { None } else { Some(a.to_owned()) }
            };
            let phone = {
                let p = get(phone_col);
                if p.is_empty() { None } else { Some(p.to_owned()) }
            };
            let url = {
                let u = get(url_col);
                if u.is_empty() { None } else { Some(u.to_owned()) }
            };
            let hours = {
                let h = get(hours_col);
                if h.is_empty() { None } else { Some(Hours(h.to_owned())) }
            };
            let description = {
                let d = get(desc_col);
                if d.is_empty() { None } else { Some(d.to_owned()) }
            };
            let external_id = {
                let e = get(ext_col);
                if e.is_empty() {
                    format!("csv-{}", resources.len())
                } else {
                    e.to_owned()
                }
            };

            let id = Resource::dedup_key(org_name, service_name, location);

            resources.push(Resource {
                id,
                org_name: org_name.to_owned(),
                service_name: service_name.to_owned(),
                service_types,
                location,
                address,
                phone,
                url,
                hours,
                eligibility,
                languages,
                description,
                provenance: Provenance {
                    source_id: self.source_id.clone(),
                    external_id,
                    ingested_at: Utc::now(),
                },
            });
        }

        let resources = dedup(resources);
        Ok(IngestReport { resources, skipped })
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn parse_eligibility(s: &str) -> Vec<EligibilityFlag> {
    if s.trim().is_empty() {
        return vec![];
    }
    s.split([',', ';', '|'])
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(|token| {
            let t = token.to_lowercase();
            if t.contains("under18") || t.contains("under 18") || t.contains("minor") {
                EligibilityFlag::Under18Only
            } else if t.contains("18+") || t.contains("adult") {
                EligibilityFlag::Adults18Plus
            } else if t.contains("60+") || t.contains("senior") || t.contains("elder") {
                EligibilityFlag::Seniors60Plus
            } else if t.contains("veteran") {
                EligibilityFlag::VeteransOnly
            } else if t.contains("documented") || t.contains("citizen") {
                EligibilityFlag::DocumentedOnly
            } else if t.contains("resident") || t.contains("local") {
                EligibilityFlag::LocalResidentOnly
            } else {
                EligibilityFlag::Other(token.to_owned())
            }
        })
        .collect()
}
