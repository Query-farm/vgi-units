//! Library surface of the `units` VGI worker.
//!
//! The binary (`main.rs`) is the actual worker; this `lib` target exposes the
//! pure conversion engine so integration tests under `tests/` can exercise it
//! directly, without Arrow or RPC. See [`units`] for the engine.

pub mod units;
