<p align="center">
  <img src="docs/vgi-logo.png" alt="Vector Gateway Interface (VGI)" width="320">
</p>

<p align="center"><em>A <a href="https://query.farm">Query.Farm</a> VGI worker for DuckDB.</em></p>

# vgi-units

A [VGI](https://query.farm) worker that brings **runtime, string-driven physical
unit conversion** and **dimensional analysis** to DuckDB over Apache Arrow.

```sql
LOAD vgi;
ATTACH 'units' (TYPE vgi, LOCATION './target/release/units-worker');
SET search_path = 'units.main';

SELECT convert(1, 'mi', 'km');        -- 1.609344
SELECT convert(0, 'C', 'F');          -- 32.0
SELECT convert(1, 'GiB', 'byte');     -- 1073741824.0
SELECT dimension('mi');               -- 'length'
SELECT compatible('mi', 'kg');        -- false
SELECT to_base(1, 'km');              -- 1000.0   (SI base unit of the dimension)
SELECT (parse_quantity('5 km')).*;    -- (value := 5.0, unit := 'km')
SELECT * FROM supported_units();      -- discovery: unit, dimension, base_unit
```

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
| `dimension` | `dimension(unit VARCHAR) -> VARCHAR` | `'length'`, `'mass'`, …; NULL if unknown |
| `compatible` | `compatible(a VARCHAR, b VARCHAR) -> BOOLEAN` | Same dimension? Unknown unit is never compatible |
| `parse_quantity` | `parse_quantity(text VARCHAR) -> STRUCT(value DOUBLE, unit VARCHAR)` | Parses `"5 km"`, `"3.2kg"`, `"10 m/s"`; NULL if unparseable/unknown |
| `units_version` | `units_version() -> VARCHAR` | Worker version |

Table function:

| Function | Columns |
| --- | --- |
| `supported_units()` | `unit VARCHAR, dimension VARCHAR, base_unit VARCHAR` |

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

Written by [Query.Farm](https://query.farm) — every VGI worker is designed and built by Query.Farm.

Copyright 2026 Query Farm LLC - https://query.farm

