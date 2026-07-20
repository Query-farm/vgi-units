//! Scalar functions exposed by the units worker, registered under `units.main`.

mod analysis;
mod convert;
mod parse;

use vgi::Worker;

/// Register every scalar function on the worker.
pub fn register(worker: &mut Worker) {
    worker.register_scalar(convert::Convert);
    worker.register_scalar(convert::ToBase);
    worker.register_scalar(analysis::DimensionFn);
    worker.register_scalar(analysis::Compatible);
    worker.register_scalar(parse::ParseQuantity);
}
