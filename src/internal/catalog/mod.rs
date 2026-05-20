//! AI Catalog Integration
//!
//! Auto-generates AI Catalog entries from the tool registry.
//! Serves `/.well-known/ai-catalog.json` for domain-based discovery.

pub mod generator;

pub use generator::{AiCatalog, CatalogGenerator};
