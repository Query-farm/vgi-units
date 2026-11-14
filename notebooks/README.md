# vgi-units notebooks

<a href="https://query.farm/"><img src="assets/query-farm-strata-sun.svg" alt="Query.Farm — Strata Sun" width="252"></a>

<a href="https://query.farm/vgi/"><img src="assets/vgi-logo.png" alt="VGI — Vector Gateway Interface" width="280"></a>

[Query.Farm](https://query.farm/) · [VGI](https://query.farm/vgi/) · [Haybarn](https://query.farm/products/haybarn/)

| Notebook | Investigation |
|---|---|
| [01 · One report, mixed units](01_one_report_mixed_units.ipynb) | Validate a synthetic three-site sampling round, discover compatible units, normalize operating measurements, and distinguish absolute temperatures from temperature rises. |

The saved notebook includes successful execution outputs and three embedded charts.
Python handles setup, presentation and assertions. Visible JupySQL `%%sql` cells
connect Haybarn to the actual **Rust worker** and perform discovery, parsing,
validation, conversions and aggregation. All measurements are explicitly synthetic,
defined in the notebook's SQL; there is no external dataset, API or replay mode.
The single hypothetical sampling round has no acquisition timestamp or time window.

## Install and run

Prerequisite: `uv`. Download or copy this entire **notebooks/** directory, then
run from it:

```sh
uv sync --locked
uv run --locked python execute.py
```

Select `.venv/bin/python` in a Jupyter-compatible editor and **Restart Kernel →
Run All**. The notebook environment supports Python 3.13–3.14 and pins published
Haybarn **2.0.0rc11**. No Rust compiler, native build or worker checkout is needed.
The visible SQL uses:

```sql
ATTACH 'units' (TYPE vgi, LOCATION 'github-auto://Query-farm/vgi-units@v0.4.0');
```

VGI selects the native platform archive and verifies its published SHA-256 sidecar.
The release lists Linux amd64/arm64, macOS arm64 and Windows amd64 assets.
This notebook was executed on macOS arm64; other platforms need a matching
release asset accepted by the transport. Initial package,
extension and worker downloads need network access. All calculations are local.
To upgrade, change the release pin in ATTACH, execute the default and Kelvin
experiment, and review conversion semantics and relationship checks.

Version **v0.4.0** exposes `convert`, `to_base`, `dimension`, `compatible`,
`parse_quantity` and `supported_units`. The newer strict functions and
`unit_catalog` in the development source are absent from this published binary.
This example deliberately uses the verified release surface: it audits inputs,
then subtracts a converted zero to cancel offsets for temperature differences.

Python setup, plotting, prose source and cleanup carry portable folding metadata.
Keep parameters, analytical SQL, charts and interpretations visible. The runner
uses the notebook directory as its kernel working directory and needs no globally
registered kernel.

## Change the reporting scale

Edit `REPORT_TEMP_UNIT` in the visible parameters cell and **Restart Kernel → Run
All**. Accepted exact strings are `"°C"`, `"°F"` (default), and `"K"`. Pressure
and sample volume keep their report units, while absolute temperatures and
temperature differences use the selected scale. The experiment at the end asks
you to change only this parameter to `"K"` and compare the same physical samples.

Validate the experiment without replacing the successful default notebook:

```sh
uv run --locked python execute.py \
  --parameters '{"REPORT_TEMP_UNIT":"K"}' --output-dir artifacts/kelvin
```

The runner rejects unknown parameter names, requires a separate output directory
for overrides, and creates a temporary kernel specification using the current
environment. Each execution starts a fresh kernel; a failed run leaves the saved
notebook unchanged. The runner atomically saves only after success, rejects source
overwrite destinations, and copies branding, this guide, helpers and environment
files beside experiments. The main notebook embeds all three charts; generated
logs, experiments and extracted charts under `artifacts/` are Git-ignored.

## Interpretation and checks

The examples assume pressure readings share one absolute reference and that `gal`
means the worker's US liquid gallon. A sampled volume is not a flow rate. No
timestamp, calibration, sensor precision, process efficiency or operating limit is
inferred from these inputs. Missing, unparsed and incompatible rows remain visible
in the audit and are never converted to zero. Zero is a valid temperature reading.
`parse_quantity` returns NULL for both malformed text and unknown units, so those
two cases deliberately share one audit status.

Absolute temperatures use `convert`; differences use the affine identity
`convert(delta, source, target) - convert(0, source, target)`. Unknown units return
NULL and incompatible known dimensions error, so only audited rows are converted. Assertions verify round trips, agreement with
`to_base`, cancellation of temperature offsets, unique reading identifiers,
coverage accounting and absence of conversions for rejected rows. Tolerances
reflect binary64 arithmetic; displayed digits are not sensor precision claims.
Assertions do not depend on a site's ranking.

Repository gates remain:

```sh
cargo test --locked --workspace --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
uv tool install 'haybarn-unittest==2.0.0rc11'  # if not already installed
make test-sql  # subprocess, HTTP and Unix transports
```

The small [support module](notebook_support.py) contains presentation and SQL literal
quoting; analytical SQL remains in the notebook. See the worker
[function reference](../README.md#function-surface) for NULL/error semantics and
the [conversion engine](../crates/units-worker/src/units.rs) for factor provenance.

Brand assets are unchanged copies from `vgi-perps/notebooks/assets/`. The adaptive
Strata Sun + Query.Farm wordmark originates from the official website media kit
(`logo/wordmark-adaptive.svg`); the VGI logo comes from its `products/vgi/` directory.

---

<a href="https://query.farm/"><img src="assets/query-farm-strata-sun.svg" alt="Query.Farm — Strata Sun" width="210"></a>
