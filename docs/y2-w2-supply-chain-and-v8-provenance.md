# Y2-W2 supply chain and V8 provenance

This week introduces a fail-closed dependency and V8 provenance contract. It does
not publish a release and does not integrate a runtime backend yet.

## Rust dependency gates

`deny.toml` is evaluated with cargo-deny 0.19.7 in CI. The policy:

- denies known advisories and yanked dependencies, except two reviewed
  quick-xml advisories currently confined to the wayland-scanner proc macro;
- treats unmaintained direct workspace dependencies as failures;
- allows an explicit SPDX license set and requires high-confidence detection;
- denies wildcard requirements, unknown registries, and all unapproved git sources;
- bans `openssl` and `native-tls` so the Rustls transport path cannot silently drift;
- reports duplicate versions for cleanup without blocking the current graph;
- denies stale advisory exceptions so the quick-xml carve-out must be removed as
  soon as the Wayland stack can select `quick-xml >= 0.41.0`.

The checked-in reports are generated only from `Cargo.lock`, `cargo metadata`,
`deny.toml`, and `vendor/v8/provenance.json`:

```bash
cargo xtask supply-chain validate
cargo xtask supply-chain update
cargo xtask supply-chain check
cargo deny --locked check
```

`release/supply-chain/sbom.spdx.json` is an SPDX 2.3 dependency SBOM.
`licenses.json`, `dependencies.json`, `v8-provenance.json`, and `manifest.json`
provide license inventory, dependency edges, V8 evidence, and content digests.
The generator uses a fixed SPDX creation timestamp so report drift reflects
content changes rather than the wall clock.

## Canonical V8 identity

`vendor/v8/provenance.json` is the sole accepted identity for V8 inputs. The
initial pin is:

- Rust binding crate `v8` / rusty_v8 `150.2.0`, tag `v150.2.0`, exact git commit;
- V8 `15.0.245.2`, exact `denoland/v8` submodule commit, with canonical upstream recorded;
- immutable release static archives for Linux x86_64 and aarch64, including byte size and SHA-256;
- commit-pinned MIT and BSD-3-Clause license files with SHA-256;
- a content-addressed cache key derived from version, profile, target, and checksum.

Normal validation is network-free. Remote tag, submodule revision, archive, and license verification is explicit
because it downloads tens of megabytes:

```bash
cargo xtask v8-verify --target x86_64-unknown-linux-gnu
cargo xtask v8-verify --all-targets
```

The default cache is `~/.cache/meowengine/v8`; override it with
`MEOW_V8_CACHE_DIR`. A cached object is accepted only when both size and SHA-256
match. A mismatch deletes the object and fails immediately.

## Updating the V8 pin

1. Select an immutable rusty_v8 release and record its exact tag and commit.
2. Record the exact V8 submodule commit and human-readable V8 version.
3. Copy the release asset byte size and SHA-256 into the manifest.
4. Pin both license URLs to commits and record their SHA-256 values.
5. Recompute every cache key using the policy format. Never reuse an old key.
6. Run `cargo xtask v8-verify` for every supported target.
7. Run `cargo xtask supply-chain update`, `cargo deny --locked check`, tests, and CI.
8. Review the SBOM/license delta before merging.

Mutable tags, branch names, `latest` URLs, unpinned mirrors, and a checksum copied
without independent verification are rejected.

## Rebuild and rollback

Release builds must not fetch V8. They consume an already verified archive via
`RUSTY_V8_ARCHIVE` or an immutable internal mirror through `RUSTY_V8_MIRROR`.
Building from source with `V8_FROM_SOURCE=1` is a fallback, not an implicit
network path. A source rebuild must use the pinned engine revision, produce a
new archive, record its byte size and SHA-256, and invalidate the old cache key
before it can enter a release build.

Rollback is a repository revert of the binding/source/artifact pin plus its
regenerated reports. Delete the rejected cache-key directory, regenerate the
supply-chain reports, rerun remote verification, and rebuild. Do not repair a
bad archive in place under an existing key.

## Failure modes

CI fails on advisory, license, source, wildcard, banned-crate, report, manifest,
checksum, size, cache-key, or license-evidence drift. The optional workflow
dispatch input performs the remote V8 download check. Local policy checks never
silently fall back to the network.
