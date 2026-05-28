# Implementation Context: Programmatic Registration API

## Owns

- Registration behavior for capabilities, event contracts, workflows, and bundles.
- Workspace-scoped persistence semantics.
- `workspace_persisted` and `session_ephemeral` registration scopes.
- Idempotent same-digest re-registration.
- Different-digest conflicts.
- All-or-nothing bundle registration.
- Pre-storage validation requirement.

## Does Not Own

- HTTP envelope, path versioning, CORS, and Problem Details shape; use `033-http-json-api`.
- Auth scopes and audit storage mechanics; use `035-multi-agent-isolation`.
- Dependency resolution; use `043-module-dependency-management`.
- Contract enforcement policy; use `040-contractual-enforcement-gate`.

## Key Invariants

- Invalid artifacts must never be persisted, cached as active registrations, or made executable.
- Same id/version/digest is idempotent success.
- Same id/version with a different digest returns `409 Conflict`.
- Validation failure returns `422`.
- Bundle registration validates every artifact before writing any artifact.
- Partial bundle success is not allowed in v0.
- Registration mutations should support `Idempotency-Key` as defined by `033-http-json-api`.

## Dependencies

- Capability registry: `005-capability-registry`.
- Event registry: `011-event-registry`.
- Workflow registry/traversal: `007-workflow-registry-traversal`.
- HTTP transport: `033-http-json-api`.
- Workspace isolation and audit: `035-multi-agent-isolation`.

## Implementation Tickets

- `#397` implements capability registration.
- `#398` implements event contract registration.
- `#399` implements workflow registration.
- `#400` implements atomic bundle registration.
