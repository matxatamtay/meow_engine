#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

renderer="gpu"
backend="auto"
log_level="debug"
custom_filter=""
log_file=""
write_log_file=true
smoke_test=false

usage() {
    cat <<'EOF'
Run the MeowEngine browser with development diagnostics.

usage:
  cargo xtask dev [options]

options:
  --renderer=cpu|gpu              presentation backend (default: gpu)
  --backend=auto|wayland|x11      window-system backend (default: auto)
  --debug                         project debug logs (default)
  --trace                         project trace logs
  --rust-log=<filter>             custom tracing EnvFilter
  --log-file=<path>               write the merged process log to this path
  --no-log-file                   stream logs only to the terminal
  --smoke-test                    present one frame and exit
  -h, --help                      show this help
EOF
}

for argument in "$@"; do
    case "$argument" in
        --renderer=cpu|--renderer=gpu)
            renderer="${argument#*=}"
            ;;
        --backend=auto|--backend=wayland|--backend=x11)
            backend="${argument#*=}"
            ;;
        --debug)
            log_level="debug"
            ;;
        --trace)
            log_level="trace"
            ;;
        --rust-log=*)
            custom_filter="${argument#*=}"
            if [[ -z "$custom_filter" ]]; then
                printf 'error: --rust-log requires a non-empty filter\n' >&2
                exit 2
            fi
            ;;
        --log-file=*)
            log_file="${argument#*=}"
            if [[ -z "$log_file" ]]; then
                printf 'error: --log-file requires a path\n' >&2
                exit 2
            fi
            ;;
        --no-log-file)
            write_log_file=false
            ;;
        --smoke-test)
            smoke_test=true
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            printf 'error: unknown dev option %q\n\n' "$argument" >&2
            usage >&2
            exit 2
            ;;
    esac
done

case "$log_level" in
    debug)
        default_filter="meow_browser=debug,meow_embedder_api=debug,meow_engine=debug,meow_renderer=debug,winit=info,softbuffer=info,vello=info,wgpu_core=warn,wgpu_hal=warn"
        ;;
    trace)
        default_filter="meow_browser=trace,meow_embedder_api=trace,meow_engine=trace,meow_renderer=trace,winit=debug,softbuffer=debug,vello=debug,wgpu_core=info,wgpu_hal=info"
        ;;
esac

export RUST_LOG="${custom_filter:-$default_filter}"
export RUST_BACKTRACE="${RUST_BACKTRACE:-1}"
export MEOW_DEV_SESSION="$(date -u +%Y%m%dT%H%M%SZ)-${BASHPID}"
export NO_COLOR="${NO_COLOR:-1}"
export CARGO_TERM_COLOR="${CARGO_TERM_COLOR:-never}"

command=(
    cargo run --locked -p meow-browser --
    "--renderer=$renderer"
    "--backend=$backend"
)
if [[ "$smoke_test" == true ]]; then
    command+=(--smoke-test)
fi

printf 'MeowEngine dev session: %s\n' "$MEOW_DEV_SESSION"
printf 'RUST_LOG: %s\n' "$RUST_LOG"
printf 'command:'
printf ' %q' "${command[@]}"
printf '\n'

if [[ "$write_log_file" == false ]]; then
    exec "${command[@]}"
fi

if [[ -z "$log_file" ]]; then
    log_file="artifacts/logs/meow-browser-$MEOW_DEV_SESSION.log"
fi
mkdir -p "$(dirname "$log_file")"

printf 'log file: %s\n\n' "$log_file"
set +e
"${command[@]}" 2>&1 | tee "$log_file"
process_status=${PIPESTATUS[0]}
set -e

{
    printf '\ndev session: %s\n' "$MEOW_DEV_SESSION"
    printf 'process exit code: %d\n' "$process_status"
    printf 'saved log: %s\n' "$log_file"
} | tee -a "$log_file"
exit "$process_status"
