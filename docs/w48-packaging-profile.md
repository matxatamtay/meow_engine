# W48 packaging, profile migration, and diagnostics bundles

Profiles use `profile.json` schema version 2. Preparation creates `recovery`,
`crashes`, and `diagnostics`, writes by temporary-file rename, adopts legacy
`local-storage/` as schema 1, moves malformed manifests into recovery, and
rejects future schemas.

```bash
cargo run -p meow-headless -- \
  --profile artifacts/profile --url about:blank \
  --output artifacts/profile-smoke.png
```

`cargo xtask package` produces a Linux AppDir, runnable tar, source archive,
SHA-256 artifact manifest, and an AppImage when `appimagetool` is installed. The
tar is extracted and smoke-tested with packaged binaries.

`cargo xtask diagnostics --profile artifacts/release-profile` creates a bundle
with profile/recovery/crash/diagnostic data, WPT/fuzz/budget reports, system and
toolchain metadata, process-smoke logs, and intentional content-crash recovery
status. Environment values are excluded; selected key names are recorded.

The canonical local artifact is built inside Ubuntu 24.04 to avoid a newer-host
glibc dependency. That exact tar was executed in Docker on Ubuntu 24.04 and
Fedora 42: headless help, multiprocess crash recovery, and seccomp smoke all
passed. Results are recorded in `release/distro-smoke.json`; CI repeats both
paths.
