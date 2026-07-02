//! `supported_units() -> (unit VARCHAR, dimension VARCHAR, base_unit VARCHAR)` —
//! the discovery table listing every recognized unit string, its dimension, and
//! the SI base unit of that dimension.

use std::sync::Arc;

use arrow_array::builder::StringBuilder;
use arrow_array::{ArrayRef, RecordBatch};
use arrow_schema::{DataType, Field, Schema, SchemaRef};
use vgi::table_function::{TableFunction, TableProducer};
use vgi::{ArgSpec, BindParams, BindResponse, FunctionMetadata, ProcessParams};
use vgi_rpc::{OutputCollector, Result, RpcError};

use crate::units;

/// Guaranteed-runnable, catalog-qualified examples (VGI509). Each `sql` is
/// self-contained and re-runnable against an attached `units` worker. We omit
/// `expected_result` deliberately — the linter only needs each query to execute
/// cleanly, and pinning exact floating-point output would be brittle.
const EXECUTABLE_EXAMPLES: &str = r#"[
  {
    "description": "Convert a marathon distance from miles to kilometres.",
    "sql": "SELECT units.main.convert(26.2, 'mi', 'km') AS km"
  },
  {
    "description": "Express 1 GiB in bytes via the SI base unit.",
    "sql": "SELECT units.main.to_base(1, 'GiB') AS bytes"
  },
  {
    "description": "Look up the physical dimension of a unit.",
    "sql": "SELECT units.main.dimension('kWh') AS dim"
  },
  {
    "description": "Check whether two units can be converted between.",
    "sql": "SELECT units.main.compatible('mi', 'km') AS ok"
  },
  {
    "description": "Parse a quantity string into a (value, unit) struct.",
    "sql": "SELECT units.main.parse_quantity('5 km') AS q"
  },
  {
    "description": "Discover the units in the length dimension.",
    "sql": "SELECT unit, base_unit FROM units.main.supported_units() WHERE dimension = 'length' ORDER BY unit LIMIT 5"
  }
]"#;

pub struct SupportedUnits;

/// A column field carrying a `comment` (surfaced via `duckdb_columns().comment`),
/// so both the scan output and the catalog table document every column.
fn commented(name: &str, comment: &str) -> Field {
    Field::new(name, DataType::Utf8, false).with_metadata(std::collections::HashMap::from([(
        "comment".to_string(),
        comment.to_string(),
    )]))
}

/// The columns produced by `supported_units` — shared by the table function's
/// `on_bind` and by the catalog table that exposes it as `units.main.supported_units`.
/// Each column carries a comment so it is documented wherever the schema surfaces.
pub fn output_schema() -> SchemaRef {
    Arc::new(Schema::new(vec![
        commented(
            "unit",
            "The recognized unit string, e.g. 'km', 'kWh', '°C'. Valid as the unit \
             argument to convert, to_base, dimension, compatible, and parse_quantity.",
        ),
        commented(
            "dimension",
            "The physical dimension the unit measures, as a lowercase name such as \
             'length', 'mass', 'time', 'energy', or 'data'.",
        ),
        commented(
            "base_unit",
            "The SI base unit of the dimension that all conversions in that dimension \
             pass through, e.g. 'm' for length or 'kg' for mass.",
        ),
    ]))
}

impl TableFunction for SupportedUnits {
    fn name(&self) -> &str {
        "supported_units"
    }

    fn metadata(&self) -> FunctionMetadata {
        let mut tags = crate::meta::object_tags(
            "Supported Units Catalog",
            "List every unit string the worker recognizes, together with its physical dimension \
             and the SI base unit of that dimension. Use it to discover which unit strings are \
             valid inputs to convert, to_base, dimension, compatible, and parse_quantity.",
            "List every supported unit with its dimension and SI base unit. Columns: \
             `unit`, `dimension`, `base_unit`.",
            "supported units, list units, available units, unit catalog, discovery, what units, \
             dimension, base unit",
            "Discovery",
            "table/supported.rs",
        );
        tags.push((
            "vgi.result_columns_md".into(),
            "| column | type | description |\n\
             |---|---|---|\n\
             | `unit` | VARCHAR | The unit string, e.g. `km`, `kWh`, `°C`. |\n\
             | `dimension` | VARCHAR | Physical dimension, e.g. `length`, `energy`. |\n\
             | `base_unit` | VARCHAR | The SI base unit for the dimension. |"
                .into(),
        ));
        tags.push(("vgi.executable_examples".into(), EXECUTABLE_EXAMPLES.into()));
        FunctionMetadata {
            description: "List every supported unit with its dimension and SI base unit".into(),
            tags,
            ..Default::default()
        }
    }

    fn argument_specs(&self) -> Vec<ArgSpec> {
        Vec::new()
    }

    fn on_bind(&self, _params: &BindParams) -> Result<BindResponse> {
        Ok(BindResponse {
            output_schema: output_schema(),
            opaque_data: Vec::new(),
        })
    }

    fn producer(&self, params: &ProcessParams) -> Result<Box<dyn TableProducer>> {
        Ok(Box::new(SupportedProducer {
            schema: params.output_schema.clone(),
            done: false,
        }))
    }
}

struct SupportedProducer {
    schema: SchemaRef,
    done: bool,
}

impl TableProducer for SupportedProducer {
    fn next_batch(&mut self, _out: &mut OutputCollector) -> Result<Option<RecordBatch>> {
        if self.done {
            return Ok(None);
        }
        self.done = true;

        let mut unit = StringBuilder::new();
        let mut dimension = StringBuilder::new();
        let mut base_unit = StringBuilder::new();
        for r in units::supported_units() {
            unit.append_value(r.unit);
            dimension.append_value(r.dimension);
            base_unit.append_value(r.base_unit);
        }
        let cols: Vec<ArrayRef> = vec![
            Arc::new(unit.finish()),
            Arc::new(dimension.finish()),
            Arc::new(base_unit.finish()),
        ];
        Ok(Some(
            RecordBatch::try_new(self.schema.clone(), cols)
                .map_err(|e| RpcError::runtime_error(e.to_string()))?,
        ))
    }
}
