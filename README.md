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
```
