# MeowEngine

A Linux-first browser engine and browser shell written in Rust.

## Workspace

- `apps/meow-browser`: desktop browser shell
- `apps/meow-headless`: deterministic headless entry point
- `crates/css`: CSS Syntax adapter, selectors, typed values, property semantics, declarations, recovery, and stable rule dumps
- `crates/display-list`: backend-neutral paint commands
- `crates/embedder-api`: browser-shell/engine boundary
- `crates/engine`: frame/navigation orchestration plus cascade, typed computed styles, and subtree restyle caching
- `crates/html`: html5ever TreeSink, generational DOM arena, explicit mutation records, traversal, streaming decode, and selector matching/query
- `crates/net`: Tokio/Hyper/Rustls HTTP(S) loader
- `crates/renderer`: tiny-skia CPU and Vello/wgpu GPU backends
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
cargo run -p meow-browser
```

The browser shell uses `winit` with Wayland and X11 support, structured tracing,
and DPI-aware resize tracking. The default `--renderer=gpu` path lowers the
engine display list into Vello and presents it through a wgpu surface.
`--renderer=cpu` presents the identical commands through tiny-skia and softbuffer.
Use `--smoke-test` to present one frame and exit through the normal lifecycle.

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
cancellation, charset sniffing, `<base>` resolution, and a committed history entry.

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
- [Current limitations](docs/limitations.md)
- [ADR template](docs/adr/0000-template.md)
- [ADR 0001: Bootstrap workspace and tooling](docs/adr/0001-bootstrap-workspace-and-tooling.md)
- [ADR 0002: Display-list, renderer, and embedder boundaries](docs/adr/0002-display-list-renderer-and-embedder-boundaries.md)
- [ADR 0003: URL, network, HTML, and navigation boundaries](docs/adr/0003-loading-and-navigation-boundaries.md)
