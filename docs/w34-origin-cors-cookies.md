# W34 same-origin, CORS, and cookies

Fetch compares the caller origin with the target and final response origins. `mode: "same-origin"` rejects cross-origin requests. `mode: "cors"` sends an Origin header, validates `Access-Control-Allow-Origin`, supports credential checks, and performs an OPTIONS preflight for non-simple methods or headers.

The preflight subset validates allowed methods and headers. Response headers are filtered to CORS-safelisted or explicitly exposed names. `mode: "no-cors"` accepts only simple requests and returns an opaque response for cross-origin results.

The in-memory cookie jar implements host/domain matching, path matching, Secure, HttpOnly retention, credentials modes, and basic Strict/Lax/None SameSite behavior. `SameSite=None` requires Secure. Security integration tests cover denied CORS, exact-origin success, a real PUT preflight, credentials, and cookie replay.
