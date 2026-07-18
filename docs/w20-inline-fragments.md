# W20 inline fragments and readable text paint

W20 completes the first text-to-pixels path while keeping DOM, box tree, layout tree, fragment tree, display list, and renderer as separate layers.

## Fragment tree

`layout_fragment_tree()` performs a provisional block layout, measures paragraphs with W19 line boxes, reruns vertical flow with measured inline heights, and then creates final page-space fragments. Following blocks therefore move below wrapped paragraphs instead of overlapping them.

Every paragraph, line, and glyph receives a deterministic `FragmentId`. Glyph fragments retain `BoxId`, optional source `NodeId`, font identity, script, direction, cluster, baseline, advance, color, weight, slant, and text decorations. No fragment stores DOM handles or arena borrows.

Raw DOM whitespace is retained privately by text boxes so CSS-normal collapse can preserve authored boundaries without changing the W13 box-tree dump.

## Inline style subset

W20 paints computed `color`, `font-weight`, `font-style`, and `text-decoration-line`. The decoration subset supports `none`, `underline`, `line-through`, and both lines together. The W11 and W12 snapshot property lists remain frozen even though the full registry now contains 31 longhands.

## Deterministic pixel font

The backend-neutral painter lowers glyphs to rectangle fills using a built-in 5x7 bitmap alphabet. Bold adds a second horizontal stroke, italic applies row skew, combining marks receive a small accent stroke, and Vietnamese precomposed letters map to readable Latin base forms. Underline and line-through use fragment baselines and advances.

This painter is deliberately a deterministic conformance font, not a replacement for outline rasterization. Host-installed fonts never affect fixture pixels.

## APIs

```rust
let output = layout_fragment_tree(&boxes, &styles, viewport, &mut fonts);
let list = build_fragment_display_list(
    &output.layout,
    &styles,
    &output.fragments,
    paint_viewport,
)?;
```

Committed documents expose `fragment_layout()`, `dump_fragments()`, and `readable_display_list()`.

## Fixtures

Three file-backed article/blog fixtures verify fragment dumps and full CPU framebuffer hashes. They cover mixed bold/italic/color/underline spans, justified wrapping, centered metadata, Vietnamese text, block backgrounds, borders, and two-pass vertical placement.

Outline glyph rasterization, font-size scaling, full decoration propagation, inline backgrounds/borders, ruby, vertical writing, selection geometry, and complete Unicode shaping remain future work.
