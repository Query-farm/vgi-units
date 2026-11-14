//! Fixed-signature conversions for complete governed results. Existing lenient
//! conversion functions retain their compatibility contract.

use std::sync::Arc;

use arrow_array::builder::Float64Builder;
use arrow_array::{ArrayRef, RecordBatch};
use arrow_schema::DataType;
use vgi::{ArgSpec, BindParams, BindResponse, FunctionMetadata, ProcessParams, ScalarFunction};
use vgi_rpc::{Result, RpcError};

use crate::arrow_io::{double_val, text_str};
use crate::units;

pub struct StrictConvert {
    pub difference: bool,
}

impl ScalarFunction for StrictConvert {
    fn name(&self) -> &str {
        if self.difference {
            "convert_difference_strict"
        } else {
            "convert_strict"
        }
    }

    fn metadata(&self) -> FunctionMetadata {
        let (description, title, llm, md, keywords, examples): (
            &str,
            &str,
            &str,
            &str,
            &str,
            [(&str, &str); 2],
        ) = if self.difference {
            (
                "Convert a finite quantity difference using scale factors without offsets; reject \
                 invalid units, nonfinite values and overflow; preserve valid NULL values",
                "Strict Convert Difference",
                "Convert a quantity difference (an interval or change, e.g. a 10 °C rise) from \
                 one unit to another unit of the same dimension using only the scale factors, so \
                 temperature offsets cancel: a 10 °C change is an 18 °F change. Strict: unknown, \
                 NULL or incompatible units, NaN/infinite inputs and overflowing results raise an \
                 error instead of returning NULL, so a successful query is a complete result. A \
                 NULL value with valid compatible units returns NULL.",
                "Strictly convert a quantity *difference* (offsets cancel), e.g. \
                 `convert_difference_strict(10, '°C', '°F')` → 18. Invalid units or nonfinite \
                 values raise an error.",
                "convert_difference_strict, strict conversion, difference, delta, interval, \
                 temperature change, governed, complete results, unit conversion",
                [
                    (
                        "Convert a 10 degree Celsius temperature rise into Fahrenheit degrees.",
                        "SELECT units.main.convert_difference_strict(10, '°C', '°F') AS delta_f",
                    ),
                    (
                        "Convert a 5 km difference in distance into miles.",
                        "SELECT units.main.convert_difference_strict(5, 'km', 'mi') AS delta_mi",
                    ),
                ],
            )
        } else {
            (
                "Convert a finite absolute quantity using scale factors and offsets; reject \
                 invalid units, nonfinite values and overflow; preserve valid NULL values",
                "Strict Convert Units",
                "Convert an absolute quantity from one unit to another unit of the same \
                 dimension (offset-aware, so 0 °C becomes 32 °F). Strict: unlike convert, \
                 unknown, NULL or incompatible units, NaN/infinite inputs and overflowing \
                 results raise an error instead of returning NULL, so a successful query is a \
                 complete result. A NULL value with valid compatible units returns NULL.",
                "Strictly convert an absolute quantity, e.g. \
                 `convert_strict(26.2, 'mi', 'km')`. Invalid units or nonfinite values raise an \
                 error instead of yielding NULL.",
                "convert_strict, strict conversion, governed, complete results, validated, \
                 unit conversion, miles to km, celsius to fahrenheit",
                [
                    (
                        "Strictly convert a marathon distance from miles to kilometres.",
                        "SELECT units.main.convert_strict(26.2, 'mi', 'km') AS km",
                    ),
                    (
                        "Strictly convert the freezing point of water from Celsius to \
                         Fahrenheit (offset-aware).",
                        "SELECT units.main.convert_strict(0, '°C', '°F') AS fahrenheit",
                    ),
                ],
            )
        };
        let mut tags =
            crate::meta::object_tags(title, llm, md, keywords, "Conversion", "scalar/strict.rs");
        tags.push((
            "vgi.example_queries".into(),
            crate::meta::example_queries_json(&examples),
        ));
        tags.push(("units.contract_version".into(), "1".into()));
        tags.push((
            "units.implementation_version".into(),
            crate::version().into(),
        ));
        tags.push((
            "units.catalog_v1_sha256".into(),
            crate::unit_catalog_sha256().into(),
        ));
        tags.push((
            "units.quantity_kind".into(),
            if self.difference {
                "difference"
            } else {
                "absolute"
            }
            .into(),
        ));
        FunctionMetadata {
            description: description.into(),
            return_type: Some(DataType::Float64),
            // SPECIAL: NULL inputs are still dispatched, because a NULL value
            // must not mask an invalid or NULL unit (which errors).
            null_handling: Some(vgi::protocol::enums::null_handling::SPECIAL.into()),
            tags,
            ..Default::default()
        }
    }

