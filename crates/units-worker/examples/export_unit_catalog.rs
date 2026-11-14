//! Export the exact `units.catalog_v1` JSON bytes (no trailing newline) without
//! starting an RPC worker. Its SHA-256 equals the strict scalars'
//! `units.catalog_v1_sha256` tag.
fn main() {
    print!("{}", units_worker::unit_catalog_json());
}
