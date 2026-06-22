#!/usr/bin/env bash

set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"

test -s "${repo_root}/docs/app-consumable-consumer-bundle.md"
grep -q "supported version selection" "${repo_root}/docs/app-consumable-consumer-bundle.md"
grep -q "installation steps" "${repo_root}/docs/app-consumable-consumer-bundle.md"
grep -q "browser-targeted consumer package" "${repo_root}/docs/app-consumable-consumer-bundle.md"

bash "${repo_root}/scripts/ci/app_consumable_release_prep.sh"
bash "${repo_root}/scripts/ci/react_demo_live_adapter_smoke.sh"
bash "${repo_root}/scripts/ci/mcp_consumption_validation.sh"
bash "${repo_root}/scripts/ci/youaskm3_integration_validation.sh"
bash "${repo_root}/scripts/ci/downstream_app_mvp_conformance.sh"

echo "youaskm3 compatibility conformance suite passed."
