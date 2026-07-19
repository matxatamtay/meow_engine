# W33 fetch pipeline

The live document realm now exposes `Headers`, `Request`, `Response`, `fetch`, `AbortController`, and `AbortSignal`.

JavaScript request objects are normalized into a backend-neutral descriptor. The engine resolves relative URLs against the committed document, validates methods and headers, then uses the existing Tokio/Hyper loader. Fetch completions resolve or reject the original Boa promise and drain microtasks before the next web task.

The supported body surface is deliberately buffered: strings, ArrayBuffer views, `text()`, `json()`, `arrayBuffer()`, `bodyUsed`, and `clone()`. Redirect modes support `follow` and `error`. Abort reliably cancels work that has not entered the loader yet.

The integration suite verifies same-origin JSON, redirect metadata, body consumption, an immediately aborted request, and a chained cookie fetch.
