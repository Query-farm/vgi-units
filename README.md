<p align="center">
  <img src="https://query.farm/media-kit/products/vgi/vgi-logo.png" alt="Vector Gateway Interface (VGI)" width="320">
</p>

<p align="center"><em>A <a href="https://query.farm">Query.Farm</a> VGI worker for DuckDB.</em></p>

# Physical Unit Conversion & Dimensional Analysis in DuckDB

> **vgi-units** · a [Query.Farm](https://query.farm) VGI worker

A [VGI](https://query.farm) worker that brings **runtime, string-driven physical
unit conversion** and **dimensional analysis** to DuckDB over Apache Arrow.
Convert a value between any two units given as plain strings
(`convert(1, 'mi', 'km')`), parse quantities out of text (`'5 km'`), ask a unit's
dimension, and check whether two units are compatible — across **14 physical
dimensions** (length, mass, time, temperature, data, energy, pressure, speed, …)
and ~300 unit strings. Units resolve at *runtime* from a curated table, so
neither side has to be known when the query is written; an incompatible
conversion (km → kg) is a clear error rather than a wrong number.

```sql
LOAD vgi;
ATTACH 'units' (TYPE vgi, LOCATION 'github-auto://Query-farm/vgi-units@v0.4.0');
SET search_path = 'units.main';

SELECT convert(1, 'mi', 'km');        -- 1.609344
SELECT convert(0, 'C', 'F');          -- 32.0
SELECT convert(1, 'GiB', 'byte');     -- 1073741824.0
SELECT dimension('mi');               -- 'length'
SELECT compatible('mi', 'kg');        -- false
SELECT to_base(1, 'km');              -- 1000.0   (SI base unit of the dimension)
SELECT (parse_quantity('5 km')).*;    -- (value := 5.0, unit := 'km')
SELECT * FROM supported_units();      -- discovery: unit, dimension, base_unit
SELECT convert_strict(0, '°C', '°F'); -- 32.0     (invalid input raises an ERROR)
SELECT convert_difference_strict(10, '°C', '°F'); -- 18.0 (a temperature change)
SELECT * FROM unit_catalog;           -- factors, offsets and versions
```

## Quick start

Every [GitHub release](https://github.com/Query-farm/vgi-units/releases) ships a prebuilt worker binary. The `vgi` extension downloads the one for your platform, checks its SHA-256 and caches it, so there is nothing to install besides DuckDB:

```sql
INSTALL vgi FROM community;
LOAD vgi;

ATTACH 'units' AS units (TYPE vgi, LOCATION 'github-auto://Query-farm/vgi-units@v0.4.0');

-- Convert a marathon distance from miles to kilometres.
SELECT units.main.convert(26.2, 'mi', 'km') AS km;

-- Reduce 1 GiB to bytes via the SI base unit.
SELECT units.main.to_base(1, 'GiB') AS bytes;
```

Prebuilt binaries cover macOS (Apple silicon), Linux (x86-64 and arm64) and Windows (x86-64). `github-auto://` needs an explicit release tag; change `@v0.4.0` to move to a newer release.

### Other ways to run it

- **Docker** (the image bundles the worker; needs a container runtime):

  ```sql
  ATTACH 'units' AS units (TYPE vgi, LOCATION 'docker://ghcr.io/query-farm/vgi-units:0.4.0');
  ```

- **Manual download.** Download the archive for your platform from the [releases page](https://github.com/Query-farm/vgi-units/releases), verify it against its `.sha256` file (a cosign `.cosign.bundle` is published alongside), unpack it, and set `LOCATION` to the path of the extracted worker executable.

### Build from source

To build the worker yourself, see [Development](#development).

## Why a runtime engine (not `uom`)

The need here is *runtime* conversion driven by arbitrary strings —
`convert(5, 'mi', 'km')` — where neither unit is known at compile time. The
`uom` crate encodes units in the type system, which is the wrong fit. Instead the
worker ships a curated static table mapping each unit string to its
[`Dimension`], a linear `factor` (how many SI base units one of this unit
equals), and an additive `offset` (non-zero only for the temperature scales).

Conversion within a dimension is an affine round-trip through the SI base unit:

```text
base = value * from.factor + from.offset      // value -> SI base unit
out  = (base - to.offset) / to.factor         // SI base unit -> target
```

The offset path is what makes °C / °F / K correct (0 °C = 273.15 K = 32 °F);
everything else is purely multiplicative.

## Function surface

Scalars (positional-only):

| Function | Signature | Notes |
| --- | --- | --- |
| `convert` | `convert(value DOUBLE, from VARCHAR, to VARCHAR) -> DOUBLE` | Unknown unit → **NULL**; incompatible dimension (km → kg) → **ERROR** |
| `to_base` | `to_base(value DOUBLE, unit VARCHAR) -> DOUBLE` | Value in the SI base unit of its dimension; unknown unit → NULL |
| `convert_strict` | `convert_strict(value DOUBLE, from VARCHAR, to VARCHAR) -> DOUBLE` | Absolute quantities; invalid or NULL units, nonfinite values and overflow → **ERROR** |
| `convert_difference_strict` | `convert_difference_strict(value DOUBLE, from VARCHAR, to VARCHAR) -> DOUBLE` | Quantity differences; uses scale factors without temperature offsets; same strict validation |
| `dimension` | `dimension(unit VARCHAR) -> VARCHAR` | `'length'`, `'mass'`, …; NULL if unknown |
| `compatible` | `compatible(a VARCHAR, b VARCHAR) -> BOOLEAN` | Same dimension? Unknown unit is never compatible |
| `parse_quantity` | `parse_quantity(text VARCHAR) -> STRUCT(value DOUBLE, unit VARCHAR)` | Parses `"5 km"`, `"3.2kg"`, `"10 m/s"`; NULL if unparseable/unknown |

The worker's software version is exposed as catalog metadata
(`implementation_version`, read via `vgi_catalogs()`) rather than as a scalar
function.

Table function:

| Function | Columns |
| --- | --- |
| `supported_units()` | `unit VARCHAR, dimension VARCHAR, base_unit VARCHAR` |
| `unit_catalog()` | `unit VARCHAR, dimension VARCHAR, base_unit VARCHAR, quantity_space VARCHAR, factor DOUBLE, offset DOUBLE, implementation_version VARCHAR, contract_version VARCHAR` |

Both are also exposed as plain tables (`SELECT * FROM units.main.unit_catalog`).
`offset` is a SQL keyword, so quote it when selecting it by name: `"offset"`.

### Strict reporting contract (version 1)

The strict functions accept a fixed `DOUBLE, VARCHAR, VARCHAR` input schema.
They validate both unit strings and compatibility even when the value is NULL.
A NULL value with valid compatible units produces NULL; missing, unknown or
incompatible units raise an error. Nonfinite input and nonfinite output,
including arithmetic overflow, raise an error. Finite negative values are
allowed. An error aborts the entire batch; no partial successful batch is
returned.

Absolute temperature conversion includes offsets: 0 °C becomes 32 °F.
Temperature differences omit offsets: a 10 °C change becomes an 18 °F change.
The caller must choose the quantity kind correctly. Converting absolute
temperatures does not make their sum meaningful.

`unit_catalog()` exposes the authoritative engine definitions for administrative
approval: `base = value * factor + offset`, or `base_difference = value * factor`.
The result includes implementation and contract versions and has one row per
exact recognized unit string. Pin the worker artifact as well as its version
when reproducibility is required. Lookup also trims whitespace and accepts a
lowercase fallback when exact lookup fails; a governed application may restrict
inputs to an explicitly approved subset of exact strings.

The same definitions are available as the `units.catalog_v1` JSON document: a
complete compact JSON object with `version: 1`, `implementation_version`, and
`units`. Each unit row has `unit`, `dimension`, `base_unit`, `quantity_space`,
`factor`, and `offset`. Rows are sorted by unit UTF-8 bytes, preserving case-significant
aliases (`B`/`b`, `MB`/`Mb`). The catalog contains finite positive factors and
finite offsets and is bounded to 4096 rows and 256 KiB. Its definitions come
from the existing conversion engine. All temperature units, including
zero-offset Kelvin and Rankine, belong to an `affine` quantity space; other
dimensions are `linear`.

Each strict scalar carries `units.implementation_version`,
`units.contract_version`, `units.quantity_kind` (`absolute` / `difference`) and
`units.catalog_v1_sha256` tags. The last is the lower-case hex SHA-256 of the
exact `units.catalog_v1` JSON bytes; the document itself (about 34 KB) is too
large for a function tag, so a governed host verifies an exported copy against
the digest without executing `unit_catalog()` or conversion work. Export the
exact JSON bytes, without a trailing newline, using
`cargo run -q --example export_unit_catalog`.

Results are IEEE binary64, including when a caller first casts decimal inputs
to DOUBLE. This is not an exact decimal conversion capability. Acceptance
examples use absolute tolerance `1e-9 * max(1, abs(expected))`; applications must
choose an appropriate tolerance for their magnitude and reporting requirements.
No guarantee of relative accuracy near a cancellation point is implied.

### NULL-vs-error policy

An **unknown unit** is treated as missing data and yields **NULL** (so dirty
data doesn't abort a scan). An **incompatible-dimension** conversion is a genuine
logic error — both units are valid, the request is nonsensical — so it raises a
DuckDB **ERROR**.

## Dimensions and units

14 dimensions: **length, mass, time, temperature, area, volume, speed, pressure,
energy, power, data, angle, frequency, force**. 300 distinct unit strings
(including aliases such as `km`/`kilometer`/`kilometre`, `lb`/`lbs`/`pound`, and
SI prefixes), each mapping to one of those dimensions.

### Source of factors

Factors are the exact SI definitions and internationally agreed constants: the
international yard/inch (1 in = 0.0254 m, 1959/1981), avoirdupois pound
(0.45359237 kg), standard atmosphere (101325 Pa), thermochemical calorie
(4.184 J), IEC binary prefixes (Ki/Mi/Gi = 1024ⁿ) vs decimal (k/M/G = 1000ⁿ),
and so on. See the module docs in `crates/units-worker/src/units.rs`.

## Development

```sh
make test       # cargo unit/integration tests + SQL E2E
make test-unit  # cargo test --workspace
make test-sql   # build release worker + DuckDB sqllogictest suite (haybarn-unittest)
make lint       # clippy (deny warnings) + rustfmt --check
make fmt        # rustfmt the workspace
```

The SQL E2E suite uses [`haybarn-unittest`](https://query.farm)
(`uv tool install haybarn-unittest`).

## License

MIT — see [LICENSE](LICENSE).

---

## Authorship & License

Written by [Query.Farm](https://query.farm).

Copyright 2026 Query Farm LLC - https://query.farm

