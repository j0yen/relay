//! Query layer: structured query builder and ranked result type.

use crate::{
    error::DirectoryError,
    schema::{EligibilityFlag, GeoPoint, Resource, ServiceType},
    store::Store,
};

/// A structured query against the resource directory.
///
/// Call [`QueryBuilder::new`], chain filters, then [`QueryBuilder::run`].
#[derive(Debug, Clone, Default)]
pub struct QueryBuilder {
    need: Option<ServiceType>,
    near: Option<GeoPoint>,
    radius_km: Option<f64>,
    eligibility: Option<EligibilityFlag>,
}

/// A single result from a [`QueryBuilder::run`] call.
#[derive(Debug, Clone)]
pub struct QueryResult {
    /// The matched resource.
    pub resource: Resource,
    /// Distance from the query point, or `None` if no location filter was set.
    pub distance_km: Option<f64>,
}

impl QueryBuilder {
    /// Create a new, empty query builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter by service type.
    #[must_use]
    pub fn need(mut self, service_type: ServiceType) -> Self {
        self.need = Some(service_type);
        self
    }

    /// Centre the proximity filter on `point`.
    #[must_use]
    pub const fn near(mut self, point: GeoPoint) -> Self {
        self.near = Some(point);
        self
    }

    /// Apply a radius filter (only meaningful when [`near`](Self::near) is also set).
    #[must_use]
    pub const fn radius_km(mut self, km: f64) -> Self {
        self.radius_km = Some(km);
        self
    }

    /// Filter by eligibility flag — resources whose eligibility list is empty
    /// (open to all) are always included; resources that list *only* flags that
    /// exclude the given flag are excluded.
    #[must_use]
    pub fn eligibility(mut self, flag: EligibilityFlag) -> Self {
        self.eligibility = Some(flag);
        self
    }

    /// Execute the query against `store` and return results sorted by proximity
    /// (closest first) when a location filter is active.
    ///
    /// # Errors
    /// Returns [`DirectoryError`] on store access failure.
    pub fn run(&self, store: &Store) -> Result<Vec<QueryResult>, DirectoryError> {
        let resources = store.all()?;

        let mut results: Vec<QueryResult> = resources
            .into_iter()
            .filter(|r| self.matches(r))
            .map(|r| {
                let distance_km = self
                    .near
                    .map(|centre| r.location.map_or(f64::MAX, |loc| loc.distance_km(centre)));
                QueryResult { resource: r, distance_km }
            })
            .collect();

        // Sort by distance (ascending); resources without a location sort last.
        if self.near.is_some() {
            results.sort_by(|a, b| {
                a.distance_km
                    .unwrap_or(f64::MAX)
                    .partial_cmp(&b.distance_km.unwrap_or(f64::MAX))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }

        Ok(results)
    }

    fn matches(&self, r: &Resource) -> bool {
        // Service type filter.
        if let Some(need) = &self.need {
            if !r.service_types.iter().any(|st| service_type_matches(st, need)) {
                return false;
            }
        }

        // Radius filter.
        if let (Some(centre), Some(radius)) = (self.near, self.radius_km) {
            match r.location {
                Some(loc) => {
                    if loc.distance_km(centre) > radius {
                        return false;
                    }
                }
                None => return false,
            }
        }

        // Eligibility filter: exclude resources whose eligibility list is
        // non-empty AND none of its flags match the requested flag.
        if let Some(flag) = &self.eligibility {
            if !r.eligibility.is_empty() && !r.eligibility.iter().any(|ef| ef == flag) {
                return false;
            }
        }

        true
    }
}

fn service_type_matches(have: &ServiceType, want: &ServiceType) -> bool {
    // Exact variant match (handles Other(x) == Other(x) too).
    have == want
}
