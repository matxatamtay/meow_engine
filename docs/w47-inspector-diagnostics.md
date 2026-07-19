# W47 inspector and diagnostics

Press **F12** in `meow-browser` to write
`artifacts/diagnostics/inspector-latest.json`. The snapshot crosses content IPC
and contains the deterministic DOM tree, computed styles, principal box tree,
used layout geometry, selected accessibility tree, network waterfall, retained
console, and stylesheet/image load errors.

The waterfall includes method, requested/final URL, status or error, bytes,
elapsed time, and direct/brokered backend. Network and console histories are
bounded to 512 entries each.

This is a JSON inspector, not a live DevTools dock. It has no element picker,
CSS editing, JavaScript debugger, source maps, timeline, or request replay.
`crates/embedder-api/tests/inspector.rs` verifies one page with a stylesheet 404
and console error can be diagnosed from one snapshot.
