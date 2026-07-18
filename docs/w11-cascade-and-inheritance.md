# W11 cascade and inheritance

W11 turns parsed declarations and W10 selector matches into deterministic computed styles.
The stage deliberately stops before layout, used-value resolution, and painting.

## Crate boundaries

- `meow-css` owns the supported property registry, initial values, inheritance flags, and
  parsing of `inherit`, `initial`, and `unset`.
- `meow-html` exposes read-only element traversal, parent lookup, names, attributes, and the
  W10 selector matcher. The DOM arena remains private.
- `meow-engine` owns stylesheet origins, matched-rule collection, cascade winner selection,
  inheritance, computed-style storage, diagnostics, and snapshots.

`DocumentState::computed_styles()` computes author sheets attached during navigation.
Call `compute_styles()` directly with `CascadeStylesheet` inputs to include user-agent or
user stylesheets. `DocumentState` treats empty media, `all`, and `screen` sheets as active;
other media values remain loaded but do not participate until a full media evaluator exists.

## Cascade order

W11 compares declarations using this key, from least to greatest precedence:

```text
(origin + importance, selector specificity, stylesheet order, rule order, declaration order)
```

Normal declaration origins are:

```text
user-agent < user < author
```

Important declaration origins reverse:

```text
author !important < user !important < user-agent !important
```

Within one selector list, only selectors that match the element participate. The greatest
specificity among those matching selectors is used for that rule. Later stylesheets, rules,
and declarations break otherwise equal ties.

Animations, transitions, cascade layers, scopes, and presentation hints are outside W11.

## Supported property subset

W11 computes these 13 longhands:

| Property | Inherited | Initial value |
| --- | --- | --- |
| `display` | no | `inline` |
| `color` | yes | `black` |
| `background-color` | no | `transparent` |
| `font-family` | yes | `serif` |
| `font-size` | yes | `medium` |
| `font-style` | yes | `normal` |
| `font-weight` | yes | `normal` |
| `line-height` | yes | `normal` |
| `text-align` | yes | `start` |
| `visibility` | yes | `visible` |
| `opacity` | no | `1` |
| `width` | no | `auto` |
| `height` | no | `auto` |

Unknown properties, custom properties, empty values, and shorthands are ignored by the W11
semantic stage. Non-keyword values remain normalized source text. Property-specific grammar,
unit conversion, percentages, `currentColor`, and used-value resolution remain later work.

## Inheritance and CSS-wide keywords

Elements are computed parent-first in document tree order.

- no winning declaration: inherited properties take the parent computed value; other
  properties take their initial value;
- `inherit`: takes the parent value for any property, or the initial value at the root;
- `initial`: takes the property initial value;
- `unset`: behaves as `inherit` for inherited properties and `initial` for all others.

`revert`, `revert-layer`, variables, and invalid-at-computed-value-time behavior are not
implemented yet.

## Stable snapshots

`ComputedStyleSnapshot::dump()` emits every element in document tree order and every property
in registry order. It serializes the arena slot but intentionally omits the process-global
document ID, so two equivalent parses produce byte-identical output.

The file-backed suite under `crates/engine/tests/fixtures/computed-style/` covers:

- normal and important origin precedence;
- selector specificity and source-order ties;
- inherited defaults;
- explicit `inherit`, `initial`, and `unset`;
- unsupported selector diagnostics;
- complete, deterministic property output.

Regenerate snapshots deliberately with:

```bash
UPDATE_W11_SNAPSHOTS=1 cargo test --locked -p meow-engine --test computed_style_snapshots
```

Run the canonical repository gate with:

```bash
bash scripts/verify.sh
```
