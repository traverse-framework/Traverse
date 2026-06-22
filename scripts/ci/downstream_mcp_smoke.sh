#!/usr/bin/env bash

set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "${repo_root}"

cargo test -p traverse-mcp validates_youaskm3_mcp_consumption_path
cargo test -p traverse-mcp mcp_observation_report_exposes_model_resolution_evidence

echo "downstream MCP smoke passed."
