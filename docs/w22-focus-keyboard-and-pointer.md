# W22 focus, keyboard, and pointer input

W22 adds deterministic focus and default actions without coupling engine behavior to winit.

## Event flow

The desktop shell maps platform events into `InteractionPoint` and `KeyboardCommand`. The engine owns pointer-down, pointer-up, focus, activation, text editing, checkbox state, and navigation results.

A pointer press records the target and moves focus. Pointer release activates only when it lands on the same target, preventing drag-out clicks. Activation results can request repaint or return a canonical navigation URL.

## Focus chain

Links, supported inputs, checkboxes, and buttons enter DOM-order focus navigation. Tab moves forward and Shift+Tab moves backward with wraparound. A visible focus ring is painted by the engine display list, so CPU and GPU backends receive identical commands.

## Keyboard defaults

- Enter activates a focused link or submit button.
- Space activates a focused button or toggles a checkbox.
- Enter in a text or search input submits its ancestor GET form.
- Backspace edits the focused text control.
- Character keys append logical text after platform key mapping.

The browser shell also maps Alt+Left, Alt+Right, and Ctrl+R or Command+R to history and reload actions.
