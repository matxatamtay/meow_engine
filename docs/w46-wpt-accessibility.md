# W46 selected WPT, layout, and accessibility basics

W46 extends the selected suite with CPU pixel reftests, roles, accessible names,
sequential focus order, and a keyboard audit.

`meow-accessibility` supports a bounded subset: implicit/explicit document,
landmark, heading, link, button, textbox, checkbox, image, list, paragraph, and
generic roles; names from ARIA, labels, alt/value/placeholder/title and subtree
text; hidden/disabled filtering; and positive/zero/negative `tabindex` order.
The audit reports unnamed interactive controls and duplicate positive tabindex.

The browser interaction layer uses the same focus-order function, so test and
runtime keyboard behavior share one implementation. AT-SPI/UIA/AX integration,
complete AccName/ARIA, live regions, and broad states are year-two work.

```bash
cargo xtask wpt --check
```

Current selected pass rate: 20/20, 100%.
