#!/usr/bin/env bash

set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "${repo_root}"

cargo test -p traverse-registry --test application_manifest \
  loads_checked_in_application_manifest_with_real_wasm_component
cargo test -p traverse-registry --test application_manifest \
  registers_application_bundle_atomically_with_created_status
cargo test -p traverse-registry --test application_manifest \
  application_registration_exposes_only_non_sensitive_effective_config
cargo test -p traverse-registry --test application_manifest \
  application_registration_records_model_readiness_evidence

echo "downstream app bundle registration smoke passed."
