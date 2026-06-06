//! Deterministic resource matcher.
//!
//! Given [`Needs`] + a [`Store`], scores each resource by five factors and
//! returns a ranked [`Vec<Match>`] with per-factor scores and a plain-language
//! "why this matched" string (AC4 — built from factors, no LLM).
//!
//! # Scoring model (all factors 0.0–1.0, summed then normalised)
//!
//! | Factor               | Weight | Notes                                      |
//! |----------------------|--------|--------------------------------------------|
//! | Service-type overlap | 0.40   | Fraction of needed types that match        |
//! | Eligibility compat.  | 0.25   | 0 if incompatible, 1 otherwise             |
//! | Proximity            | 0.20   | 1 − sigmoid(dist_km / PROX_HALF_KM)       |
//! | Hours / open-now     | 0.10   | 1.0 if hours field is present, else 0.5    |
//! | Language match       | 0.05   | 1.0 if a requested language is listed      |

use crate::{error::MatchError, extractor::NeedExtractor, needs::Needs};
use relay_directory::{
    schema::{EligibilityFlag, GeoPoint, Resource, ServiceType},
    Store,
};
use tracing::warn;

// ─── Weights ─────────────────────────────────────────────────────────────────

const W_SERVICE: f64 = 0.40;
const W_ELIG: f64 = 0.25;
const W_PROX: f64 = 0.20;
const W_HOURS: f64 = 0.10;
const W_LANG: f64 = 0.05;

/// Distance (km) at which proximity score is 0.5 (logistic half-point).
const PROX_HALF_KM: f64 = 5.0;

// ─── FactorScores ────────────────────────────────────────────────────────────

/// Per-factor scores that contributed to a resource's overall ranking.
#[derive(Debug, Clone)]
pub struct FactorScores {
    /// Service-type overlap fraction (0.0–1.0).
    pub service_overlap: f64,
    /// Eligibility compatibility score (0.0 or 1.0).
    pub eligibility_compat: f64,
    /// Proximity score (0.0–1.0; `None` if no location available).
    pub proximity: Option<f64>,
    /// Hours/open-now score (0.5 if unknown, 1.0 if hours data present).
    pub hours: f64,
    /// Language match score (0.0 or 1.0).
    pub language: f64,
}

impl FactorScores {
    /// Weighted sum of all factors.
    #[must_use]
    pub fn total(&self) -> f64 {
        let prox = self.proximity.unwrap_or(0.5); // neutral if unknown
        self.service_overlap * W_SERVICE
            + self.eligibility_compat * W_ELIG
            + prox * W_PROX
            + self.hours * W_HOURS
            + self.language * W_LANG
    }

    /// Name of the top-contributing factor (for the "why" string).
    #[must_use]
    pub fn top_factor_name(&self) -> &'static str {
        let prox = self.proximity.unwrap_or(0.5);
        let mut best = ("service match", self.service_overlap * W_SERVICE);
        let candidates = [
            ("eligibility", self.eligibility_compat * W_ELIG),
            ("proximity", prox * W_PROX),
            ("hours", self.hours * W_HOURS),
            ("language", self.language * W_LANG),
        ];
        for (name, score) in candidates {
            if score > best.1 {
                best = (name, score);
            }
        }
        best.0
    }
}

// ─── Match ───────────────────────────────────────────────────────────────────

/// A single ranked match result.
#[derive(Debug, Clone)]
pub struct Match {
    /// The matched resource.
    pub resource: Resource,
    /// Per-factor breakdown.
    pub scores: FactorScores,
    /// Overall weighted score (higher = better match).
    pub total_score: f64,
    /// Plain-language explanation built deterministically from the factors.
    pub why: String,
    /// Distance from the query point in km, if a location was provided.
    pub distance_km: Option<f64>,
}

// ─── MatcherConfig ───────────────────────────────────────────────────────────

/// Configuration for the [`Matcher`].
#[derive(Debug, Clone)]
pub struct MatcherConfig {
    /// Maximum number of matches to return (default: 10).
    pub top_n: usize,
    /// If `true`, resources that are eligibility-incompatible are hard-filtered
    /// (score 0, excluded from results).  Default: `true`.
    pub hard_filter_ineligible: bool,
    /// Optional query centre (overrides [`Needs::location_hint`] if provided).
    pub near: Option<GeoPoint>,
}

impl Default for MatcherConfig {
    fn default() -> Self {
        Self {
            top_n: 10,
            hard_filter_ineligible: true,
            near: None,
        }
    }
}

// ─── Matcher ─────────────────────────────────────────────────────────────────

