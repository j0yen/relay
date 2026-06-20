# relay

An offline human-services toolkit for frontline helpers: it ingests resource directories, matches a person's situation to services they're eligible for, structures an intake story into a case record, and drafts referral letters — all on-device, with no network required.

## Why it exists

A helper can't refer someone to a service they can't find, and they don't think in taxonomy codes — they hear "she's couch-surfing with two kids and her benefits got cut off." The gap between that sentence and a ranked list of services the person actually qualifies for is where referrals fail.

`relay` closes that gap on one machine. It does the whole arc a helper works through — directory, match, intake, letter — without sending anything about a vulnerable person over a network. That constraint isn't incidental; it's the point. The deterministic core (ingest, dedup, eligibility, proximity, scoring) is what's tested and relied on. A local LLM, when present, only sharpens the free-text steps; when it's absent, `relay` degrades to keyword and rule-based paths rather than failing.

## Install

```sh
git clone https://github.com/j0yen/relay
cd relay
cargo build --release          # Rust 1.85+
cp target/release/relay ~/.local/bin/
relay --help
```

The local store defaults to `~/.local/share/relay/directory.db`; override with `--db <path>` on any subcommand.

## The four stages

```sh
# 1. DIRECTORY — build a local, queryable resource store
relay directory import path/to/hsds_export.json        # HSDS 2.x JSON
relay directory import --csv path/to/211_resources.csv # 211-style CSV
relay directory query --need food --near 37.7749,-122.4194 --radius-km 10
relay directory query --need shelter --eligibility adults18plus --json
relay directory stats

# 2. MATCH — free-text situation → ranked, eligible resources
relay match --situation "single mom, two kids, lost SNAP, needs food now" \
            --near 37.77,-122.41 --top 5
echo "..." | relay match            # also reads stdin, or --file <path>

# 3. INTAKE — spoken-style story → structured, consent-aware case record
relay intake --story story.txt --minimized --json
cat story.txt | relay intake --story -

# 4. LETTER — case record → drafted letter
relay letter --type referral --case case.json
relay letter --type benefits-appeal --case case.json --out letter.txt
```

## How it works

Each stage is its own crate in the workspace; `relay-bin` is the thin CLI that wires them together.

| Crate | Stage | What it does |
| --- | --- | --- |
| `relay-directory` | directory | Normalized `Resource` schema, HSDS-JSON + CSV ingest, dedup, SQLite store, proximity + eligibility query |
| `relay-match` | match | Free-text → structured needs, then deterministic ranking by service overlap, eligibility, proximity, hours, language, with a plain-language "why this matched" |
| `relay-intake` | intake | Story → `CaseRecord` + next-action checklist; rule-based PII redaction (emails, phones, SSNs); consent defaults to most restrictive and export refuses fields above it |
| `relay-letters` | letter | Deterministic template engine for four letter types (referral, benefits-appeal, intake-summary, support-letter), entity-diff safety check, advice-phrase linter, `DRAFT` guardrails |

Two principles run through all of them. **Offline and private:** situation and case text are not written to disk or sent over the network in the default path. **Deterministic core, optional model:** the LLM does need-extraction and prose smoothing only; the scoring, eligibility, and redaction logic is rule-based and is what the tests cover.

## Status

The directory, match, intake, and letter stages are built and wired into the `relay` CLI. The deterministic acceptance criteria for each crate are verified green in CI.

Two honest caveats. The model-backed paths ship behind mock/stub defaults: `relay match` falls back to keyword extraction and prints a notice when no local model is reachable (install ollama + qwen2.5:3b for full extraction), and `relay intake` / `relay letter` use deterministic mock structuring and prose in this build. And the live-model quality criterion (AC8 — extraction/structuring quality against real qwen2.5:3b) is deferred to manual hand-verification across the crates. See [CHANGELOG.md](CHANGELOG.md) for the per-crate history.

## License

Licensed under either of Apache-2.0 or MIT, at your option. — Joe Yen
