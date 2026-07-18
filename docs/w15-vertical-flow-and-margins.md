# W15 vertical flow and margins

W15 adds vertical normal flow to the horizontally resolved W14 layout tree. The result remains an integer-pixel structural layout snapshot and does not yet shape or paint text.

## Normal-flow subset

- block children flow from top to bottom inside the parent content box;
- contiguous inline-level children share one 16px line-height baseline box;
- text runs have a deterministic 16px structural height;
- `height`, `min-height`, and `max-height` resolve with `content-box` or `border-box` sizing;
- explicit or constrained heights retain overflowing descendants and expose overflow metadata;
- percentage heights resolve only when the containing block has a definite height;
- root percentage heights use the viewport height.

## Margin collapsing

W15 collapses only adjacent block sibling vertical margins:

- two positive margins use the larger value;
- two negative margins use the more negative value;
- mixed signs are added.

Parent-child collapsing, empty-block through-collapse, clearance, floats, and formatting-context exceptions remain outside this milestone.

## Overflow metadata

Each `LayoutBox` reports horizontal and vertical overflow booleans plus `scroll_width` and `scroll_height`. The engine does not create scrollbars or clip content at W15.

## APIs

```rust
let layout = layout_normal_flow(&boxes, &styles, LayoutViewport::new(800, 600));
println!("{}", layout.dump());
```

`DocumentState::flow_layout()` and `dump_flow_layout()` provide the same pipeline for a committed document.

## Fixtures

Six file-backed snapshots cover simple flow, positive and negative margin collapse, overflow, height constraints, and a one-column article with header, lead, sections, and footer.

```bash
UPDATE_W15_SNAPSHOTS=1 cargo test --locked -p meow-engine --test vertical_flow_snapshots
```

Text shaping, line breaking, inline widths, floats, positioning, scrolling, and fragmentation remain future work.
