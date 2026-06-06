//! SQLite-backed persistent store for [`Resource`] records.
//!
//! The store lives at `~/.local/share/relay/directory.db` by default.
//! All mutations use upsert semantics keyed on [`Resource::id`].

use crate::{
    error::DirectoryError,
    schema::{EligibilityFlag, GeoPoint, Hours, Provenance, Resource, ServiceType},
};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use tracing::debug;

/// Opened, migrated SQLite store for relay resources.
pub struct Store {
    conn: Connection,
}

impl Store {
    /// Open (or create) a store at the given path.
    ///
    /// Applies schema migrations automatically.
    ///
    /// # Errors
    /// Returns [`DirectoryError`] on SQLite failure.
    pub fn open(path: &Path) -> Result<Self, DirectoryError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        // WAL mode for concurrent reads + durability.
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    /// Open a store at the default path (`~/.local/share/relay/directory.db`).
    ///
    /// # Errors
    /// Returns [`DirectoryError`] if the path cannot be determined or SQLite fails.
    pub fn open_default() -> Result<Self, DirectoryError> {
        let path = default_db_path();
        Self::open(&path)
    }

    /// Open an in-memory store (useful for tests).
    ///
    /// # Errors
    /// Returns [`DirectoryError`] on SQLite failure.
    pub fn open_in_memory() -> Result<Self, DirectoryError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA journal_mode=MEMORY; PRAGMA foreign_keys=ON;")?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<(), DirectoryError> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS resources (
                id              TEXT PRIMARY KEY,
                org_name        TEXT NOT NULL,
                service_name    TEXT NOT NULL,
                service_types   TEXT NOT NULL DEFAULT '[]',
                lat             REAL,
                lon             REAL,
                address         TEXT,
                phone           TEXT,
                url             TEXT,
                hours           TEXT,
                eligibility     TEXT NOT NULL DEFAULT '[]',
                languages       TEXT NOT NULL DEFAULT '[]',
                description     TEXT,
                source_id       TEXT NOT NULL,
                external_id     TEXT NOT NULL,
                ingested_at     TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_resources_org ON resources(org_name);
            ",
        )?;
        Ok(())
    }

    /// Upsert a single resource.  Existing records are overwritten on conflict.
    ///
    /// # Errors
    /// Returns [`DirectoryError`] on SQLite failure.
    pub fn upsert(&self, r: &Resource) -> Result<(), DirectoryError> {
        let service_types = serde_json::to_string(&r.service_types)?;
        let eligibility = serde_json::to_string(&r.eligibility)?;
        let languages = serde_json::to_string(&r.languages)?;
        let ingested_at = r.provenance.ingested_at.to_rfc3339();

        self.conn.execute(
            "INSERT INTO resources
                (id, org_name, service_name, service_types,
                 lat, lon, address, phone, url, hours,
                 eligibility, languages, description,
                 source_id, external_id, ingested_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)
             ON CONFLICT(id) DO UPDATE SET
                org_name=excluded.org_name, service_name=excluded.service_name,
                service_types=excluded.service_types,
                lat=excluded.lat, lon=excluded.lon, address=excluded.address,
                phone=excluded.phone, url=excluded.url, hours=excluded.hours,
                eligibility=excluded.eligibility, languages=excluded.languages,
                description=excluded.description,
                source_id=excluded.source_id, external_id=excluded.external_id,
                ingested_at=excluded.ingested_at",
            params![
                r.id,
                r.org_name,
                r.service_name,
                service_types,
                r.location.map(|p| p.lat),
                r.location.map(|p| p.lon),
                r.address,
                r.phone,
                r.url,
                r.hours.as_ref().map(|h| &h.0),
                eligibility,
                languages,
                r.description,
                r.provenance.source_id,
                r.provenance.external_id,
                ingested_at,
            ],
        )?;
        debug!(id=%r.id, "upserted resource");
        Ok(())
    }

    /// Upsert a batch of resources.
    ///
    /// # Errors
    /// Returns [`DirectoryError`] on the first SQLite failure.
    pub fn upsert_all(&self, resources: &[Resource]) -> Result<(), DirectoryError> {
        for r in resources {
            self.upsert(r)?;
        }
        Ok(())
    }

    /// Total number of resources in the store.
    ///
    /// # Errors
    /// Returns [`DirectoryError`] on SQLite failure.
    pub fn count(&self) -> Result<u64, DirectoryError> {
        let n: i64 =
            self.conn.query_row("SELECT COUNT(*) FROM resources", [], |row| row.get(0))?;
        Ok(u64::try_from(n).unwrap_or(0))
    }

    /// Fetch all resources (for query layer use).
    ///
    /// # Errors
    /// Returns [`DirectoryError`] on SQLite failure.
    pub fn all(&self) -> Result<Vec<Resource>, DirectoryError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, org_name, service_name, service_types,
                    lat, lon, address, phone, url, hours,
                    eligibility, languages, description,
                    source_id, external_id, ingested_at
             FROM resources",
        )?;
        let rows = stmt.query_map([], row_to_resource)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }
}

fn row_to_resource(row: &rusqlite::Row<'_>) -> rusqlite::Result<Resource> {
    let id: String = row.get(0)?;
    let org_name: String = row.get(1)?;
    let service_name: String = row.get(2)?;
    let service_types_json: String = row.get(3)?;
    let lat: Option<f64> = row.get(4)?;
    let lon: Option<f64> = row.get(5)?;
    let address: Option<String> = row.get(6)?;
    let phone: Option<String> = row.get(7)?;
    let url: Option<String> = row.get(8)?;
    let hours_str: Option<String> = row.get(9)?;
    let eligibility_json: String = row.get(10)?;
    let languages_json: String = row.get(11)?;
    let description: Option<String> = row.get(12)?;
    let source_id: String = row.get(13)?;
    let external_id: String = row.get(14)?;
    let ingested_at_str: String = row.get(15)?;

    let service_types: Vec<ServiceType> =
        serde_json::from_str(&service_types_json).unwrap_or_default();
    let eligibility: Vec<EligibilityFlag> =
        serde_json::from_str(&eligibility_json).unwrap_or_default();
    let languages: Vec<String> = serde_json::from_str(&languages_json).unwrap_or_default();

    let location = lat.zip(lon).and_then(|(la, lo)| GeoPoint::new(la, lo));
    let hours = hours_str.map(Hours);

    let ingested_at = chrono::DateTime::parse_from_rfc3339(&ingested_at_str)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(|_| chrono::Utc::now());

    Ok(Resource {
        id,
        org_name,
        service_name,
        service_types,
        location,
        address,
        phone,
        url,
        hours,
        eligibility,
        languages,
        description,
        provenance: Provenance {
            source_id,
            external_id,
            ingested_at,
        },
    })
}

fn default_db_path() -> PathBuf {
    let base = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"));
    base.join(".local/share/relay/directory.db")
}
