# CI: the vgi-units worker integration suite

[`.github/workflows/ci.yml`](../.github/workflows/ci.yml) runs fmt/clippy/build,
the Rust unit + integration tests, and this repo's sqllogictest suite
(`test/sql/*.test`) against the vgi-units VGI worker through the **real DuckDB
`vgi` extension** on every push / PR.

## How it works (no C++ build)

Rather than building the vgi DuckDB extension from source, the integration job
drives a **prebuilt** standalone `haybarn-unittest` (the DuckDB/Haybarn
sqllogictest runner, published in Haybarn's releases) and installs the
**signed** `vgi` extension from the Haybarn community channel:

1. **Build the worker** — `cargo build --release --bin units-worker`. The
   compiled `target/release/units-worker` is a self-contained stdio worker the
   extension spawns (the `.test` files `ATTACH` it via `${VGI_UNITS_WORKER}`).
2. **Download the runner** — the matching `haybarn_unittest-*` asset per
   platform from the latest Haybarn release.
3. **Preprocess** — the standalone runner links none of the extensions the
   tests gate on, so [`preprocess-require.awk`](preprocess-require.awk) rewrites
   each `require <ext>` into an explicit signed `INSTALL <ext> FROM
   {community,core}; LOAD <ext>;`. `require-env` and everything else pass
   through untouched. (The vgi-units `.test` files already `LOAD vgi;`
   explicitly and use `require-env VGI_UNITS_WORKER`, so the rewrite is a
   no-op here, but the awk is kept verbatim for parity with the other workers.)
4. **Run** — [`run-integration.sh`](run-integration.sh) stages the preprocessed
   tree, points `VGI_UNITS_WORKER` at the release binary, warms the extension
   cache once (`INSTALL vgi FROM community;` — this is what makes the tests'
   explicit `LOAD vgi;` succeed), then runs the suite in a single
   `haybarn-unittest` invocation. Any failed assertion exits non-zero and fails
   the job.

## Run it locally

```bash
cargo build --release --bin units-worker
# point HAYBARN_UNITTEST at a haybarn-unittest binary (or a local DuckDB
# `unittest` built with the vgi extension), and the worker at the release binary:
HAYBARN_UNITTEST=/path/to/haybarn-unittest \
VGI_UNITS_WORKER="$PWD/target/release/units-worker" \
  ci/run-integration.sh
```

Or use the Makefile target `make test-sql`, which builds the release worker and
runs the suite against a `haybarn-unittest` on `PATH` (`uv tool install
haybarn-unittest`).
