# W42 content process

W42 moves document ownership into an isolated child process and leaves the
browser shell responsible for windowing and presentation.

## Process topology

```text
browser shell
  | versioned content IPC
  v
content process
  | permission-mediated network IPC
  v
network process
```

`ProcessSupervisor` starts the network child first, then starts one content
child and connects a `ContentProcessClient`. The desktop shell uses the same
`BrowserSession` API for local and remote modes, so input, history, rendering,
and task pumping share one UI path.

On Linux the desktop browser selects multiprocess mode by default. Use
`--single-process` to retain the W40 in-process path for debugging or for the
current direct WebSocket implementation.

## Document ownership and frame submission

The content child owns `BrowserEngine`, its live DOM/JavaScript realm, history,
layout state, and profile-backed storage. The shell sends navigation, input,
history, task-pump, title, and render requests.

A submitted frame contains:

- the viewport;
- display commands;
- raster image resources.

The shell does not trust the serialized list blindly. It rebuilds the
`DisplayList` command by command and rechecks image IDs, deterministic layer
IDs, and balanced layer/clip stacks before handing the frame to a renderer.

## Crash containment

The content request loop is wrapped by a panic boundary. A panic writes a JSON
`CrashReport` containing process name, PID, active request ID, timestamp, and
panic message, then the child exits nonzero. `ProcessSupervisor` can read that
report and replace the content child without replacing the shell or network
child.

The W42 model currently hosts one active content child for the browser session.
It proves the tab-crash boundary, but it is not yet a multi-tab/site-instance
scheduler.

## Verification

```bash
cargo test -p meow-process-model --test process_smoke
cargo test -p meow-browser --test multiprocess_smoke
```

The tests submit frames, trigger an intentional content panic, verify the shell
survives, restart content, and submit another frame through the real browser
binary.
