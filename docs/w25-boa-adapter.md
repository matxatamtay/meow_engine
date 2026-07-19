# W25 Boa adapter

MeowEngine's first JavaScript backend is Boa 0.21.1. The engine-facing boundary is the `JsRuntime` trait, with `BoaRuntime` as the initial implementation.

## Runtime model

Each loaded document receives one persistent Boa `Context`, which supplies the global realm shared by every classic script in that document. `ScriptSource` carries owned source text, the resolved source URL, and the originating DOM node. The URL is attached to Boa's parser source so syntax errors and stack traces identify inline or external script origins.

The runtime is deliberately separate from navigation and rendering. Navigation discovers and schedules scripts, while the adapter only evaluates one source to completion and returns a backend-neutral `ScriptValue` or `ScriptError`.

## Host bridge

Native Boa functions use a scoped thread-local host activation. The active host owns an `Rc<RefCell<...>>` containing the DOM document, location, and mutation queue. Native functions never capture untraced Boa GC values and the crate keeps `unsafe_code = "deny"`.

A JavaScript bootstrap layer binds the global `window` object and builds `Document`, `Node`, `Element`, and `Location` wrappers over the small native primitive surface. This keeps Rust lifetime handling compact while retaining one stable JavaScript object per encoded DOM handle.

## Limits and errors

Default limits are:

- 512 KiB decoded source per script
- 1,000,000 loop iterations
- recursion depth 128
- VM stack size 4,096
- 16 exception backtrace frames

Boa syntax errors, ordinary exceptions, resource-limit failures, host errors, and script-load failures map to distinct `ScriptErrorKind` values. Page-script failures are retained in `DocumentState::script_executions` and do not abort an otherwise valid document commit. Runtime bootstrap failure and explicit network cancellation remain fatal.

## Supported sources

The W25 path executes inline and external classic scripts. External resources use a JavaScript-oriented `Accept` header, follow the existing redirect policy, retain the final URL, honor an HTTP charset when present, and otherwise decode as UTF-8.
