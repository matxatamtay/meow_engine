# Bootstrap guide

## Prerequisites

- Linux development environment
- Git
- rustup
- Docker or Podman only for the optional clean-Ubuntu reproduction

The repository pins the exact Rust toolchain and required components in
`rust-toolchain.toml`. Cargo installs that toolchain automatically through
rustup when a project command is first run.

## First checkout

```bash
git clone https://github.com/matxatamtay/meow_engine.git
cd meow_engine
cargo xtask doctor
bash scripts/verify.sh
```

`cargo xtask doctor` checks the required repository files, Rust tools, and
Cargo workspace metadata. `scripts/verify.sh` is the complete local quality
gate and runs formatting, Clippy, tests, and doctor.

## Clean Ubuntu verification

With Docker or Podman installed:

```bash
bash scripts/verify-clean-ubuntu.sh
```

When the account was just added to the `docker` group, run `newgrp docker` or
log out and back in before invoking the script from that shell.

The script sends a clean copy of the repository into `ubuntu:24.04`, installs
only the bootstrap packages and rustup, then runs the canonical quality gate.
Select a runtime or image explicitly when needed:

```bash
CONTAINER_RUNTIME=podman \
MEOWENGINE_UBUNTU_IMAGE=ubuntu:24.04 \
  bash scripts/verify-clean-ubuntu.sh
```

GitHub Actions repeats this check in a clean Ubuntu 24.04 job, so pull requests
do not depend solely on a contributor's local container runtime.

## Common recovery

When rustup reports a partially installed component, make sure no IDE process is
installing Rust components concurrently, then reinstall the affected component:

```bash
rustup component remove rustfmt clippy --toolchain 1.97.1
rustup component add rustfmt clippy --toolchain 1.97.1
```
