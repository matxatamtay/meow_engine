# Current limitations

MeowEngine currently covers the W1-W11 foundation, including reference rendering,
HTTP(S) loading, HTML tree construction, top-level navigation, stylesheet discovery,
parsed CSS rule snapshots, deterministic selector matching, and a bounded cascade with
computed-style snapshots. It is not yet a complete web platform.

- CI covers Linux through `ubuntu-latest` and Ubuntu 24.04; macOS and Windows
  are not supported by the W1 gate.
- The clean-Ubuntu local script requires Docker or Podman plus network access to
  download the Ubuntu image, packages, and Rust toolchain.
- `cargo xtask doctor` diagnoses bootstrap availability. It does not replace
  formatting, linting, or tests; use `scripts/verify.sh` for the full gate.
- CI intentionally has no dependency cache. Reproducibility remains preferred
  over optimization while the foundation is changing.

## W5-W8 loading and navigation limits

- Only `http`, `https`, and synthetic `about:blank` top-level navigations are supported.
- Request bodies are buffered; cookies, cache, authentication, proxy policy, and referrer
  policy are not implemented yet.
- DNS uses Hyper's Tokio connector. Custom resolver policy and Happy Eyeballs diagnostics
  are not exposed yet.
- Charset sniffing implements BOM, HTTP charset, a first-1024-byte meta subset, and the
  Windows-1252 HTML fallback. Full encoding prescan rules remain future work.
- The DOM arena supports parser mutations and deterministic dumping, not Web IDL, events,
  scripting wrappers, style invalidation, or garbage collection.
- History records committed entries, but back/forward traversal and same-document
  navigation are not implemented.

## W4 rendering limits

- Display lists support only full-target clears and solid, axis-aligned rectangles.
- The GPU backend requires a compatible wgpu adapter and Vello compute support at runtime.
- Vello is isolated behind `meow-renderer` because its API is still alpha-stage.
- GPU output is a visual demo, not a byte-equivalence oracle; deterministic tests use
  the CPU renderer and PNG encoder.
- Text, images, paths, clipping, transforms, filters, color management, and CSS paint
  semantics remain out of scope.

## W9-W14 CSS, box-tree, and horizontal-layout limits

- W9 stylesheet snapshots still retain raw selector preludes for deterministic compatibility.
  W10 parses them on demand through `StyleRule::selector_list()`.
- The W10 subset covers basic selectors, attribute operators, four combinators, specificity,
  and structural child/of-type pseudo-classes. Dynamic pseudo-classes, pseudo-elements,
  namespaces, logical selector functions, shadow DOM, and `nth-child(... of ...)` are not
  implemented.
- W12 computes 26 typed longhands, including four-sided margin, padding, and border-width
  properties plus `box-sizing`. It supports a bounded `margin`/`padding`/`border-width`
  expansion, fixed-precision lengths and opacity, named/hex colors, `currentColor`, and selected
  display/font/text keywords. `calc()`, broad unit coverage, color functions, complete value
  grammars, and general shorthand expansion remain future work.
- Custom properties inherit and support recursive `var()` substitution, fallback, and cycle
  diagnostics. `@property`, environment variables, registered values, animation semantics, and
  token-list shorthand re-expansion are not implemented.
- `StyleEngine` consumes explicit DOM mutation records and caches per-element generations.
  Dependency summaries distinguish ancestor, adjacent-sibling, subsequent-sibling, structural,
  and `:empty` effects. They are feature-level rather than a per-rule selector index, so some
  affected subtrees may be conservatively restyled, but unrelated branches keep their cache.
- Cascade layers, animations, transitions, style attributes, presentation hints, used values,
  layout integration, and paint invalidation remain future work.
- Top-level at-rules are retained as raw prelude/block syntax. `DocumentState` activates only
  empty, `all`, or `screen` media attributes; full media-query evaluation, `@import`, font
  loading, and nested semantic parsing are not active yet.
- Linked stylesheets load sequentially in document order. Cache, preload, integrity,
  referrer, CORS, alternate/disabled sheet state, and render-blocking policy are not yet modeled.
- CSS decoding honors an HTTP charset when present and otherwise uses UTF-8. The complete
  CSS encoding-detection algorithm remains future work.
- The W9 golden snapshot suite contains 100 curated fixtures. The W10 selector suite adds
  70 valid matching cases and 19 invalid/unsupported syntax cases. W11 adds two complete
  computed-style snapshots. W12 adds typed-value and mutation-invalidation snapshots plus
  focused cache-generation conformance tests.
