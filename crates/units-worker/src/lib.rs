//! The `units` VGI worker (library).
//!
//! Function registration and catalog metadata live here so both entrypoints
//! share them verbatim: `main.rs` (the native binary, stdio/HTTP transport) and
//! the `units-wasm` crate (the browser build, which serves the same `Worker`
//! over a SharedArrayBuffer byte channel instead).
//!
//! A standalone worker that DuckDB launches and talks to over Apache Arrow IPC
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
pub mod units;

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
                 **When to reach for it.** Use this worker whenever a query must reconcile \
                 measurements recorded in different units, confirm that two quantities are even \
                 comparable before combining them, reduce heterogeneous values to a common SI \
                 base so they can be aggregated, or turn free-text quantity strings into \
                 structured numeric values. It fits reporting pipelines that mix imperial and \
                 metric sources, scientific workloads that must normalize before doing math, and \
                 data-cleaning steps that quarantine unrecognized units as NULL rather than \
                 failing."
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
        // Surface the running worker's software version as catalog metadata
        // (read from `vgi_catalogs()` without spending a query) rather than as a
        // parameterless `units_version()` scalar (VGI328).
        implementation_version: Some(version().to_string()),
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
                    "# Units — main schema\n\n\
                     The single namespace for the `units` worker, bringing runtime, \
                     string-driven physical-unit conversion and dimensional analysis to SQL.\n\n\
                     Its capabilities are organized into a few areas:\n\n\
                     - **Conversion** — convert and normalize numeric quantities between units of \
                     the same dimension, including reduction to the SI base unit.\n\
                     - **Analysis** — look up a unit's physical dimension and test whether two \
                     units are compatible for conversion.\n\
                     - **Parsing** — turn free-text quantity strings such as `5 km` into a \
                     structured value and unit.\n\
                     - **Discovery** — browse the catalog of every recognized unit string, its \
                     dimension, and its SI base unit.\n\n\
                     Reach for this schema whenever a query must reconcile mixed-unit \
                     measurements, confirm that two quantities are comparable, or normalize \
                     values to a common base before aggregating."
                        .to_string(),
                ),
                // VGI413 category registry — an ordered JSON array of
                // {"name","description"} sections. Every function/table tags
                // itself with a matching `vgi.category`; these drive the
                // worker's navigation and listing sections.
                (
                    "vgi.categories".to_string(),
                    r#"[
  {"name": "Conversion", "description": "Convert and normalize numeric quantities between units of the same physical dimension, including reduction to the SI base unit."},
  {"name": "Analysis", "description": "Inspect units: look up a unit's physical dimension and test whether two units are compatible for conversion."},
  {"name": "Parsing", "description": "Parse free-text quantity strings such as '5 km' into a structured value and unit."},
  {"name": "Discovery", "description": "Explore the catalog of every recognized unit string, its dimension, and its SI base unit."}
]"#
                    .to_string(),
                ),
                // VGI506/VGI515 representative example queries for the schema —
                // a JSON array of {description, sql} so every example is
                // self-explanatory.
                (
                    "vgi.example_queries".to_string(),
                    crate::meta::example_queries_json(&[
                        (
                            "Convert a marathon distance from miles to kilometres.",
                            "SELECT units.main.convert(26.2, 'mi', 'km') AS km",
                        ),
                        (
                            "Reduce 1 GiB to bytes via the SI base unit.",
                            "SELECT units.main.to_base(1, 'GiB') AS bytes",
                        ),
                        (
                            "Look up the physical dimension of a unit.",
                            "SELECT units.main.dimension('kWh') AS dim",
                        ),
                        (
                            "Check whether two units can be converted between.",
                            "SELECT units.main.compatible('mi', 'km') AS ok",
                        ),
                        (
                            "Parse a free-text quantity string into a (value, unit) struct.",
                            "SELECT units.main.parse_quantity('5 km') AS q",
                        ),
                        (
                            "List the recognized units in the length dimension.",
                            "SELECT unit, base_unit FROM units.main.supported_units \
                             WHERE dimension = 'length' ORDER BY unit",
                        ),
                    ]),
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

/// The catalog name DuckDB sees in `ATTACH 'units' (TYPE vgi, …)`. Defaults to
/// `units`, but honors an explicit override so a test harness can rename it.
/// Also exports the variable so downstream SDK code observes the same default.
pub fn catalog_name() -> String {
    if std::env::var_os("VGI_WORKER_CATALOG_NAME").is_none() {
        std::env::set_var("VGI_WORKER_CATALOG_NAME", "units");
    }
    std::env::var("VGI_WORKER_CATALOG_NAME").unwrap_or_else(|_| "units".to_string())
}

/// Build a fully-registered worker: every scalar and table function plus the
/// catalog metadata. Callers choose the transport — `run()` natively,
/// `serve_reader_writer()` in the browser.
pub fn build_worker() -> Worker {
    let catalog_name = catalog_name();

    let mut worker = Worker::new();
    scalar::register(&mut worker);
    // The `supported_units` table function is auto-registered by `set_catalog`
    // via the `CatTable::with_function` entry in `catalog_metadata`, so no
    // separate `table::register` call is needed here.
    worker.set_catalog(catalog_metadata(&catalog_name));
    worker
}
