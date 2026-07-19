# W44 experimental Linux sandbox

W44 adds `meow-sandbox`, an explicit experimental boundary for the content
child. It reports every applied control and every kernel-dependent gap rather
than treating a partial setup as invisible success.

## Applied controls

The default content policy applies:

- `RLIMIT_NOFILE=128`;
- a 64 MiB file-size limit;
- a 2 GiB address-space limit;
- a restrictive `umask` and a dedicated working directory;
- best-effort IPC, UTS, mount, and network namespace separation;
- a seccomp-BPF filter that returns `EPERM` for socket creation/connection,
  bind/listen/accept, `ptrace`, mount, and unmount syscalls.

Sockets needed for content-to-shell and content-to-network IPC are opened
before the irreversible seccomp filter is installed.

## Brokered file access

`FileAccessBroker` canonicalizes allowlisted roots and requested paths, rejects
path escapes and symlink escapes, enforces a byte cap before reading, and
exposes read-only bytes. The current browser does not support top-level
`file:` navigation, so this broker is a sandbox service primitive rather than a
content-visible file API.

## Documented gaps

Namespace creation commonly depends on container, user-namespace, and host
kernel policy. Each failed namespace is recorded in `SandboxReport.gaps` and
does not hide the controls that did apply.

The filesystem view currently consists of a dedicated mount namespace when
available, a dedicated working directory, and restrictive permissions. It does
not yet perform rootless `pivot_root`, `chroot`, bind-mount construction, or a
fully empty filesystem tree. Seccomp is a denylist for the current alpha, not a
production syscall allowlist. There is no PID/user namespace setup, cgroup
budget, Landlock policy, or production broker audit log yet.

Use `--no-sandbox` only for debugging a host that rejects required controls.
The browser remains multiprocess in that mode. Use `--single-process` to disable
the process boundary entirely.

## Verification

```bash
cargo test -p meow-sandbox
cargo test -p meow-process-model --test process_smoke
cargo test -p meow-browser --test multiprocess_smoke
```

The sandbox probe runs in a subprocess because seccomp is irreversible. It
verifies rlimit reporting and confirms a new TCP socket fails with `EPERM`.
