# W41 IPC schema

W41 adds `meow-ipc`, a small transport-independent framing layer shared by the
content and network process protocols.

## Envelope

Every message contains:

- protocol major/minor version;
- message kind (`request`, `response`, or `event`);
- a monotonically allocated 64-bit request ID;
- either a typed payload or a typed remote error.

Protocol `1.0` accepts the same major version and a peer minor version no newer
than the local decoder. Unknown major versions and future minor versions fail
closed before payload dispatch.

The current payload codec is JSON. Process-specific DTOs avoid serializing
engine implementation types directly. Values that are awkward or unsupported
on the JSON wire, such as internal `u128` timing counters, are converted to
bounded `u64` wire fields and expanded again at the API boundary.

## Framing and caps

`StreamTransport` uses a four-byte big-endian length prefix followed by the
encoded envelope. One frame is capped at 32 MiB. The receiver validates the
announced length before allocating its body buffer. The cap is deliberately
large enough for the current image-backed display list while still bounding a
hostile or corrupted peer.

`FrameTransport` is the abstraction boundary. The W42-W43 Linux process model
uses blocking Unix streams, but envelope encoding and validation do not depend
on Unix sockets.

## Error behavior

Malformed JSON, invalid payload/error combinations, unsupported versions,
oversized frames, unexpected message kinds, and request-ID mismatches are
reported as typed `IpcError` values. Remote failures use a stable string code,
human-readable message, and retryability bit rather than exposing crate-local
error enums.

## Verification

Run:

```bash
cargo test -p meow-ipc
```

The suite covers stable round trips, compatibility decisions, frame prefixing,
cap-before-allocation, and a deterministic hostile-byte corpus that asserts the
decoder never panics.
