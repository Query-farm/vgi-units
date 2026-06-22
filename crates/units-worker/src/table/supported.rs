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

pub struct SupportedUnits;

fn output_schema() -> SchemaRef {
    Arc::new(Schema::new(vec![
        Field::new("unit", DataType::Utf8, false),
        Field::new("dimension", DataType::Utf8, false),
        Field::new("base_unit", DataType::Utf8, false),
    ]))
}

impl TableFunction for SupportedUnits {
    fn name(&self) -> &str {
        "supported_units"
    }

    fn metadata(&self) -> FunctionMetadata {
        FunctionMetadata {
            description: "List every supported unit with its dimension and SI base unit".into(),
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
