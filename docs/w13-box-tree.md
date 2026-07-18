# W13 box tree

W13 introduces a formatting tree that is generated from the DOM and computed styles but stored independently from both.

## Generation subset

- `display:none` generates no principal box and suppresses the complete element subtree.
- `display:block` generates a principal block box.
- `display:inline` generates a principal inline box.
- `inline-block` is treated as inline-level for W13 tree construction.
- `flex` and `grid` are treated as block-level containers until their own layout algorithms exist.
- Non-whitespace text becomes a text-run box with collapsed whitespace.
- Comments, doctypes, and processing instructions do not generate boxes.

When a block container has both block-level children and inline-level content, each contiguous inline-level run is wrapped in an anonymous block box. A block containing only inline-level children keeps them directly because no block/inline mixture needs normalization.

## Identity and ownership

A `BoxTree` owns `BoxNode` values and assigns deterministic `BoxId` values. Principal boxes and text runs retain only a stable DOM `NodeId`; anonymous boxes have no DOM source. The tree does not retain `NodeHandle`, DOM arena borrows, or computed-style references.

`BoxTree::dump()` starts with `#box-tree` and is intentionally distinct from `Document::dump()`.

## APIs

```rust
let styles = compute_styles(&document, &stylesheets);
let tree = build_box_tree(&document, &styles);
println!("{}", tree.dump());
```

A committed `DocumentState` also exposes `box_tree()` and `dump_box_tree()`.

## Tests

Four file-backed fixtures cover block-only trees, inline content, anonymous wrappers, and `display:none`. Regenerate deliberately with:

```bash
UPDATE_W13_SNAPSHOTS=1 cargo test --locked -p meow-engine --test box_tree_snapshots
```

Layout geometry, line boxes, floats, positioned layout, generated content, and pseudo-elements remain outside W13.
