//! `relay-directory` — normalized human-services resource directory.
//!
//! Ingests resource listings from open, standard sources (HSDS JSON exports,
//! 211-style CSV sheets), normalises them into a local [`SQLite`] store, and
//! answers proximity + eligibility queries entirely on-device.
//!
//! # Modules
//! - [`schema`] — normalised [`Resource`] model and supporting types.
//! - [`ingest`] — [`Ingestor`] trait, HSDS-JSON ingestor, CSV ingestor, dedup.
//! - [`store`] — SQLite-backed persistent store (upsert + query).
//! - [`query`] — structured query builder and ranked result type.
//! - [`error`] — unified error type.

pub mod error;
pub mod ingest;
pub mod query;
pub mod schema;
pub mod store;

pub use error::DirectoryError;
pub use query::{QueryBuilder, QueryResult};
pub use schema::Resource;
pub use store::Store;
