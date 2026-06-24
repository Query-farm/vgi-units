# vgi-units worker — dev, test, and lint targets.
#
# Usage:
#   make test         # cargo unit/integration tests + SQL E2E (all transports)
#   make test-unit    # cargo test --workspace (pure-Rust + Arrow-boundary tests)
#   make test-sql     # build the release worker, run the DuckDB sqllogictest suite
#                     #   over every transport (subprocess, http, unix)
#   make test-sql-subprocess / test-sql-http / test-sql-unix   # one transport
#   make lint         # clippy (deny warnings) + rustfmt --check
#   make fmt          # rustfmt the workspace
#
# The SQL E2E suite drives the compiled worker through DuckDB via
# `haybarn-unittest` (install with: `uv tool install haybarn-unittest`).

# Path to the released worker binary. Used directly as the subprocess ATTACH
# LOCATION and launched in --http / --unix mode for those transports.
WORKER         ?= $(CURDIR)/target/release/units-worker
# DuckDB sqllogictest runner (haybarn-unittest; on PATH after `uv tool install`).
SQL_RUNNER     ?= haybarn-unittest

.PHONY: test test-unit test-sql test-sql-subprocess test-sql-http test-sql-unix lint fmt build clean

# Full local gate: everything CI runs.
test: test-unit test-sql

# Pure-Rust unit + integration tests (includes the in-process Arrow-boundary
# tests for the scalar dispatch/marshalling layer).
test-unit:
	cargo test --workspace --all-features

# Build the release worker, then run the SQL E2E suite over every transport
# (matching CI's matrix). The worker is a compiled binary, so build first.
test-sql: test-sql-subprocess test-sql-http test-sql-unix

test-sql-subprocess: build
	HAYBARN_UNITTEST="$(SQL_RUNNER)" WORKER_BIN="$(WORKER)" TRANSPORT=subprocess ci/run-integration.sh

test-sql-http: build
	HAYBARN_UNITTEST="$(SQL_RUNNER)" WORKER_BIN="$(WORKER)" TRANSPORT=http ci/run-integration.sh

test-sql-unix: build
	HAYBARN_UNITTEST="$(SQL_RUNNER)" WORKER_BIN="$(WORKER)" TRANSPORT=unix ci/run-integration.sh

# clippy (warnings are errors) + formatting check.
lint:
	cargo clippy --all-targets --all-features -- -D warnings
	cargo fmt --all -- --check

fmt:
	cargo fmt --all

build:
	cargo build --release --bin units-worker

clean:
	cargo clean
