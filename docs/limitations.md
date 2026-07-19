# Current limitations

MeowEngine currently covers the W1-W44 process-alpha path, including HTTP(S)
loading, HTML and CSS processing, block/inline/single-line flex layout, transformed opacity
layers, static raster/SVG images, interaction, persistent classic JavaScript realms,
Fetch/CORS, storage, WebSocket events, and observable rendering caches. It is not yet a
complete web platform or a production security boundary.

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
- Navigation and Fetch request bodies are buffered. Multiprocess HTTP(S) uses a small
  conservative network-process cache; authentication, proxy policy, and referrer policy are
  not implemented yet.
- DNS uses Hyper's Tokio connector. Custom resolver policy and Happy Eyeballs diagnostics
  are not exposed yet.
- Charset sniffing implements BOM, HTTP charset, a first-1024-byte meta subset, and the
  Windows-1252 HTML fallback. Full encoding prescan rules remain future work.
- The DOM arena supports parser and script mutations, deterministic dumping, selector
  queries, and a small bootstrap-defined binding layer. Full Web IDL, mutation
  observers, ranges, composed paths, shadow DOM, and browser-compatible wrapper garbage
  collection remain absent.
- Back, forward, and reload are implemented by re-fetching the selected entry.
  Same-document fragment navigation, BFCache, persisted user state, and history APIs are not implemented.

## W4 rendering limits

- Display lists support target clears, solid rectangles, raster images, clips, and 2D layer metadata. General paths, gradients, filters, masks, and production text outlines remain absent.
- The GPU backend requires a compatible wgpu adapter and Vello compute support at runtime.
- Vello is isolated behind `meow-renderer` because its API is still alpha-stage.
- GPU output is a visual demo, not a byte-equivalence oracle; deterministic tests use
  the CPU renderer and PNG encoder.
- Viewport clipping, deterministic bitmap text, static images, and 2D transforms are implemented. Filters, color management, production outline rasterization, and broad CSS paint semantics remain out of scope.

## W9-W20 CSS, layout, fragments, and text-paint limits

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

## W21-W24 interaction and static-alpha limits

- Root scrolling is interactive. Nested overflow nodes are exposed in the scroll tree but do not yet receive independent wheel routing or scrollbars.
- Hit testing covers links and the supported form-control subset. Selection, drag and drop, hover state, context menus, touch gestures, pointer capture, and PointerEvent/MouseEvent-specific fields are not implemented.
- Keyboard text input uses winit logical keys. IME composition, clipboard editing, selection ranges, undo, and platform accessibility integration are not implemented.
- Focus order is DOM order for supported controls. `tabindex`, focus delegation, autofocus, focus-visible heuristics, and accessibility-tree semantics are not implemented.
- Forms support GET submission for text/search, hidden, checkbox, and button controls plus a required-field validation subset and cancelable `submit`. POST, full constraint validation, radio-group semantics, select, textarea editing, file input, reset, labels, autocomplete, and multipart encoding are not implemented.
- The desktop shell performs synchronous IPC waits on the event-loop thread. Network I/O is
  owned by a separate process, but a slow navigation can still leave the window unresponsive
  until the content request returns.
- Authentication, service workers, general permissions, downloads, and site isolation are not
  implemented. The W41-W44 process split and sandbox are experimental alpha boundaries, not a
  production security guarantee. Use controlled content only.

## W25-W32 JavaScript, events, timers, and forms limits

- Boa executes classic scripts only. JavaScript modules, import maps, dynamic import, workers, and module fetch/CORS semantics are not implemented.
- The source budget, loop limit, recursion limit, VM stack limit, and per-pump timer budget protect development runs from common accidental runaway scripts. They are not a security sandbox or a substitute for process isolation.
- Parsing completes before initial script scheduling. Parser-blocking order is preserved, but scripts do not pause and resume the streaming HTML tokenizer. External `defer` is deterministic; `async` is still treated as blocking.
- `EventTarget` supports listener registration/removal, capture/target/bubble, cancellation, propagation stops, and `once`. It does not yet implement passive/signal options, composed paths, event timestamps, trusted-event policy, specialized event classes, inline `on*` handlers, or default actions beyond the current interaction subset.
- Timers use an embedder-advanced monotonic clock. Nested timer clamping, background throttling, wall-clock APIs, `requestAnimationFrame`, networking task sources, and cross-realm task queues are not implemented.
- `queueMicrotask` uses Boa promise jobs. Unhandled-rejection reporting and browser-compatible exception reporting for microtasks remain future work.
- Mutation records are engine-internal `DomMutation` values, not a JavaScript `MutationObserver` API. The embedder coalesces a task burst into one frame, but `DocumentView` currently performs a conservative full style/layout rebuild at flush time.
- The binding surface is limited to `window`, `document`, read-only `location.href`, title access, selected traversal properties, attributes, `textContent`, event targets, timers, console, and form `value`/`checked`.
- Detached nodes are not retained as independent JavaScript objects. Stale generational handles throw instead of exposing a detached subtree.
- Dynamically inserted scripts and stylesheets are not rediscovered. External scripts do not implement CSP, SRI, referrer policy, JavaScript MIME blocking, same-origin policy, CORS, or Fetch metadata.
- Script execution and external loads remain synchronous within navigation. The desktop event loop can still block while a page loads or executes long JavaScript.


## W33-W36 Fetch, security, storage, and WebSocket limits

