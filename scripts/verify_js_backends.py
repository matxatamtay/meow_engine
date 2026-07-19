#!/usr/bin/env python3
from __future__ import annotations

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def run(*args: str) -> str:
    result = subprocess.run(args, cwd=ROOT, check=True, text=True, capture_output=True)
    return result.stdout


def main() -> int:
    run(
        "cargo", "clippy", "-p", "meow-engine", "--no-default-features", "--features", "js-v8", "--", "-D", "warnings"
    )
    run(
        "cargo", "clippy", "-p", "meow-engine", "--no-default-features", "--features", "js-boa", "--", "-D", "warnings"
    )
    run(
        "cargo",
        "test",
        "-p",
        "meow-js-runtime",
        "-p",
        "meow-js-boa",
        "-p",
        "meow-js-v8",
        "--locked",
    )
    production_tree = run(
        "cargo", "tree", "-p", "meow-browser", "--edges", "normal", "--prefix", "none"
    )
    forbidden = [
        line
        for line in production_tree.splitlines()
        if "boa_engine" in line or "meow-js-boa" in line
    ]
    if forbidden:
        print("production browser dependency graph contains Boa:", file=sys.stderr)
        print("\n".join(forbidden), file=sys.stderr)
        return 1
    required = ("meow-js-v8", "meow-js-runtime")
    missing = [name for name in required if name not in production_tree]
    if missing:
        print(f"production browser dependency graph is missing: {', '.join(missing)}", file=sys.stderr)
        return 1
    print("JavaScript backend matrix passed; production browser graph is V8-only")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
