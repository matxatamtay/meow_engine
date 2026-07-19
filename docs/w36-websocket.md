# W36 WebSocket

The document realm exposes a WebSocket EventTarget with `open`, `message`, `error`, and `close` events, handler properties, ready-state constants, text and binary sends, ArrayBuffer binary delivery, close codes, reasons, and subprotocol negotiation.

The engine performs the HTTP upgrade through tokio-tungstenite, includes the document Origin header, handles ping/pong, and moves frames through bounded document-task pumping. Navigation closes sockets owned by the previous document.

The integration test runs a real local echo server and verifies handshake, text, binary, and a clean close. A bundled broadcast chat server and page provide the W36 acceptance demo.

```bash
cargo run -p meow-net --example websocket_chat
python3 -m http.server 8004 -d demo/websocket-chat
cargo run -p meow-browser -- http://127.0.0.1:8004/
```
