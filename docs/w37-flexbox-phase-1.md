# W37 flexbox phase 1

`display:flex` now creates a dedicated flex formatting context rather than falling through to block flow. The typed CSS layer supports `flex-direction`, `flex-grow`, `flex-shrink`, `flex-basis`, `justify-content`, `align-items`, `gap`, and the bounded `flex` shorthand.

The layout pass handles one flex line on the row or column main axis. It resolves length and auto bases, distributes positive free space by grow factors, distributes negative free space by shrink factor × basis, places deterministic gaps, and supports start/end/center/space distribution. Replaced images participate with intrinsic dimensions and aspect ratios.

Focused layout tests cover a common navigation row and equal card shrink. The bundled landing page combines navigation, hero, cards, and a metric strip.
