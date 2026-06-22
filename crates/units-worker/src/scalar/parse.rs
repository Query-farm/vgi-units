//! `parse_quantity(text VARCHAR) -> STRUCT(value DOUBLE, unit VARCHAR)`.
//!
//! Parses free-form quantity text like `"5 km"`, `"3.2kg"`, `"10 m/s"`. The unit
//! must be recognized; otherwise (or if no number is present) the row is NULL.

use std::sync::Arc;

use arrow_array::builder::{Float64Builder, StringBuilder};
use arrow_array::{ArrayRef, RecordBatch, StructArray};
use arrow_buffer::NullBuffer;
use arrow_schema::DataType;
use vgi::{ArgSpec, BindParams, BindResponse, FunctionMetadata, ProcessParams, ScalarFunction};
use vgi_rpc::{Result, RpcError};

use crate::arrow_io::{quantity_struct_fields, text_str};
use crate::units;

pub struct ParseQuantity;

impl ScalarFunction for ParseQuantity {
    fn name(&self) -> &str {
        "parse_quantity"
    }

    fn metadata(&self) -> FunctionMetadata {
        FunctionMetadata {
            description: "Parse quantity text like '5 km' or '3.2kg' into a \
                          STRUCT(value DOUBLE, unit VARCHAR); NULL if unparseable \
                          or the unit is unknown"
                .into(),
            ..Default::default()
        }
    }

    fn argument_specs(&self) -> Vec<ArgSpec> {
        vec![ArgSpec::any_column(
            "text",
            0,
            "Quantity text, e.g. '5 km' (VARCHAR)",
        )]
    }

    fn on_bind(&self, _params: &BindParams) -> Result<BindResponse> {
        Ok(BindResponse::result(DataType::Struct(
            quantity_struct_fields(),
        )))
    }

    fn process(&self, params: &ProcessParams, batch: &RecordBatch) -> Result<RecordBatch> {
        let col = batch.column(0);
        let rows = batch.num_rows();

        let mut value = Float64Builder::new();
        let mut unit = StringBuilder::new();
        let mut valid: Vec<bool> = Vec::with_capacity(rows);

        for i in 0..rows {
            let parsed = match text_str(col, i)? {
                Some(text) => units::parse_quantity(text),
                None => None,
            };
            match parsed {
                Some(q) => {
                    value.append_value(q.value);
                    unit.append_value(&q.unit);
                    valid.push(true);
                }
                None => {
                    value.append_null();
                    unit.append_null();
                    valid.push(false);
                }
            }
        }

        let arrays: Vec<ArrayRef> = vec![Arc::new(value.finish()), Arc::new(unit.finish())];
        let out: ArrayRef = Arc::new(StructArray::new(
            quantity_struct_fields(),
            arrays,
            Some(NullBuffer::from(valid)),
        ));
        RecordBatch::try_new(params.output_schema.clone(), vec![out])
            .map_err(|e| RpcError::runtime_error(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrow_io::test_support::{bound_type, run_scalar_text};
    use arrow_array::cast::AsArray;
    use arrow_array::types::Float64Type;
    use arrow_array::Array;
    use vgi::arguments::Arguments;

    #[test]
    fn binds_the_struct_it_builds() {
        assert_eq!(
            bound_type(&ParseQuantity),
            DataType::Struct(quantity_struct_fields())
        );
    }

    #[test]
    fn parses_known_forms() {
        let out = run_scalar_text(
            &ParseQuantity,
            &[
                Some("5 km"),
                Some("3.2kg"),
                Some("10 m/s"),
                Some("nope"),
                None,
            ],
            Arguments::default(),
        )
        .unwrap();
        let s = out.as_struct();
        let val = s.column(0).as_primitive::<Float64Type>();
        let unit = s.column(1).as_string::<i32>();

        assert!(!out.is_null(0));
        assert_eq!(val.value(0), 5.0);
        assert_eq!(unit.value(0), "km");

        assert_eq!(val.value(1), 3.2);
        assert_eq!(unit.value(1), "kg");

        assert_eq!(val.value(2), 10.0);
        assert_eq!(unit.value(2), "m/s");

        assert!(out.is_null(3), "unparseable unit → NULL struct row");
        assert!(out.is_null(4), "NULL input → NULL struct row");
    }
}
