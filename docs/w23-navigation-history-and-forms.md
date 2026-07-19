# W23 navigation, history, and forms

W23 upgrades top-level navigation from append-only history into a session-history cursor and adds a deliberately bounded form subset.

## Session history

`Navigator` exposes `can_go_back`, `can_go_forward`, `back`, `forward`, and `reload`. Traversal re-fetches and parses the selected entry. A failed load leaves the current document, cursor, and history list unchanged.

A new navigation after moving backward truncates the old forward branch before appending the committed entry. Reload keeps the same index and history length.

## Supported controls

The alpha supports:

- text and search inputs
- hidden inputs for submission data
- checkboxes
- input buttons and submit inputs
- button elements

Disabled controls are excluded. Live input and checkbox values are held in `InteractionState`, separate from immutable DOM attributes. A small alpha control-layout pass gives supported controls stable non-overlapping hit geometry while the broader inline replaced-element layout model remains future work.

## GET submission

Ancestor forms with missing method or `method=get` are supported. Submission resolves the action against the committed base URL, percent-encodes successful name/value pairs, includes checked checkboxes, includes the activated submit button, and navigates through the ordinary top-level pipeline.

POST bodies, validation, selects, textareas, radio groups, file inputs, reset behavior, autocomplete, and JavaScript form events are outside this release.
