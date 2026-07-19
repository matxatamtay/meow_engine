# v0.2-alpha known issues

- Selected WPT is 20 reviewed cases, not the full upstream suite.
- Accessibility is not connected to AT-SPI or another platform API.
- F12 writes JSON; there is no interactive DevTools dock or debugger.
- Synchronous IPC waits can freeze the window during slow loads/scripts.
- Multiprocess WebSocket is absent; `--single-process` removes isolation.
- Content restart begins fresh; crashed tab/form state is not replayed.
- Linux namespace availability is host-dependent and seccomp is a denylist.
- HTTP, Fetch, images, and WebSocket frames are buffered.
- AppImage is conditional on `appimagetool`; tar/AppDir is canonical.
- macOS and Windows are unsupported.
- Broad HTML/CSS/layout/JS/media/security gaps remain in `docs/limitations.md`.