    fn argument_specs(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::column_typed(
                "value",
                0,
                DataType::Float64,
                "The finite quantity to convert, in the source unit. NULL yields NULL (after the \
                 units are validated); NaN or infinity raises an error.",
            ),
            ArgSpec::column_typed(
                "from_unit",
                1,
                DataType::Utf8,
                "The unit the value is in, e.g. 'mi'. Must be a recognized unit string (see \
                 supported_units or unit_catalog); unknown or NULL raises an error.",
            ),
            ArgSpec::column_typed(
                "to_unit",
                2,
                DataType::Utf8,
                "The unit to convert into, e.g. 'km'. Must be recognized and share a dimension \
                 with from_unit; otherwise the call raises an error.",
            ),
        ]
    }

    fn on_bind(&self, params: &BindParams) -> Result<BindResponse> {
        let schema = params
            .input_schema
            .as_ref()
            .ok_or_else(|| RpcError::value_error("Strict conversion requires an input schema"))?;
        let expected = [DataType::Float64, DataType::Utf8, DataType::Utf8];
        if schema.fields().len() != expected.len()
            || schema
                .fields()
                .iter()
                .zip(expected.iter())
                .any(|(field, ty)| field.data_type() != ty)
        {
            return Err(RpcError::value_error(
                "Strict conversion requires DOUBLE, VARCHAR, VARCHAR inputs",
            ));
        }
        Ok(BindResponse::result(DataType::Float64))
    }

    fn process(&self, params: &ProcessParams, batch: &RecordBatch) -> Result<RecordBatch> {
        if batch.num_columns() != 3 || batch.column(0).data_type() != &DataType::Float64 {
            return Err(RpcError::value_error(
                "Strict conversion requires DOUBLE, VARCHAR, VARCHAR inputs",
            ));
        }
        let mut out = Float64Builder::new();
        for i in 0..batch.num_rows() {
            // Unit validity is independent of whether the quantity is NULL.
            // Missing or invalid units must never become a successful NULL result.
            let from = text_str(batch.column(1), i)?
                .ok_or_else(|| RpcError::value_error("Source unit is NULL"))?;
            let to = text_str(batch.column(2), i)?
                .ok_or_else(|| RpcError::value_error("Target unit is NULL"))?;
            let f = units::lookup(from).ok_or_else(|| {
                RpcError::value_error(format!("Unknown source unit '{}'", from.trim()))
            })?;
            let t = units::lookup(to).ok_or_else(|| {
                RpcError::value_error(format!("Unknown target unit '{}'", to.trim()))
            })?;
            if f.dimension != t.dimension {
                return Err(RpcError::value_error(format!(
                    "Incompatible unit dimensions: '{}' is {}, '{}' is {}",
                    from.trim(),
                    f.dimension.name(),
                    to.trim(),
                    t.dimension.name()
                )));
            }
            let Some(value) = double_val(batch.column(0), i)? else {
                out.append_null();
                continue;
            };
            if !value.is_finite() {
                return Err(RpcError::value_error("Nonfinite conversion input"));
            }
            let converted = if self.difference {
                units::convert_difference(value, from, to)
            } else {
                units::convert(value, from, to)
            }
            .map_err(|e| RpcError::value_error(e.to_string()))?;
            if !converted.is_finite() {
                return Err(RpcError::value_error(
                    "Nonfinite conversion result or overflow",
                ));
            }
            out.append_value(converted);
        }
        let arr: ArrayRef = Arc::new(out.finish());
        RecordBatch::try_new(params.output_schema.clone(), vec![arr])
            .map_err(|e| RpcError::runtime_error(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrow_io::test_support::process_params;
    use arrow_array::{Array, Float64Array, StringArray};
    use arrow_schema::{Field, Schema};
    use vgi::arguments::Arguments;

    #[test]
    fn both_strict_functions_expose_identical_authoritative_catalog_tags() {
        for difference in [false, true] {
            let metadata = StrictConvert { difference }.metadata();
            let tags: std::collections::HashMap<_, _> = metadata.tags.into_iter().collect();
            assert_eq!(
                tags["units.catalog_v1_sha256"],
                crate::unit_catalog_sha256()
            );
            assert!(tags.values().all(|v| v.len() <= 4000));
            assert_eq!(tags["units.implementation_version"], crate::version());
            assert_eq!(tags["units.contract_version"], "1");
            assert_eq!(
                tags["units.quantity_kind"],
                if difference { "difference" } else { "absolute" }
            );
        }
    }

    fn run(
        difference: bool,
        values: Vec<Option<f64>>,
        from: Vec<Option<&str>>,
        to: Vec<Option<&str>>,
    ) -> Result<ArrayRef> {
        let schema = Arc::new(Schema::new(vec![
            Field::new("value", DataType::Float64, true),
            Field::new("from_unit", DataType::Utf8, true),
            Field::new("to_unit", DataType::Utf8, true),
        ]));
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(Float64Array::from(values)),
                Arc::new(StringArray::from(from)),
                Arc::new(StringArray::from(to)),
            ],
        )
        .unwrap();
        let function = StrictConvert { difference };
        let bound = function.on_bind(&BindParams {
            input_schema: Some(schema),
            ..Default::default()
        })?;
        Ok(function
            .process(
                &process_params(bound.output_schema, Arguments::default()),
                &batch,
            )?
            .column(0)
            .clone())
    }

    #[test]
    fn strict_linear_absolute_difference_and_null() {
        for (difference, value, source, target, expected) in [
            (false, 1.0, "mi", "km", 1.609344),
            (false, -2.0, "kg", "g", -2000.0),
            (false, 0.0, "C", "F", 32.0),
            (false, 32.0, "F", "K", 273.15),
            (true, 10.0, "C", "F", 18.0),
            (true, -18.0, "F", "K", -10.0),
        ] {
            let out = run(
                difference,
                vec![Some(value), None],
                vec![Some(source); 2],
                vec![Some(target); 2],
            )
            .unwrap();
            let result = out.as_any().downcast_ref::<Float64Array>().unwrap();
            assert!((result.value(0) - expected).abs() < 1e-9);
            assert!(result.is_null(1));
        }
    }

    #[test]
    fn invalid_units_reject_even_null_values() {
        for difference in [false, true] {
            for value in [None, Some(1.0)] {
                for (from, to) in [
                    (Some("unknown"), Some("m")),
                    (Some("m"), Some("unknown")),
                    (Some("m"), Some("kg")),
                    (None, Some("m")),
                    (Some("m"), None),
                ] {
                    assert!(run(difference, vec![value], vec![from], vec![to]).is_err());
                }
            }
        }
    }

    #[test]
    fn nonfinite_and_overflow_reject() {
        for difference in [false, true] {
            for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, f64::MAX] {
                assert!(run(
                    difference,
                    vec![Some(value)],
                    vec![Some("km")],
                    vec![Some("m")]
                )
                .is_err());
            }
        }
    }

    #[test]
    fn mixed_units_large_batch_preserves_order_and_nulls() {
        let n = 8193;
        let out = run(
            false,
            (0..n)
                .map(|i| if i % 7 == 0 { None } else { Some(i as f64) })
                .collect(),
            (0..n)
                .map(|i| Some(if i % 2 == 0 { "m" } else { "cm" }))
                .collect(),
            vec![Some("m"); n],
        )
        .unwrap();
        let result = out.as_any().downcast_ref::<Float64Array>().unwrap();
        assert_eq!(result.len(), n);
        for i in 0..n {
            if i % 7 == 0 {
                assert!(result.is_null(i));
            } else {
                assert!(
                    (result.value(i) - i as f64 * if i % 2 == 0 { 1.0 } else { 0.01 }).abs() < 1e-9
                );
            }
        }
    }
}
