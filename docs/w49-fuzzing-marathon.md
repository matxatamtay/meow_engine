# W49 fuzzing marathon

`meow-fuzz` runs deterministic, bounded mutation campaigns over HTML, CSS and
selectors, IPC JSON envelopes, image decoder input, and URL parse/resolve.
Reports record seed, duration, requested/completed iterations, sanitizer mode,
corpus size, and crash inputs. A panic writes its input and fails the run.

The local acceptance campaign completed 10,000 mutations with seed
`0x4d454f572027` and found zero new crashes.

```bash
cargo xtask fuzz --duration-seconds 300 --iterations 1000000
```

`scripts/fuzz-sanitize.sh` uses AddressSanitizer with a compatible nightly
`-Zbuild-std` toolchain, otherwise the same sanitizer-compatible targets run on
stable. This is not coverage-guided libFuzzer, OSS-Fuzz, MSan, or a multi-day
campaign.
