# Feature Specification: Programmatic Registration API

**Feature Branch**: `034-programmatic-registration`  
**Created**: 2026-04-19  
**Amended**: 2026-05-27  
**Status**: Approved  
**Version**: 1.1.0  
**Input**: Approved product decisions for programmatic registration of capabilities, event contracts, workflows, and bundles.

## Purpose

This spec defines the programmatic registration API for Traverse v0.

Apps and agents need to register governed artifacts without shelling out to CLI commands. Registration must be reliable, idempotent, workspace-scoped, and strict enough that invalid artifacts never enter the runtime registry.

This spec is coordinated with:

- `033-http-json-api`
- `035-multi-agent-isolation`
- `005-capability-registry`
- `011-event-registry`
- `007-workflow-registry-traversal`
- `040-contractual-enforcement-gate`

## Scope

In scope:

- capability registration
- event contract registration
- workflow registration
- bundle registration
- workspace-scoped persistence
- session-ephemeral registration scope
- registration outcome envelopes
- idempotent duplicate handling
- all-or-nothing bundle transactions
- pre-storage validation
- audit logging requirements

Out of scope:

- artifact deletion/deregistration
- remote artifact fetching
- dependency resolution (governed by `043-module-dependency-management`)
- federation
- registry UI

## Required Endpoints

The HTTP surface is governed by `033-http-json-api`; this spec governs registration behavior for these endpoints:

- `POST /v1/workspaces/{workspace_id}/capabilities`
- `POST /v1/workspaces/{workspace_id}/event-contracts`
- `POST /v1/workspaces/{workspace_id}/workflows`
- `POST /v1/workspaces/{workspace_id}/bundles`

Separate endpoints are required for the three artifact types so validation and authorization stay clear. Bundle upload is also required so package/install flows can remain ergonomic.

## User Scenarios and Testing

### User Story 1 - Register Artifacts Programmatically (Priority: P1)

As an agent or application, I want to register capabilities, event contracts, and workflows through stable HTTP endpoints so that runtime setup can be automated.

**Independent Test**: Submit valid capability, event contract, and workflow registration requests in one workspace; verify each returns a registration outcome envelope and is available to runtime/discovery logic without CLI registration.

**Acceptance Scenarios**:

1. **Given** a valid capability contract, **When** it is submitted to `/capabilities`, **Then** the registry stores it and returns a registration outcome envelope.
2. **Given** a valid event contract, **When** it is submitted to `/event-contracts`, **Then** the event registry stores it and returns a registration outcome envelope.
3. **Given** a valid workflow definition, **When** it is submitted to `/workflows`, **Then** the workflow registry stores it and returns a registration outcome envelope.

### User Story 2 - Retry Safely (Priority: P1)

As an agent, I want registration to be idempotent so retries after crashes or network failures do not create conflicts when the artifact is unchanged.

**Independent Test**: Submit the same artifact twice. Verify the second response has `already_registered: true`; submit the same id/version with a different digest and verify `409 Conflict`.

### User Story 3 - Install Bundles Atomically (Priority: P1)

As an app maintainer, I want bundle registration to be all-or-nothing so that partially installed capability sets cannot leave the workspace in an inconsistent state.

**Independent Test**: Submit a bundle containing one valid artifact and one invalid artifact; verify the API returns an error and no artifacts from the bundle are stored.

### User Story 4 - Support Ephemeral Local Automation (Priority: P2)

As an automation agent, I want `session_ephemeral` registration for one-off capabilities that should disappear when the server session ends.

**Independent Test**: Register an artifact with `scope: "session_ephemeral"`; verify it is available in the current process and absent after restart.

## Registration Request Model

Every registration request is scoped by URL path `workspace_id`.

Request bodies MUST reject unknown fields.

Supported registration scopes:

- `workspace_persisted` (default)
- `session_ephemeral`

`workspace_persisted` entries survive restart. `session_ephemeral` entries are process/session-scoped and MUST NOT be written to persistent registry storage.

## Registration Outcome Envelope

Successful registration MUST return a registration outcome envelope:

```json
{
  "api_version": "v1",
  "registered": true,
  "already_registered": false,
  "artifact_type": "capability",
  "artifact_id": "summarize-note",
  "version": "1.2.3",
  "digest": "sha256:abc123",
  "scope": "workspace_persisted",
  "links": {
    "self": "/v1/workspaces/local-default/capabilities/summarize-note/1.2.3",
    "execute": "/v1/workspaces/local-default/execute"
  }
}
```

