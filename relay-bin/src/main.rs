//! `relay` — CLI frontend for the relay human-services workspace.
//!
//! # Subcommands
//! - `relay directory import <file>`     — ingest an HSDS-JSON or CSV file
//! - `relay directory query ...`         — query the local directory
//! - `relay directory stats`             — print store statistics
//! - `relay match --situation <text> ...`— match free-text situation to resources
//! - `relay letter --type <type> --case <file>` — draft a letter from a case record
//! - `relay intake --story <file|->`     — structure an intake story into a case record

#![allow(clippy::print_stdout, clippy::print_stderr)]

use clap::{Parser, Subcommand};
use relay_directory::{
    ingest::{CsvIngestor, HsdsJsonIngestor, Ingestor},
    query::QueryBuilder,
    schema::{EligibilityFlag, GeoPoint, ServiceType},
    store::Store,
    DirectoryError,
};
use relay_match::{
    extractor::LocalLlmExtractor,
    matcher::{Matcher, MatcherConfig},
    MatchError,
};
use relay_intake::{
    record::{ConsentLevel, ResourceId},
    structurer::{MockStructurer, Structurer},
    IntakeError,
};
use relay_letters::{draft, CaseRecord, MockProse, TemplateType};
use std::collections::HashSet;
use std::path::PathBuf;
use std::str::FromStr;
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
        Commands::Match(match_args) => cmd_match(match_args),
        Commands::Letter(letter_cmd) => run_letter(letter_cmd),
        Commands::Intake(intake_args) => run_intake(intake_args),
    }
}

fn run_directory(cmd: DirectoryCommand) -> Result<(), CliError> {
    match cmd.subcommand {
        DirectorySubcommand::Import(args) => cmd_import(args),
        DirectorySubcommand::Query(ref args) => cmd_query(args),
        DirectorySubcommand::Stats => cmd_stats(),
    }
}

// ─── letter ───────────────────────────────────────────────────────────────────

fn run_letter(cmd: LetterArgs) -> Result<(), CliError> {
    let template_type = TemplateType::from_str(&cmd.letter_type)
        .map_err(|e| CliError::Letter(relay_letters::LetterError::ProseError { message: e }))?;

    let json_bytes = std::fs::read(cmd.case_file()).map_err(CliError::Io)?;
    let record: CaseRecord =
        serde_json::from_slice(&json_bytes).map_err(|e| CliError::Letter(relay_letters::LetterError::ProseError {
            message: format!("invalid case JSON: {e}"),
        }))?;

    let prose = MockProse;
    let output = draft(&record, template_type, &prose, cmd.smooth)
        .map_err(CliError::Letter)?;

    // Warn about missing slots
    if !output.missing_slots.is_empty() {
        eprintln!(
            "warning: {} required slot(s) missing: {}",
            output.missing_slots.len(),
            output.missing_slots.join(", ")
        );
    }

    // Warn about advice phrases
    let flags = relay_letters::guardrails::lint_advice_phrases(&output.text);
    if !flags.is_empty() {
        eprintln!("warning: advice phrases flagged for review: {}", flags.join("; "));
    }

    let text = &output.text;
    match cmd.out {
        Some(path) => {
            std::fs::write(&path, text.as_bytes()).map_err(CliError::Io)?;
            eprintln!("letter written to {}", path.display());
        }
        None => println!("{text}"),
    }

    Ok(())
}

// ─── intake ───────────────────────────────────────────────────────────────────

