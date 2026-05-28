# Feature Specification: Workspace Auth and Multi-Agent Isolation

**Feature Branch**: `035-multi-agent-isolation`  
**Created**: 2026-04-19  
**Amended**: 2026-05-27  
**Status**: Approved  
**Version**: 1.1.0  
**Input**: Approved product decisions for workspace-scoped authentication, authorization, runtime grants, and audit logging.

## Purpose

This spec defines the workspace authentication and multi-agent isolation model for Traverse v0.

Traverse must support multiple agents, apps, users, CI jobs, and local browser sessions without allowing one caller to read, mutate, or execute another workspace's registry objects. The isolation boundary is `workspace_id`.

This spec is coordinated with:

- `033-http-json-api`
- `034-programmatic-registration`
- `030-security-identity-model`
- `031-supply-chain-hardening`
- `040-contractual-enforcement-gate`

## Scope

In scope:

- workspace identity and isolation
- dev-loopback auth mode
- bearer-required production mode
- OIDC-style bearer JWT identity
- scope-based authorization
- runtime grants
- local dev token discovery
- audit logging for registration, runtime grants, and auth failures

Out of scope:

- JWT issuance service
- external IdP configuration details
- billing/quotas
- federation
- full admin UI
- persisted workspace grants in v0

## Identity Model

Traverse uses an OIDC-style bearer JWT as the canonical caller identity format for non-dev modes.

Rules:

- Tokens are secrets and MUST NOT be exported to telemetry, trace output, or audit logs.
- The runtime MUST derive `subject_id` and `actor_id` from validated identity claims.
- Callers MUST NOT provide trusted `subject_id` or `actor_id` fields directly in request bodies.
- Telemetry and audit logs MAY include stable non-secret subject/actor identifiers and token hashes/references where useful.

## Workspace Model

- Every runtime, registry, trace, event subscription, and audit operation is scoped to a `workspace_id`.
- `workspace_id` is explicit in production URLs.
- Local dev may use `local-default` when omitted, as governed by `033-http-json-api`.
- The registry key space includes `workspace_id`.
- A module compromise is assumed to affect only the workspace plus explicitly delegated resources available to that execution/session.

## Auth Modes

### `dev-loopback`

Used only for loopback local development.

Rules:

- May allow local requests without a user-provided token.
- SHOULD mint a local ephemeral bearer token.
- MUST write local discovery metadata to `.traverse/server.json`.
- If the discovery file contains a token, it MUST be owner-read/write only (`0600`) on Unix-like systems.
- Scopes are optional in dev-loopback mode.
- Responses SHOULD expose effective scopes so clients can test scoped flows locally.

### `bearer-required`

Used for production or non-loopback bindings.

Rules:

- Requires `Authorization: Bearer <token>`.
- Missing/invalid credentials return `401`.
- Valid credentials without required scope return `403`.
- Production/non-loopback CORS must use exact configured origins.

## Scope-Based Authorization

v0 uses operation-specific scopes.

Minimum scopes:

- `workspace:read`
- `runtime:execute`
- `runtime:trace:read`
- `registry:read`
- `registry:write`
- `events:subscribe`
- `grants:approve`

Roles MAY be introduced later as a convenience layer over scopes, but v0 enforcement is scope-based.

## Runtime Grants

Modules declare static manifest permissions. Runtime grants may add temporary delegated access.

Supported v0 grant lifetimes:

- `execution`
- `session`

Persisted workspace grants are out of scope for v0.

Runtime grant rules:

- Static manifest permissions define the maximum ordinary permission envelope.
- Runtime grants require explicit approval by a caller with `grants:approve`.
- Runtime grants MUST be auditable.
- Runtime grants MUST include lifetime, granted scope/resource, approver identity, and expiration.
- Runtime grants MUST NOT silently persist beyond their declared lifetime.

## User Scenarios and Testing

### User Story 1 - Isolate Workspaces (Priority: P1)

As a platform operator, I want one agent's workspace to be invisible to another unauthorized agent so that concurrent agents cannot interfere with each other.

**Independent Test**: Register the same capability id in two workspaces; verify each workspace resolves only its own registration and unauthorized reads return `403` without leaking metadata.

### User Story 2 - Support Local Browser Development (Priority: P1)

As a local app developer, I want dev-loopback mode to be easy while still letting my app test bearer-token behavior.

