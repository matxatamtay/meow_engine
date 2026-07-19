# MeowEngine

A Linux-first browser engine and browser shell written in Rust. The current milestone is **v0.6-process-alpha**, adding versioned IPC, an isolated content process, a network broker, crash containment, and an experimental Linux sandbox to the interactive layout/media foundation.

## Workspace

- `apps/meow-browser`: desktop browser shell
- `apps/meow-headless`: deterministic headless entry point
- `crates/css`: CSS Syntax adapter, selectors, typed values, property semantics, declarations, recovery, and stable rule dumps
- `crates/display-list`: backend-neutral paint commands
- `crates/embedder-api`: browser-shell/engine boundary
- `crates/engine`: frame/navigation orchestration, cascade/layout/interaction, and the Boa-backed JavaScript runtime and scheduler
- `crates/html`: html5ever TreeSink, generational DOM arena, explicit mutation records, traversal, streaming decode, and selector matching/query
- `crates/ipc`: versioned envelopes, bounded framing, request IDs, and transport abstraction
- `crates/net`: direct or brokered HTTP(S) loader with network-owned cookies and cache
- `crates/process-model`: content/network child protocols, supervision, frame submission, and crash recovery
- `crates/renderer`: tiny-skia CPU and Vello/wgpu GPU backends
- `crates/sandbox`: experimental Linux namespaces, seccomp, rlimits, and file broker
- `crates/url-policy`: canonical URL, origin, and reference resolution
- `tools/xtask`: repository automation

## Development

The repository pins Rust through `rust-toolchain.toml`.

```bash
cargo xtask doctor
bash scripts/verify.sh
```

The doctor checks bootstrap health. The verification script is the canonical
format, Clippy, test, and doctor gate used by CI.

Run an instrumented development browser process with live logs and a persistent
session log:

```bash
cargo xtask dev --renderer=gpu
cargo xtask dev --trace --renderer=gpu
cargo xtask dev --debug --renderer=cpu --smoke-test
```

The launcher injects `RUST_LOG`, `RUST_BACKTRACE=1`, and a `MEOW_DEV_SESSION`
identifier into the browser process. Merged stdout/stderr remains visible in the
terminal and is also written to `artifacts/logs/`. Use `--rust-log=<filter>` for
a custom module filter, `--log-file=<path>` for a fixed destination, or
`--no-log-file` for terminal-only output. Follow the newest session while it is
running with:

```bash
tail -f "$(ls -t artifacts/logs/*.log | head -n 1)"
```

## Desktop shell

```bash
cargo run -p meow-browser -- https://example.com/
cargo run -p meow-browser -- --renderer=cpu https://example.com/
```

The browser shell uses `winit` with Wayland and X11 support, structured tracing,
and DPI-aware resize tracking. The default `--renderer=gpu` path lowers the
engine display list into Vello and presents it through a wgpu surface.
`--renderer=cpu` presents the identical commands through tiny-skia and softbuffer.
Use `--smoke-test` to present one frame and exit through the normal lifecycle.

On Linux the shell now starts a content child and a network child by default.
The content child owns the document and submits validated display lists over
IPC; a content panic does not terminate the shell. Use `--single-process` for
the legacy in-process path, including the current direct WebSocket transport,
or `--no-sandbox` to keep the process split while disabling experimental Linux
sandbox controls.

Run the display-free process smoke tests with:

```bash
cargo test -p meow-process-model --test process_smoke
cargo test -p meow-browser --test multiprocess_smoke
```

Browser input includes wheel scrolling, left-click activation, Tab and
Shift+Tab focus traversal, Enter and Space default actions, Alt+Left/Alt+Right
history traversal, Ctrl+R or Command+R reload, and text/search/checkbox/button
GET forms. Loaded documents execute bounded inline and external classic scripts before the first committed frame, then keep the same realm alive for click handlers, timers, microtasks, console output, and form events.

Run the bundled static interaction corpus with:

```bash
python3 -m http.server 8000 -d demo/static-alpha
cargo run -p meow-browser -- http://127.0.0.1:8000/
```

Run the W25-W28 JavaScript corpus with:

```bash
python3 -m http.server 8001 -d demo/script-alpha
cargo run -p meow-browser -- http://127.0.0.1:8001/
```

The scripted page changes its title, text, attributes, and computed style while
proving inline, external blocking, and external deferred execution order.

Run the W29-W32 interactive counter and todo mini-app with:

```bash
python3 -m http.server 8002 -d demo/interactive-alpha
cargo run -p meow-browser -- http://127.0.0.1:8002/
```

This page exercises click propagation, `preventDefault`, timers, microtasks,
`console.*`, required-field validation, live `value`, and cancelable submit events.

Run the W33-W35 same-origin fetch and persistent-storage demo with:

```bash
python3 -m http.server 8003 -d demo/network-alpha
cargo run -p meow-browser -- http://127.0.0.1:8003/
```

Reload the page to see `localStorage` survive through the default
`artifacts/profile` browser profile. Override it with `MEOW_PROFILE_DIR`.

Run the W36 local WebSocket chat with three terminals:

```bash
cargo run -p meow-net --example websocket_chat
python3 -m http.server 8004 -d demo/websocket-chat
cargo run -p meow-browser -- http://127.0.0.1:8004/
```

