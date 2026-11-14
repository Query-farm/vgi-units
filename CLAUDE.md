# CLAUDE.md — vgi-units

Contributor/agent notes. User-facing docs live in `README.md`; this is the
"how it's built and where the sharp edges are" companion.

## What this is

A [VGI](https://query.farm) worker (Rust, compiled binary) exposing **runtime,
string-driven physical unit conversion** and **dimensional analysis** to
DuckDB/SQL over Arrow IPC. Built on the `vgi` crate (crates.io), modeled on
`vgi-image` / `vgi-barcode`. Catalog name `units` (single `main` schema).

The conversion engine is a **curated static table** — no external units crate.
The need is runtime (`convert(5,'mi','km')`), so the compile-time-typed `uom`
crate is the wrong fit; a string→(dimension, factor, offset) table is the
clearest fit and lets us document every factor's source.

## Layout

```
Cargo.toml                          workspace; pins vgi = "0.51.0", arrow 59.2
crates/units-worker/
  src/main.rs                       Worker::new(); registers scalars + table
  src/lib.rs                        lib target re-exporting `units` for integration tests
  src/units.rs                      PURE engine (no Arrow): table + convert/to_base/dimension/parse + unit tests
  src/arrow_io.rs                   VARCHAR/DOUBLE cell reads + STRUCT type + in-process scalar test harness
  src/scalar/{convert,analysis,parse,strict,mod}.rs   thin Arrow scalar adapters
  src/table/{supported,unit_catalog,mod}.rs   thin Arrow table-producer adapters
  src/examples.rs                   composed example queries (defined once; schema + per-object lists)
  src/unit_catalog_metadata.rs      deterministic `units.catalog_v1` JSON + its SHA-256 (strict scalar tag)
  examples/export_unit_catalog.rs   prints that JSON byte-exactly
  tests/conversions.rs              integration tests against known constants
test/sql/*.test                     haybarn-unittest sqllogictest — authoritative E2E
Makefile                            test / test-unit / test-sql / lint / fmt / build / clean
```

Pattern: keep computation in `units.rs` (pure, unit-tested), keep Arrow
marshalling in `arrow_io.rs` + `scalar/*.rs` + `table/*.rs` (thin, harness-tested).

## The conversion model

Each unit string maps to `(Dimension, factor, offset)`. Conversion is an affine
round-trip through the SI base unit of the dimension:

```text
base = value * from.factor + from.offset
out  = (base - to.offset) / to.factor
```

`offset` is zero for everything except the temperature scales (°C/°F/K/°R), which
need it to get 0 °C = 273.15 K = 32 °F right. `to_base` is just the first line.

Factors are the exact SI/agreed constants (yard/inch 0.0254 m, pound
0.45359237 kg, atm 101325 Pa, cal 4.184 J, IEC binary 1024ⁿ vs decimal 1000ⁿ);
sources are documented in the `units.rs` module doc.

## Sharp edges

1. **`haybarn-unittest` skips `require vgi`** — `.test` files use explicit
   `statement ok` + `LOAD vgi;`. Functions live under the `units` catalog, so
   each file does `SET search_path = 'units.main'`, then `USE memory` before
   `DETACH units`. Determinism: numeric assertions use `ROUND(...)`.

2. **NULL-vs-error policy (deliberate).** `convert`/`to_base` return **NULL** for
   an *unknown* unit (treat dirty data as missing, don't abort a scan) but raise
   a DuckDB **ERROR** for an *incompatible dimension* (km → kg) — both units are
   valid, the request is nonsensical, so fail loudly. The split lives in
   `scalar/convert.rs`, matching on `UnitError::{UnknownUnit, Incompatible}`.

3. **STRUCT return type must match bind↔process.** `parse_quantity` returns
   `STRUCT(value DOUBLE, unit VARCHAR)`; the exact `Fields` are produced once by
   `arrow_io::quantity_struct_fields()` and used in both `on_bind`
   (`BindResponse::result(Struct(...))`) and `process` (the `StructArray::new`
   call), with a `NullBuffer` so unparseable/NULL rows are NULL structs.

4. **Scalars are positional-only.** `convert(value, from, to)` reads positional
   columns 0/1/2; no named args, no arity overloads here. The value column is
   read via `arrow_io::double_val` (accepts any DuckDB numeric width, since a
   literal `convert(5, …)` arrives as INTEGER, not DOUBLE).

5. **Case-sensitivity.** Lookup is exact-first, then a lowercase fallback. This
   keeps case-significant units working (e.g. `mi` mile) while still accepting
   `KM` → `km`. Unit strings like `m^2`, `m/s`, `°C` are stored verbatim.

6. **bin + lib both compile `units.rs`.** `main.rs` has `mod units;` (the binary
   copy) and `lib.rs` re-exports `pub mod units;` for `tests/`. `unit_count` is
   only used by the lib/tests, so it carries `#[allow(dead_code)]`.

## Testing

```sh
cargo test --workspace --all-features    # pure unit + arrow-boundary harness + integration
cargo clippy --all-targets --all-features -- -D warnings && cargo fmt --all -- --check
make test-sql                            # builds release, sets VGI_UNITS_WORKER, haybarn over test/sql/*
make test                                # cargo test + sql
```

CI (`.github/workflows/ci.yml`) runs fmt/clippy/build/test plus a gated
`e2e-sql` job (installs `uv` + `haybarn-unittest`, runs `make test-sql`).

## Function surface

Scalars: `convert` (DOUBLE), `to_base` (DOUBLE), `convert_strict` /
`convert_difference_strict` (DOUBLE; every invalid input is an ERROR, NULL
handling SPECIAL so a NULL value still validates its units), `dimension`
(VARCHAR), `compatible` (BOOLEAN), `parse_quantity` (STRUCT(value, unit)).
Tables: `supported_units` (unit/dimension/base_unit), `unit_catalog`
(+ quantity_space/factor/offset/implementation_version/contract_version;
`offset` is a SQL keyword, quote it). Tag values must stay under vgi-lint's
4,000-char bound (VGI417), which is why the strict scalars publish only the
catalog JSON's SHA-256, not the JSON. The worker version is catalog
metadata (`implementation_version`), not a scalar. 14 dimensions, 300 unit
strings.
