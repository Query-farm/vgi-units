//! `units_version()` — return the worker's version string.

use std::sync::Arc;

use arrow_array::{ArrayRef, RecordBatch, StringArray};
use arrow_schema::DataType;
use vgi::{
    ArgSpec, BindParams, BindResponse, FunctionExample, FunctionMetadata, ProcessParams,
    ScalarFunction,
};
use vgi_rpc::{Result, RpcError};

pub struct UnitsVersion;

impl ScalarFunction for UnitsVersion {
    fn name(&self) -> &str {
        "units_version"
    }

    fn metadata(&self) -> FunctionMetadata {
        FunctionMetadata {
            description: "Returns the units worker version string".into(),
            return_type: Some(DataType::Utf8),
            examples: vec![FunctionExample {
                sql: "SELECT units.main.units_version();".into(),
                description: "Return the units worker version string.".into(),
                expected_output: None,
            }],
            tags: crate::meta::object_tags(
                "Units Worker Version",
                "Return the semantic version string of the running units worker binary. Useful for \
                 diagnostics and confirming which build is attached.",
                "Return the units worker version string, e.g. `units_version()` → '0.1.0'.",
                "version, build version, units_version, diagnostics, worker version, semver",
                "scalar/version.rs",
            ),
            ..Default::default()
        }
    }

    fn argument_specs(&self) -> Vec<ArgSpec> {
        Vec::new()
    }

    fn on_bind(&self, _params: &BindParams) -> Result<BindResponse> {
        Ok(BindResponse::result(DataType::Utf8))
    }

    fn process(&self, params: &ProcessParams, batch: &RecordBatch) -> Result<RecordBatch> {
        let rows = batch.num_rows();
        let out: ArrayRef = Arc::new(StringArray::from(vec![crate::version(); rows]));
        RecordBatch::try_new(params.output_schema.clone(), vec![out])
            .map_err(|e| RpcError::runtime_error(e.to_string()))
    }
}
