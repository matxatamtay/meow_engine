# Current limitations

The W1 bootstrap deliberately does not implement browser-engine behavior.

- `meow-browser` and `meow-headless` are executable shells only.
- `meow-engine` exposes bootstrap identity and version information only.
- CI covers Linux through `ubuntu-latest` and Ubuntu 24.04; macOS and Windows
  are not supported by the W1 gate.
- The clean-Ubuntu local script requires Docker or Podman plus network access to
  download the Ubuntu image, packages, and Rust toolchain.
- `cargo xtask doctor` diagnoses bootstrap availability. It does not replace
  formatting, linting, or tests; use `scripts/verify.sh` for the full gate.
- CI intentionally has no dependency cache. Reproducibility remains preferred
  over optimization while the foundation is changing.

## W4 rendering limits

- Display lists support only full-target clears and solid, axis-aligned rectangles.
- The GPU backend requires a compatible wgpu adapter and Vello compute support at runtime.
- Vello is isolated behind `meow-renderer` because its API is still alpha-stage.
- GPU output is a visual demo, not a byte-equivalence oracle; deterministic tests use
  the CPU renderer and PNG encoder.
- Text, images, paths, clipping, transforms, filters, color management, and CSS paint
  semantics remain out of scope.