/// Deterministic resource matcher.
///
/// Construct with a [`Store`] reference, then call [`match_needs`] or
/// [`match_situation`] (which extracts first).
pub struct Matcher<'s> {
    store: &'s Store,
    config: MatcherConfig,
}

impl<'s> Matcher<'s> {
    /// Create a new `Matcher` with the given store and default config.
    #[must_use]
    pub fn new(store: &'s Store) -> Self {
        Self {
            store,
            config: MatcherConfig::default(),
        }
    }

    /// Create a new `Matcher` with explicit config.
    #[must_use]
    pub fn with_config(store: &'s Store, config: MatcherConfig) -> Self {
        Self { store, config }
    }

    /// Rank resources against the given `needs`.
    ///
    /// This is the core deterministic path (ACs 2–4).
    ///
    /// # Errors
    /// Returns [`MatchError::Directory`] on store access failure.
    pub fn match_needs(&self, needs: &Needs) -> Result<Vec<Match>, MatchError> {
        let resources = self.store.all()?;
        let centre = self.config.near.or(needs.location_hint);

        let mut matches: Vec<Match> = resources
            .into_iter()
            .filter_map(|r| self.score_resource(r, needs, centre))
            .collect();

        // Sort descending by total score; tie-break by distance (ascending)
        // so that closer resources appear first for equal scores.
        matches.sort_by(|a, b| {
            b.total_score
                .partial_cmp(&a.total_score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    a.distance_km
                        .unwrap_or(f64::MAX)
                        .partial_cmp(&b.distance_km.unwrap_or(f64::MAX))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });

        matches.truncate(self.config.top_n);
        Ok(matches)
    }

    /// Extract needs from `situation_text` using `extractor`, then rank resources.
    ///
    /// If the extractor fails, falls back to [`crate::extractor::KeywordExtractor`]
    /// and emits a warning (AC6 graceful degrade).
    ///
    /// # Errors
    /// Returns [`MatchError::Directory`] on store access failure.
    pub fn match_situation(
        &self,
        situation_text: &str,
        extractor: &dyn NeedExtractor,
    ) -> Result<(Vec<Match>, bool), MatchError> {
        let (needs, used_fallback) = match extractor.extract(situation_text) {
            Ok(n) => (n, false),
            Err(e) => {
                warn!("extractor failed ({e}); falling back to keyword extraction");
                let fallback = crate::extractor::KeywordExtractor::new();
                let n = fallback.extract_from_text(situation_text);
                (n, true)
            }
        };
        let matches = self.match_needs(&needs)?;
        Ok((matches, used_fallback))
    }

    /// Score a single resource against `needs`.
    ///
    /// Returns `None` if the resource is hard-filtered due to eligibility
    /// incompatibility.
    fn score_resource(
        &self,
        resource: Resource,
        needs: &Needs,
        centre: Option<GeoPoint>,
    ) -> Option<Match> {
        // ── Eligibility ────────────────────────────────────────────────────
        let eligibility_compat = eligibility_score(&resource, needs);
        if self.config.hard_filter_ineligible
            && eligibility_compat < f64::EPSILON
        {
            return None;
        }

        // ── Service-type overlap ──────────────────────────────────────────
        let service_overlap = service_overlap_score(&resource, needs);

        // ── Proximity ────────────────────────────────────────────────────
        let (proximity, distance_km) = match (centre, resource.location) {
            (Some(c), Some(loc)) => {
                let dist = loc.distance_km(c);
                let score = proximity_score(dist);
                (Some(score), Some(dist))
            }
            _ => (None, None),
        };

        // ── Hours ────────────────────────────────────────────────────────
        let hours = if resource.hours.is_some() { 1.0_f64 } else { 0.5_f64 };

        // ── Language ────────────────────────────────────────────────────
        let language = language_score(&resource, needs);

        let scores = FactorScores {
            service_overlap,
            eligibility_compat,
            proximity,
            hours,
            language,
        };

        let total_score = scores.total();
        let why = build_why(&scores, &resource, distance_km);

        Some(Match {
            resource,
            scores,
            total_score,
            why,
            distance_km,
        })
    }
}

// ─── Scoring helpers ─────────────────────────────────────────────────────────

/// Fraction of needed service types present in the resource.
///
/// Returns 0.5 (neutral) if no service types were extracted.
fn service_overlap_score(resource: &Resource, needs: &Needs) -> f64 {
    if needs.service_types.is_empty() {
        return 0.5; // no specific need — neutral
    }

    let matching = needs
        .service_types
        .iter()
        .filter(|need_type| {
            resource
                .service_types
                .iter()
                .any(|rt| service_type_matches(rt, need_type))
        })
        .count();

    // Exact match gets full overlap; partial gets proportional credit.
    let fraction = matching as f64 / needs.service_types.len() as f64;

    // Bonus if the resource exactly covers all needed types.
    if resource.service_types.len() == needs.service_types.len() && fraction >= 1.0 {
        1.0
    } else {
        fraction
    }
}

fn service_type_matches(have: &ServiceType, want: &ServiceType) -> bool {
    have == want
}

/// Eligibility compatibility: 0.0 if the resource explicitly excludes the
/// person (based on extracted signals), 1.0 otherwise.
///
/// An empty eligibility list means "open to all" — always 1.0.
fn eligibility_score(resource: &Resource, needs: &Needs) -> f64 {
    if resource.eligibility.is_empty() {
        return 1.0; // open to all
    }
    if needs.eligibility_signals.is_empty() {
        return 1.0; // no constraints extracted — assume compatible
    }

    // Check for explicit incompatibilities.
    for flag in &resource.eligibility {
        if is_incompatible(flag, &needs.eligibility_signals) {
            return 0.0;
        }
    }

    1.0
}

/// Returns `true` if `flag` is mutually exclusive with all signals in `signals`.
///
/// Example: resource is `VeteransOnly` but signals contain no veteran indicator.
fn is_incompatible(flag: &EligibilityFlag, signals: &[EligibilityFlag]) -> bool {
    // A restrictive flag is incompatible if NONE of the signals match it.
    let matches_any = signals.iter().any(|s| s == flag);
    // Only treat exclusion-style flags as hard incompatible.
    match flag {
        EligibilityFlag::VeteransOnly
        | EligibilityFlag::DocumentedOnly
        | EligibilityFlag::Under18Only
        | EligibilityFlag::Adults18Plus
        | EligibilityFlag::Seniors60Plus
        | EligibilityFlag::LocalResidentOnly => !matches_any,
        EligibilityFlag::Other(_) => false, // unknown flags: assume compatible
        _ => false,
    }
}

/// Proximity score: 1.0 at 0 km, 0.5 at `PROX_HALF_KM`, approaching 0 at infinity.
fn proximity_score(dist_km: f64) -> f64 {
    // Logistic decay: score = 1 / (1 + e^(k * (d - half)))
    // where k = ln(9) / half so that score(half) = 0.1 ... use simpler form:
    // score = 1 / (1 + (d / half)^2) — gives 1.0 at d=0, 0.5 at d=half.
    1.0 / (1.0 + (dist_km / PROX_HALF_KM).powi(2))
}

/// Language match: 1.0 if a needed language is listed, 0.5 if unknown, 0.0 only
/// if languages are listed and none match.
fn language_score(resource: &Resource, needs: &Needs) -> f64 {
    if needs.languages.is_empty() || resource.languages.is_empty() {
        return 0.5; // no information — neutral
    }
    let any_match = needs.languages.iter().any(|nl| {
        resource
            .languages
            .iter()
            .any(|rl| rl.to_lowercase() == nl.to_lowercase())
    });
    if any_match {
        1.0
    } else {
        0.0
    }
}

/// Build a plain-language explanation from scored factors (no LLM; AC4).
fn build_why(scores: &FactorScores, resource: &Resource, distance_km: Option<f64>) -> String {
    let top = scores.top_factor_name();
    let mut parts: Vec<String> = Vec::new();

    // Lead with the top factor.
    match top {
        "service match" => {
            if scores.service_overlap >= 1.0 {
                parts.push(format!(
                    "exact service match ({})",
                    resource
                        .service_types
                        .iter()
                        .map(|t| format!("{t:?}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            } else {
                parts.push(format!(
                    "partial service match ({:.0}%)",
                    scores.service_overlap * 100.0
                ));
            }
        }
        "proximity" => {
            if let Some(d) = distance_km {
                parts.push(format!("nearby ({d:.1} km)"));
            } else {
                parts.push("in the area".to_owned());
            }
        }
        "eligibility" => parts.push("eligible".to_owned()),
        "hours" => parts.push("hours information available".to_owned()),
        "language" => parts.push("language match".to_owned()),
        _ => parts.push("general match".to_owned()),
    }

    // Append secondary factors with something to say.
    if top != "service match" && scores.service_overlap > 0.0 {
        parts.push(format!(
            "offers {}",
            resource
                .service_types
                .first()
                .map_or("services", |_| "relevant services")
        ));
    }
    if top != "proximity" {
        if let Some(d) = distance_km {
            parts.push(format!("{d:.1} km away"));
        }
    }
    if top != "language" && scores.language >= 1.0 {
        parts.push("language match".to_owned());
    }

    parts.join("; ")
}
