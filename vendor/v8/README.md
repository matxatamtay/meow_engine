# V8 provenance pin

`provenance.json` is the only accepted source of V8 binding, engine-source,
static-archive, checksum, license, and cache identity for MeowEngine.

Do not place downloaded archives in this directory. Use
`cargo xtask v8-verify --target <triple>` so the content-addressed cache is
validated before use. Runtime integration belongs to Y2-W3/Y2-W4; this directory
only defines the supply-chain contract.
