#!/usr/bin/env bash
set -euo pipefail
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
IMAGE=${MEOW_PORTABLE_IMAGE:-ubuntu:24.04}
OUTPUT=${MEOW_PORTABLE_OUTPUT:-release/portable-bin}
mkdir -p "$ROOT/$OUTPUT" "$ROOT/release/container-target"
docker run --rm \
  -v "$ROOT:/work" \
  -v "$HOME/.cargo/registry:/root/.cargo/registry" \
  -v "$HOME/.cargo/git:/root/.cargo/git" \
  -w /work \
  -e CARGO_TARGET_DIR=/work/release/container-target/ubuntu-24.04 \
  "$IMAGE" bash -lc '
    set -euo pipefail
    export DEBIAN_FRONTEND=noninteractive
    apt-get update >/dev/null
    apt-get install -y --no-install-recommends build-essential ca-certificates curl pkg-config git >/dev/null
    curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain none >/dev/null
    export PATH="$HOME/.cargo/bin:$PATH"
    rustup show active-toolchain
    cargo build --release -p meow-browser -p meow-headless
    install -m755 "$CARGO_TARGET_DIR/release/meow-browser" /work/'"$OUTPUT"'/meow-browser
    install -m755 "$CARGO_TARGET_DIR/release/meow-headless" /work/'"$OUTPUT"'/meow-headless
  '
