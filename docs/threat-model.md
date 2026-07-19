# Public-alpha threat model

Assets are shell availability, profile/cookies/cache/diagnostics, host filesystem
and network credentials, and rendering/script correctness. Trust boundaries are
shell, content, network, versioned Unix IPC, profile filesystem, and package.
Untrusted HTML/CSS/JS, images, URLs, and IPC bytes are parser inputs.

Defenses include bounded IPC/request correlation, content/network separation,
crash recovery, network permission filtering, response/image/storage/task/script
limits, best-effort namespaces/rlimits/seccomp/file broker, and deterministic
fuzz campaigns.

This alpha is not hardened. It lacks complete site isolation, user/PID namespace
policy, syscall allowlisting, Landlock/cgroups, signed updates, phishing/malware
protection, full CSP/COOP/COEP/CORP, and IPC peer authentication. WebSocket
requires single-process mode. Use controlled content and a disposable OS user or
VM for hostile pages.
