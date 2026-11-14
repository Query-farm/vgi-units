//! Table functions exposed by the units worker, registered under `units.main`.

mod supported;
mod unit_catalog;

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
            crate::meta::example_queries_json(&[
                (
                    "List the length units and their SI base unit.",
                    "SELECT unit, base_unit FROM units.main.supported_units WHERE dimension = 'length' ORDER BY unit",
                ),
                (
                    "Count how many units exist in each dimension.",
                    "SELECT dimension, count(*) AS n FROM units.main.supported_units \
                     GROUP BY dimension ORDER BY n DESC, dimension",
                ),
                crate::examples::MILE_IN_EVERY_LENGTH_UNIT,
            ]),
        ),
    ];
    t
}

/// Build the catalog `CatTable` that exposes `unit_catalog` as a regular table
/// (`SELECT * FROM units.main.unit_catalog`), backed by the
/// [`unit_catalog::UnitCatalog`] scan function. `unit` is the primary key and
/// every column is NOT NULL.
pub fn unit_catalog_table() -> CatTable {
    let mut t = CatTable::with_function(
        "unit_catalog",
        unit_catalog::output_schema(),
        Arc::new(unit_catalog::UnitCatalog),
        Some(
            "Authoritative, versioned unit definitions (factor and offset per exact unit string) \
             behind the strict conversion functions."
                .to_string(),
        ),
        Some(crate::units::unit_count() as i64),
    );
    let columns = [
        "unit",
        "dimension",
        "base_unit",
        "quantity_space",
        "factor",
        "offset",
        "implementation_version",
        "contract_version",
    ];
    let mut records: Vec<(String, ConstraintKind, &str)> = vec![
        (
            "unit_catalog.pk".to_string(),
            ConstraintKind::PrimaryKey,
            "unit",
        ),
        (
            "unit_catalog.unit.unique".to_string(),
            ConstraintKind::Unique,
            "unit",
        ),
    ];
    records.extend(columns.iter().map(|c| {
        (
            format!("unit_catalog.{c}.not_null"),
            ConstraintKind::NotNull,
            *c,
        )
    }));
    t.constraints = records
        .into_iter()
        .map(|(id, kind, column)| {
            constraint_record(
                &ConstraintRecord::new(&["main"], "unit_catalog", &id, kind)
                    .with_columns(&[&[column]])
                    .enforced(),
            )
            .expect("valid constraint record")
        })
        .collect();
    // Keep the `unit_catalog()` call form callable alongside the bare table.
    t.expose_scan = true;
    t.tags = vec![
        ("vgi.title".to_string(), "Unit Definition Catalog".to_string()),
        (
            "vgi.doc_llm".to_string(),
            "The authoritative unit definitions behind convert_strict and \
             convert_difference_strict: one row per exact unit string with its dimension, SI base \
             unit, scale factor and offset (base = value * factor + offset; differences use the \
             factor only), plus the worker implementation and contract versions. Query it to \
             audit or approve the exact constants a conversion uses."
                .to_string(),
        ),
        (
            "vgi.doc_md".to_string(),
            "# unit_catalog\n\nVersioned, authoritative unit definitions for administrative \
             approval of strict conversions. One row per exact unit string with `unit`, \
             `dimension`, `base_unit`, `quantity_space`, `factor`, `offset` (`base = value * factor + offset`; \
             differences use `factor` only), `implementation_version` and `contract_version`. \
             Quote the `\"offset\"` column, which is a SQL keyword."
                .to_string(),
        ),
        (
            "vgi.keywords".to_string(),
            crate::meta::keywords_json(
                "unit catalog, unit definitions, conversion factor, offset, version, audit, \
                 approval, governance, strict conversion",
            ),
        ),
        ("domain".to_string(), "units-and-measurement".to_string()),
        ("category".to_string(), "discovery".to_string()),
        ("topic".to_string(), "unit-definitions".to_string()),
        ("vgi.category".to_string(), "Discovery".to_string()),
        (
            "vgi.example_queries".to_string(),
            crate::meta::example_queries_json(&[
                (
                    "Show the factor and offset that define each temperature unit.",
                    "SELECT unit, factor, \"offset\" FROM units.main.unit_catalog \
                     WHERE dimension = 'temperature' ORDER BY unit",
                ),
                (
                    "Report the worker and contract versions the definitions belong to.",
                    "SELECT DISTINCT implementation_version, contract_version \
                     FROM units.main.unit_catalog ORDER BY ALL",
                ),
                crate::examples::AUDIT_STRICT_AGAINST_CATALOG,
            ]),
        ),
    ];
    t
}
