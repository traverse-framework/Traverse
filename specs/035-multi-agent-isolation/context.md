# Implementation Context: Workspace Auth and Multi-Agent Isolation

## Owns

- Workspace identity and isolation.
- `dev-loopback` and `bearer-required` auth modes.
- OIDC-style bearer JWT identity interpretation.
- Scope-based authorization.
- Runtime grants.
- Local dev token discovery requirements.
- Workspace-local append-only JSONL audit log requirements.

## Does Not Own

- HTTP endpoint envelope and CORS mechanics; use `033-http-json-api`.
- Registration mutation semantics; use `034-programmatic-registration`.
- Artifact provenance and signing; use `031-supply-chain-hardening`.
- Broader trust model direction; use `030-security-identity-model`.

## Key Invariants

- `workspace_id` is the isolation boundary for runtime, registry, trace, event subscription, and audit operations.
- Non-dev bindings require `Authorization: Bearer <token>`.
- Missing or invalid credentials return `401`.
- Valid credentials without required scope return `403`.
- Tokens must not appear in telemetry, trace output, or audit logs.
- Callers must not provide trusted `subject_id` or `actor_id` in request bodies.
- v0 enforcement is scope-based, not role-based.
- Runtime grants are temporary only in v0: `execution` or `session`.
- Runtime grants require approval by a caller with `grants:approve`.

## Minimum Scopes

- `workspace:read`.
- `runtime:execute`.
- `runtime:trace:read`.
- `registry:read`.
- `registry:write`.
- `events:subscribe`.
- `grants:approve`.

## Implementation Tickets

- `#401` implements workspace scopes and bearer authorization.
- `#402` implements runtime grants.
- `#403` implements workspace audit JSONL logs.
