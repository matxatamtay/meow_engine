# W38 transforms and opacity

The computed-style layer parses a deterministic 2D subset: `translate`, `translateX/Y`, `scale`, `scaleX/Y`, `rotate` in degrees or turns, and six-value `matrix`. Operations lower to a fixed-point affine matrix around the border-box center.

Display lists now contain explicit `PushLayer` and `PopLayer` commands plus stacking-context metadata: context ID, parent, affine transform, opacity, and command range. Backgrounds, borders, text, descendants, and images stay inside the same context.

The CPU renderer paints each context into an offscreen pixmap and composites opacity once. Pixel reftests verify translated opacity groups and rotated geometry. The GPU path carries transform and opacity metadata into Vello, but its opacity implementation remains a per-primitive approximation until Vello layer compositing is integrated.
