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

## Documentation

- [Bootstrap guide](docs/bootstrap.md)
- [Week 1 limitations](docs/limitations.md)
- [ADR template](docs/adr/0000-template.md)
- [ADR 0001: Bootstrap workspace and tooling](docs/adr/0001-bootstrap-workspace-and-tooling.md)
