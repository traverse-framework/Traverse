#!/usr/bin/env bash

set -euo pipefail

required_files=(
  "specs/README.md"
  "specs/033-http-json-api/context.md"
  "specs/034-programmatic-registration/context.md"
  "specs/035-multi-agent-isolation/context.md"
)

for file in "${required_files[@]}"; do
  test -f "$file"
  test -s "$file"
done

grep -q "split, focused governing specs" specs/README.md
grep -q "033-http-json-api" specs/README.md
grep -q "034-programmatic-registration" specs/README.md
grep -q "035-multi-agent-isolation" specs/README.md
grep -q "029-integrated-observability" specs/README.md
grep -q "043-module-dependency-management" specs/README.md

grep -q "#387" specs/033-http-json-api/context.md
grep -q "#390" specs/033-http-json-api/context.md
grep -q "#396" specs/033-http-json-api/context.md
grep -q "Problem Details" specs/033-http-json-api/context.md
grep -q "openapi.yaml" specs/033-http-json-api/context.md

grep -q "#397" specs/034-programmatic-registration/context.md
grep -q "#400" specs/034-programmatic-registration/context.md
grep -q "All-or-nothing" specs/034-programmatic-registration/context.md
grep -q "session_ephemeral" specs/034-programmatic-registration/context.md

grep -q "#401" specs/035-multi-agent-isolation/context.md
grep -q "#403" specs/035-multi-agent-isolation/context.md
grep -q "workspace_id" specs/035-multi-agent-isolation/context.md
grep -q "grants:approve" specs/035-multi-agent-isolation/context.md

echo "Spec context check passed."
