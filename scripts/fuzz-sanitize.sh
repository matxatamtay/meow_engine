#!/usr/bin/env bash
set -euo pipefail
mkdir -p release
if rustup toolchain list | grep -q '^nightly'; then
  export RUSTFLAGS="-Zsanitizer=address"
  export RUSTDOCFLAGS="-Zsanitizer=address"
  export MEOW_SANITIZER=address
  cargo +nightly run -Zbuild-std --target "$(rustc -vV | sed -n 's|host: ||p')" -p meow-fuzz -- \
    --duration-seconds "${MEOW_FUZZ_SECONDS:-10}" --output release/fuzz-asan-report.json
else
  echo "nightly toolchain unavailable; running sanitizer-compatible stable campaign"
  cargo run -p meow-fuzz -- --duration-seconds "${MEOW_FUZZ_SECONDS:-10}" --output release/fuzz-stable-report.json
fi
