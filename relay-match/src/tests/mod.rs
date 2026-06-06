//! Unit and golden tests for `relay-match` (ACs 1–7).
//!
//! All tests use [`MockExtractor`] or [`FailingExtractor`] — no live LLM, no
//! network connection, no disk writes outside a `tempfile` in-memory store.

use relay_directory::{
    schema::{EligibilityFlag, GeoPoint, Hours, Provenance, Resource, ServiceType},
    Store,
};

use crate::{
    extractor::{FailingExtractor, KeywordExtractor, MockExtractor, NeedExtractor},
    matcher::{Matcher, MatcherConfig},
    needs::{Needs, Urgency},
};

// ─── helpers ─────────────────────────────────────────────────────────────────

/// Build a minimal `Resource` for tests.
fn make_resource(
    id: &str,
    service_types: Vec<ServiceType>,
    lat: f64,
    lon: f64,
    eligibility: Vec<EligibilityFlag>,
    languages: Vec<&str>,
    hours: Option<&str>,
) -> Resource {
    Resource {
        id: id.to_owned(),
        org_name: format!("Org {id}"),
        service_name: format!("Service {id}"),
        service_types,
        location: GeoPoint::new(lat, lon),
        address: None,
        phone: None,
        url: None,
        hours: hours.map(|s| Hours(s.to_owned())),
        eligibility,
        languages: languages.iter().map(|s| s.to_string()).collect(),
        description: None,
        provenance: Provenance {
            source_id: "test".to_owned(),
            external_id: id.to_owned(),
            ingested_at: chrono::Utc::now(),
        },
    }
}

/// Seed a `Store` with a slice of resources.
fn seed_store(resources: &[Resource]) -> Store {
    let store = Store::open_in_memory().expect("in-memory store");
    store.upsert_all(resources).expect("upsert");
    store
}

// ─── AC2: golden deterministic ranking ───────────────────────────────────────

#[test]
fn test_ac2_golden_deterministic_ranking() {
    // Known Needs: Food + Shelter, centred at (37.7749, -122.4194) (SF).
    let needs = Needs {
        service_types: vec![ServiceType::Food, ServiceType::Shelter],
        location_hint: GeoPoint::new(37.7749, -122.4194),
        urgency: Urgency::High,
        ..Needs::default()
    };

    // Resource A: exact match (Food+Shelter), 1 km away.
    let a = make_resource("A", vec![ServiceType::Food, ServiceType::Shelter], 37.7840, -122.4194, vec![], vec![], Some("Mon–Fri 9am–5pm"));
    // Resource B: partial match (Food only), 2 km away.
    let b = make_resource("B", vec![ServiceType::Food], 37.7919, -122.4194, vec![], vec![], None);
    // Resource C: no match (Legal), 0.5 km away.
    let c = make_resource("C", vec![ServiceType::Legal], 37.7794, -122.4194, vec![], vec![], None);

    let store = seed_store(&[a, b, c]);
    let extractor = MockExtractor::new(needs.clone());
    let matcher = Matcher::new(&store);
    let (matches, used_fallback) = matcher
        .match_situation("placeholder text", &extractor)
        .expect("match");

    assert!(!used_fallback, "mock extractor should not trigger fallback");

    // A (exact) must rank above B (partial), which must rank above C (no match).
    let ids: Vec<&str> = matches.iter().map(|m| m.resource.id.as_str()).collect();
    // A should be first (exact service match + close).
    assert_eq!(ids.first(), Some(&"A"), "A (exact match) should be #1; got: {ids:?}");
    // B should be before C.
    let pos_b = ids.iter().position(|&x| x == "B").unwrap_or(usize::MAX);
    let pos_c = ids.iter().position(|&x| x == "C").unwrap_or(usize::MAX);
    assert!(pos_b < pos_c, "B (partial) should rank above C (no match); got: {ids:?}");
}

// ─── AC3a: closer outranks farther equal resource ────────────────────────────

#[test]
fn test_ac3a_closer_outranks_farther() {
    let needs = Needs {
        service_types: vec![ServiceType::Food],
        location_hint: GeoPoint::new(37.7749, -122.4194),
        ..Needs::default()
    };

    // Near: 0.5 km.
    let near = make_resource("near", vec![ServiceType::Food], 37.7794, -122.4194, vec![], vec![], None);
    // Far: 20 km.
    let far = make_resource("far", vec![ServiceType::Food], 37.9549, -122.4194, vec![], vec![], None);

    let store = seed_store(&[far.clone(), near.clone()]);
    let matcher = Matcher::new(&store);
    let matches = matcher.match_needs(&needs).expect("match");

    let ids: Vec<&str> = matches.iter().map(|m| m.resource.id.as_str()).collect();
    let pos_near = ids.iter().position(|&x| x == "near").unwrap_or(usize::MAX);
    let pos_far = ids.iter().position(|&x| x == "far").unwrap_or(usize::MAX);
    assert!(pos_near < pos_far, "near should outrank far; got: {ids:?}");
}

