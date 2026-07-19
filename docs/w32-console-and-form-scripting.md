# W32 console and form scripting

The runtime exposes `console.log`, `console.info`, `console.warn`, and `console.error`. Messages are retained by the document runtime and forwarded to structured tracing by the desktop shell.

Text controls expose `value`; checkboxes expose `checked`. Native keyboard and pointer state is mirrored into DOM attributes before validation and submit dispatch, so JavaScript reads the same state the user sees.

The basic validation subset checks required text-like controls and required checkbox/radio controls. Invalid controls receive a cancelable non-bubbling `invalid` event. Valid form activation dispatches a cancelable bubbling `submit` event before GET navigation. `preventDefault()` supports client-side mini-apps.

Run the bundled counter and todo page with:

```bash
python3 -m http.server 8002 -d demo/interactive-alpha
cargo run -p meow-browser -- http://127.0.0.1:8002/
```
