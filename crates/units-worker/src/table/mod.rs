//! Table functions exposed by the units worker, registered under `units.main`.

mod supported;

use std::sync::Arc;

use vgi::catalog::CatTable;
use vgi::constraints::{constraint_record, ConstraintKind, ConstraintRecord};

/// Build the catalog `CatTable` that exposes `supported_units` as a regular
/// table (`SELECT * FROM units.main.supported_units`, no parentheses — VGI311),
/// backed by the [`supported::SupportedUnits`] scan function.
///
/// `CatTable::with_function` stores the function instance, so
/// [`vgi::Worker::set_catalog`] auto-registers it into the dispatch table; no
/// separate `register_table` call is needed. The table carries the same
/// discovery tags, example queries, column comments, and a primary key as any
/// well-documented catalog object so it lints clean on its own.
pub fn supported_units_table() -> CatTable {
    let mut t = CatTable::with_function(
        "supported_units",
        supported::output_schema(),
        Arc::new(supported::SupportedUnits),
        Some(
            "Every unit string the worker recognizes, with its physical dimension and SI base unit."
                .to_string(),
        ),
        Some(crate::units::unit_count() as i64),
    );
    // `unit` is the unique row identity — declare it the primary key (VGI807)
    // and unique; every column is NOT NULL (VGI806). Served nullability is
    // derived from these records.
    t.constraints = [
        ("supported_units.pk", ConstraintKind::PrimaryKey, "unit"),
        (
            "supported_units.unit.not_null",
            ConstraintKind::NotNull,
            "unit",
        ),
        (
            "supported_units.dimension.not_null",
            ConstraintKind::NotNull,
            "dimension",
        ),
        (
            "supported_units.base_unit.not_null",
            ConstraintKind::NotNull,
            "base_unit",
        ),
        (
            "supported_units.unit.unique",
            ConstraintKind::Unique,
            "unit",
        ),
    ]
    .into_iter()
    .map(|(id, kind, column)| {
        constraint_record(
            &ConstraintRecord::new(&["main"], "supported_units", id, kind)
                .with_columns(&[&[column]])
                .enforced(),
        )
        .expect("valid constraint record")
    })
    .collect();
    // Protocol 3.3 hides a table's scan function (table_only) by default; the
    // README and test suite document the `supported_units()` call form, so keep
    // it callable alongside the bare table.
    t.expose_scan = true;
    t.tags = vec![
        ("vgi.title".to_string(), "Supported Units Catalog".to_string()),
        (
            "vgi.doc_llm".to_string(),
            "Every unit string the worker recognizes, each with its physical dimension and the \
             SI base unit of that dimension. Query it to discover which unit strings are valid \
             inputs to convert, to_base, dimension, compatible, and parse_quantity, or to filter \
             the catalog by dimension."
                .to_string(),
        ),
        (
            "vgi.doc_md".to_string(),
            "# supported_units\n\nThe discovery table of every recognized unit string. One row \
             per unit, with columns `unit` (the string, e.g. `km`), `dimension` (e.g. `length`), \
             and `base_unit` (the SI base unit of that dimension, e.g. `m`). Use it to find valid \
             unit inputs for the scalar functions, or `WHERE dimension = '…'` to browse a single \
             dimension."
                .to_string(),
        ),
        (
            "vgi.keywords".to_string(),
            crate::meta::keywords_json(
                "supported units, list units, available units, unit catalog, discovery, \
                 what units, dimension, base unit",
            ),
        ),
        ("domain".to_string(), "units-and-measurement".to_string()),
        ("category".to_string(), "discovery".to_string()),
        ("topic".to_string(), "unit-catalog".to_string()),
        // VGI413 navigation category — one of the schema's `vgi.categories`.
        ("vgi.category".to_string(), "Discovery".to_string()),
        (
            "vgi.example_queries".to_string(),
            r#"[
  {
    "description": "List the length units and their SI base unit.",
    "sql": "SELECT unit, base_unit FROM units.main.supported_units WHERE dimension = 'length' ORDER BY unit"
  },
  {
    "description": "Count how many units exist in each dimension.",
    "sql": "SELECT dimension, count(*) AS n FROM units.main.supported_units GROUP BY dimension ORDER BY n DESC"
  }
]"#
            .to_string(),
        ),
    ];
    t
}
