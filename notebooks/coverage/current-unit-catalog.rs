//! `unit_catalog() -> (unit, dimension, base_unit, quantity_space, factor, offset,
//! implementation_version, contract_version)` — the versioned, authoritative
//! unit definitions behind the strict conversion functions, for administrative
//! approval: `base = value * factor + offset`, or `base_difference = value *
//! factor` for differences. One row per exact recognized unit string.

use arrow_array::{ArrayRef, Float64Array, RecordBatch, StringArray};
use arrow_schema::{DataType, Field, Schema, SchemaRef};
use std::sync::Arc;
use vgi::table_function::{TableFunction, TableProducer};
use vgi::{ArgSpec, BindParams, BindResponse, FunctionMetadata, ProcessParams};
use vgi_rpc::{OutputCollector, Result, RpcError};

/// The strict-conversion contract version reported in every row.
pub const CONTRACT_VERSION: &str = "1";

/// Guaranteed-runnable, catalog-qualified examples (VGI509).
const EXECUTABLE_EXAMPLES: &str = r#"[
  {
    "description": "Show the factor and offset that define each temperature unit.",
    "sql": "SELECT unit, factor, \"offset\" FROM units.main.unit_catalog() WHERE dimension = 'temperature' ORDER BY unit"
  },
  {
    "description": "Report the worker and contract versions the definitions belong to.",
    "sql": "SELECT DISTINCT implementation_version, contract_version FROM units.main.unit_catalog()"
  }
]"#;

pub struct UnitCatalog;

fn commented(name: &str, ty: DataType, comment: &str) -> Field {
    Field::new(name, ty, false).with_metadata(std::collections::HashMap::from([(
        "comment".to_string(),
        comment.to_string(),
    )]))
}

/// The columns produced by `unit_catalog` — shared by the table function's
/// `on_bind` and the catalog table `units.main.unit_catalog`.
pub fn output_schema() -> SchemaRef {
    Arc::new(Schema::new(vec![
        commented(
            "unit",
            DataType::Utf8,
            "The exact recognized unit string, e.g. 'km', 'MB', '°C'. Case-significant.",
        ),
        commented(
            "dimension",
            DataType::Utf8,
            "The physical dimension the unit measures, e.g. 'length' or 'temperature'.",
        ),
        commented(
            "base_unit",
            DataType::Utf8,
            "The SI base unit of the dimension that conversions pass through, e.g. 'm'.",
        ),
        commented(
            "quantity_space",
            DataType::Utf8,
            "'affine' for temperature units (offsets apply to absolute values and cancel for \
             differences); 'linear' for every other dimension.",
        ),
        commented(
            "factor",
            DataType::Float64,
            "Scale factor to the base unit: base = value * factor + offset. Finite and positive.",
        ),
        commented(
            "offset",
            DataType::Float64,
            "Additive offset to the base unit (non-zero only for affine temperature scales); \
             ignored when converting differences.",
        ),
        commented(
            "implementation_version",
            DataType::Utf8,
            "The worker software version that serves these definitions.",
        ),
        commented(
            "contract_version",
            DataType::Utf8,
            "The strict-conversion contract version these definitions satisfy.",
        ),
    ]))
}

impl TableFunction for UnitCatalog {
    fn name(&self) -> &str {
        "unit_catalog"
    }

    fn metadata(&self) -> FunctionMetadata {
        let mut tags = crate::meta::object_tags(
            "Unit Definition Catalog",
            "The authoritative, versioned unit definitions used by convert_strict and \
             convert_difference_strict: one row per exact unit string with its dimension, SI \
             base unit, scale factor and offset (base = value * factor + offset; differences use \
             the factor only), plus the worker implementation and contract versions. Use it to \
             audit or approve the exact conversion constants.",
            "Versioned unit definitions for conversion approval. Columns: `unit`, `dimension`, \
             `base_unit`, `quantity_space`, `factor`, `offset`, `implementation_version`, `contract_version`.",
            "unit catalog, unit definitions, conversion factor, offset, version, audit, approval, \
             governance, strict conversion",
            "Discovery",
            "table/unit_catalog.rs",
        );
        tags.push((
            "vgi.result_columns_schema".into(),
            r#"[
  {"name": "unit", "type": "VARCHAR", "description": "The exact recognized unit string."},
  {"name": "dimension", "type": "VARCHAR", "description": "The physical dimension the unit measures."},
  {"name": "base_unit", "type": "VARCHAR", "description": "The SI base unit of the dimension."},
  {"name": "quantity_space", "type": "VARCHAR", "description": "'affine' for temperature units, 'linear' otherwise."},
  {"name": "factor", "type": "DOUBLE", "description": "Scale factor: base = value * factor + offset."},
  {"name": "offset", "type": "DOUBLE", "description": "Additive offset to the base unit (temperature scales only)."},
  {"name": "implementation_version", "type": "VARCHAR", "description": "The worker software version."},
  {"name": "contract_version", "type": "VARCHAR", "description": "The strict-conversion contract version."}
]"#
            .into(),
        ));
        tags.push(("vgi.executable_examples".into(), EXECUTABLE_EXAMPLES.into()));
        FunctionMetadata {
            description: "Versioned unit definitions for administrative validation: \
                          base=value*factor+offset; differences use factor only"
                .into(),
            tags,
            ..Default::default()
        }
    }

    fn argument_specs(&self) -> Vec<ArgSpec> {
        Vec::new()
    }

    fn on_bind(&self, _: &BindParams) -> Result<BindResponse> {
        Ok(BindResponse {
            output_schema: output_schema(),
            opaque_data: Vec::new(),
        })
    }

    fn producer(&self, params: &ProcessParams) -> Result<Box<dyn TableProducer>> {
        Ok(Box::new(Producer {
            schema: params.output_schema.clone(),
            done: false,
        }))
    }
}

