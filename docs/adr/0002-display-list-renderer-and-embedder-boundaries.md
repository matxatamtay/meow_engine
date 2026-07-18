# ADR 0002: Display-list, renderer, and embedder boundaries

- Status: Accepted
- Date: 2026-07-18

## Context

W3 placed deterministic `tiny-skia` drawing directly in `meow-engine`. W4 needs the
same resolved paint commands to run through a CPU backend and a Vello/wgpu backend
without allowing either renderer to decide layout or CSS semantics. The desktop
shell also needs a narrow API that does not expose internal engine coordination.

## Decision

Split the rendering path into four layers:

1. `meow-display-list` owns physical-pixel paint data and commands.
2. `meow-engine` produces a resolved `DisplayList` and selects no renderer.
3. `meow-embedder-api` exposes frame requests to browser and headless embedders.
4. `meow-renderer` owns the `Renderer` trait, deterministic `ReferenceRenderer`,
   and interactive `GpuRenderer` using Vello 0.9 over wgpu.

The GPU backend renders to Vello's Rgba8 intermediate texture and blits that
texture to the wgpu surface. The CPU backend retains byte-deterministic PNG output.

## Consequences

- CPU and GPU consume the exact same ordered commands.
- Browser-shell code owns lifecycle and backend selection, not paint semantics.
- Vello remains isolated behind `meow-renderer`, which limits churn from its alpha API.
- The W4 display list intentionally supports only clear and solid rectangle commands.
