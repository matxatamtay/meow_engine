# W9 CSS syntax and stylesheet discovery

W9 turns CSS-bearing DOM nodes into parsed stylesheet snapshots while keeping syntax
failures non-fatal.

## Crate boundaries

- `meow-html` discovers HTML `<style>` and `<link rel="stylesheet">` nodes in tree order.
  It exposes node identity, inline CSS or href, and the raw `media` attribute without
  exposing the DOM arena internals.
- `meow-css` adapts `cssparser` into owned top-level rules, declarations, source locations,
  `!important` flags, diagnostics, and deterministic dumps.
- `meow-net::Request::stylesheet` selects a CSS-oriented `Accept` header.
- `meow-engine::Navigator` resolves linked hrefs against the document base URL, loads them
  sequentially, parses successful responses, records non-fatal load failures, and commits
  the resulting stylesheets atomically with the document.
- `meow-headless --dump-css URL` exposes the complete discovery/load/parse path for fixtures
  and debugging.

## Syntax model

Qualified-rule selector preludes are retained as raw trimmed text for W10. Declaration
names are ASCII-lowercased except custom properties, values remain raw component text,
and a trailing case-insensitive `!important` is separated into a boolean flag. Top-level
at-rules retain their name, prelude, and optional raw block for later semantic stages.

`cssparser::StyleSheetParser` and `RuleBodyParser` provide recovery boundaries. A malformed
declaration is skipped while later declarations and rules continue to parse. Diagnostics
store one-based line/column coordinates plus the skipped source fragment.

## Stylesheet discovery and loading

The DOM walk recognizes:

- `<style>` with an absent, empty, or `text/css` type;
- `<link>` whose ASCII-whitespace-separated `rel` tokens include `stylesheet` and which has
  an `href`.

Candidates stay in document order. Inline CSS parses immediately. Linked hrefs resolve
against the post-`<base>` document base URL and load sequentially. Invalid URLs, failed
requests, and non-success HTTP statuses are retained as stylesheet errors rather than
aborting the top-level document. Cancellation remains fatal so an interrupted navigation
is not committed halfway.

## Deterministic dump

```bash
cargo run --locked -p meow-headless -- --dump-css https://example.com/
cargo run --locked -p meow-headless -- \
  --dump-css https://example.com/ \
  --output artifacts/example.css.txt
```

The dump includes source kind, resolved URL metadata, media text, rule order, declarations,
`!important`, and recoverable diagnostics. DOM document IDs are intentionally omitted so
snapshots remain stable between processes.

## Verification status

- CSS unit tests cover declarations, custom-property case preservation, `!important`,
  top-level at-rules, and recovery across invalid declarations and empty selector preludes.
- Exactly 100 curated `.css`/`.dump` golden fixtures cover whitespace, selectors as raw
  preludes, component values, comments, escapes, `!important`, malformed recovery,
  at-rules, Unicode, BOM/line endings, and mixed stylesheets.
- HTML tests cover discovery order, rel token handling, media retention, and non-CSS style
  types.
- Engine tests cover inline and external loading in document order, diagnostics, media, and
  deterministic dumps.
- Headless tests cover the public `--dump-css` path with a local HTML/CSS server.

The W9 acceptance target of 100 deterministic fixtures is complete. Selector validation,
selector matching, and specificity intentionally begin in W10.
