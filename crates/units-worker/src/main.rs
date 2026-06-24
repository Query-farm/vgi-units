//! The `units` VGI worker.
//!
//! A standalone binary that DuckDB launches and talks to over Apache Arrow IPC
//! (`ATTACH 'units' (TYPE vgi, LOCATION '…')`). It brings runtime, string-driven
//! physical-unit conversion and dimensional analysis to SQL under the catalog
//! `units`, schema `main`:
//!
//! ```sql
//! ATTACH 'units' (TYPE vgi, LOCATION './target/release/units-worker');
//! SET search_path = 'units.main';
//!
//! SELECT convert(5, 'mi', 'km');          -- 8.04672
//! SELECT dimension('mi');                  -- 'length'
//! SELECT compatible('mi', 'kg');           -- false
//! SELECT to_base(1, 'GiB');                -- 1073741824
//! SELECT parse_quantity('5 km').*;         -- (5.0, 'km')
//! SELECT * FROM supported_units();         -- discovery
//! ```
//!
//! The pure conversion engine (a curated runtime unit table) lives in `units.rs`;
//! the `scalar/` and `table/` modules are thin Arrow adapters over it.

mod arrow_io;
mod scalar;
mod table;
mod units;

use vgi::catalog::{CatSchema, CatalogModel};
use vgi::Worker;

/// Worker version string, surfaced by `units_version()`.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Catalog + schema metadata (description, provenance) surfaced to DuckDB and
/// the `vgi-lint` metadata-quality linter. The function objects themselves are
/// served from the registered scalars/table; this only adds catalog/schema-level
/// comments and tags.
fn catalog_metadata(name: &str) -> CatalogModel {
    CatalogModel {
        name: name.to_string(),
        comment: Some(
            "Runtime, string-driven physical-unit conversion and dimensional analysis.".to_string(),
        ),
        tags: vec![
            (
                "vgi.description_llm".to_string(),
                "Convert physical quantities between units of the same dimension (length, mass, \
                 time, energy, data, temperature, …), express a value in its SI base unit, test \
                 whether two units are compatible, parse quantity strings like '5 km', and look \
                 up a unit's dimension. Use for unit conversion and dimensional analysis in SQL."
                    .to_string(),
            ),
            (
                "vgi.description_md".to_string(),
                "# units\n\nRuntime physical-unit conversion and dimensional analysis over Apache \
                 Arrow.\n\nScalars: `convert`, `to_base`, `dimension`, `compatible`, \
                 `parse_quantity`, `units_version`. Table: `supported_units`."
                    .to_string(),
            ),
            ("vgi.author".to_string(), "Query.Farm".to_string()),
            (
                "vgi.copyright".to_string(),
                "Copyright 2026 Query Farm LLC - https://query.farm".to_string(),
            ),
            ("vgi.license".to_string(), "MIT".to_string()),
            (
                "vgi.support_contact".to_string(),
                "https://github.com/Query-farm/vgi-units/issues".to_string(),
            ),
            (
                "vgi.support_policy_url".to_string(),
                "https://github.com/Query-farm/vgi-units/blob/main/README.md".to_string(),
            ),
        ],
        source_url: Some("https://github.com/Query-farm/vgi-units".to_string()),
        schemas: vec![CatSchema {
            name: "main".to_string(),
            comment: Some("Unit-conversion and dimensional-analysis functions.".to_string()),
            tags: vec![
                (
                    "vgi.description_llm".to_string(),
                    "Unit-conversion and dimensional-analysis functions: convert between units, \
                     express values in SI base units, test unit compatibility, parse quantity \
                     strings, and look up a unit's dimension."
                        .to_string(),
                ),
                (
                    "vgi.description_md".to_string(),
                    "Unit-conversion and dimensional-analysis functions over Apache Arrow."
                        .to_string(),
                ),
            ],
            views: Vec::new(),
            macros: Vec::new(),
            tables: Vec::new(),
        }],
        ..Default::default()
    }
}

fn main() {
    // Logs MUST go to stderr — stdout is the Arrow-IPC channel.
    let _ = env_logger::Builder::from_env(env_logger::Env::default().filter_or("VGI_LOG", "info"))
        .format_timestamp_millis()
        .try_init();

    // The catalog name DuckDB sees in `ATTACH 'units' (TYPE vgi, …)`. Default to
    // `units`, but honor an explicit override so a test harness can rename it.
    if std::env::var_os("VGI_WORKER_CATALOG_NAME").is_none() {
        std::env::set_var("VGI_WORKER_CATALOG_NAME", "units");
    }
    let catalog_name =
        std::env::var("VGI_WORKER_CATALOG_NAME").unwrap_or_else(|_| "units".to_string());

    let mut worker = Worker::new();
    scalar::register(&mut worker);
    table::register(&mut worker);
    worker.set_catalog(catalog_metadata(&catalog_name));
    worker.run();
}