Run the W37-W40 layout and media landing page with:

```bash
python3 -m http.server 8005 -d demo/layout-media-alpha
cargo run -p meow-browser -- http://127.0.0.1:8005/
```

The page combines flex navigation and cards, responsive PNG/SVG images, a data-SVG,
2D transforms, and isolated opacity groups. Measure the rendering pipeline with:

```bash
cargo run --release -p meow-engine --example pipeline_benchmark -- 200
```

Measure the W21 cached-layout scroll path with:

```bash
cargo run --release -p meow-engine --example scroll_benchmark
```

## Reference rendering

```bash
cargo run --locked -p meow-headless -- \
  --output artifacts/reference.png
```

The headless app requests a display list through the embedder API, rasterizes it
with the reference renderer, and writes deterministic PNG bytes.

Load an HTTP(S) URL through Tokio, Hyper, and Rustls, then print the committed DOM:

```bash
cargo run --locked -p meow-headless -- --dump-dom https://example.com/
```

Write the DOM dump to a file by adding `--output artifacts/example.dom.txt`.
The same path supports `about:blank`, redirect metadata, byte limits, timeouts,
cancellation, charset sniffing, `<base>` resolution, classic-script execution,
and a committed history entry. `--dump-dom` reflects DOM mutations made during
initial script scheduling.

Dump parsed inline and linked stylesheets, including recoverable CSS diagnostics:

```bash
cargo run --locked -p meow-headless -- --dump-css https://example.com/
```

## Documentation

- [Bootstrap guide](docs/bootstrap.md)
- [W2 window lifecycle](docs/w2-window-lifecycle.md)
- [W3 reference renderer](docs/w3-reference-renderer.md)
- [W4 GPU skeleton and embedder API](docs/w4-gpu-and-embedder.md)
- [W5-W8 loading, HTML, and navigation](docs/w5-w8-loading-html-navigation.md)
- [W9 CSS syntax and stylesheet discovery](docs/w9-css-syntax-and-stylesheet-discovery.md)
- [W10 selector engine](docs/w10-selector-engine.md)
- [W11 cascade and inheritance](docs/w11-cascade-and-inheritance.md)
- [W12 values and invalidation](docs/w12-values-and-invalidation.md)
- [W13 box tree](docs/w13-box-tree.md)
- [W14 box model and width resolution](docs/w14-box-model-and-width.md)
- [W15 vertical flow and margins](docs/w15-vertical-flow-and-margins.md)
- [W16 paint backgrounds and borders](docs/w16-paint-backgrounds-and-borders.md)
- [W17 font database](docs/w17-font-database.md)
- [W18 shaping and bidi](docs/w18-shaping-and-bidi.md)
- [W19 line breaking](docs/w19-line-breaking.md)
- [W20 inline fragments and readable text paint](docs/w20-inline-fragments.md)
- [W21 scrolling and hit testing](docs/w21-scrolling-and-hit-testing.md)
- [W22 focus, keyboard, and pointer](docs/w22-focus-keyboard-and-pointer.md)
- [W23 navigation, history, and forms](docs/w23-navigation-history-and-forms.md)
- [W24 static browser alpha](docs/w24-static-browser-alpha.md)
- [W25 Boa adapter](docs/w25-boa-adapter.md)
- [W26 Window and Document bindings](docs/w26-window-and-document-bindings.md)
- [W27 Node and Element bindings](docs/w27-node-and-element-bindings.md)
- [W28 script scheduling](docs/w28-script-scheduling.md)
- [W29 EventTarget](docs/w29-event-target.md)
- [W30 timers and microtasks](docs/w30-timers-and-microtasks.md)
- [W31 mutation pipeline](docs/w31-mutation-pipeline.md)
- [W32 console and form scripting](docs/w32-console-and-form-scripting.md)
- [W33 fetch pipeline](docs/w33-fetch-pipeline.md)
- [W34 same-origin, CORS, and cookies](docs/w34-origin-cors-cookies.md)
- [W35 storage](docs/w35-storage.md)
- [W36 WebSocket](docs/w36-websocket.md)
- [W37 Flexbox phase 1](docs/w37-flexbox-phase-1.md)
- [W38 transforms and opacity](docs/w38-transforms-opacity.md)
- [W39 images, data URLs, and SVG](docs/w39-images-svg.md)
- [W40 profiling and cache](docs/w40-profiling-cache.md)
- [W41 IPC schema](docs/w41-ipc-schema.md)
- [W42 content process](docs/w42-content-process.md)
- [W43 network broker](docs/w43-network-broker.md)
- [W44 experimental Linux sandbox](docs/w44-linux-sandbox.md)
- [Current limitations](docs/limitations.md)
- [ADR template](docs/adr/0000-template.md)
- [ADR 0001: Bootstrap workspace and tooling](docs/adr/0001-bootstrap-workspace-and-tooling.md)
- [ADR 0002: Display-list, renderer, and embedder boundaries](docs/adr/0002-display-list-renderer-and-embedder-boundaries.md)
- [ADR 0003: URL, network, HTML, and navigation boundaries](docs/adr/0003-loading-and-navigation-boundaries.md)
