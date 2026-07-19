# W29 EventTarget

Each committed document now keeps its Boa realm alive after navigation. `window`, `document`, and element wrappers share an `EventTarget` implementation inside that realm.

The supported subset includes `addEventListener`, `removeEventListener`, capture, target, bubble, `preventDefault`, `stopPropagation`, `stopImmediatePropagation`, and `{ once: true }`. Rust supplies a stable ancestor path from generational `NodeId` handles; callback identity and listener lifetime remain managed by JavaScript.

Pointer and keyboard activation dispatch a cancelable bubbling `click` before native default actions. A canceled event blocks link navigation, checkbox toggling, and submit-button activation. Tests cover phase ordering, one-shot listeners, default prevention, and a real browser-engine click path.
