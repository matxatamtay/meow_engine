# W52 public alpha

The prepared identity is **v0.2-alpha**, represented in Cargo as
`0.2.0-alpha.0`. `cargo xtask package` creates the Linux tar/AppDir, conditional
AppImage, source archive, and `release/dist/artifact-manifest.json` with sizes
and SHA-256 digests. Release evidence also includes diagnostics, selected WPT,
fuzz, budgets, RC corpus, notes, and issue templates.

A tag is intentionally not created in this dirty working tree. Create the real
`v0.2-alpha` tag only after all year-one changes are committed and artifacts
are regenerated from that exact commit.

```bash
cargo xtask release-check
```

Year two is in `docs/roadmap-year-2.md`.