Re-registration with the same digest MUST return success with:

```json
{
  "api_version": "v1",
  "registered": false,
  "already_registered": true,
  "artifact_type": "capability",
  "artifact_id": "summarize-note",
  "version": "1.2.3",
  "digest": "sha256:abc123",
  "links": {
    "self": "/v1/workspaces/local-default/capabilities/summarize-note/1.2.3"
  }
}
```

## Validation and Conflicts

- Traverse MUST validate before storing.
- Invalid artifacts MUST be rejected.
- Invalid artifacts MUST NOT be persisted, cached as active registrations, or made executable.
- Same id/version/digest is idempotent success.
- Same id/version with different digest is `409 Conflict`.
- Contract/schema validation failure is `422`.
- Bundle registration is all-or-nothing.
- Failed bundle registration MUST leave the registry in its pre-request state.

## Bundle Registration

`POST /v1/workspaces/{workspace_id}/bundles`

Bundle registration MUST:

- validate every artifact first
- compute every digest deterministically
- detect internal duplicate/conflicting ids before writing
- stage writes before commit
- commit all artifacts atomically
- roll back all staged writes if any validation or conflict fails

Partial success is not allowed in v0.

## Idempotency-Key

Registration endpoints are mutation endpoints and SHOULD support `Idempotency-Key` as specified by `033-http-json-api`.

If the same `Idempotency-Key` is reused with a different request body, the API MUST return `409 Conflict` with Problem Details.

## Audit Log

Every successful or failed registry mutation attempt MUST emit a workspace-local append-only JSONL audit log entry as governed by `035-multi-agent-isolation`.

At minimum, audit entries MUST include:

- timestamp
- workspace_id
- actor_id or subject_id (non-secret)
- artifact_type
- artifact_id when parseable
- version when parseable
- digest when computed
- outcome (`registered`, `already_registered`, `conflict`, `validation_failed`)
- traverse_code for failures

## Functional Requirements

- **FR-001**: The API MUST expose separate endpoints for capabilities, event contracts, workflows, and bundles.
- **FR-002**: The API MUST validate every artifact before storing.
- **FR-003**: Invalid artifacts MUST be rejected and MUST NOT be stored.
- **FR-004**: Registration MUST be workspace-scoped by URL path.
- **FR-005**: `workspace_persisted` MUST be the default scope.
- **FR-006**: `session_ephemeral` MUST be supported for current-session registrations.
- **FR-007**: Same id/version/digest MUST return idempotent success.
- **FR-008**: Same id/version with different digest MUST return `409 Conflict`.
- **FR-009**: Contract/schema validation failures MUST return `422`.
- **FR-010**: Successful registration MUST return a registration outcome envelope.
- **FR-011**: Registration outcomes MUST include digest evidence.
- **FR-012**: Registration outcomes MUST include stable links for next actions.
- **FR-013**: Bundle registration MUST be all-or-nothing.
- **FR-014**: Bundle registration MUST detect conflicting entries before storage.
- **FR-015**: Registration endpoints SHOULD honor `Idempotency-Key`.
- **FR-016**: Registration endpoints MUST reject unknown request fields.
- **FR-017**: Registry mutations MUST emit audit log entries.
- **FR-018**: Registered artifacts MUST be immediately visible to the runtime within the same workspace.

## Quality Gates

- **QG-001**: Invalid artifacts reaching persistent registry storage are a blocking defect.
- **QG-002**: Partial bundle installation is a blocking defect.
- **QG-003**: Idempotent retry behavior MUST be tested for each endpoint.
- **QG-004**: Conflict behavior MUST be tested for each artifact type.
- **QG-005**: Registration endpoint examples MUST appear in `specs/033-http-json-api/openapi.yaml`.

## Success Criteria

- **SC-001**: An app can register a capability, event contract, workflow, and bundle over HTTP.
- **SC-002**: Duplicate registration with the same digest returns idempotent success.
- **SC-003**: Duplicate registration with a different digest returns `409 Conflict`.
- **SC-004**: Invalid registration returns `422` before storage.
- **SC-005**: Bundle registration is atomic.
- **SC-006**: Registration writes append audit log entries.
