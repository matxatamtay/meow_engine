# ADR 0003: URL, network, HTML, and navigation boundaries

- Status: Accepted
- Date: 2026-07-18

## Context

W5-W8 require URL and origin semantics, a cancellable HTTPS loader, html5ever tree
construction, and a document navigation lifecycle. Putting these concerns directly
inside `meow-engine` would couple policy, transport, parser callbacks, and embedder
APIs into one crate and make later cache, cookie, DOM, and process isolation work
harder.

## Decision

Split the loading path into four ownership layers:

1. `meow-url-policy` owns canonical `BrowserUrl`, origin serialization, and reference
   resolution over the `url` crate.
2. `meow-net` owns first-party request/response models and adapts Tokio, Hyper,
   Hyper-Rustls, DNS, redirects, timeouts, body limits, and cancellation.
3. `meow-html` owns streaming byte decoding and a custom html5ever `TreeSink` backed by
   a document-scoped generational arena.
4. `meow-engine::Navigator` owns `about:blank`, charset sniffing, base URL selection,
   pending document state, atomic commit, and history.

`meow-embedder-api` reexports stable navigation types and keeps transport and parser
implementation details out of applications.

## Consequences

- URL, network, DOM, and lifecycle tests can run independently.
- Failed navigation cannot partially replace the current document.
- Renderer crates remain isolated from DOM and network state.
- The current body model is buffered and single-process. Streaming into the parser,
  cache ownership, cookies, referrer policy, and process boundaries can evolve behind
  the same crate interfaces.