struct Producer {
    schema: SchemaRef,
    done: bool,
}

impl TableProducer for Producer {
    fn next_batch(&mut self, _: &mut OutputCollector) -> Result<Option<RecordBatch>> {
        if self.done {
            return Ok(None);
        }
        self.done = true;
        Ok(Some(catalog_batch(self.schema.clone())?))
    }
    // Single batch: the bounded static catalog is emitted in one RecordBatch,
    // so there is no cursor to resume across HTTP turns.
    fn resume_supported(&self) -> bool {
        false
    }
}

fn catalog_batch(schema: SchemaRef) -> Result<RecordBatch> {
    let units = crate::units::supported_units();
    let definitions: Vec<_> = units
        .iter()
        .map(|row| crate::units::lookup(row.unit).expect("catalog unit must resolve"))
        .collect();
    let columns: Vec<ArrayRef> = vec![
        Arc::new(StringArray::from(
            units.iter().map(|r| r.unit).collect::<Vec<_>>(),
        )),
        Arc::new(StringArray::from(
            units.iter().map(|r| r.dimension).collect::<Vec<_>>(),
        )),
        Arc::new(StringArray::from(
            units.iter().map(|r| r.base_unit).collect::<Vec<_>>(),
        )),
        Arc::new(StringArray::from(
            definitions
                .iter()
                .map(|d| crate::unit_catalog_metadata::quantity_space(d.dimension))
                .collect::<Vec<_>>(),
        )),
        Arc::new(Float64Array::from(
            definitions.iter().map(|d| d.factor).collect::<Vec<_>>(),
        )),
        Arc::new(Float64Array::from(
            definitions.iter().map(|d| d.offset).collect::<Vec<_>>(),
        )),
        Arc::new(StringArray::from(vec![crate::version(); units.len()])),
        Arc::new(StringArray::from(vec![CONTRACT_VERSION; units.len()])),
    ];
    RecordBatch::try_new(schema, columns).map_err(|e| RpcError::runtime_error(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::collections::HashMap;

    #[test]
    fn complete_table_catalog_matches_scalar_metadata_and_engine() {
        let batch = catalog_batch(output_schema()).unwrap();
        let catalog: Value = serde_json::from_str(crate::unit_catalog_json()).unwrap();
        let json_rows: HashMap<_, _> = catalog["units"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| (row["unit"].as_str().unwrap(), row))
            .collect();
        assert_eq!(batch.num_rows(), json_rows.len());
        assert_eq!(batch.num_columns(), 8);
        let strings = |col: usize| {
            batch
                .column(col)
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap()
        };
        let doubles = |col: usize| {
            batch
                .column(col)
                .as_any()
                .downcast_ref::<Float64Array>()
                .unwrap()
        };
        for row in 0..batch.num_rows() {
            let unit = strings(0).value(row);
            let definition = crate::units::lookup(unit).unwrap();
            let json = json_rows[unit];
            assert_eq!(strings(1).value(row), definition.dimension.name());
            assert_eq!(strings(2).value(row), definition.dimension.base_unit());
            assert_eq!(doubles(4).value(row).to_bits(), definition.factor.to_bits());
            assert_eq!(doubles(5).value(row).to_bits(), definition.offset.to_bits());
            assert_eq!(strings(1).value(row), json["dimension"].as_str().unwrap());
            assert_eq!(
                strings(3).value(row),
                json["quantity_space"].as_str().unwrap()
            );
            assert_eq!(strings(2).value(row), json["base_unit"].as_str().unwrap());
            assert_eq!(
                doubles(4).value(row).to_bits(),
                json["factor"].as_f64().unwrap().to_bits()
            );
            assert_eq!(
                doubles(5).value(row).to_bits(),
                json["offset"].as_f64().unwrap().to_bits()
            );
            assert_eq!(strings(6).value(row), crate::version());
            assert_eq!(strings(7).value(row), "1");
        }
    }
}
