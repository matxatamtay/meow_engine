# W4 GPU skeleton and embedder API

Week 4 establishes the backend-neutral path from engine frame production to CPU
or GPU presentation.

## Pipeline

```text
meow-browser / meow-headless
        |
        v
meow-embedder-api::BrowserEngine
        |
        v
meow-engine::Engine
        |
        v
meow-display-list::DisplayList
        |
        +--> meow-renderer::ReferenceRenderer --> tiny-skia framebuffer / PNG
        |
        +--> meow-renderer::GpuRenderer -------> Vello --> wgpu surface
```

The engine resolves the scene into physical-pixel clear and rectangle commands.
Renderers may rasterize those commands but may not make layout, CSS, or browser
lifecycle decisions.

## Demo

Deterministic CPU output:

```bash
cargo run --locked -p meow-headless -- \
  --output artifacts/w4-cpu.png \
  --width 800 \
  --height 600
```

Interactive GPU output through Vello/wgpu:

```bash
cargo run --locked -p meow-browser -- --renderer=gpu
```

Interactive CPU output using the identical display list:

```bash
cargo run --locked -p meow-browser -- --renderer=cpu
```

Wayland and X11 selection remains available through
`--backend=auto|wayland|x11`.

## Boundary details

- `BrowserEngine::render_frame` is the shell-facing frame request.
- `Frame` exposes only a validated viewport and resolved `DisplayList`.
- `Renderer` takes the same viewport and display list for both backends.
- Vello owns GPU rasterization; wgpu owns the window surface and presentation.
- The GPU surface is recreated after suspend and resized only for non-zero windows.

## Current scope

W4 does not add paths, text, images, clips, transforms, compositing layers, or CSS
painting. GPU availability and driver support are runtime requirements for the GPU
backend; `--renderer=cpu` remains the deterministic fallback and test oracle.
