# W12 values and invalidation

W12 turns W11's canonical declaration strings into typed computed values and adds a retained
style cache that can respond to explicit DOM mutation records without recomputing the complete
document. Layout and painting remain later stages.

## Crate boundaries

- `meow-css` owns the W12 property registry, fixed-precision numbers, typed lengths, colors,
  display values, box values, property validation, and the bounded box shorthand expansion.
- `meow-html` owns explicit arena mutation APIs and returns `DomMutation` records describing
  attribute, character-data, and child-list changes. No-op mutations return `None`.
- `meow-engine` owns custom-property resolution, typed computed styles, selector dependency
  summaries, dirty flags, cached generations, mutation invalidation, and partial restyle.

The W9 stylesheet dump remains byte-compatible. The syntax parser now retains complete nested
component values for semantic consumers, while the legacy dump serializer replays the original
W9 truncation behavior for its 100 historical golden fixtures.

## Typed property subset

W12 computes 26 longhands. The original W11 properties remain, plus:

```text
margin-top/right/bottom/left
padding-top/right/bottom/left
border-top/right/bottom/left-width
box-sizing
```

The bounded value grammar includes:

- lengths: unitless zero plus `px`, `em`, `rem`, and `%`;
- `auto` for width, height, and margins;
- named colors, `transparent`, `currentColor`, and `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`;
- display: `none`, `block`, `inline`, `inline-block`, `flex`, and `grid`;
- opacity as a fixed-precision number clamped to `0..1`;
- border widths: non-negative lengths plus `thin`, `medium`, and `thick`;
- `content-box` and `border-box`;
- validated W12 subsets for font size/style/weight, line height, text alignment, and visibility.

`margin`, `padding`, and `border-width` expand their one-to-four component forms into longhands.
A variable used as the entire shorthand must resolve to one component in this bounded stage;
token-list re-expansion after `var()` substitution is future work.

`ComputedStyle::get()` continues to return canonical CSS text. `ComputedStyle::typed()` returns
the typed value, and `ComputedStyleSnapshot::dump_typed()` emits all 26 properties with their
value kinds. `DocumentState::dump_typed_computed_styles()` exposes the same one-shot view for a
committed navigation. The W11 `dump()` format intentionally remains limited to its original 13
properties.

## Custom properties subset

Custom property names remain case-sensitive and participate in the normal cascade. They inherit
by default. W12 supports recursive `var(--name)` substitution, nested fallbacks, inherited
variables, missing-variable fallback, and deterministic cycle diagnostics.

The subset does not implement `@property`, registered initial values, animation tainting,
environment variables, token-stream reserialization rules, or invalid-at-computed-value-time
behavior beyond falling back to the property's inherited or initial result.

## Mutation and dirty flags

Mutation is explicit. Call a `Document` mutation method, pass the returned `DomMutation` to
`StyleEngine::invalidate()`, then call `restyle_dirty()`.

```rust
let mutation = document
    .set_element_attribute(&element, "class", "active")?
    .expect("the attribute changed");
let invalidation = styles.invalidate(&mutation);
let restyle = styles.restyle_dirty();
```

Dirty states are `Clean`, `SelfOnly`, and `Subtree`. The selector dependency summary drives the
initial invalidation roots:

- a simple attribute mutation starts at the element itself;
- child/descendant selectors conservatively expand that element to its subtree;
- `+` invalidates only the next element sibling, while `~` invalidates following siblings;
- character-data changes restyle the parent element only when `:empty` is present;
- child-list changes restyle the parent subtree only for structural or sibling-sensitive
  selectors; otherwise only an `:empty` parent and newly connected element subtrees are dirty;
- removed nodes are deleted from the style and diagnostic caches immediately.

After recomputing a dirty node, descendants are added only when inherited computed values or
resolved custom properties changed. Thus an attribute mutation can start as `SelfOnly` and grow
to exactly one inheritance branch. Unrelated siblings retain their previous style generation.

The dependency summary is feature-level rather than a per-rule selector index. It may restyle a
subtree conservatively when any active selector uses an ancestor combinator, but it never falls
back to an unconditional whole-document restyle.

## Stable snapshots

W12 adds two file-backed artifacts under `crates/engine/tests/fixtures/w12/`:

- `typed-values` covers canonical typed serialization, box longhands, shorthand expansion,
  custom-property inheritance, Unicode before `var()`, fallback, `currentColor`, and cycles;
- `invalidation` records mutation roots, dirty nodes, restyled and changed nodes, selected
  computed values, and per-node generations. Its unrelated `tail` branch remains generation 1.

Regenerate deliberately with:

```bash
UPDATE_W12_SNAPSHOTS=1 cargo test --locked -p meow-engine --test w12_snapshots
```

Run the canonical repository gate with:

```bash
bash scripts/verify.sh
```

## Explicit limits

W12 does not implement `calc()`, viewport/physical units, RGB/HSL functions, system colors,
full font parsing, shorthand token-list re-expansion, style attributes, selector invalidation
indexes, shadow DOM, layout, used values, or paint invalidation.
