# Roadmap year 2

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
