//! `dimension(unit VARCHAR) -> VARCHAR` (NULL if unknown) and
//! `compatible(unit_a VARCHAR, unit_b VARCHAR) -> BOOLEAN` (same dimension?).

use std::sync::Arc;

use arrow_array::builder::{BooleanBuilder, StringBuilder};
use arrow_array::{ArrayRef, RecordBatch};
use arrow_schema::DataType;
use vgi::{ArgSpec, BindParams, BindResponse, FunctionMetadata, ProcessParams, ScalarFunction};
use vgi_rpc::{Result, RpcError};

use crate::arrow_io::text_str;
use crate::units;

/// `dimension(unit)` → the lowercase dimension name, or NULL for unknown units.
pub struct DimensionFn;

impl ScalarFunction for DimensionFn {
    fn name(&self) -> &str {
        "dimension"
    }

    fn metadata(&self) -> FunctionMetadata {
        FunctionMetadata {
            description: "Return the physical dimension of a unit \
                          ('length'|'mass'|'time'|…), or NULL if the unit is unknown"
                .into(),
            return_type: Some(DataType::Utf8),
            ..Default::default()
        }
    }

    fn argument_specs(&self) -> Vec<ArgSpec> {
        vec![ArgSpec::any_column(
            "unit",
            0,
            "Unit string, e.g. 'mi' (VARCHAR)",
        )]
    }

    fn on_bind(&self, _params: &BindParams) -> Result<BindResponse> {
        Ok(BindResponse::result(DataType::Utf8))
    }

    fn process(&self, params: &ProcessParams, batch: &RecordBatch) -> Result<RecordBatch> {
        let col = batch.column(0);
        let rows = batch.num_rows();
        let mut out = StringBuilder::new();
        for i in 0..rows {
            match text_str(col, i)? {
                Some(u) => match units::dimension(u) {
                    Some(d) => out.append_value(d.name()),
                    None => out.append_null(),
                },
                None => out.append_null(),
            }
        }
        let arr: ArrayRef = Arc::new(out.finish());
        RecordBatch::try_new(params.output_schema.clone(), vec![arr])
            .map_err(|e| RpcError::runtime_error(e.to_string()))
    }
}

/// `compatible(a, b)` → whether the two units share a dimension. Unknown units
/// are never compatible (false). NULL operand → NULL.
pub struct Compatible;

impl ScalarFunction for Compatible {
    fn name(&self) -> &str {
        "compatible"
    }

    fn metadata(&self) -> FunctionMetadata {
        FunctionMetadata {
            description: "Whether two units share a dimension (i.e. can be converted between). \
                          Unknown units are never compatible"
                .into(),
            return_type: Some(DataType::Boolean),
            ..Default::default()
        }
    }

    fn argument_specs(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::any_column("unit_a", 0, "First unit (VARCHAR)"),
            ArgSpec::any_column("unit_b", 1, "Second unit (VARCHAR)"),
        ]
    }

    fn on_bind(&self, _params: &BindParams) -> Result<BindResponse> {
        Ok(BindResponse::result(DataType::Boolean))
    }

    fn process(&self, params: &ProcessParams, batch: &RecordBatch) -> Result<RecordBatch> {
        let a = batch.column(0);
        let b = batch.column(1);
        let rows = batch.num_rows();
        let mut out = BooleanBuilder::new();
        for i in 0..rows {
            match (text_str(a, i)?, text_str(b, i)?) {
                (Some(x), Some(y)) => out.append_value(units::compatible(x, y)),
                _ => out.append_null(),
            }
        }
        let arr: ArrayRef = Arc::new(out.finish());
        RecordBatch::try_new(params.output_schema.clone(), vec![arr])
            .map_err(|e| RpcError::runtime_error(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrow_io::test_support::{bound_type, process_params, run_scalar_text};
    use arrow_array::cast::AsArray;
    use arrow_array::{Array, RecordBatch, StringArray};
    use arrow_schema::{Field, Schema};
    use vgi::arguments::Arguments;

    #[test]
    fn dimension_binds_utf8_and_resolves() {
        assert_eq!(bound_type(&DimensionFn), DataType::Utf8);
        let out = run_scalar_text(
            &DimensionFn,
            &[Some("mi"), Some("kg"), Some("frob"), None],
            Arguments::default(),
        )
        .unwrap();
        let s = out.as_string::<i32>();
        assert_eq!(s.value(0), "length");
        assert_eq!(s.value(1), "mass");
        assert!(out.is_null(2), "unknown unit → NULL dimension");
        assert!(out.is_null(3));
    }

    fn run_compatible(a: &[Option<&str>], b: &[Option<&str>]) -> ArrayRef {
        let ca: ArrayRef = Arc::new(StringArray::from(a.to_vec()));
        let cb: ArrayRef = Arc::new(StringArray::from(b.to_vec()));
        let schema = Arc::new(Schema::new(vec![
            Field::new("a", DataType::Utf8, true),
            Field::new("b", DataType::Utf8, true),
        ]));
        let batch = RecordBatch::try_new(schema.clone(), vec![ca, cb]).unwrap();
        let bind = BindParams {
            input_schema: Some(schema),
            ..Default::default()
        };
        let bound = Compatible.on_bind(&bind).unwrap();
        let params = process_params(bound.output_schema, Arguments::default());
        Compatible
            .process(&params, &batch)
            .unwrap()
            .column(0)
            .clone()
    }

    #[test]
    fn compatible_basics() {
        assert_eq!(bound_type(&Compatible), DataType::Boolean);
        let out = run_compatible(
            &[Some("mi"), Some("mi"), Some("frob"), None],
            &[Some("km"), Some("kg"), Some("m"), Some("m")],
        );
        let bb = out.as_boolean();
        assert!(bb.value(0), "mi/km same dimension");
        assert!(!bb.value(1), "mi/kg different dimensions");
        assert!(!bb.value(2), "unknown unit never compatible");
        assert!(out.is_null(3), "NULL operand → NULL");
    }
}
