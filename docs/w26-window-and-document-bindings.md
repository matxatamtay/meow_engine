# W26 Window and Document bindings

The document realm exposes `window`, `document`, and `location` through a small bootstrap-defined host-object layer.

## Globals

- `window` aliases the Boa global object.
- `document` is a persistent `Document` wrapper.
- `location` exposes a read-only `href` getter and string conversion.
- `document.title` is readable and writable.
- `document.documentElement` returns the HTML element when present.
- `document.querySelector()` uses the engine's W10 selector subset.

Writing `document.title` replaces the current title element's text content. If no title exists, the host creates one under `head` and records the resulting child-list mutations.

## Lifetime handles

DOM objects carry encoded generational IDs containing the document identity, arena slot, and generation. The JavaScript bootstrap caches wrappers by this encoded handle. Rust validates every handle before property access or mutation, converting stale or malformed handles into JavaScript `TypeError` exceptions rather than dereferencing raw pointers.

The current alpha does not retain detached-node state. A wrapper whose arena handle becomes stale is rejected by subsequent host operations.
