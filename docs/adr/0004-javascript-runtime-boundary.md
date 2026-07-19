# ADR 0004: JavaScript runtime boundary and backend policy

- Status: Accepted
- Date: 2027-08-02

## Context

The W25 implementation placed the browser host bridge, JavaScript bootstrap, and
Boa VM inside `meow-engine`. The public `JsRuntime` trait was backend-neutral in
shape, but navigation, task pumping, dependency selection, and bootstrap asset
ownership still named Boa directly. Adding V8 without a stricter seam would
create two diverging browser hosts and allow production builds to retain Boa by
accident.

## Decision

1. `meow-js-runtime` owns backend identity, values, errors, host calls, factory
   construction, product selection policy, and the shared conformance suite.
2. V8 is the production default. Fallback is never implicit. Selecting Boa
   requires an explicit feature or policy entry.
3. `meow-js-boa` is the reference adapter and test oracle.
4. `meow-js-v8` exists in W3 as a fail-closed contract scaffold. W4 replaces the
   unavailable factory with a real isolate without changing the contract.
5. The browser bootstrap asset moves out of `meow-engine` into
   `meow-js-runtime`. Backend adapters provide native operations; bootstrap code
   may not import backend-specific APIs.
6. `meow-browser` disables default features on engine-facing crates and enables
   only `js-v8`. CI rejects a production dependency graph containing
   `boa_engine` or `meow-js-boa`.
7. The existing full browser-host implementation remains behind `js-boa` during
   migration so regression coverage is preserved. The W3 V8 browser host reports
   `BackendUnavailable` for non-empty page scripts instead of silently executing
   them with Boa.

## Consequences

- Product and reference graphs are mechanically distinguishable.
- V8 initialization, compile, exception, host, resource-limit, and unavailable
  failures share stable categories.
- Static browsing remains available while the V8 host is incomplete; script
  failures are explicit and non-fatal to a valid document commit.
- Boa remains a deliberate compatibility feature, not a hidden transitive
  production dependency.
- W4 must make the V8 conformance expectation `Ready` and wire isolates only in
  the content child.
