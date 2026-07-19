# W27 Node and Element bindings

The first DOM scripting subset reuses MeowEngine's existing traversal, selector, and explicit-mutation APIs.

## Node and Element surface

`Node` supports:

- `textContent` getter and setter
- `parentElement`

`Element` supports:

- `localName`
- `firstElementChild`
- `nextElementSibling`
- `getAttribute()`
- `setAttribute()`
- `removeAttribute()`
- scoped `querySelector()`

Selectors are parsed by the W10 selector engine. Invalid or unsupported selector syntax becomes a JavaScript `SyntaxError`.

## Mutation and style visibility

Attribute and text replacements emit ordinary `DomMutation` records. Script mutations are retained in `DocumentState::script_mutations`. Navigation executes scripts before the committed document view is built, so the first post-script style, layout, fragment, and paint pass observes the mutated DOM.

The integration suite proves this by setting a class from JavaScript and then checking that the matching author rule changes the element's computed color. The embedder integration also verifies that a script-mutated title reaches the browser window-title path.

Dynamically added stylesheet or script elements are not rediscovered in this milestone. Mutation-driven rendering after later event callbacks is also future work because W25-W28 do not yet expose DOM events or timers.
