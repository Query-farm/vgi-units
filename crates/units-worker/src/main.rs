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
                "# Units — Physical Unit Conversion & Dimensional Analysis in SQL\n\n\
                 **Convert physical quantities between units and run dimensional analysis \
                 directly in DuckDB SQL** — length, mass, time, energy, temperature, pressure, \
                 data sizes, and more, all resolved at query time. The `units` worker brings \
                 runtime, string-driven unit conversion to your queries: units are named by \
                 ordinary strings (`'mi'`, `'GiB'`, `'°C'`, `'kWh'`), so you can normalize \
                 measurements, check that two quantities are comparable, and reduce values to \
                 their SI base unit without leaving SQL.\n\n\
                 This extension is for data engineers, analysts, and scientists who deal with \
                 messy, mixed-unit measurement data — energy bills in kWh, distances in miles \
                 and kilometers, storage in GiB and GB, sensor readings in mixed temperature \
                 scales. Instead of hard-coding conversion factors into every query or pipeline, \
                 you call a single well-tested function and get consistent, documented results. \
                 Because unit names are plain strings evaluated per row, it works naturally over \
                 columns of dirty real-world data, returning `NULL` for unrecognized units rather \
                 than aborting a scan.\n\n\
                 Under the hood the worker is powered by a **curated internal unit table** rather \
                 than an external dependency: each unit string maps to a `(dimension, factor, \
                 offset)` triple, and every conversion is an exact affine round-trip through the \
                 SI base unit of its dimension (`base = value * factor + offset`). Offsets are \
                 used only by the temperature scales (°C/°F/K/°R) so that 0 °C = 273.15 K = 32 °F \
                 comes out right; all other factors are the exact agreed SI constants (e.g. inch \
                 = 0.0254 m, pound = 0.45359237 kg, atm = 101325 Pa, calorie = 4.184 J, IEC \
                 binary prefixes as 1024ⁿ vs decimal 1000ⁿ). The factors follow the official SI \
                 definitions maintained by the [BIPM](https://www.bipm.org/en/measurement-units) \
                 and the [NIST reference on constants, units, and \
                 uncertainty](https://physics.nist.gov/cuu/Units/).\n\n\
                 **SQL use cases & function surface.** The scalar functions are `convert(value, \
                 from, to)` (convert between two units of the same dimension), `to_base(value, \
                 unit)` (express a value in the SI base unit of its dimension), `dimension(unit)` \
                 (look up a unit's physical dimension), `compatible(a, b)` (test whether two \
                 units share a dimension and can be converted), `parse_quantity('5 km')` (split a \
                 quantity string into a `STRUCT(value, unit)`), and `units_version()`. The \
                 `supported_units` table function lists every recognized unit alongside its \
                 dimension and base unit for discovery. Typical queries: `SELECT convert(100, \
                 'kWh', 'J')`, `SELECT to_base(1, 'GiB')`, `SELECT compatible('mi', 'kg')`, or \
                 `SELECT * FROM supported_units() WHERE dimension = 'length'`.\n\n\
                 The `units` worker is open source and part of the \
                 [Query.Farm](https://query.farm) VGI ecosystem of DuckDB workers — see the \
                 [source repository on GitHub](https://github.com/Query-farm/vgi-units) for the \
                 full unit catalog, conversion-factor provenance, and usage examples."
                    .to_string(),
            ),
            // Fixed agent-suitability suite run by `vgi-lint simulate`. Each
            // prompt is solvable using only the exposed functions; the hidden
            // reference_sql is the canonical solution used to grade.
            (
                "vgi.agent_test_tasks".to_string(),
                crate::meta::agent_test_tasks_json(&[
                    (
                        "kwh_to_joules",
                        "A utility bill lists energy usage as 100 kWh. How many joules is that? \
                         Return a single column named joules.",
                        "SELECT units.main.convert(100, 'kWh', 'J') AS joules",
                    ),
                    (
                        "richest_dimension",
                        "Which physical dimension does this catalog support the most units for, \
                         and how many units does it have? Return one row with a column named \
                         dimension and a column named n (the unit count).",
                        "SELECT dimension, count(*) AS n FROM units.main.supported_units \
                         GROUP BY dimension ORDER BY n DESC, dimension LIMIT 1",
                    ),
                    (
                        "parse_and_normalize_to_si",
                        "I have the measurement written as the text '5 km'. Parse it and express \
                         its value in the SI base unit of its dimension. Return a single column \
                         named base_value.",
                        "WITH q AS (SELECT units.main.parse_quantity('5 km') AS p) \
                         SELECT units.main.to_base((p).value, (p).unit) AS base_value FROM q",
                    ),
                    (
                        "dimension_of_unit",
                        "What physical dimension does the unit 'kWh' belong to? Return a single \
                         column named dimension.",
                        "SELECT units.main.dimension('kWh') AS dimension",
                    ),
                    (
                        "compatible_units_check",
                        "Before running conversions I want to sanity-check some unit pairs. For \
                         each pair below, tell me whether the two units can be converted between \
                         each other. Return the pairs in this exact order, with columns unit_a, \
                         unit_b, and is_compatible: (mi, km), (kg, lb), (kg, m), (mi, zzz).",
                        "SELECT a AS unit_a, b AS unit_b, units.main.compatible(a, b) AS \
                         is_compatible FROM (VALUES (1, 'mi', 'km'), (2, 'kg', 'lb'), \
                         (3, 'kg', 'm'), (4, 'mi', 'zzz')) AS t(ord, a, b) ORDER BY ord",
                    ),
                    (
                        "worker_version",
                        "What version of the units worker is currently running? Return a single \
                         row with one column named version.",
                        "SELECT units.main.units_version() AS version",
                    ),
                ]),
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
