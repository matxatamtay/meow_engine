# W28 script scheduling

Classic scripts run in one persistent realm through two deterministic task queues.

## Ordering

Navigation discovers supported scripts in DOM tree order.

1. Inline classic scripts and external classic scripts without `defer` enter the parser-blocking queue and run immediately to completion.
2. External classic scripts with `defer` enter the deferred queue.
3. After discovery reaches the end of the document, deferred tasks run in original DOM order.
4. Each evaluation completes, including Boa's queued promise jobs, before the next script task begins.

External resources are fetched sequentially in discovery order. A load failure or JavaScript exception is recorded and the scheduler continues with the next task. Cancellation stops navigation atomically.

The order conformance test covers:

```text
inline-1 > external-blocking > inline-2 > defer-a > defer-b
```

It also verifies shared global state, external source loading, deferred execution, title mutation, text replacement, attribute mutation, and the final computed-style result.

## Alpha approximation

HTML parsing currently finishes before the scheduler runs. Therefore “parser-blocking” means preserved classic-script execution order, not a streaming tokenizer pause at each script token. Inline `defer` follows normal blocking behavior. `async` is accepted but uses deterministic blocking scheduling instead of network-race semantics.
