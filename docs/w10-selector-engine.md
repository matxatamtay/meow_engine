# W10 selector engine

W10 turns the raw selector preludes retained by W9 into a semantic selector model and
matches that model against the parsed HTML document tree. Cascade, computed values, and
layout remain later stages.

## Crate boundaries

- `meow-css` owns selector tokenization, validation, ASTs, `An+B` parsing, and specificity.
  `StyleRule::selector_list()` parses the raw W9 prelude without changing the stable W9
  stylesheet dump format.
- `meow-html` depends on the semantic selector model and performs right-to-left matching
  against its generational DOM arena. It exposes `matches_selector`,
  `matches_selector_list`, `query_selector`, and `query_selector_all` on `Document`.
- `meow-engine` continues to own document and stylesheet orchestration. W10 does not yet
  apply declarations or build a cascade.

## Supported selector subset

### Basic selectors

- universal selector: `*`;
- HTML type selectors, matched ASCII-insensitively for HTML elements;
- ID selectors: `#app`;
- class selectors: `.card`;
- attribute existence and value selectors:
  - `[name]`
  - `[name=value]`
  - `[name~=word]`
  - `[name|=prefix]`
  - `[name^=prefix]`
  - `[name$=suffix]`
  - `[name*=fragment]`
- attribute `i` and `s` comparison modifiers.

Attribute names are ASCII-insensitive in the HTML DOM adapter. Attribute values are
case-sensitive by default; the `i` modifier enables ASCII-insensitive comparison.
Empty operands for `^=`, `$=`, and `*=` do not match.

### Combinators

Matching runs right-to-left for:

- descendant: `article .card`;
- child: `article > .card`;
- adjacent sibling: `h1 + p`;
- general sibling: `h1 ~ p`.

Text and comment nodes are ignored for sibling position and combinator traversal where CSS
requires element siblings.

### Structural pseudo-classes

W10 supports:

- `:root`
- `:empty`
- `:first-child`, `:last-child`, `:only-child`
- `:nth-child()`, `:nth-last-child()`
- `:first-of-type`, `:last-of-type`, `:only-of-type`
- `:nth-of-type()`, `:nth-last-of-type()`

`An+B`, `odd`, `even`, positive, zero, and negative coefficients use the `cssparser`
implementation and one-based element indices. `:empty` ignores comments but treats any text
node, including whitespace-only text, as content.

## Specificity

Specificity is stored as three saturating counters:

```text
(id selectors, class/attribute/pseudo-class selectors, type selectors)
```

Combinators and the universal selector contribute no specificity. Selector lists expose the
maximum member specificity while every `ComplexSelector` retains its own value.

## Explicitly unsupported in W10

The parser rejects unsupported syntax rather than silently broadening the match:

- pseudo-elements;
- dynamic pseudo-classes such as `:hover`, `:focus`, and `:checked`;
- logical selector functions such as `:is()`, `:not()`, `:where()`, and `:has()`;
- namespace-qualified selectors;
- shadow-DOM and scoped-selector semantics;
- the `of <selector-list>` extension in `:nth-child()`.

## Internal conformance suite

The file-backed suite under `crates/html/tests/fixtures/selectors/` contains:

- one deterministic HTML document where every element has an ID;
- 70 valid selector cases with expected matching IDs in tree order;
- 19 malformed or unsupported selectors that must be rejected.

The cases cover parser escapes, all supported attribute operators, all combinators,
selector-list deduplication, structural pseudo-classes, and deeply chained selectors.
The existing 100 W9 CSS syntax golden fixtures remain byte-for-byte stable.

Run the focused suite with:

```bash
cargo test --locked -p meow-css
cargo test --locked -p meow-html --test selector_conformance
```

Run the canonical repository gate with:

```bash
bash scripts/verify.sh
```
