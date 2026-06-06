//! `relay` — CLI frontend for the relay human-services workspace.
//!
//! # Subcommands
//! - `relay directory import <file>`  — ingest an HSDS-JSON or CSV file
//! - `relay directory query ...`      — query the local directory
//! - `relay directory stats`          — print store statistics

#![allow(clippy::print_stdout, clippy::print_stderr)]

use clap::{Parser, Subcommand};
use relay_directory::{
    ingest::{CsvIngestor, HsdsJsonIngestor, Ingestor},
    query::QueryBuilder,
    schema::{EligibilityFlag, GeoPoint, ServiceType},
    store::Store,
    DirectoryError,
};
use std::path::PathBuf;
use thiserror::Error;

fn main() {
    // SIGPIPE safety: prevent panic on broken pipe (e.g. `relay query | head`).
    sigpipe::reset();

    // "relay=info" is a valid directive string — parse can't fail on this literal.
    #[allow(clippy::expect_used)]
    let directive = "relay=info"
        .parse()
        .expect("valid directive");
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(directive),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), CliError> {
    match cli.command {
        Commands::Directory(dir_cmd) => run_directory(dir_cmd),
    }
}

fn run_directory(cmd: DirectoryCommand) -> Result<(), CliError> {
    match cmd.subcommand {
        DirectorySubcommand::Import(args) => cmd_import(args),
        DirectorySubcommand::Query(ref args) => cmd_query(args),
        DirectorySubcommand::Stats => cmd_stats(),
    }
}

// ─── import ──────────────────────────────────────────────────────────────────

fn cmd_import(args: ImportArgs) -> Result<(), CliError> {
    let data = std::fs::read(&args.file).map_err(DirectoryError::from)?;
    let source_id = args
        .source_id
        .unwrap_or_else(|| args.file.display().to_string());

    let report = if args.csv {
        CsvIngestor::new(&source_id).ingest(&data)?
    } else {
        HsdsJsonIngestor::new(&source_id).ingest(&data)?
    };

    let store = open_store(args.db.as_deref())?;
    store.upsert_all(&report.resources)?;

    println!(
        "imported {} resources ({} skipped)",
        report.resources.len(),
        report.skipped
    );
    Ok(())
}

// ─── query ────────────────────────────────────────────────────────────────────

fn cmd_query(args: &QueryArgs) -> Result<(), CliError> {
    let mut qb = QueryBuilder::new();

    if let Some(need) = &args.need {
        qb = qb.need(ServiceType::from_taxonomy(need));
    }

    if let Some(near_str) = &args.near {
        let (lat, lon) = parse_latlng(near_str)?;
        let pt = GeoPoint::new(lat, lon).ok_or_else(|| CliError::InvalidCoord(near_str.clone()))?;
        qb = qb.near(pt);
        if let Some(km) = args.radius_km {
            qb = qb.radius_km(km);
        }
    }

    if let Some(elig) = &args.eligibility {
        qb = qb.eligibility(parse_eligibility_flag(elig));
    }

    let store = open_store(args.db.as_deref())?;
    let results = qb.run(&store)?;

    if args.json {
        let values: Vec<serde_json::Value> = results
            .iter()
            .map(|r| {
                let mut v = serde_json::to_value(&r.resource).unwrap_or(serde_json::Value::Null);
                if let (Some(obj), Some(dist)) = (v.as_object_mut(), r.distance_km) {
                    obj.insert(
                        "distance_km".to_owned(),
                        serde_json::Value::Number(
                            serde_json::Number::from_f64(dist)
                                .unwrap_or_else(|| serde_json::Number::from(0)),
                        ),
                    );
                }
                v
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&values).unwrap_or_default());
    } else {
        if results.is_empty() {
            println!("no results");
        }
        for r in &results {
            let dist = r
                .distance_km
                .filter(|&d| d < f64::MAX)
                .map_or_else(String::new, |d| format!(" ({d:.1} km)"));
            println!(
                "{}{} — {} [{}]",
                r.resource.service_name,
                dist,
                r.resource.org_name,
                r.resource
                    .service_types
                    .iter()
                    .map(|s| format!("{s:?}"))
                    .collect::<Vec<_>>()
                    .join(",")
            );
        }
    }

    Ok(())
}

