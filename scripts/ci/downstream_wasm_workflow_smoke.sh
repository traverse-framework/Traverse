#!/usr/bin/env bash

set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "${repo_root}"

cargo test -p traverse-runtime --test expedition_wasm_tests \
  expedition_wasm_execution_writes_trace
cargo test -p traverse-runtime --test expedition_wasm_tests \
  placement_router_routes_expedition_to_wasm_executor

echo "downstream WASM workflow smoke passed."
