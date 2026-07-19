# Y2-W3 JsRuntime boundary

Y2-W3 converts the historical Boa-shaped seam into an enforceable backend
contract.

## Crates

- `meow-js-runtime`: values, errors, host calls, factory, selection policy, and
  shared conformance suite.
- `meow-js-boa`: ready reference adapter using Boa 0.21.1.
- `meow-js-v8`: fail-closed W3 scaffold upgraded to a real isolate in W4.

The browser bootstrap now lives at
`crates/js-runtime/src/browser_bootstrap.js`. Native adapters own only native
operation registration and value/error conversion.

## Feature policy

`meow-engine` keeps `js-boa` as its compatibility default so the existing
browser-host regression suite remains intact while migration is underway.
`meow-browser` is the production configuration and explicitly enables `js-v8`
with default features disabled throughout its engine/process dependency chain.
No automatic fallback occurs.

```bash
python3 scripts/verify_js_backends.py
cargo tree -p meow-browser --edges normal | grep -E 'boa|meow-js-boa'
```

The second command must produce no matches. The verification script compiles
both engine feature configurations, runs one shared adapter suite, and rejects a
production graph that contains Boa.

## Failure modes

- `BackendUnavailable`: selected adapter is not built or cannot start.
- `Initialization`: platform/isolate/realm setup failed.
- `Compile`: source compilation failed.
- `Exception`: JavaScript execution threw.
- `Host`: native host operation failed.
- `ResourceLimit`: configured execution bounds were exceeded.

W3 deliberately returns `BackendUnavailable` for non-empty page scripts in the
V8 production path. It does not silently use Boa. W4 changes that state only
after the real isolate and content-process containment checks pass.
