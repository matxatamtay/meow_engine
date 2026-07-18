# Week 1 limitations

The W1 bootstrap deliberately does not implement browser-engine behavior.

- `meow-browser` and `meow-headless` are executable shells only.
- `meow-engine` exposes bootstrap identity and version information only.
- CI covers Linux through `ubuntu-latest` and Ubuntu 24.04; macOS and Windows
  are not supported by the W1 gate.
- The clean-Ubuntu local script requires Docker or Podman plus network access to
  download the Ubuntu image, packages, and Rust toolchain.
- `cargo xtask doctor` diagnoses bootstrap availability. It does not replace
  formatting, linting, or tests; use `scripts/verify.sh` for the full gate.
- CI intentionally has no dependency cache during W1. Reproducibility is
  preferred over optimization until the baseline is stable.
