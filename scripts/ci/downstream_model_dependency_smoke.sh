#!/usr/bin/env bash

set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "${repo_root}"

cargo test -p traverse-registry --test application_manifest \
  loads_valid_governed_model_dependency_schema
cargo test -p traverse-runtime --test inference_tests \
  ollama_model_resolution_selects_available_candidate_at_setup
cargo test -p traverse-runtime --test inference_tests \
  ollama_model_resolution_revalidates_at_execution_time
cargo test -p traverse-runtime --test inference_tests \
  ollama_model_resolution_reports_unsatisfied_dependency
cargo test -p traverse-runtime --test trace_tests \
  public_trace_entry_exposes_redacted_model_resolution_evidence

if [[ "${TRAVERSE_RUN_LOCAL_OLLAMA_CONFORMANCE:-0}" == "1" ]]; then
  cargo test -p traverse-runtime --test inference_tests \
    ollama_provider_generates_real_response_through_local_http_endpoint
else
  echo "Skipping local Ollama conformance; set TRAVERSE_RUN_LOCAL_OLLAMA_CONFORMANCE=1 to require a reachable local provider."
fi

echo "downstream model dependency smoke passed."
