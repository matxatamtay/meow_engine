# ADR 0001: Bootstrap workspace and tooling

- Status: Accepted
- Date: 2026-07-18

## Context

MeowEngine needs a reproducible foundation before browser-engine work begins. The
first week must establish repository ownership, licensing, a pinned compiler,
quality gates, and a bootstrap check without introducing runtime architecture.

## Decision

- Use a Cargo workspace with first-party applications, the engine crate, and an
  `xtask` automation crate.
- Pin Rust with `rust-toolchain.toml`, including `rustfmt` and Clippy.
- License the workspace under MPL-2.0.
- Deny unsafe Rust through inherited workspace lints.
- Keep `cargo xtask doctor` focused on bootstrap health.
- Keep formatting, Clippy, tests, and doctor in `scripts/verify.sh` and execute
  that script from CI.
- Verify the same gates in both `ubuntu-latest` and a clean Ubuntu 24.04
  container.

## Consequences

The project starts with deterministic tooling and one canonical quality-gate
entry point. Contributors need rustup and the pinned components. CI takes longer
because the clean-container job installs its toolchain from scratch, but that
cost exposes missing bootstrap assumptions early.
