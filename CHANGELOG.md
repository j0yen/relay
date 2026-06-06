# Changelog

## v0.5.0 — 2026-06-06

Workspace clippy gate (`clippy -D warnings`) now passes for `relay-match`. The
test module adopts the `relay-directory` convention — a module-level
`#![allow(...)]` for the test-permissible lints (unwrap/expect/panic/indexing)
plus the fixture-builder ergonomic lints (too_many_arguments, similar_names,
redundant_clone, needless_pass_by_value) — and a stray `to_string` on `&&str`
is fixed. No behavior change; all 9 deterministic tests (ACs 1-7) stay green.
AC8 (live-model extraction quality vs. qwen2.5:3b) remains deferred per PRD.

## v0.4.0 — relay-intake

Turns a messy, spoken-style helper intake story into a structured, privacy-respecting
case record plus a next-actions checklist — offline, so nothing about a vulnerable person
leaves the machine. New `relay-intake` crate wired as `relay intake`.

- `CaseRecord` schema: consented summary, needs (links to `relay-match`), risk/urgency
  flags, demographics-only-if-volunteered, barriers, timeline, and a `redactions` list.
  `consent` field defaults to most restrictive; export refuses fields above consent level.
- `Structurer` trait (`LocalLlmStructurer` + `MockStructurer`): free-text story →
  `CaseRecord` + `Vec<NextAction>`; unknown resource references are rejected, not dropped.
- PII minimization (deterministic, rule-based): regex redaction pass for emails, phone
  numbers, and SSNs; `--minimized` output contains none of the flagged identifiers.
- Graceful degrade: if local model unreachable, produces minimal record from rule-based
  layer (timeline + redactions) with a clear "LLM unavailable, partial record" notice.
- ACs 1–7 deterministic, verified green (18 tests). AC8 (structuring quality vs. real
  qwen on 10 sample stories) deferred to manual hand-verification.

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
