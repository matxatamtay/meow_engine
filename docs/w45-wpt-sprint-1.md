# W45 selected WPT sprint 1

W45 adds the offline `meow-wpt` runner. It does not vendor the complete upstream
Web Platform Tests repository. `tests/wpt/manifest.json` records a reviewed
selection and an upstream path or section for each case.

The sprint covers selected HTML tree building, DOM queries, CSS selectors, and
cascade/inheritance. Each case runs in a subprocess. The parent applies a
wall-clock timeout, kills a stuck worker, and records `pass`, `fail`, or
`timeout` without losing the remaining run.

Outputs are `artifacts/wpt/report.json`, the self-contained
`artifacts/wpt/dashboard.html`, and the checked `tests/wpt/baseline.json`.
Manifest changes require an explicit reviewed update:

```bash
cargo xtask wpt --update-baseline
cargo xtask wpt --check
```

The W45-W46 baseline contains 20 cases. Current result: 20 passed, 0 failed, 0
timed out, 100% against a 95% target.
