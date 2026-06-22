# youaskm3 Compatibility Conformance Suite

This document defines the first deterministic compatibility and conformance suite for Traverse and `youaskm3`.

The suite exists so a reviewer can prove that the released Traverse surfaces remain consumable by `youaskm3` without depending on repo archaeology, ad hoc environment setup, or private Traverse internals.
The real browser-hosted shell validation is documented separately in [docs/youaskm3-real-shell-validation.md](youaskm3-real-shell-validation.md).

This youaskm3 compatibility conformance suite is the release-aligned proof path for the downstream consumer contract.

## Scope

The suite covers the released downstream path end to end:

- the versioned app-consumable browser consumer bundle
- the live browser-hosted consumer path
- the app-facing MCP consumption path
- the first real `youaskm3` integration path
- the Traverse-side downstream app MVP conformance suite
- the browser-hosted `youaskm3` real shell validation path

It does not define new runtime behavior. It verifies that the supported Traverse surfaces continue to fit together as a released consumer set.

## Version Pairing

This version pairing is the release-aligned Traverse and `youaskm3` combination the suite proves.

The suite proves a single Traverse release pairing:

- the approved Traverse v0.1 consumer bundle
- the browser-targeted consumer package
- the dedicated MCP consumer and validation paths

If the release pairing changes, the suite should move with it rather than silently proving an older or mixed combination.

## Deterministic Suite

Run the conformance suite with:

```bash
bash scripts/ci/youaskm3_compatibility_conformance.sh
```

That command runs the release-prep evidence first and then verifies the live browser-hosted and MCP-facing downstream paths.
It also runs `bash scripts/ci/downstream_app_mvp_conformance.sh` to prove the shared Traverse-side app manifest, WASM workflow, HTTP/JSON, MCP, and model dependency evidence required by the first knowledge-app MVP.

## Expected Evidence

The suite should prove:

- the versioned consumer bundle is documented
- the supported browser-hosted path is runnable
- the supported MCP-facing path is runnable
- the first real `youaskm3` integration path can be followed without private Traverse knowledge
- the downstream app MVP conformance path passes
- the observed runtime outcome is completed

## Known Failure Modes

The suite is expected to fail deterministically when:

- the versioned consumer bundle documentation is missing
- the live browser-hosted adapter path is unavailable
- the MCP consumer path is unavailable
- the first real `youaskm3` validation path is unavailable
- the browser-hosted shell validation path is unavailable

## Verification

A reviewer can verify the suite by checking:

1. the versioned consumer bundle
2. the live browser-hosted smoke path
3. the MCP consumption validation path
4. the first real `youaskm3` integration validation path
5. the browser-hosted real shell validation path
