# W21 scrolling and hit testing

W21 introduces a backend-neutral interaction geometry layer built from committed layout and fragment output.

## Scroll model

`DocumentView` owns a `ScrollTree` for one viewport. Node zero represents the root viewport clip and records total document width and height. Layout boxes with overflow metadata are also exposed as nested `ScrollNode` entries, while the alpha input path intentionally scrolls only the root viewport.

`InteractionState::scroll_by()` clamps offsets to the root content bounds. Scrolling does not rebuild DOM, style, boxes, layout, or fragments. The cached document view produces a new display list by translating box and glyph paint coordinates and clipping them to the viewport.

## Hit testing

The final layout tree and glyph fragments are folded into document-space source bounds. Interactive elements become a stable `HitTestList` in paint order. Pointer coordinates arrive in viewport space, then the current scroll offset converts them back to document space before reverse-order hit testing.

The public list distinguishes links, text inputs, checkboxes, and buttons. It contains source `NodeId`, geometry, kind, and a readable label without retaining DOM borrows.

## Performance baseline

Run the target benchmark with:

```bash
cargo run --release -p meow-engine --example scroll_benchmark
```

The baseline corpus contains 1,000 block lines at a 1280 by 800 viewport and renders 600 cached-layout scroll frames. The release run used for the static alpha measured **6.306 ms per frame**, approximately **158.6 FPS**, against a 16.667 ms 60 FPS budget. This is a developer-machine baseline, not a cross-device guarantee.
