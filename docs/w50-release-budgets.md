# W50 release budgets

`meow-bench` writes `release/budget-report.json` and checks machine-readable
thresholds in `release/budgets.json`. It measures engine startup, Linux high-
water RSS, 200 cached scroll/render samples and p95, local page load, optimized
browser/headless size, and two-load cache hit rate.

Recorded local release result:

| Metric | Measured | Budget |
| --- | ---: | ---: |
| Startup | 5 ms | 5,000 ms |
| Peak RSS | 22.2 MiB | 1,024 MiB |
| Scroll/render p95 | 1.161 ms | 100 ms |
| Local load | 5 ms | 5,000 ms |
| Binary total | 62.1 MiB | 500 MiB |
| Cache hit rate | 50% | at least 40% |

```bash
cargo build --release -p meow-browser -p meow-headless
cargo xtask budgets --browser-bin target/release/meow-browser \
  --headless-bin target/release/meow-headless
```

A debug-binary run correctly failed the size gate. These are synthetic
regression guards, not universal performance guarantees.
