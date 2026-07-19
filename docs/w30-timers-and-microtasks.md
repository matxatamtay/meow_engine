# W30 timers and microtasks

The document realm exposes `setTimeout`, `clearTimeout`, `setInterval`, `clearInterval`, and `queueMicrotask`.

Timers use a deterministic monotonic document clock. The embedder advances that clock with an explicit task budget, and Boa promise jobs are drained after every script source and every timer callback. Equal-deadline tasks retain insertion order. Zero-delay intervals are clamped to one millisecond so a single pump cannot become an unbounded loop.

The desktop shell wakes at a 16 ms cadence only while timers remain pending, then returns to `ControlFlow::Wait`. Callback errors are reported without discarding the realm.
