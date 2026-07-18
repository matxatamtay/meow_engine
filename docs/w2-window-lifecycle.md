# W2 window lifecycle

Week 2 establishes the Linux desktop shell boundary before browser rendering is
introduced.

## Implemented

- `winit` `ApplicationHandler` lifecycle with idempotent `resumed`, explicit
  `suspended`, close handling, window destruction, and clean event-loop exit.
- Structured console diagnostics through `tracing` and `RUST_LOG` filtering.
- A panic hook that records the panic payload, source location, thread, and a
  forced backtrace before delegating to Rust's standard panic hook.
- Physical and logical window metrics, scale-factor updates, resize handling,
  and zero-sized/minimized protection.
- A minimal `softbuffer` frame so Wayland commits and maps the window instead of
  leaving a never-presented surface invisible.
- Both Wayland and X11 are compiled into the desktop binary.

## Run

Use the compositor-selected backend:

```bash
cargo run -p meow-browser
```

Increase diagnostics when investigating lifecycle events:

```bash
RUST_LOG=meow_browser=trace,winit=debug,softbuffer=debug \
  cargo run -p meow-browser
```

## Backend smoke tests

The smoke mode opens a window, presents one frame, and exits through the normal
`ActiveEventLoop::exit` path.

Wayland session:

```bash
cargo run --locked -p meow-browser -- \
  --backend=wayland --smoke-test
```

X11 session, including XWayland when available:

```bash
cargo run --locked -p meow-browser -- \
  --backend=x11 --smoke-test
```

The backend flag uses winit's Linux-specific event-loop builder extensions, so
it does not depend on desktop-session backend-selection heuristics.

A successful run logs the detected backend, `first frame presented`,
`event loop exiting`, and `browser shell stopped cleanly`.

## Current boundary

The frame is deliberately a solid software buffer. Browser chrome, GPU
selection, renderer ownership, tabs, input routing, and document content belong
to later milestones. The panic hook covers Rust panics, not process aborts,
segmentation faults, or external termination signals.
