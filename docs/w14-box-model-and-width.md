# W14 box model and width resolution

W14 adds deterministic horizontal geometry to the W13 box tree. Layout uses integer CSS pixels and remains separate from DOM and paint.

## Supported sizing

- containing blocks formed by the parent content box;
- `width:auto` fill for block-level boxes;
- fixed and percentage widths;
- left/right auto margins, including centered blocks;
- margin, padding, and border-width edges;
- `content-box` and `border-box` sizing;
- `min-width`, `max-width`, `min-height`, and `max-height` typed values;
- `px`, `%`, `em`, and `rem` resolution with a fixed 16px font metric baseline for this stage.

`thin`, `medium`, and `thick` border widths resolve to 1px, 3px, and 5px. Fractions are truncated toward zero after fixed-point arithmetic, keeping snapshots independent of floating-point behavior.

When min/max constraints change a selected width, the block-width equation is rerun so auto margins and over-constrained right margins are resolved against the constrained size. Constraints apply to the box selected by `box-sizing`.

## APIs

```rust
let boxes = build_box_tree(&document, &styles);
let layout = layout_box_tree(&boxes, &styles, LayoutViewport::new(800, 600));
println!("{}", layout.dump());
```

`LayoutBox` exposes content geometry plus used margin, padding, border, border-box dimensions, and containing-block width.

## Reftests

Six file-backed reftests cover auto width, content-box, border-box, auto margins, min/max constraints, and nested percentage containing blocks.

```bash
UPDATE_W14_SNAPSHOTS=1 cargo test --locked -p meow-engine --test layout_width_reftests
```

Vertical placement, intrinsic inline sizing, line layout, replaced elements, floats, and positioned layout remain outside W14.