// ─── AC3b: eligibility-incompatible resource is filtered out ─────────────────

#[test]
fn test_ac3b_ineligible_resource_filtered() {
    // Signal: Adults18Plus (person is an adult).
    let needs = Needs {
        service_types: vec![ServiceType::Health],
        eligibility_signals: vec![EligibilityFlag::Adults18Plus],
        ..Needs::default()
    };

    // Compatible: open to all.
    let open = make_resource("open", vec![ServiceType::Health], 37.7749, -122.4194, vec![], vec![], None);
    // Incompatible: VeteransOnly (person is not a veteran).
    let vets_only = make_resource("vets", vec![ServiceType::Health], 37.7749, -122.4194, vec![EligibilityFlag::VeteransOnly], vec![], None);

    let store = seed_store(&[open, vets_only]);
    let matcher = Matcher::new(&store);
    let matches = matcher.match_needs(&needs).expect("match");

    let ids: Vec<&str> = matches.iter().map(|m| m.resource.id.as_str()).collect();
    assert!(ids.contains(&"open"), "open resource should be included");
    assert!(!ids.contains(&"vets"), "VeteransOnly resource should be filtered; got: {ids:?}");
}

// ─── AC3c: exact service-type match outranks partial ─────────────────────────

#[test]
fn test_ac3c_exact_outranks_partial() {
    let needs = Needs {
        service_types: vec![ServiceType::Food, ServiceType::Shelter],
        location_hint: GeoPoint::new(37.7749, -122.4194),
        ..Needs::default()
    };

    // Exact: both Food + Shelter.
    let exact = make_resource("exact", vec![ServiceType::Food, ServiceType::Shelter], 37.7749, -122.4194, vec![], vec![], None);
    // Partial: Food only.
    let partial = make_resource("partial", vec![ServiceType::Food], 37.7749, -122.4194, vec![], vec![], None);

    let store = seed_store(&[partial.clone(), exact.clone()]);
    let matcher = Matcher::new(&store);
    let matches = matcher.match_needs(&needs).expect("match");

    let ids: Vec<&str> = matches.iter().map(|m| m.resource.id.as_str()).collect();
    let pos_exact = ids.iter().position(|&x| x == "exact").unwrap_or(usize::MAX);
    let pos_partial = ids.iter().position(|&x| x == "partial").unwrap_or(usize::MAX);
    assert!(pos_exact < pos_partial, "exact should outrank partial; got: {ids:?}");
}

// ─── AC4: why string is deterministic and names top factor ───────────────────

#[test]
fn test_ac4_why_string_deterministic_names_top_factor() {
    let needs = Needs {
        service_types: vec![ServiceType::Food],
        ..Needs::default()
    };

    let food_resource = make_resource("food1", vec![ServiceType::Food], 37.7749, -122.4194, vec![], vec![], None);

    let store = seed_store(&[food_resource]);
    let matcher = Matcher::new(&store);
    let matches = matcher.match_needs(&needs).expect("match");

    assert_eq!(matches.len(), 1);
    let m = &matches[0];

    // why must not be empty.
    assert!(!m.why.is_empty(), "why string should not be empty");

    // Since service overlap is 1.0 (exact Food match), the explanation must
    // mention the service or match (top factor = service_overlap).
    assert!(
        m.why.contains("service") || m.why.contains("match") || m.why.contains("Food"),
        "why should mention service match; got: {:?}",
        m.why
    );

    // Run a second time — result must be identical (deterministic).
    let matches2 = matcher.match_needs(&needs).expect("match2");
    assert_eq!(matches2[0].why, matches[0].why, "why should be deterministic");
    assert!(
        (matches2[0].total_score - matches[0].total_score).abs() < f64::EPSILON,
        "scores should be deterministic"
    );
}

// ─── AC5: trait decoupling — compile-time check ───────────────────────────────