- Fetch bodies are fully buffered. There is no `ReadableStream`, upload streaming, Blob, FormData, multipart encoder, compression API, or byte-perfect non-UTF-8 response body path. `arrayBuffer()` is the current simplified string-to-byte adapter.
- Abort rejects requests canceled before loader dispatch. Because the desktop pump currently awaits each loader call, an abort fired while a request is already in flight cannot interrupt that call yet.
- Redirect handling supports `follow` and `error`; `manual` and opaque-redirect responses are not implemented. CORS is validated after the loader's redirect chain, not at every redirect hop.
- The CORS subset covers simple requests, exact/wildcard allow-origin, credential checks, method/header preflight, and exposed response headers. Preflight caching, safelist byte-value constraints, Private Network Access, CORP, COEP, COOP, and Fetch Metadata are absent.
- In multiprocess mode cookies live in the network process; in single-process mode they live
  in the direct loader. Public-suffix validation, expiry dates, clock-based Max-Age expiry,
  prefixes, partitioned cookies, priority, persistence, `document.cookie`, and complete
  RFC6265bis parsing are absent. SameSite uses a simplified schemeful last-two-label site key
  rather than the public suffix list.
- Web Storage exposes method-based access only. Property-style access, storage events, cross-tab synchronization, eviction policy, async I/O, private-mode policy, and UTF-16 quota accounting are absent. Persistence uses synchronous JSON files and should only be used with controlled profile directories.
- WebSocket frames are buffered. Blob delivery, bufferedAmount accounting, compression
  exposure, cookies, proxy/auth policy, CSP, mixed-content blocking, reconnection, and
  production backpressure are absent. WebSocket is not brokered yet: isolated content fails
  closed, while `--single-process` retains the W36 direct socket path.
- Fetch is pumped through synchronous shell-to-content IPC. The network operation runs in the
  network process, but the UI can still freeze until the content request completes; fully
  asynchronous shell IPC and independently cancelable broker requests remain future work.


## W37-W40 Flexbox, transforms, images, and profiling limits

- Flexbox is a single-line phase-one implementation. There is no `flex-wrap`, `order`, `align-self`, baseline alignment, min/max-content sizing, automatic minimum size, multi-line cross-axis distribution, writing-mode support, or complete anonymous-flex-item behavior. Column flow and `stretch` are conservative subsets.
- `gap`, `row-gap`, and `column-gap` currently share one value. Flex basis uses explicit lengths, width/height, or a bounded intrinsic text/image estimate rather than the full flex base-size algorithm.
- Transforms support only 2D translate, scale, rotate, and matrix around the default border-box center. `transform-origin`, skew functions, individual transform properties, 3D transforms, perspective, animation, transformed hit testing, and transformed scroll overflow are not implemented. Clip handling uses transformed bounding rectangles.
- CPU opacity groups are isolated and composited once. The Vello GPU path currently multiplies opacity into each primitive, so overlapping descendants can differ from CPU output until GPU offscreen layers are added. CPU reference rendering remains the conformance oracle.
- `<img>` supports PNG, JPEG, the first GIF frame, and basic SVG rasterization. Animated GIF, APNG, WebP, AVIF, `<picture>`, `srcset`, CSS background images, object-fit/position, lazy loading, image maps, SVG scripting, external SVG resources, filters, and embedded fonts are absent.
- Image response bodies and decoded pixels are buffered. The dimension/pixel limit is enforced at decode output; compressed-bomb defenses beyond the loader byte limit and decoded pixel limit are not production-grade.
- Style sharing is scoped to one style-engine computation. The glyph cache is process-wide and currently unbounded by entry count, though the supported bitmap repertoire is small. The image cache is bounded by entry count rather than bytes and is not persisted across process restarts.
- Pipeline timings are observational microsecond counters and tracing spans, not a sampling profiler. They provide hooks for `perf`, flamegraph, or tracing subscribers, but the repository does not automate privileged profiler installation.
- DOM mutation bursts still invalidate and rebuild a complete `DocumentView` at frame flush. Metrics expose rebuild and reuse counts, but subtree-level incremental layout remains future work.


## W41-W44 IPC, process, broker, and sandbox limits

- The IPC payload codec is bounded JSON over blocking Unix streams. Shared-memory images,
  zero-copy frame transport, asynchronous multiplexing, per-message deadlines, peer
  authentication, and cryptographic channel integrity are absent.
- The browser currently supervises one active content child, not one process per tab, origin,
  or site instance. There is no renderer-process pool, cross-origin iframe isolation, BFCache
  process retention, or process priority policy.
- A content crash is reported and can be restarted, but normal desktop navigation state is not
  automatically replayed into the replacement child yet. The smoke path proves shell/network
  survival and a fresh frame after restart.
- The network broker permits bounded HTTP(S) requests and rejects raw Cookie, CONNECT/TRACE,
  proxy, upgrade, and hop-by-hop controls. It does not yet implement download destinations,
  certificate exceptions, proxy configuration, authentication challenges, streaming bodies,
  WebSocket brokering, or a persistent disk cache.
- The response cache is a small in-memory GET cache with conservative exclusions. It does not
  implement RFC-complete freshness, validators, Vary matching, stale policies, partitioning,
  or persistent eviction.
- Linux namespaces are best effort and host-policy dependent. The sandbox report records gaps.
  The filesystem view does not yet use `pivot_root`/`chroot`, the seccomp policy is a denylist
  rather than an allowlist, and PID/user namespaces, cgroups, Landlock, and broker audit logs
  are absent.
- `FileAccessBroker` is a canonical-root, read-only primitive with a byte cap. Top-level `file:`
  navigation and a content-visible brokered file protocol are not implemented.
