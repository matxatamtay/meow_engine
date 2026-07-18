#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
image="${MEOWENGINE_UBUNTU_IMAGE:-ubuntu:24.04}"
runtime="${CONTAINER_RUNTIME:-}"

if [[ -z "$runtime" ]]; then
    if command -v docker >/dev/null 2>&1; then
        runtime="docker"
    elif command -v podman >/dev/null 2>&1; then
        runtime="podman"
    else
        echo "error: Docker or Podman is required for the clean Ubuntu check" >&2
        exit 2
    fi
fi

if ! command -v "$runtime" >/dev/null 2>&1; then
    echo "error: container runtime not found: $runtime" >&2
    exit 2
fi

if [[ "$runtime" == "docker" ]] && ! docker info >/dev/null 2>&1; then
    echo "error: the current shell cannot access the Docker daemon" >&2
    echo "run 'newgrp docker' or log out and back in, then retry" >&2
    exit 2
fi

printf 'Running MeowEngine verification in %s with %s...\n' "$image" "$runtime"

tar \
    --exclude=.git \
    --exclude=.idea \
    --exclude=target \
    -C "$repo_root" \
    -cf - . \
    | "$runtime" run --rm -i \
        --env CI=1 \
        --env DEBIAN_FRONTEND=noninteractive \
        "$image" \
        bash -lc '
            set -euo pipefail
            apt-get update
            apt-get install --yes --no-install-recommends \
                build-essential \
                ca-certificates \
                curl
            rm -rf /var/lib/apt/lists/*

            mkdir -p /workspace
            tar -xf - -C /workspace
            cd /workspace

            curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs \
                | sh -s -- -y --profile minimal --default-toolchain none
            source "$HOME/.cargo/env"
            rustup show active-toolchain
            bash scripts/verify.sh
        '
