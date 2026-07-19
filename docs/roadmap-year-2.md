# Roadmap year 2

## Foundation sequence

- **Y2-W2:** cargo-deny, deterministic SBOM/license reports, and an immutable V8 binding/source/archive provenance policy.
- **Y2-W3 (02/08/2027 - 08/08/2027):** backend-neutral `JsRuntime` boundary, V8 production default, explicit Boa fallback/reference, dual-backend conformance, and ADR 0004.
- **Y2-W4 (09/08/2027 - 15/08/2027):** V8 isolate spike inside the content child with reproducible-enough Linux packaging and crash containment.

The public release/tag operation is intentionally external to this sequence.

## Q1: conformance and platform depth

Expand upstream WPT ingestion, CSS/layout, HTML edge cases, modules, streams,
URL/Fetch compliance, and platform accessibility bridges.

## Q2: asynchronous architecture and site isolation

Make IPC asynchronous with cancellation/deadlines, broker WebSocket, add tab and
site-instance scheduling, iframe boundaries, shared-memory frames, and stateful
crash restoration.

## Q3: security hardening

Move seccomp to allowlisting; add user/PID namespaces, Landlock/cgroups, peer
credentials, broker audit, coverage-guided fuzzing, sanitizer matrix, and
independent review.

## Q4: product and compatibility

Add downloads, permissions, proxy/auth, certificate UX, persistent cache,
private profiles, signed updates, wider packaging, interactive DevTools, and a
larger compatibility corpus.

Every milestone needs tests, diagnostics, budgets, threat-model changes, and
transparent gaps.
