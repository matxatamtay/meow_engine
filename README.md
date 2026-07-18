# MeowEngine

A Linux-first browser engine and browser shell written in Rust.

## Workspace

- `apps/meow-browser`: desktop browser shell
- `apps/meow-headless`: deterministic headless entry point
- `crates/engine`: top-level engine orchestration
- `tools/xtask`: repository automation

## Development

The repository pins Rust through `rust-toolchain.toml`.

```bash
cargo xtask doctor
bash scripts/verify.sh
```

The doctor checks bootstrap health. The verification script is the canonical
format, Clippy, test, and doctor gate used by CI.

## Desktop shell

```bash
cargo run -p meow-browser
```

The browser shell uses `winit` with Wayland and X11 support, structured tracing,
DPI-aware resize tracking, and a software-presented bootstrap frame. Use
`--smoke-test` to present one frame and exit through the normal lifecycle.

## Reference rendering

```bash
cargo run --locked -p meow-headless -- \
  --output artifacts/reference.png
```

The headless app uses the engine's `tiny-skia` framebuffer to paint a fixed
background and pixel-aligned rectangles, then writes deterministic PNG bytes.

## Documentation

- [Bootstrap guide](docs/bootstrap.md)
- [W2 window lifecycle](docs/w2-window-lifecycle.md)
- [W3 reference renderer](docs/w3-reference-renderer.md)
- [Week 1 limitations](docs/limitations.md)
- [ADR template](docs/adr/0000-template.md)
- [ADR 0001: Bootstrap workspace and tooling](docs/adr/0001-bootstrap-workspace-and-tooling.md)
