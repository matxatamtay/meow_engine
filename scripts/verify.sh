#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

run_gate() {
    local name="$1"
    shift

    printf '\n==> %s\n' "$name"
    "$@"
}

run_gate "format" cargo fmt --all -- --check
run_gate "clippy" cargo clippy --workspace --all-targets -- -D warnings
run_gate "tests" cargo test --workspace --locked
run_gate "supply-chain tests" python3 -m unittest scripts.test_supply_chain
run_gate "supply-chain drift" cargo xtask supply-chain check
run_gate "doctor" cargo xtask doctor

printf '\nAll MeowEngine quality gates passed.\n'
