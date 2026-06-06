# Changelog

## v0.3.0 — relay-letters

Added `relay-letters` crate to the workspace: deterministic template engine for
four letter types (referral, benefits-appeal, intake-summary, support-letter),
`Prose` trait with `MockProse` + `InjectingMockProse` impls, entity-diff post-check
safety gate, advice-phrase linter, and `DRAFT` header/footer guardrails. The
`relay letter` subcommand is wired into the relay-bin CLI. Uses a local
`CaseRecord` mirror (seam comment in `case_record.rs`) pending relay-intake landing.
19 tests pass; clippy clean on the new crate. No auto-send; no legal/medical advice.

## v0.2.0 — relay match

Turn a free-text situation ("she's couch-surfing with two kids and her benefits
got cut off") into structured needs and rank the directory resources that fit —
by service match, eligibility, and proximity. Need-extraction uses the **local**
LLM (offline, private; graceful keyword fallback if unreachable); the ranking is
deterministic and is the tested core. New `relay match --situation <text> --near
<lat,lon> --top N --json` subcommand, backed by the new `relay-match` crate.

- `NeedExtractor` trait (Mock/Local impls); matcher depends only on the trait.
- Deterministic scoring (service-type overlap, eligibility, proximity, hours,
  language) with a plain-language "why this matched" explanation built from the
  factors, no LLM in the loop.
- Privacy: no situation text written to disk or sent over the network in the
  default path.

ACs 1–7 deterministic and verified green (9 tests). AC8 (live qwen2.5:3b
extraction quality on 10 example situations) is deferred to manual verification.

## v0.1.0 — relay directory

Initial workspace: HSDS-JSON/CSV ingest, SQLite store, `relay directory` query CLI.
