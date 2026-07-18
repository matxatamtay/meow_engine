# W16 paint backgrounds and borders

W16 lowers the W15 layout tree into the backend-neutral display list and renders it through the existing CPU and GPU backends.

## Paint order

The baseline painter emits:

1. a white canvas clear;
2. one viewport clip push;
3. each box background in depth-first tree order;
4. that box's top, right, bottom, and left border strips;
5. child boxes in document/tree order;
6. one matching clip pop.

Backgrounds therefore sit beneath borders, and descendants paint above their ancestors. This is the baseline stacking order only; positioned descendants, opacity groups, transforms, and stacking contexts remain outside W16.

## Colors and borders

`background-color` paints the border-box area, with border strips painted afterward. Until the property registry gains `border-color`, border strips use the element's computed `color`. Transparent backgrounds are skipped. CPU alpha blending uses source-over.

## Clips

`DisplayCommand` now includes `PushClip` and `PopClip`. Both tiny-skia and Vello lowering maintain an intersection stack. The layout painter pushes only the viewport clip, so W15's default visible overflow is preserved rather than accidentally behaving as `overflow:hidden`.

## API

```rust
let display_list = build_layout_display_list(&layout, &styles, viewport)?;
let frame = ReferenceRenderer::new().render(viewport, &display_list)?;
```

`DocumentState::display_list()` exposes the complete computed-style, box-tree, flow-layout, and paint pipeline.

## 500 visual fixtures

The visual conformance test generates exactly 500 full-pipeline cases from this compact matrix:

```text
5 widths × 5 heights × 5 paddings × 4 border widths = 500
```

Each case parses HTML and CSS, computes styles, generates boxes, resolves normal flow, builds display-list commands, rasterizes a 96×96 CPU framebuffer, and compares an individual FNV-1a hash of the premultiplied RGBA bytes. The 500 expected hashes live in `crates/engine/tests/fixtures/visual-500.hashes`.

Regenerate deliberately with:

```bash
UPDATE_W16_VISUALS=1 cargo test --locked -p meow-engine --test visual_500
```

Text glyph painting, images, gradients, border colors/styles/radii, shadows, filters, transforms, compositing groups, and full stacking contexts remain future work.
