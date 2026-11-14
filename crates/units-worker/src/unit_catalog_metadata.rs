//! Deterministic metadata for validating a governed model without invoking a
//! conversion or a table producer. Definitions come only from the unit engine.

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

const MAX_CATALOG_BYTES: usize = 256 * 1024;
const MAX_CATALOG_UNITS: usize = 4096;

// Struct field order is the serialized contract; rows use UTF-8 byte ordering.
#[derive(Serialize)]
struct Catalog {
    version: u32,
    implementation_version: &'static str,
    units: Vec<Unit>,
}

#[derive(Serialize)]
struct Unit {
    unit: &'static str,
    dimension: &'static str,
    base_unit: &'static str,
    quantity_space: &'static str,
    factor: f64,
    offset: f64,
}

fn serialize_catalog() -> String {
    let mut definitions = crate::units::supported_units();
    definitions.sort_by(|a, b| a.unit.as_bytes().cmp(b.unit.as_bytes()));
    assert!(definitions.len() <= MAX_CATALOG_UNITS);
    assert!(definitions
        .windows(2)
        .all(|rows| rows[0].unit != rows[1].unit));
    let units = definitions
        .into_iter()
        .map(|row| {
            let definition = crate::units::lookup(row.unit).expect("catalog unit must resolve");
            assert!(definition.factor.is_finite() && definition.factor > 0.0);
            assert!(definition.offset.is_finite());
            Unit {
                unit: row.unit,
                dimension: row.dimension,
                base_unit: row.base_unit,
                quantity_space: quantity_space(definition.dimension),
                factor: definition.factor,
                offset: definition.offset,
            }
        })
        .collect();
    let serialized = serde_json::to_string(&Catalog {
        version: 1,
        implementation_version: crate::version(),
        units,
    })
    .expect("finite static unit definitions must serialize");
    assert!(serialized.len() <= MAX_CATALOG_BYTES);
    serialized
}

/// The quantity space of a dimension: `affine` for temperature (offsets apply
/// to absolute values and cancel for differences), `linear` otherwise. It
/// belongs to the dimension, not the offset: Kelvin and Rankine are affine even
/// with a zero offset.
pub fn quantity_space(dimension: crate::units::Dimension) -> &'static str {
    if dimension == crate::units::Dimension::Temperature {
        "affine"
    } else {
        "linear"
    }
}

/// Lower-case hex SHA-256 of the exact [`unit_catalog_json`] bytes, published
/// as the strict scalars' `units.catalog_v1_sha256` tag. The full JSON is too
/// large for a tag (agent tools bound each value to 4,000 characters), so a
/// governed host verifies an exported copy against this digest instead.
pub fn unit_catalog_sha256() -> &'static str {
    static DIGEST: OnceLock<String> = OnceLock::new();
    DIGEST.get_or_init(|| {
        Sha256::digest(unit_catalog_json().as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    })
}

/// Complete `units.catalog_v1` JSON document, with exact case-preserving
/// aliases and versioned binary64 factors. At most 4096 rows and 256 KiB UTF-8.
/// The JSON is compact, byte deterministic, and has no trailing newline.
pub fn unit_catalog_json() -> &'static str {
    static CATALOG: OnceLock<String> = OnceLock::new();
    CATALOG.get_or_init(serialize_catalog).as_str()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn catalog_is_deterministic_bounded_sorted_and_complete() {
        let text = unit_catalog_json();
        assert_eq!(text, serialize_catalog());
        assert_eq!(text, serialize_catalog());
        assert!(text.len() <= MAX_CATALOG_BYTES);
        assert!(!text.ends_with('\n'));
        assert!(text.starts_with(&format!(
            "{{\"version\":1,\"implementation_version\":\"{}\",\"units\":[",
            crate::version()
        )));
        let catalog: Value = serde_json::from_str(text).unwrap();
        assert_eq!(catalog.as_object().unwrap().len(), 3);
        assert_eq!(catalog["version"], 1);
        assert_eq!(catalog["implementation_version"], crate::version());
        let rows = catalog["units"].as_array().unwrap();
        assert_eq!(rows.len(), crate::units::unit_count());
        assert!(rows
            .windows(2)
            .all(|r| r[0]["unit"].as_str().unwrap().as_bytes()
                < r[1]["unit"].as_str().unwrap().as_bytes()));
        for row in rows {
            assert_eq!(row.as_object().unwrap().len(), 6);
            let definition = crate::units::lookup(row["unit"].as_str().unwrap()).unwrap();
            assert_eq!(row["dimension"], definition.dimension.name());
            assert_eq!(row["base_unit"], definition.dimension.base_unit());
            assert_eq!(
                row["factor"].as_f64().unwrap().to_bits(),
                definition.factor.to_bits()
            );
            assert_eq!(
                row["offset"].as_f64().unwrap().to_bits(),
                definition.offset.to_bits()
            );
            assert_eq!(
                row["quantity_space"],
                if definition.dimension == crate::units::Dimension::Temperature {
                    "affine"
                } else {
                    "linear"
                }
            );
        }
    }

    #[test]
    fn digest_is_hex_sha256_of_exact_json() {
        let digest = unit_catalog_sha256();
        assert_eq!(digest.len(), 64);
        assert!(digest
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        let again: String = Sha256::digest(unit_catalog_json().as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(digest, again);
    }

    #[test]
    fn catalog_preserves_case_significant_aliases_and_affine_zero_offsets() {
        let catalog: Value = serde_json::from_str(unit_catalog_json()).unwrap();
        let rows = catalog["units"].as_array().unwrap();
        let find = |unit| rows.iter().find(|r| r["unit"] == unit).unwrap();
        assert_eq!(find("B")["factor"], 1.0);
        assert_eq!(find("b")["factor"], 0.125);
        assert_eq!(find("MB")["factor"], 1_000_000.0);
        assert_eq!(find("Mb")["factor"], 125_000.0);
        for unit in ["K", "kelvin", "R", "rankine"] {
            assert_eq!(find(unit)["quantity_space"], "affine");
            assert_eq!(find(unit)["offset"], 0.0);
        }
        assert!(rows.iter().all(|r| r["unit"] != "KM"));
        assert!(crate::units::lookup("KM").is_some());
    }

    #[test]
    fn catalog_bases_are_self_consistent_and_linear_offsets_are_zero() {
        let catalog: Value = serde_json::from_str(unit_catalog_json()).unwrap();
        let rows: std::collections::HashMap<_, _> = catalog["units"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| (row["unit"].as_str().unwrap(), row))
            .collect();
        for row in rows.values() {
            let base = rows[row["base_unit"].as_str().unwrap()];
            assert_eq!(base["unit"], base["base_unit"]);
            assert_eq!(base["dimension"], row["dimension"]);
            assert_eq!(base["quantity_space"], row["quantity_space"]);
            assert_eq!(base["factor"], 1.0);
            assert_eq!(base["offset"], 0.0);
            if row["quantity_space"] == "linear" {
                assert_eq!(row["offset"], 0.0);
            }
        }
    }
}