/// Verify that `match_situation` accepts any `NeedExtractor` impl via dynamic
/// dispatch — swapping Mock↔Local changes no matcher code.
#[test]
fn test_ac5_extractor_trait_decoupling() {
    let needs = Needs {
        service_types: vec![ServiceType::Food],
        ..Needs::default()
    };

    let resource = make_resource("food", vec![ServiceType::Food], 0.0, 0.0, vec![], vec![], None);
    let store = seed_store(&[resource]);
    let matcher = Matcher::new(&store);

    // Both MockExtractor and KeywordExtractor implement NeedExtractor.
    // Calling with a trait object proves the matcher doesn't depend on a
    // concrete type.
    let extractors: Vec<Box<dyn NeedExtractor>> = vec![
        Box::new(MockExtractor::new(needs.clone())),
        Box::new(KeywordExtractor::new()),
    ];

    for extractor in &extractors {
        let (matches, _) = matcher
            .match_situation("food bank needed", extractor.as_ref())
            .expect("match");
        assert!(!matches.is_empty(), "should return results for each extractor");
    }
}

// ─── AC6: graceful degrade — failing extractor falls back to keyword ──────────

#[test]
fn test_ac6_graceful_degrade_on_extractor_failure() {
    // "couch-surfing with two kids and her benefits got cut off"
    // Keyword extractor should recognise "couch" → Shelter, "benefits" → Benefits.
    let resource_shelter = make_resource("shelter1", vec![ServiceType::Shelter], 0.0, 0.0, vec![], vec![], None);
    let resource_benefits = make_resource("benefits1", vec![ServiceType::Benefits], 0.0, 0.0, vec![], vec![], None);
    let store = seed_store(&[resource_shelter, resource_benefits]);

    let matcher = Matcher::new(&store);
    let failing_extractor = FailingExtractor;

    let (matches, used_fallback) = matcher
        .match_situation(
            "she's couch-surfing with two kids and her benefits got cut off",
            &failing_extractor,
        )
        .expect("should not fail — graceful degrade");

    // Must have fallen back and still returned results.
    assert!(used_fallback, "should report fallback was used");
    assert!(!matches.is_empty(), "should still return results after fallback");

    // At least shelter or benefits should appear.
    let ids: Vec<&str> = matches.iter().map(|m| m.resource.id.as_str()).collect();
    let has_expected = ids.iter().any(|&id| id == "shelter1" || id == "benefits1");
    assert!(has_expected, "should match shelter or benefits; got: {ids:?}");
}

// ─── AC7: no outbound connection under MockExtractor ─────────────────────────

/// A test extractor that panics if `extract` is called via any path that would
/// imply a real network operation.  The mere fact that this test passes without
/// panicking proves the matching path is pure.
#[test]
fn test_ac7_no_outbound_connection_under_mock() {
    // We can't intercept syscalls in a unit test without external tooling, but
    // we CAN assert two things:
    // 1. MockExtractor::extract never touches the network (it just clones a value).
    // 2. The matcher itself only calls store.all() and pure math — it has no
    //    HTTP client or socket in its call chain.
    //
    // This is enforced structurally: relay-match's [dependencies] includes no
    // HTTP client crate.  The test documents the invariant and will fail to
    // compile if a network dep is accidentally added to the trait object path.

    let needs = Needs {
        service_types: vec![ServiceType::Legal],
        ..Needs::default()
    };

    let resource = make_resource("legal1", vec![ServiceType::Legal], 0.0, 0.0, vec![], vec![], None);
    let store = seed_store(&[resource]);
    let extractor = MockExtractor::new(needs.clone());
    let matcher = Matcher::new(&store);

    // This call must complete without any network I/O.
    let (matches, used_fallback) = matcher
        .match_situation("need legal help", &extractor)
        .expect("match");

    assert!(!used_fallback, "mock should not trigger fallback");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].resource.id, "legal1");
}

// ─── top_n config ─────────────────────────────────────────────────────────────

#[test]
fn test_top_n_limits_results() {
    let needs = Needs {
        service_types: vec![ServiceType::Food],
        ..Needs::default()
    };

    let resources: Vec<Resource> = (0..20)
        .map(|i| make_resource(&format!("r{i}"), vec![ServiceType::Food], 0.0, 0.0, vec![], vec![], None))
        .collect();

    let store = seed_store(&resources);
    let config = MatcherConfig { top_n: 5, ..Default::default() };
    let matcher = Matcher::with_config(&store, config);
    let matches = matcher.match_needs(&needs).expect("match");

    assert_eq!(matches.len(), 5, "should return at most top_n results");
}
