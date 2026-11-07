#!/usr/bin/env bash
# PX-168: the product-code lint policy. No unwrap or expect, no lock held
# across an await, no raw process spawn outside a file that owns spawning.
# Test code is exempt through tools/lint-policy/clippy.toml only.
set -euo pipefail
cd "$(dirname "$0")/.."
CLIPPY_CONF_DIR="$PWD/tools/lint-policy" cargo clippy --workspace --lib --bins --examples --locked -- \
  -D clippy::unwrap_used -D clippy::expect_used \
  -D clippy::await_holding_lock -D clippy::await_holding_refcell_ref \
  -D clippy::disallowed_methods
