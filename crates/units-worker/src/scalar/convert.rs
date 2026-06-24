//! `convert(value DOUBLE, from_unit VARCHAR, to_unit VARCHAR) -> DOUBLE` and
//! `to_base(value DOUBLE, unit VARCHAR) -> DOUBLE`.
//!
//! ## NULL-vs-error policy
//!
//! An **unknown unit** is treated as missing data → the row's result is NULL
//! (mirrors how `NULL` input flows through to a `NULL` output). An **incompatible
//! dimension** (e.g. `convert(1,'km','kg')`) is a *programming/logic* error — the
//! two units are both perfectly valid, the request is just nonsensical — so it
//! surfaces as a DuckDB ERROR rather than silently yielding NULL. This makes
//! genuinely wrong queries fail loudly while tolerating dirty/unknown unit
//! strings in data.

use std::sync::Arc;

use arrow_array::builder::Float64Builder;
use arrow_array::{ArrayRef, RecordBatch};
use arrow_schema::DataType;
use vgi::{
    ArgSpec, BindParams, BindResponse, FunctionExample, FunctionMetadata, ProcessParams,
    ScalarFunction,
};
use vgi_rpc::{Result, RpcError};

use crate::arrow_io::{double_val, text_str};
use crate::units::{self, UnitError};

pub struct Convert;

impl ScalarFunction for Convert {
    fn name(&self) -> &str {
        "convert"
    }

    fn metadata(&self) -> FunctionMetadata {
        FunctionMetadata {
            description: "Convert a value between two units of the same dimension (NULL if a unit \
                          is unknown; ERROR if the dimensions are incompatible)"
                .into(),
            return_type: Some(DataType::Float64),
            examples: vec![FunctionExample {
                sql: "SELECT units.main.convert(26.2, 'mi', 'km');".into(),
                description: "Convert a marathon distance from miles to kilometres.".into(),
                expected_output: None,
            }],
            ..Default::default()
        }
    }

    fn argument_specs(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::any_column("value", 0, "Numeric value to convert (DOUBLE)"),
            ArgSpec::any_column("from_unit", 1, "Source unit, e.g. 'mi' (VARCHAR)"),
            ArgSpec::any_column("to_unit", 2, "Target unit, e.g. 'km' (VARCHAR)"),
        ]
    }

    fn on_bind(&self, _params: &BindParams) -> Result<BindResponse> {
        Ok(BindResponse::result(DataType::Float64))
    }

    fn process(&self, params: &ProcessParams, batch: &RecordBatch) -> Result<RecordBatch> {
        let value = batch.column(0);
        let from = batch.column(1);
        let to = batch.column(2);
        let rows = batch.num_rows();
        let mut out = Float64Builder::new();
        for i in 0..rows {
            match (double_val(value, i)?, text_str(from, i)?, text_str(to, i)?) {
                (Some(v), Some(f), Some(t)) => match units::convert(v, f, t) {
                    Ok(r) => out.append_value(r),
                    // Unknown unit → NULL (treat as missing data).
                    Err(UnitError::UnknownUnit(_)) => out.append_null(),
                    // Incompatible dimensions → loud error.
                    Err(e @ UnitError::Incompatible { .. }) => {
                        return Err(RpcError::value_error(e.to_string()))
                    }
                },
                // Any NULL operand → NULL result.
                _ => out.append_null(),
            }
        }
        let arr: ArrayRef = Arc::new(out.finish());
        RecordBatch::try_new(params.output_schema.clone(), vec![arr])
            .map_err(|e| RpcError::runtime_error(e.to_string()))
    }
}

pub struct ToBase;

impl ScalarFunction for ToBase {
    fn name(&self) -> &str {
        "to_base"
    }

    fn metadata(&self) -> FunctionMetadata {
        FunctionMetadata {
            description: "Express a value in the SI base unit of its dimension (NULL if the unit \
                          is unknown)"
                .into(),
            return_type: Some(DataType::Float64),
            examples: vec![FunctionExample {
                sql: "SELECT units.main.to_base(100, 'cm');".into(),
                description: "Express 100 centimetres in the SI base unit (metres).".into(),
                expected_output: None,
            }],
            ..Default::default()
        }
    }

