# Current limitations

MeowEngine currently covers the W1-W8 foundation, reference rendering, HTTP(S)
loading, HTML tree construction, and top-level navigation. It is not yet a complete
web platform.

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
- Request bodies are buffered; cookies, cache, authentication, proxy policy, and referrer
  policy are not implemented yet.
- DNS uses Hyper's Tokio connector. Custom resolver policy and Happy Eyeballs diagnostics
  are not exposed yet.
- Charset sniffing implements BOM, HTTP charset, a first-1024-byte meta subset, and the
  Windows-1252 HTML fallback. Full encoding prescan rules remain future work.
- The DOM arena supports parser mutations and deterministic dumping, not Web IDL, events,
  scripting wrappers, style invalidation, or garbage collection.
- History records committed entries, but back/forward traversal and same-document
  navigation are not implemented.

## W4 rendering limits

- Display lists support only full-target clears and solid, axis-aligned rectangles.
- The GPU backend requires a compatible wgpu adapter and Vello compute support at runtime.
- Vello is isolated behind `meow-renderer` because its API is still alpha-stage.
- GPU output is a visual demo, not a byte-equivalence oracle; deterministic tests use
  the CPU renderer and PNG encoder.
- Text, images, paths, clipping, transforms, filters, color management, and CSS paint
  semantics remain out of scope.
