//! The `units` VGI worker.
//!
//! A standalone binary that DuckDB launches and talks to over Apache Arrow IPC
//! (`ATTACH 'units' (TYPE vgi, LOCATION '…')`). It brings runtime, string-driven
//! physical-unit conversion and dimensional analysis to SQL under the catalog
//! `units`, schema `main`:
//!
//! ```sql
//! ATTACH 'units' (TYPE vgi, LOCATION './target/release/units-worker');
//! SET search_path = 'units.main';
//!
//! SELECT convert(5, 'mi', 'km');          -- 8.04672
//! SELECT dimension('mi');                  -- 'length'
//! SELECT compatible('mi', 'kg');           -- false
//! SELECT to_base(1, 'GiB');                -- 1073741824
//! SELECT parse_quantity('5 km').*;         -- (5.0, 'km')
//! SELECT * FROM supported_units();         -- discovery
//! ```
//!
//! The pure conversion engine (a curated runtime unit table) lives in `units.rs`;
//! the `scalar/` and `table/` modules are thin Arrow adapters over it.

mod arrow_io;
mod scalar;
mod table;
mod units;

use vgi::Worker;

/// Worker version string, surfaced by `units_version()`.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

fn main() {
    // Logs MUST go to stderr — stdout is the Arrow-IPC channel.
    let _ = env_logger::Builder::from_env(env_logger::Env::default().filter_or("VGI_LOG", "info"))
        .format_timestamp_millis()
        .try_init();

    // The catalog name DuckDB sees in `ATTACH 'units' (TYPE vgi, …)`. Default to
    // `units`, but honor an explicit override so a test harness can rename it.
    if std::env::var_os("VGI_WORKER_CATALOG_NAME").is_none() {
        std::env::set_var("VGI_WORKER_CATALOG_NAME", "units");
    }

    let mut worker = Worker::new();
    scalar::register(&mut worker);
    table::register(&mut worker);
    worker.run();
}
