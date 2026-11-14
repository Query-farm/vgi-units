# Unit Conversion

[01 · One report, mixed units](01_one_report_mixed_units.ipynb) combines a synthetic
three-site sampling round without losing invalid rows or confusing absolute
temperatures with temperature changes. Visible SQL handles discovery, parsing,
validation, conversion and aggregation; three SVG figures explain the results.

Open the notebook's **Open in Colab** link, or use [uv](https://docs.astral.sh/uv/):

```sh
cd notebooks
uv sync --locked
uv run --locked vgi-notebooks
```

A notebook downloaded on its own retrieves its verified plotting companion.
Colab installs the shared base package and preserves compatible preloaded kernel
libraries. The local lock includes the runner; select `notebooks/.venv/bin/python`
in a notebook editor. Restart and run all after changing a parameter.

The visible connection uses the native release:

```sql
INSTALL vgi FROM community;
LOAD vgi;
ATTACH 'units' (TYPE vgi,
  LOCATION 'github-auto://Query-farm/vgi-units@v0.4.0');
```

Initial setup needs network access. Calculations then use authored SQL inputs;
no Rust compiler, credentials or local worker source is needed. Execution is
verified on macOS arm64. The notebook-only bootstrap is checked with libraries
already imported in a compatible local kernel; hosted Colab itself was not
manually run. GitHub displays saved tables and sharp SVG figures.

## Change the reporting scale

`REPORT_TEMP_UNIT` accepts `"°C"`, `"°F"` or `"K"`. The saved default is Fahrenheit.
Keep it intact by executing the Kelvin experiment into a separate directory:

```sh
uv run --locked vgi-notebooks --parameters '{"REPORT_TEMP_UNIT":"K"}' \
  --output-dir artifacts/kelvin
```

Pressure, volume and input eligibility stay fixed. Absolute temperatures change
scale, while temperature rises keep their physical size and ordering. The two
panels in the offset comparison use independently labeled vertical ranges so
Kelvin's large absolute offset does not flatten the difference curve.

## Inputs and interpretation

All seventeen input rows are visible SQL literals: twelve operating measurements
and five diagnostic controls. They describe one hypothetical sampling round with
no acquisition time. The pressure reference is assumed consistent; converting
units cannot reconcile gauge and absolute pressure. Volumes are samples, not flow
rates, because no elapsed time is provided. `gal` means the worker's US liquid gallon.

The input audit retains missing, unparsed and incompatible readings. Accepted
pairs are materialized before conversion. Unknown units can produce NULL in this
release; incompatible recognized dimensions produce an error. Ten relationship
checks require complete accepted results, preserve the input population, verify
round trips and reconcile two ways of computing temperature differences.

Release v0.4.0 provides all **6/6 SQL surfaces demonstrated here**. Current public
source adds `convert_strict`, `convert_difference_strict` and `unit_catalog`, which
are absent from the latest binary release. Current-interface coverage is therefore
**6/9**, with those three explicitly uncovered. The notebook cancels affine offsets
using the released conversion functions; it does not pretend to call the newer API.
Complete catalogs, mappings and execution receipts are in [coverage.json](coverage.json)
and [coverage/](coverage/). No current-main runtime execution is claimed.

The shared [notebook standard](https://github.com/Query-farm/vgi-example-notebooks/blob/main/docs/standard.md)
owns branding, setup, execution and SVG publication.
