# W24 static browser alpha

Release label: **v0.1-static-alpha**

The alpha is a Linux-first browser shell for static documentation and blog-style pages over HTTP or HTTPS. It connects the existing HTML, CSS, layout, fragment, display-list, CPU, and GPU layers to scrolling, hit testing, focus, links, session history, and a GET form subset.

## Run the demo corpus

Start a local static server:

```bash
python3 -m http.server 8000 -d demo/static-alpha
```

In another terminal, launch either renderer:

```bash
cargo run -p meow-browser -- http://127.0.0.1:8000/
cargo run -p meow-browser -- --renderer=cpu http://127.0.0.1:8000/
```

The corpus includes a home page, long scroll article, linked stylesheet, focusable controls, checkbox state, GET search submission, and links suitable for back and forward traversal.

## Controls

- Mouse wheel or trackpad: scroll
- Left click: focus and activate
- Tab or Shift+Tab: move focus
- Enter: activate link or submit
- Space: toggle checkbox or activate button
- Alt+Left or Alt+Right: back or forward
- Ctrl+R or Command+R: reload

## Quality and performance gates

The canonical repository gate remains:

```bash
bash scripts/verify.sh
```

The W21 release benchmark is:

```bash
cargo run --release -p meow-engine --example scroll_benchmark
```

The recorded alpha baseline is 6.306 ms per cached-layout scroll frame, approximately 158.6 FPS, on the current development machine.

## Release boundary

This release does not execute JavaScript and is not a general-purpose secure web browser. It is suitable for controlled static corpora and engine demonstrations. Network sandboxing, cookies, cache, authentication, accessibility, complete forms, images, production text rasterization, and broad CSS coverage remain future work.
