# W5-W8 loading, HTML, and navigation

This slice turns a canonical URL into a committed DOM document and history entry.

## Crate boundaries

- `meow-url-policy` wraps the WHATWG `url` crate and owns canonical URLs, tuple or
  opaque origins, and relative-reference resolution.
- `meow-net` owns request and response types, cooperative cancellation, redirect
  traversal, Tokio DNS and I/O, Hyper HTTP/1.1 and HTTP/2, and Rustls TLS.
- `meow-html` adapts html5ever through a first-party `TreeSink`. Nodes live in a
  document-scoped generational arena and expose stable `(document, slot, generation)`
  identities.
- `meow-engine::Navigator` owns `about:blank`, charset selection, parsing, base URL
  resolution, atomic document commit, and session-history entries.
- `meow-embedder-api::BrowserEngine` exposes navigation without leaking Hyper or
  html5ever implementation types to applications.

## Loader policy

The default loader permits HTTP and HTTPS, follows at most 10 redirects, applies a
10-second connect timeout and 30-second request/body timeout, and retains at most
8 MiB per response. Each completed response stores requested and final URLs,
redirect hops, HTTP version, Content-Type, declared Content-Length, received bytes,
and elapsed milliseconds.

Dropping a redirect response and issuing the resolved request keeps redirect logic
inside `meow-net`. A `CancellationToken` can abort both request and body phases.

## HTML construction

`StreamingParser` incrementally decodes byte chunks with `encoding_rs` and feeds UTF-8
tendrils into html5ever. The TreeSink handles element, text, comment, doctype,
processing-instruction, template-fragment, reparenting, and foster-parenting mutations.
Adjacent text is coalesced and dumps are deterministic, including malformed HTML.

## Navigation commit

A navigator starts with a parsed `about:blank` document at history index zero. For a
network URL it:

1. loads the request and follows redirects;
2. selects encoding by BOM, HTTP charset, meta charset subset, then Windows-1252;
3. parses the response bytes into a document arena;
4. resolves the first `<base href>` against the final response URL;
5. constructs a pending `DocumentState`;
6. appends one history entry and swaps the current document only after every prior
   step succeeds.

A failed load therefore leaves the previous committed document and history untouched.

## Headless usage

```bash
cargo run --locked -p meow-headless -- --dump-dom https://example.com/
cargo run --locked -p meow-headless -- \
  --dump-dom https://example.com/ \
  --output artifacts/example.dom.txt
```

The first form prints only the deterministic DOM dump to stdout. The second writes it
to a file and prints a completion summary.

## Verification

- URL policy tests execute 256 independent relative-reference cases plus origin and
  canonicalization cases.
- Network tests cover metadata, relative redirects, response byte limits, timeout,
  and cancellation using a local HTTP server.
- HTML tests cover empty-document skeletons, malformed fixtures, split multibyte UTF-8,
  legacy decoding, replacement reporting, and base lookup.
- Engine and headless tests cover URL-to-DOM commit, encoding, base URL, history,
  rollback on failure, and the executable CLI path.
