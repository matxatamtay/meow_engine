# W43 network process and broker

W43 introduces a permission-mediated loader boundary. In multiprocess mode the
content process receives a brokered `Loader`; only the network child constructs
the direct Hyper/Rustls loader.

## Request boundary

Content sends a bounded HTTP request DTO containing method, URL, headers, body,
document URL, and credentials mode. The broker rejects:

- non-HTTP(S) schemes;
- `CONNECT` and `TRACE`;
- bodies larger than 8 MiB at the broker boundary;
- caller-supplied `Cookie`, `Host`, proxy, upgrade, and other hop-by-hop headers.

Responses carry status, headers, body, redirect history, final URL, HTTP
version, content metadata, and bounded timing fields. Request IDs are checked
on both content and network channels.

## Cookie and cache ownership

The direct loader now owns the cookie jar and response cache. The content-side
Fetch adapter supplies only document and credentials context; it cannot inject
a raw Cookie header.

The current cookie subset applies secure, domain, path, simplified schemeful
same-site, and `SameSite=None; Secure` rules. The cache is a conservative,
bounded in-memory GET cache: 64 entries, 16 MiB total, and 1 MiB per response.
It skips credential-bearing requests, `Set-Cookie`, `private`, `no-store`,
non-success responses, and request bodies.

## No direct content sockets

The content child connects its Unix IPC channels before sandbox activation.
The W44 seccomp filter then denies creation and connection of new sockets. The
multiprocess HTTP smoke test starts a local server, loads it successfully
through the network child, and confirms the document title in content while
that socket denial is active.

WebSocket transport has not yet moved into the broker. In isolated content mode
WebSocket creation therefore fails closed with an error/close event. Use
`--single-process` for the W36 direct WebSocket path until a framed WebSocket
broker is implemented.

## Verification

```bash
cargo test -p meow-net
cargo test -p meow-process-model --test process_smoke
```