fn run_intake(args: IntakeArgs) -> Result<(), CliError> {
    use std::io::Read as _;

    // Read story from file or stdin.
    let story = if args.story == PathBuf::from("-") {
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        buf
    } else {
        std::fs::read_to_string(&args.story)?
    };

    // MockStructurer is the default (no live LLM dep required).
    let structurer = MockStructurer::new();
    let known: HashSet<ResourceId> = HashSet::new();

    let (mut record, actions) = structurer.structure(&story, &known)?;

    // Apply minimization if requested.
    if args.minimized {
        record = record.minimized(ConsentLevel::FullExport);
    }

    // Build output value.
    let output = serde_json::json!({
        "record": record,
        "actions": actions,
    });

    let json_str = serde_json::to_string_pretty(&output)?;

    if let Some(out_path) = args.out {
        std::fs::write(&out_path, &json_str)?;
    } else if args.json {
        println!("{json_str}");
    } else {
        // Human-readable summary.
        println!("=== Intake Record ===");
        println!("Summary: {}", record.summary);
        if !record.risk_flags.is_empty() {
            println!(
                "Risk flags: {}",
                record
                    .risk_flags
                    .iter()
                    .map(|f| format!("{f:?}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        if !record.barriers.is_empty() {
            println!(
                "Barriers: {}",
                record
                    .barriers
                    .iter()
                    .map(|b| format!("{b:?}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        if !record.redactions.is_empty() {
            println!("PII detected: {} item(s) flagged", record.redactions.len());
        }
        if record.partial {
            println!(
                "Notice: {}",
                record.partial_notice.as_deref().unwrap_or("partial record")
            );
        }
        println!("Next actions ({}):", actions.len());
        for (i, a) in actions.iter().enumerate() {
            println!("  {}. [{}] {}", i + 1, a.owner, a.what);
        }
    }

    Ok(())
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

// ─── match ────────────────────────────────────────────────────────────────────

fn cmd_match(args: MatchArgs) -> Result<(), CliError> {
    // Read situation from --situation, a file, or stdin.
    let situation = if let Some(text) = args.situation {
        text
    } else if let Some(path) = args.situation_file {
        std::fs::read_to_string(&path).map_err(MatchError::Io)?
    } else {
        use std::io::Read as _;
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf).map_err(MatchError::Io)?;
        buf
    };

    let near = args
        .near
        .as_deref()
        .map(parse_latlng)
        .transpose()?
        .and_then(|(lat, lon)| GeoPoint::new(lat, lon));

    let config = MatcherConfig {
        top_n: args.top.unwrap_or(10),
        near,
        ..Default::default()
    };

    let store = open_store(args.db.as_deref())?;
    let resource_matcher = Matcher::with_config(&store, config);

    // Try local LLM first; it will fail-fast (stub) and fall back to keyword.
    let llm_extractor = LocalLlmExtractor::default();
    let (matches, used_fallback) = resource_matcher.match_situation(&situation, &llm_extractor)?;

    if used_fallback {
        eprintln!(
            "notice: local model unavailable — using keyword extraction \
             (install ollama + qwen2.5:3b for full extraction)"
        );
    }

    if matches.is_empty() {
        println!("no matches found");
        return Ok(());
    }

    if args.json {
        let values: Vec<serde_json::Value> = matches
            .iter()
            .map(|m| {
                let mut v =
                    serde_json::to_value(&m.resource).unwrap_or(serde_json::Value::Null);
                if let Some(obj) = v.as_object_mut() {
                    obj.insert("score".to_owned(), serde_json::json!(m.total_score));
                    obj.insert("why".to_owned(), serde_json::json!(m.why));
                    if let Some(d) = m.distance_km {
                        obj.insert(
                            "distance_km".to_owned(),
                            serde_json::Value::Number(
                                serde_json::Number::from_f64(d)
                                    .unwrap_or_else(|| serde_json::Number::from(0)),
                            ),
                        );
                    }
                }
                v
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&values).unwrap_or_default());
    } else {
        for (i, m) in matches.iter().enumerate() {
            let dist = m
                .distance_km
                .map_or_else(String::new, |d| format!(" ({d:.1} km)"));
            println!(
                "{}. {}{} — {} [{:.2}]\n   {}",
                i + 1,
                m.resource.service_name,
                dist,
                m.resource.org_name,
                m.total_score,
                m.why,
            );
        }
    }

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
    /// Match a free-text situation description to directory resources.
    Match(MatchArgs),
    /// Draft a letter from a case record.
    Letter(LetterArgs),
    /// Structure an intake story into a case record and next-action list.
    Intake(IntakeArgs),
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
struct MatchArgs {
    /// Free-text situation description (reads from stdin if omitted and no --file).
    #[arg(long)]
    situation: Option<String>,

    /// Path to a file containing the situation description.
    #[arg(long = "file")]
    situation_file: Option<PathBuf>,

    /// Centre of proximity search as "lat,lon".
    #[arg(long)]
    near: Option<String>,

    /// Maximum number of results to return (default: 10).
    #[arg(long)]
    top: Option<usize>,

    /// Output results as JSON.
    #[arg(long)]
    json: bool,

    /// Path to the store database (defaults to ~/.local/share/relay/directory.db).
    #[arg(long)]
    db: Option<PathBuf>,
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

// ─── letter args ──────────────────────────────────────────────────────────────

#[derive(Parser)]
struct LetterArgs {
    /// Template type: referral, benefits-appeal, intake-summary, support-letter.
    #[arg(long = "type", value_name = "TYPE")]
    letter_type: String,
    /// Path to the case record JSON file.
    #[arg(long)]
    case: PathBuf,
    /// Apply prose smoothing (uses `MockProse` in this build; `LocalLlm` in future).
    #[arg(long)]
    smooth: bool,
    /// Write the letter to this file instead of stdout.
    #[arg(long)]
    out: Option<PathBuf>,
}

impl LetterArgs {
    const fn case_file(&self) -> &PathBuf {
        &self.case
    }
}

// ─── intake args ──────────────────────────────────────────────────────────────

#[derive(Parser)]
struct IntakeArgs {
    /// Path to the intake story file, or `-` to read from stdin.
    #[arg(long)]
    story: PathBuf,
    /// Output as JSON.
    #[arg(long)]
    json: bool,
    /// Apply PII redaction pass and suppress fields above consent level.
    #[arg(long)]
    minimized: bool,
    /// Write output to this path instead of stdout.
    #[arg(long)]
    out: Option<PathBuf>,
}

// ─── Error type ───────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
enum CliError {
    #[error(transparent)]
    Directory(#[from] DirectoryError),
    #[error(transparent)]
    Match(#[from] MatchError),
    #[error(transparent)]
    Intake(#[from] IntakeError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("invalid coordinate: {0}")]
    InvalidCoord(String),
    #[error("letter error: {0}")]
    Letter(relay_letters::LetterError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