    fn argument_specs(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::any_column("value", 0, "Numeric value (DOUBLE)"),
            ArgSpec::any_column("unit", 1, "Unit of the value, e.g. 'GiB' (VARCHAR)"),
        ]
    }

    fn on_bind(&self, _params: &BindParams) -> Result<BindResponse> {
        Ok(BindResponse::result(DataType::Float64))
    }

    fn process(&self, params: &ProcessParams, batch: &RecordBatch) -> Result<RecordBatch> {
        let value = batch.column(0);
        let unit = batch.column(1);
        let rows = batch.num_rows();
        let mut out = Float64Builder::new();
        for i in 0..rows {
            match (double_val(value, i)?, text_str(unit, i)?) {
                (Some(v), Some(u)) => match units::to_base(v, u) {
                    Ok(r) => out.append_value(r),
                    Err(_) => out.append_null(), // unknown unit → NULL
                },
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
    use crate::arrow_io::test_support::{bound_type, process_params};
    use arrow_array::cast::AsArray;
    use arrow_array::types::Float64Type;
    use arrow_array::{Array, Float64Array, RecordBatch, StringArray};
    use arrow_schema::{Field, Schema};
    use vgi::arguments::Arguments;

    /// Build a 3-column `(value, from, to)` batch and run `convert`.
    fn run_convert(
        vals: &[Option<f64>],
        froms: &[Option<&str>],
        tos: &[Option<&str>],
    ) -> Result<ArrayRef> {
        let v: ArrayRef = Arc::new(Float64Array::from(vals.to_vec()));
        let f: ArrayRef = Arc::new(StringArray::from(froms.to_vec()));
        let t: ArrayRef = Arc::new(StringArray::from(tos.to_vec()));
        let schema = Arc::new(Schema::new(vec![
            Field::new("value", DataType::Float64, true),
            Field::new("from", DataType::Utf8, true),
            Field::new("to", DataType::Utf8, true),
        ]));
        let batch = RecordBatch::try_new(schema.clone(), vec![v, f, t]).unwrap();
        let bind = BindParams {
            input_schema: Some(schema),
            ..Default::default()
        };
        let bound = Convert.on_bind(&bind)?;
        let params = process_params(bound.output_schema, Arguments::default());
        Ok(Convert.process(&params, &batch)?.column(0).clone())
    }

    fn run_to_base(vals: &[Option<f64>], units: &[Option<&str>]) -> Result<ArrayRef> {
        let v: ArrayRef = Arc::new(Float64Array::from(vals.to_vec()));
        let u: ArrayRef = Arc::new(StringArray::from(units.to_vec()));
        let schema = Arc::new(Schema::new(vec![
            Field::new("value", DataType::Float64, true),
            Field::new("unit", DataType::Utf8, true),
        ]));
        let batch = RecordBatch::try_new(schema.clone(), vec![v, u]).unwrap();
        let bind = BindParams {
            input_schema: Some(schema),
            ..Default::default()
        };
        let bound = ToBase.on_bind(&bind)?;
        let params = process_params(bound.output_schema, Arguments::default());
        Ok(ToBase.process(&params, &batch)?.column(0).clone())
    }

    fn close(a: f64, b: f64) {
        assert!((a - b).abs() <= 1e-9 * b.abs().max(1.0), "{a} != {b}");
    }

    #[test]
    fn binds_double() {
        assert_eq!(bound_type(&Convert), DataType::Float64);
        assert_eq!(bound_type(&ToBase), DataType::Float64);
    }

    #[test]
    fn known_conversions() {
        let out = run_convert(
            &[Some(1.0), Some(0.0), Some(1.0)],
            &[Some("mi"), Some("C"), Some("GiB")],
            &[Some("km"), Some("F"), Some("byte")],
        )
        .unwrap();
        let d = out.as_primitive::<Float64Type>();
        close(d.value(0), 1.609344);
        close(d.value(1), 32.0);
        close(d.value(2), 1073741824.0);
    }

    #[test]
    fn unknown_unit_yields_null() {
        let out = run_convert(&[Some(1.0)], &[Some("frob")], &[Some("m")]).unwrap();
        assert!(out.is_null(0), "unknown unit must be NULL, not an error");
    }

    #[test]
    fn incompatible_dimension_errors() {
        let err = run_convert(&[Some(1.0)], &[Some("km")], &[Some("kg")]);
        assert!(err.is_err(), "km -> kg must error");
    }

    #[test]
    fn null_operands_yield_null() {
        let out = run_convert(&[None], &[Some("mi")], &[Some("km")]).unwrap();
        assert!(out.is_null(0));
        let out = run_convert(&[Some(1.0)], &[None], &[Some("km")]).unwrap();
        assert!(out.is_null(0));
    }

    #[test]
    fn to_base_known_and_unknown() {
        let out = run_to_base(
            &[Some(1.0), Some(0.0), Some(1.0)],
            &[Some("km"), Some("C"), Some("frob")],
        )
        .unwrap();
        let d = out.as_primitive::<Float64Type>();
        close(d.value(0), 1000.0);
        close(d.value(1), 273.15);
        assert!(out.is_null(2));
    }
}
