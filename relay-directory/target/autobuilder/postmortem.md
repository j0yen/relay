# Postmortem — relay-directory

**Run date:** 2026-06-12
**Result:** SHIPPED — all 8 ACs green in iter 1

## Summary

The `relay-directory` crate was already implemented as part of the initial `relay`
workspace scaffold. All 8 PRD acceptance criteria were satisfied without requiring any
source edits:

- AC1: `relay directory --help` lists `import`, `query`, `stats`
- AC2–AC4: HSDS-JSON golden ingest, CSV ingestor, dedup — all passing
- AC5: Proximity query with Haversine distance ranking
- AC6: Eligibility filtering
- AC7: SQLite store durability (tempfile-based test)
- AC8: SIGPIPE-safe (`sigpipe::reset()`), clippy -D warnings clean

## Fixes applied this run

1. **Added `deny.toml`** to the relay workspace root (was missing — cargo deny
   used default config which rejected all licenses). Added `BSL-1.0` and `Unlicense`
   to the allow list to cover `ryu` and `aho-corasick` transitive deps. Set
   `wildcards = "allow"` since workspace-internal path deps have no version specifier
   by design.

## What worked

- The homeward pattern (ingest→normalize→store→query) transferred cleanly to
  human-services data. The 6 source files in `relay-directory/src/` implement the
  full AC surface with no architectural gaps.
- All tests are deterministic — no network, no LLM, no hardware. Builds clean on
  both local laptop and Hetzner cloud box (x86_64, rustc 1.85).

## What could improve

- `deny.toml` should be scaffolded by the workspace initializer so future relay
  crates don't start without it.
- No mutation testing run (Phase 1 telemetry only) — cargo-mutants not invoked
  because the test suite was already green at iter 1.

## Evolution proposals

None queued — no new patterns discovered that merit a SKILL.md update.
