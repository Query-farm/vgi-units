//! Table functions exposed by the units worker, registered under `units.main`.

mod supported;

use vgi::Worker;

/// Register every table function on the worker.
pub fn register(worker: &mut Worker) {
    worker.register_table(supported::SupportedUnits);
}