**Independent Test**: Start `traverse-cli serve` in dev-loopback mode; verify `.traverse/server.json` contains a local token, has owner-only permissions, and `/healthz` reports `auth_mode: "dev-loopback"`.

### User Story 3 - Require Production Authorization (Priority: P1)

As an operator, I want non-loopback bindings to require bearer auth and scopes so that exposed servers do not run with unsafe defaults.

**Independent Test**: Start the server with a non-loopback binding; verify missing credentials return `401` and missing scopes return `403`.

### User Story 4 - Delegate Temporary Access Safely (Priority: P2)

As an app user, I want to approve a runtime grant for a specific execution or session so a module can access a delegated resource without broad workspace permissions.

**Independent Test**: Approve an execution-scoped grant, verify the module can use it during the execution, and verify it is unavailable after the execution ends.

## Audit Log

v0 audit logs are workspace-local append-only JSONL files.

Required audited operations:

- registration attempts and outcomes
- runtime grant creation/use/revocation/expiry
- auth failures

Audit entries MUST NOT contain bearer tokens or raw secrets.

At minimum, audit entries MUST include:

- timestamp
- workspace_id
- event_type
- subject_id or actor_id when available
- effective scopes when relevant
- target resource when relevant
- outcome
- traverse_code for failures

## Errors

Error response shape is governed by `033-http-json-api`.

Required mappings:

- Missing/invalid credentials: `401`, `traverse_code: "unauthenticated"`
- Valid credentials without scope: `403`, `traverse_code: "unauthorized"`
- Unauthorized workspace access: `403`, `traverse_code: "unauthorized_workspace"`
- Invalid workspace id: `422`, `traverse_code: "workspace_id_invalid"`
- Missing workspace id in production: `400`, `traverse_code: "workspace_id_required"`

## Functional Requirements

- **FR-001**: Every registry, runtime, trace, event subscription, and audit operation MUST be workspace-scoped.
- **FR-002**: Production/non-loopback requests MUST require bearer auth.
- **FR-003**: Dev-loopback mode MAY mint a local ephemeral bearer token.
- **FR-004**: Dev-loopback discovery files containing tokens MUST use owner-read/write permissions on Unix-like systems.
- **FR-005**: v0 authorization MUST be scope-based.
- **FR-006**: v0 MUST define the minimum scopes listed in this spec.
- **FR-007**: Dev-loopback mode MAY make scopes optional but SHOULD expose effective scopes in responses.
- **FR-008**: The runtime MUST derive identity from validated credentials and MUST NOT trust caller-supplied identity fields.
- **FR-009**: Workspace access requires the required scope for the operation.
- **FR-010**: Unauthorized workspace access MUST return `403` without leaking workspace metadata.
- **FR-011**: Module blast radius MUST be limited to the workspace plus explicitly delegated resources.
- **FR-012**: Modules MUST declare static manifest permissions.
- **FR-013**: Runtime grants MUST support `execution` and `session` lifetimes.
- **FR-014**: Runtime grants MUST require approval by a caller with `grants:approve`.
- **FR-015**: Runtime grants MUST be audited.
- **FR-016**: Registration attempts, runtime grants, and auth failures MUST write audit log entries.
- **FR-017**: Audit logs MUST be workspace-local append-only JSONL files.
- **FR-018**: Audit logs MUST NOT contain bearer tokens or raw secrets.

## Quality Gates

- **QG-001**: Cross-workspace registry leakage is a blocking defect.
- **QG-002**: Non-loopback unauthenticated access is a blocking defect.
- **QG-003**: Missing audit entries for registration, runtime grants, or auth failures are blocking defects.
- **QG-004**: Bearer tokens appearing in traces, telemetry, or audit logs are blocking security defects.
- **QG-005**: Runtime grants that outlive their declared lifetime are blocking defects.

## Success Criteria

- **SC-001**: Two workspaces can register the same artifact id/version without visibility or mutation conflicts.
- **SC-002**: Non-loopback requests without valid bearer auth return `401`.
- **SC-003**: Valid credentials without required scopes return `403`.
- **SC-004**: Dev-loopback mode produces usable local discovery metadata with safe token permissions.
- **SC-005**: Execution-scoped grants expire when execution ends.
- **SC-006**: Session-scoped grants expire when the session ends.
- **SC-007**: Registration, runtime grant, and auth failure audit entries are written as JSONL.
