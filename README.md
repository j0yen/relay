# relay

A frontline helper can't refer someone to a service they can't find. `relay` is an
on-device human-services resource directory: it ingests resource listings from open
standard sources (HSDS JSON exports, 211-style CSV sheets), normalises them into a
single queryable local SQLite store, and answers "what services exist for need X near
location Y that this person is eligible for?" — entirely offline, no network required.

## Workspace layout

| Crate | Role |
|---|---|
| `relay-directory` | Normalised `Resource` schema, HSDS-JSON + CSV ingestors, dedup, SQLite store, query layer |
| `relay-bin` | Thin `relay` CLI binary (`relay directory import / query / stats`) |

Sibling PRDs (`relay-intake`, `relay-letters`, `relay-match`) will extend this
workspace with intake forms, letter generation, and ML-assisted need matching.

## Install

```sh
# Build from source (Rust 1.85+)
git clone https://github.com/j0yen/relay
cd relay
cargo build --release
# Binary lands at target/release/relay
cp target/release/relay ~/.local/bin/

# Verify
relay directory --help
```

## Usage

```sh
# Import an HSDS 2.x JSON export
relay directory import path/to/hsds_export.json

# Import a 211-style CSV sheet
relay directory import --csv path/to/211_resources.csv

# Query: food within 10 km of a location
relay directory query --need food --near 37.7749,-122.4194 --radius-km 10

# Query with eligibility filter, JSON output
relay directory query --need shelter --eligibility adults18plus --json

# Statistics
relay directory stats
```

The local store defaults to `~/.local/share/relay/directory.db`.
Override with `--db <path>` on any subcommand.

## License

MIT OR Apache-2.0 — Joe Yen
