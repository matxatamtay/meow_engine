# MeowEngine

A Linux-first browser engine and browser shell written in Rust.

## Workspace

- `apps/meow-browser`: desktop browser shell
- `apps/meow-headless`: deterministic headless entry point
- `crates/display-list`: backend-neutral paint commands
- `crates/embedder-api`: browser-shell/engine boundary
- `crates/engine`: top-level frame orchestration
- `crates/renderer`: tiny-skia CPU and Vello/wgpu GPU backends
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

## Documentation

- [Bootstrap guide](docs/bootstrap.md)
- [W2 window lifecycle](docs/w2-window-lifecycle.md)
- [W3 reference renderer](docs/w3-reference-renderer.md)
- [W4 GPU skeleton and embedder API](docs/w4-gpu-and-embedder.md)
- [Current limitations](docs/limitations.md)
- [ADR template](docs/adr/0000-template.md)
- [ADR 0001: Bootstrap workspace and tooling](docs/adr/0001-bootstrap-workspace-and-tooling.md)
- [ADR 0002: Display-list, renderer, and embedder boundaries](docs/adr/0002-display-list-renderer-and-embedder-boundaries.md)
