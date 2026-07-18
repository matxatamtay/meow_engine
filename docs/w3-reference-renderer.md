# W3 reference renderer

Week 3 adds a deterministic software rendering baseline before browser layout,
text, images, or GPU compositing are introduced.

## Renderer boundary

`meow_engine::reference_renderer` owns the reference framebuffer and exposes:

- an owned premultiplied-RGBA `Framebuffer` backed by `tiny-skia::Pixmap`;
- full-frame background clearing;
- pixel-aligned, filled rectangles;
- PNG encoding with no timestamps or runtime metadata;
- a built-in scene shared by deterministic tests and `meow-headless`.

Rectangle painting disables antialiasing and uses source replacement. The W3
palette is fully opaque, making pixel values and PNG output stable for identical
inputs with the locked dependency graph.

## Generate a PNG

The default command writes an 800 by 600 image to `meow-reference.png`:

```bash
cargo run --locked -p meow-headless
```

Select an output path and dimensions:

```bash
cargo run --locked -p meow-headless -- \
  --output artifacts/reference.png \
  --width 1280 \
  --height 720
```

The built-in scene requires each side to be at least 64 pixels. Framebuffer
allocation is capped at 16384 pixels per side to reject accidental giant
allocations early.

## Determinism checks

The engine tests render and encode the same scene twice, compare PNG bytes, and
decode the PNG back into the original pixel buffer. The headless CLI integration
test launches two separate processes and verifies that identical arguments
produce byte-identical files with the expected PNG dimensions.

Determinism here covers the locked Rust and crate versions, scene inputs, pixel
buffer, and encoded PNG bytes. Filesystem timestamps and paths are outside the
image payload and are not part of the guarantee.

## Current boundary

The reference renderer intentionally supports only background fills and solid
rectangles. Text shaping, paths beyond rectangles, images, CSS painting,
compositing layers, color management, and GPU acceleration belong to later
milestones.
