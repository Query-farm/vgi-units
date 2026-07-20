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
use vgi::{ArgSpec, BindParams, BindResponse, FunctionMetadata, ProcessParams, ScalarFunction};
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
            tags: {
                let mut tags = crate::meta::object_tags(
                    "Convert Units",
                    "Convert a numeric value from one unit to another unit of the same physical \
                     dimension, e.g. miles to kilometres, pounds to kilograms, or °C to °F. \
                     Returns NULL when either unit string is unknown, and raises an error when \
                     the two units belong to incompatible dimensions (e.g. km to kg).",
                    "Convert a value between two units of the same dimension, e.g. \
                     `convert(26.2, 'mi', 'km')`.",
                    "convert, conversion, unit conversion, change units, miles to km, pounds to \
                     kg, celsius to fahrenheit, length, mass, temperature, scale",
                    "Conversion",
                    "scalar/convert.rs",
                );
                tags.push((
                    "vgi.example_queries".into(),
                    crate::meta::example_queries_json(&[
                        (
                            "Convert a marathon distance from miles to kilometres.",
                            "SELECT units.main.convert(26.2, 'mi', 'km') AS km",
                        ),
                        (
                            "Convert a body temperature from Celsius to Fahrenheit (affine, \
                             offset-aware).",
                            "SELECT units.main.convert(37, '°C', '°F') AS fahrenheit",
                        ),
                    ]),
                ));
                tags
            },
            ..Default::default()
        }
    }

    fn argument_specs(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::any_column(
                "value",
                0,
                "The quantity to convert, expressed in the source unit (e.g. 26.2 for a marathon \
                 in miles).",
            ),
            ArgSpec::column_typed(
                "from_unit",
                1,
                DataType::Utf8,
                "The unit the value is currently in, e.g. 'mi'. Must be a unit string the worker \
                 recognizes (see supported_units); an unknown unit yields NULL.",
            ),
            ArgSpec::column_typed(
                "to_unit",
                2,
                DataType::Utf8,
                "The unit to convert the value into, e.g. 'km'. Must share a dimension with \
                 from_unit, otherwise the call errors.",
            ),
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
            tags: {
                let mut tags = crate::meta::object_tags(
                    "Convert to SI Base Unit",
                    "Express a numeric value in the SI base unit of its dimension, e.g. \
                     centimetres to metres, grams to kilograms, or GiB to bytes. Returns NULL \
                     when the unit string is unknown.",
                    "Express a value in the SI base unit of its dimension, e.g. \
                     `to_base(100, 'cm')` → 1.0 (metres).",
                    "to_base, base unit, SI, normalize, canonical unit, metres, kilograms, bytes, \
                     normalise units",
                    "Conversion",
                    "scalar/convert.rs",
                );
                tags.push((
                    "vgi.example_queries".into(),
                    crate::meta::example_queries_json(&[
                        (
                            "Express 100 centimetres in the SI base unit (metres).",
                            "SELECT units.main.to_base(100, 'cm') AS metres",
                        ),
                        (
                            "Reduce 1 GiB to bytes so mixed data sizes can be summed on a common \
                             base.",
                            "SELECT units.main.to_base(1, 'GiB') AS bytes",
                        ),
                    ]),
                ));
                tags
            },
            ..Default::default()
        }
    }

    fn argument_specs(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::any_column(
                "value",
                0,
                "The quantity to express in its SI base unit, given in the unit named by `unit` \
                 (e.g. 100 for 100 centimetres).",
            ),
            ArgSpec::column_typed(
                "unit",
                1,
                DataType::Utf8,
                "The unit the value is given in, e.g. 'GiB'. Must be recognized by the worker; an \
                 unknown unit yields NULL.",
            ),
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
    fn convert_accepts_decimal_value() {
        // DuckDB types a bare literal like `26.2` as DECIMAL(3,1), not DOUBLE, so
        // `double_val` must accept Decimal128 (and round-trip the scale).
        use arrow_array::Decimal128Array;
        let v: ArrayRef = Arc::new(
            Decimal128Array::from(vec![262i128])
                .with_precision_and_scale(3, 1)
                .unwrap(),
        );
        let f: ArrayRef = Arc::new(StringArray::from(vec![Some("mi")]));
        let t: ArrayRef = Arc::new(StringArray::from(vec![Some("km")]));
        let schema = Arc::new(Schema::new(vec![
            Field::new("value", v.data_type().clone(), true),
            Field::new("from", DataType::Utf8, true),
            Field::new("to", DataType::Utf8, true),
        ]));
        let batch = RecordBatch::try_new(schema.clone(), vec![v, f, t]).unwrap();
        let bind = BindParams {
            input_schema: Some(schema),
            ..Default::default()
        };
        let bound = Convert.on_bind(&bind).unwrap();
        let params = process_params(bound.output_schema, Arguments::default());
        let out = Convert.process(&params, &batch).unwrap();
        let d = out.column(0).as_primitive::<Float64Type>();
        close(d.value(0), 26.2 * 1.609344);
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
