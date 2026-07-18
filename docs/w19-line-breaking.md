# W19 line breaking

W19 converts shaped text into deterministic line boxes.

## Whitespace and wrapping

The `white-space: normal` subset collapses each whitespace sequence to one ASCII space and removes leading and trailing whitespace. A greedy wrapper measures complete candidate lines with the W18 shaper. Words move intact when they fit; words wider than the containing width are hard-wrapped at combining-mark-aware clusters.

Each line is shaped independently after logical wrapping, allowing Arabic and mixed-direction visual ordering to restart correctly per line.

## Line boxes and alignment

Line boxes use a fixed 16px line height and 12px baseline. They contain positioned visual runs and glyphs, used width, available width, and alignment offset.

Supported alignment values are `start`, `end`, `left`, `right`, `center`, and `justify`. Start and end depend on paragraph direction. Justification distributes remaining integer pixels across spaces on non-final lines, including deterministic remainder distribution from left to right in visual order.

## Fixtures

Six snapshots cover narrow, medium, and wide paragraph widths, centered alignment, RTL start alignment, and justification. The same paragraph therefore has byte-stable but intentionally different line breaks across viewports.
