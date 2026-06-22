#!/usr/bin/env bash

set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "${repo_root}"

cargo test -p traverse-cli execute_endpoint_returns_completed_trace_on_success
cargo test -p traverse-cli trace_fetch_endpoint_returns_public_trace_envelope
cargo test -p traverse-cli trace_fetch_endpoint_does_not_expose_internal_runtime_trace_fields

echo "downstream HTTP/JSON smoke passed."