// ─── stats ────────────────────────────────────────────────────────────────────

fn cmd_stats() -> Result<(), CliError> {
    let store = open_store(None)?;
    let count = store.count()?;
    println!("resources: {count}");
    Ok(())
}

// ─── helpers ─────────────────────────────────────────────────────────────────

fn open_store(path: Option<&std::path::Path>) -> Result<Store, CliError> {
    let store = match path {
        Some(p) => Store::open(p)?,
        None => Store::open_default()?,
    };
    Ok(store)
}

fn parse_latlng(s: &str) -> Result<(f64, f64), CliError> {
    let mut parts = s.splitn(2, ',');
    let lat: f64 = parts
        .next()
        .ok_or_else(|| CliError::InvalidCoord(s.to_owned()))?
        .trim()
        .parse()
        .map_err(|_| CliError::InvalidCoord(s.to_owned()))?;
    let lon: f64 = parts
        .next()
        .ok_or_else(|| CliError::InvalidCoord(s.to_owned()))?
        .trim()
        .parse()
        .map_err(|_| CliError::InvalidCoord(s.to_owned()))?;
    Ok((lat, lon))
}

fn parse_eligibility_flag(s: &str) -> EligibilityFlag {
    match s.to_lowercase().as_str() {
        "under18" => EligibilityFlag::Under18Only,
        "adults18plus" | "18+" => EligibilityFlag::Adults18Plus,
        "seniors60plus" | "60+" => EligibilityFlag::Seniors60Plus,
        "veterans" | "veteransonly" => EligibilityFlag::VeteransOnly,
        "documented" | "documentedonly" => EligibilityFlag::DocumentedOnly,
        "localresident" | "resident" => EligibilityFlag::LocalResidentOnly,
        other => EligibilityFlag::Other(other.to_owned()),
    }
}

// ─── CLI shape ────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "relay", about = "Human-services resource directory", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage the local resource directory.
    Directory(DirectoryCommand),
}

#[derive(Parser)]
struct DirectoryCommand {
    #[command(subcommand)]
    subcommand: DirectorySubcommand,
}

#[derive(Subcommand)]
enum DirectorySubcommand {
    /// Import resources from an HSDS JSON export or 211-style CSV.
    Import(ImportArgs),
    /// Query the local directory.
    Query(QueryArgs),
    /// Print store statistics.
    Stats,
}

#[derive(Parser)]
struct ImportArgs {
    /// Path to the HSDS JSON or CSV file.
    file: PathBuf,
    /// Treat the file as a CSV (auto-detected if omitted by extension).
    #[arg(long)]
    csv: bool,
    /// Override the source identifier embedded in provenance.
    #[arg(long)]
    source_id: Option<String>,
    /// Path to the store database (defaults to ~/.local/share/relay/directory.db).
    #[arg(long)]
    db: Option<PathBuf>,
}

#[derive(Parser)]
struct QueryArgs {
    /// Filter by service need (food, shelter, legal, health, benefits, ...).
    #[arg(long)]
    need: Option<String>,
    /// Centre of proximity search as "lat,lon".
    #[arg(long)]
    near: Option<String>,
    /// Radius in km for proximity filter (requires --near).
    #[arg(long)]
    radius_km: Option<f64>,
    /// Eligibility flag to filter by (under18, adults18plus, veterans, ...).
    #[arg(long)]
    eligibility: Option<String>,
    /// Output results as JSON.
    #[arg(long)]
    json: bool,
    /// Path to the store database.
    #[arg(long)]
    db: Option<PathBuf>,
}

// ─── Error type ───────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
enum CliError {
    #[error(transparent)]
    Directory(#[from] DirectoryError),
    #[error("invalid coordinate: {0}")]
    InvalidCoord(String),
}
