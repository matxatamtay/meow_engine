# W35 storage

`localStorage` and `sessionStorage` are partitioned by tuple origin. Opaque origins receive a security error.

Each area supports `length`, `key`, `getItem`, `setItem`, `removeItem`, and `clear`. The default quota is five MiB per origin. A failed write restores the previous value rather than leaving a partially updated map.

`sessionStorage` lives for one `BrowserEngine` instance. `localStorage` is synchronously persisted as deterministic JSON under the browser profile. The desktop shell defaults to `artifacts/profile`; set `MEOW_PROFILE_DIR` to select another profile. Integration tests prove reload persistence, profile restart persistence, session reset, origin partitioning, and quota rollback.
