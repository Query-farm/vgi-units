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
mod meta;
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
                "vgi.title".to_string(),
                "Unit Conversion & Dimensional Analysis".to_string(),
            ),
            (
                "vgi.keywords".to_string(),
                crate::meta::keywords_json(
                    "units, unit conversion, convert, dimensional analysis, measurement, length, \
                     mass, time, energy, temperature, data, SI, metric, imperial",
                ),
            ),
            (
                "vgi.doc_llm".to_string(),
                "Convert physical quantities between units of the same dimension (length, mass, \
                 time, energy, data, temperature, …), express a value in its SI base unit, test \
                 whether two units are compatible, parse quantity strings like '5 km', and look \
                 up a unit's dimension. Use for unit conversion and dimensional analysis in SQL."
                    .to_string(),
            ),
            (
                "vgi.doc_md".to_string(),
                "# units\n\nRuntime, string-driven physical-unit conversion and dimensional \
                 analysis over Apache Arrow. Unlike compile-time-typed unit libraries, units are \
                 named by ordinary strings (`'mi'`, `'GiB'`, `'°C'`) resolved at query time, so \
                 you can convert and analyse quantities directly in SQL.\n\nA curated table maps \
                 each unit to a `(dimension, factor, offset)` triple; conversion is an affine \
                 round-trip through the SI base unit of the dimension, covering length, mass, \
                 time, energy, temperature, data sizes, and more.\n\n**Scalars:** `convert` \
                 (between two units), `to_base` (to the SI base unit), `dimension` (a unit's \
                 dimension), `compatible` (do two units share a dimension?), `parse_quantity` \
                 (split `'5 km'` into value + unit), and `units_version`.\n\n**Table:** \
                 `supported_units` lists every recognized unit with its dimension and base unit."
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
                ("vgi.title".to_string(), "Units — main".to_string()),
                (
                    "vgi.keywords".to_string(),
                    crate::meta::keywords_json(
                        "units, unit conversion, convert, to_base, dimension, compatible, \
                         parse_quantity, supported_units, dimensional analysis, measurement",
                    ),
                ),
                // VGI123 classifying tags (bare keys: domain/category/topic) for faceting.
                ("domain".to_string(), "units-and-measurement".to_string()),
                ("category".to_string(), "conversion".to_string()),
                ("topic".to_string(), "dimensional-analysis".to_string()),
                // NOTE: no per-schema `vgi.source_url` (VGI139) — `source_url`
                // lives on the catalog object below.
                (
                    "vgi.doc_llm".to_string(),
                    "Unit-conversion and dimensional-analysis functions: convert between units, \
                     express values in SI base units, test unit compatibility, parse quantity \
                     strings, and look up a unit's dimension."
                        .to_string(),
                ),
                (
                    "vgi.doc_md".to_string(),
                    "The single schema for the `units` worker. It holds the unit-conversion and \
                     dimensional-analysis functions — `convert`, `to_base`, `dimension`, \
                     `compatible`, `parse_quantity`, `units_version` — plus the `supported_units` \
                     discovery table listing every recognized unit, its dimension, and base unit."
                        .to_string(),
                ),
                // VGI506 representative example queries for the schema.
                (
                    "vgi.example_queries".to_string(),
                    "SELECT units.main.convert(26.2, 'mi', 'km');\n\
                     SELECT units.main.to_base(1, 'GiB');\n\
                     SELECT units.main.dimension('kWh');\n\
                     SELECT units.main.compatible('mi', 'km');\n\
                     SELECT units.main.parse_quantity('5 km');\n\
                     SELECT * FROM units.main.supported_units() WHERE dimension = 'length';"
                        .to_string(),
                ),
            ],
            views: Vec::new(),
            macros: Vec::new(),
            // Expose the parameterless `supported_units` scan as a regular table
            // (VGI311) so `SELECT * FROM units.main.supported_units` works
            // without parentheses. `with_function` auto-registers the backing
            // table function, so no separate `table::register` is needed.
            tables: vec![table::supported_units_table()],
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
    // The `supported_units` table function is auto-registered by `set_catalog`
    // via the `CatTable::with_function` entry in `catalog_metadata`, so no
    // separate `table::register` call is needed here.
    worker.set_catalog(catalog_metadata(&catalog_name));
    worker.run();
}
