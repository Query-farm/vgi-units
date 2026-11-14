//! Composed example queries — each combines two or more of the worker's own
//! objects in one realistic query. Every `(description, sql)` pair is defined
//! once here and referenced from both the schema-level `vgi.example_queries`
//! list (`lib.rs`) and the example list of the object that drives the query,
//! so the two copies can never drift apart.

/// A `(description, sql)` example pair, as consumed by
/// [`crate::meta::example_queries_json`].
pub type Example = (&'static str, &'static str);

/// `parse_quantity` + `dimension` + `to_base`: total free-text readings per
/// dimension in SI base units. The unknown `furlongs` reading parses to NULL
/// and is dropped.
pub const PARSED_TOTALS_BY_DIMENSION: Example = (
    "Parse free-text readings, group them by physical dimension, and total each group in its SI \
     base unit; '7 furlongs' is not a recognized unit, so it parses to NULL and is skipped.",
    "WITH r AS (SELECT units.main.parse_quantity(txt) AS q FROM (VALUES ('5 km'), ('3 mi'), ('800 m'), ('2 kg'), ('12 lb'), ('7 furlongs')) AS t(txt)) SELECT units.main.dimension((q).unit) AS dim, count(*) AS readings, round(sum(units.main.to_base((q).value, (q).unit)), 4) AS total_si FROM r WHERE q IS NOT NULL GROUP BY dim ORDER BY dim",
);

/// `parse_quantity` + `supported_units` + `to_base`: label each normalized
/// reading with the SI base unit it was reduced to.
pub const PARSED_TO_BASE_WITH_UNIT: Example = (
    "Normalize mixed free-text readings to SI and label each with its base unit by joining the \
     parsed unit to supported_units (e.g. '98.6 °F' becomes 310.15 K).",
    "WITH r AS (SELECT txt, units.main.parse_quantity(txt) AS q FROM (VALUES ('5 km'), ('2 GiB'), ('1 kWh'), ('98.6 °F')) AS t(txt)) SELECT r.txt, units.main.to_base((r.q).value, (r.q).unit) AS base_value, s.base_unit FROM r JOIN units.main.supported_units AS s ON s.unit = (r.q).unit ORDER BY r.txt",
);

/// `supported_units` + `convert`: express one mile in every length unit.
pub const MILE_IN_EVERY_LENGTH_UNIT: Example = (
    "Express one mile in every length unit the worker recognizes, largest number first.",
    "SELECT unit, units.main.convert(1, 'mi', unit) AS one_mile FROM units.main.supported_units WHERE dimension = 'length' ORDER BY one_mile DESC, unit",
);

/// `compatible` + `convert`: guard a conversion so rows in the wrong
/// dimension yield NULL instead of raising convert's incompatible-dimension
/// error.
pub const GUARDED_CONVERT_TO_KG: Example = (
    "Guard a conversion: convert each row to kilograms only when its unit is compatible with kg, \
     so the length row ('m') yields NULL instead of an incompatible-dimension error.",
    "SELECT id, qty, unit, CASE WHEN units.main.compatible(unit, 'kg') THEN units.main.convert(qty, unit, 'kg') END AS kg FROM (VALUES (1, 150.0, 'lb'), (2, 2.5, 'kg'), (3, 3.0, 'm'), (4, 16.0, 'oz')) AS t(id, qty, unit) ORDER BY id",
);

/// `convert_strict` + `convert_difference_strict`: absolute reading vs.
/// temperature change, where only the absolute one applies the offset.
pub const STRICT_READING_AND_RISE: Example = (
    "Report each reactor's final temperature in °F with convert_strict and its rise in °F with \
     convert_difference_strict: the 15 °C rise of reactor_a is 27 °F, not the 59 °F an absolute \
     conversion would give.",
    "SELECT sensor, round(units.main.convert_strict(end_c, '°C', '°F'), 2) AS end_f, round(units.main.convert_difference_strict(end_c - start_c, '°C', '°F'), 2) AS rise_f FROM (VALUES ('reactor_a', 20.0, 35.0), ('reactor_b', 18.5, 21.0)) AS t(sensor, start_c, end_c) ORDER BY sensor",
);

/// `unit_catalog` + `convert_strict`: audit strict conversions against the
/// published factor/offset definitions.
pub const AUDIT_STRICT_AGAINST_CATALOG: Example = (
    "Audit strict temperature conversions: convert 100 of each unit with convert_strict and \
     recompute the same result from the factor and offset published in unit_catalog \
     ((value * f.factor + f.offset - t.offset) / t.factor).",
    "SELECT f.unit AS from_unit, t.unit AS to_unit, round(units.main.convert_strict(100, f.unit, t.unit), 6) AS converted, round((100 * f.factor + f.\"offset\" - t.\"offset\") / t.factor, 6) AS recomputed FROM units.main.unit_catalog AS f JOIN units.main.unit_catalog AS t ON t.dimension = f.dimension AND t.unit <> f.unit WHERE f.unit IN ('°C', '°F', 'K') AND t.unit IN ('°C', '°F', 'K') ORDER BY from_unit, to_unit",
);
