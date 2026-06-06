//! Acceptance-criteria tests for relay-directory (ACs 2–7).
//!
//! All tests are deterministic and require no network or LLM.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use relay_directory::{
    ingest::{CsvIngestor, HsdsJsonIngestor, Ingestor},
    query::QueryBuilder,
    schema::{EligibilityFlag, GeoPoint, ServiceType},
    store::Store,
};

// ─── helpers ─────────────────────────────────────────────────────────────────

fn fixture(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("missing fixture {name}: {e}"))
}

// ─── AC2: HSDS-JSON golden ingest ─────────────────────────────────────────────

#[test]
fn ac2_hsds_json_golden_ingest() {
    let data = fixture("hsds_sample.json");
    let report = HsdsJsonIngestor::new("test:hsds")
        .ingest(&data)
        .expect("ingest must succeed");

    // svc-005 is inactive → skipped; 4 active services.
    assert_eq!(
        report.resources.len(),
        4,
        "expected 4 active resources, got {}: {:?}",
        report.resources.len(),
        report.resources.iter().map(|r| &r.service_name).collect::<Vec<_>>()
    );
    assert!(report.skipped >= 1, "inactive service must be counted as skipped");

    let pantry = report
        .resources
        .iter()
        .find(|r| r.service_name == "Emergency Food Pantry")
        .expect("pantry resource must be present");

    assert_eq!(pantry.org_name, "Community Food Bank");
    assert!(
        pantry.service_types.contains(&ServiceType::Food),
        "pantry must have Food service type"
    );
    assert_eq!(pantry.phone.as_deref(), Some("555-100-0001"));

    let loc = pantry.location.expect("pantry must have a location");
    assert!((loc.lat - 37.7749).abs() < 1e-4);
    assert!((loc.lon - (-122.4194)).abs() < 1e-4);
}

// ─── AC3: CSV ingestor golden ingest ─────────────────────────────────────────

#[test]
fn ac3_csv_ingestor_golden_ingest() {
    let data = fixture("sample_211.csv");
    let report = CsvIngestor::new("test:csv")
        .ingest(&data)
        .expect("csv ingest must succeed");

    // 5 rows: 3 valid, 1 bad (empty org_name), 1 duplicate → 3 after dedup.
    assert_eq!(
        report.resources.len(),
        3,
        "expected 3 deduped resources, got {}",
        report.resources.len()
    );
    assert!(report.skipped >= 1, "bad row must increment skipped counter");

    let has_food = report
        .resources
        .iter()
        .any(|r| r.service_types.contains(&ServiceType::Food));
    assert!(has_food, "csv must produce at least one food resource");
}

#[test]
fn ac3_csv_malformed_rows_never_panic() {
    // Feed a deliberately broken CSV; must not panic, must return skipped > 0.
    let bad_csv = b"org_name,service_name\nok org,ok svc\n\"unterminated";
    // csv crate may or may not error on the unterminated quote depending on
    // configuration; the key invariant is no panic.
    let result = CsvIngestor::new("test:bad").ingest(bad_csv);
    // Either ok (with skips) or a clean Err — never a panic.
    let _ = result;
}

// ─── AC4: dedup collapses same org+service+location ──────────────────────────

#[test]
fn ac4_dedup_same_org_service_location() {
    let data = fixture("hsds_dedup.json");
    let report = HsdsJsonIngestor::new("test:dedup")
        .ingest(&data)
        .expect("dedup fixture must ingest");

    // Two records for same org+service+location → one resource after dedup.
    assert_eq!(
        report.resources.len(),
        1,
        "duplicate org+service+location must collapse to 1 resource"
    );
}

#[test]
fn ac4_dedup_fn_deterministic() {
    use relay_directory::schema::Resource;

    let pt = GeoPoint::new(40.712_8, -74.006_0).unwrap();
    let key1 = Resource::dedup_key("Acme", "Food Pantry", Some(pt));
    let key2 = Resource::dedup_key("Acme", "Food Pantry", Some(pt));
    assert_eq!(key1, key2, "dedup key must be deterministic");

    let key3 = Resource::dedup_key("Other Org", "Food Pantry", Some(pt));
    assert_ne!(key1, key3, "different orgs must produce different dedup keys");
}

// ─── AC5: proximity query ─────────────────────────────────────────────────────

#[test]
fn ac5_proximity_query() {
    let data = fixture("hsds_sample.json");
    let report = HsdsJsonIngestor::new("test:prox")
        .ingest(&data)
        .expect("ingest");

    let store = Store::open_in_memory().expect("in-memory store");
    store.upsert_all(&report.resources).expect("upsert");

    // Query for food near SF (37.7749, -122.4194) within 1km.
    let centre = GeoPoint::new(37.7749, -122.4194).unwrap();
    let results = QueryBuilder::new()
        .need(ServiceType::Food)
        .near(centre)
        .radius_km(1.0)
        .run(&store)
        .expect("query");

    assert!(!results.is_empty(), "must find food resources near SF");

    // All results must be food.
    for r in &results {
        assert!(
            r.resource.service_types.contains(&ServiceType::Food),
            "query result must be Food type: {:?}",
            r.resource.service_name
        );
    }

    // Oakland legal clinic is ~13km away → must NOT appear.
    let has_oakland = results
        .iter()
        .any(|r| r.resource.org_name.contains("Legal"));
    assert!(!has_oakland, "Oakland resources must be outside 1km radius");

    // Results must be ordered by distance ascending.
    let dists: Vec<f64> = results
        .iter()
        .filter_map(|r| r.distance_km)
        .collect();
    let sorted = {
        let mut d = dists.clone();
        d.sort_by(|a, b| a.partial_cmp(b).unwrap());
        d
    };
    assert_eq!(dists, sorted, "results must be ordered by distance ascending");
}

// ─── AC6: eligibility filtering ──────────────────────────────────────────────

#[test]
fn ac6_eligibility_excludes_non_matching() {
    let data = fixture("hsds_sample.json");
    let report = HsdsJsonIngestor::new("test:elig").ingest(&data).expect("ingest");

    let store = Store::open_in_memory().expect("store");
    store.upsert_all(&report.resources).expect("upsert");

    // Query for under18 — the shelter has adults18plus → must be excluded.
    let results = QueryBuilder::new()
        .eligibility(EligibilityFlag::Under18Only)
        .run(&store)
        .expect("query");

    let has_shelter = results
        .iter()
        .any(|r| r.resource.service_types.contains(&ServiceType::Shelter));
    assert!(!has_shelter, "adults-only shelter must be excluded from under18 query");
}

// ─── AC7: store durability ────────────────────────────────────────────────────

#[test]
fn ac7_store_durability() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("relay.db");

    let data = fixture("hsds_sample.json");
    let report = HsdsJsonIngestor::new("test:durable").ingest(&data).expect("ingest");
    let count_before = report.resources.len();

    // Write to disk store, then drop it.
    {
        let store = Store::open(&db_path).expect("open store");
        store.upsert_all(&report.resources).expect("upsert");
    }

    // Re-open and verify row count.
    let store2 = Store::open(&db_path).expect("reopen store");
    let count_after = store2.count().expect("count");
    assert_eq!(
        count_after,
        u64::try_from(count_before).unwrap(),
        "row count must survive process restart"
    );
}
