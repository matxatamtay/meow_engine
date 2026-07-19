//! Experimental Linux content sandbox and brokered file access.

use std::{
    collections::BTreeMap,
    error::Error,
    fmt, fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

/// Conservative resource limits for one content process.
#[derive(Clone, Debug)]
pub struct SandboxPolicy {
    pub filesystem_root: PathBuf,
    pub max_open_files: u64,
    pub max_file_size_bytes: u64,
    pub max_address_space_bytes: u64,
    pub attempt_namespaces: bool,
    pub deny_network_syscalls: bool,
}

impl SandboxPolicy {
    #[must_use]
    pub fn content(filesystem_root: impl Into<PathBuf>) -> Self {
        Self {
            filesystem_root: filesystem_root.into(),
            max_open_files: 128,
            max_file_size_bytes: 64 * 1024 * 1024,
            max_address_space_bytes: 2 * 1024 * 1024 * 1024,
            attempt_namespaces: true,
            deny_network_syscalls: true,
        }
    }
}

/// Exact controls applied and kernel-dependent gaps observed at runtime.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxReport {
    pub rlimits_applied: Vec<String>,
    pub namespaces_applied: Vec<String>,
    pub filesystem_view: Option<String>,
    pub seccomp_applied: bool,
    pub gaps: Vec<String>,
}

/// Applies the irreversible content-process controls in safe-to-unsafe order.
pub fn apply_content_sandbox(policy: &SandboxPolicy) -> Result<SandboxReport, SandboxError> {
    fs::create_dir_all(&policy.filesystem_root)?;
    let mut report = SandboxReport::default();
    apply_rlimits(policy, &mut report)?;
    apply_namespaces(policy, &mut report);
    apply_filesystem_view(policy, &mut report)?;
    if policy.deny_network_syscalls {
        install_network_seccomp()?;
        report.seccomp_applied = true;
    }
    Ok(report)
}

#[cfg(target_os = "linux")]
fn apply_rlimits(policy: &SandboxPolicy, report: &mut SandboxReport) -> Result<(), SandboxError> {
    use nix::sys::resource::{Resource, setrlimit};

    setrlimit(
        Resource::RLIMIT_NOFILE,
        policy.max_open_files,
        policy.max_open_files,
    )?;
    report
        .rlimits_applied
        .push(format!("nofile={}", policy.max_open_files));
    setrlimit(
        Resource::RLIMIT_FSIZE,
        policy.max_file_size_bytes,
        policy.max_file_size_bytes,
    )?;
    report
        .rlimits_applied
        .push(format!("fsize={}", policy.max_file_size_bytes));
    setrlimit(
        Resource::RLIMIT_AS,
        policy.max_address_space_bytes,
        policy.max_address_space_bytes,
    )?;
    report
        .rlimits_applied
        .push(format!("as={}", policy.max_address_space_bytes));
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn apply_rlimits(_policy: &SandboxPolicy, report: &mut SandboxReport) -> Result<(), SandboxError> {
    report
        .gaps
        .push("rlimits are only implemented on Linux".to_owned());
    Ok(())
}

#[cfg(target_os = "linux")]
fn apply_namespaces(policy: &SandboxPolicy, report: &mut SandboxReport) {
    use nix::sched::{CloneFlags, unshare};

    if !policy.attempt_namespaces {
        report
            .gaps
            .push("namespace setup disabled by policy".to_owned());
        return;
    }
    let namespaces = [
        (CloneFlags::CLONE_NEWIPC, "ipc"),
        (CloneFlags::CLONE_NEWUTS, "uts"),
        (CloneFlags::CLONE_NEWNS, "mount"),
        (CloneFlags::CLONE_NEWNET, "network"),
    ];
    for (flag, name) in namespaces {
        match unshare(flag) {
            Ok(()) => report.namespaces_applied.push(name.to_owned()),
            Err(error) => report
                .gaps
                .push(format!("{name} namespace unavailable: {error}")),
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn apply_namespaces(_policy: &SandboxPolicy, report: &mut SandboxReport) {
    report
        .gaps
        .push("namespaces are only implemented on Linux".to_owned());
}

fn apply_filesystem_view(
    policy: &SandboxPolicy,
    report: &mut SandboxReport,
) -> Result<(), SandboxError> {
    #[cfg(target_os = "linux")]
    {
        use nix::{
            sys::stat::{Mode, umask},
            unistd::chdir,
        };
        chdir(&policy.filesystem_root)?;
        umask(Mode::from_bits_truncate(0o077));
    }
    #[cfg(not(target_os = "linux"))]
    std::env::set_current_dir(&policy.filesystem_root)?;

    report.filesystem_view = Some(policy.filesystem_root.display().to_string());
    report.gaps.push(
        "filesystem view changes cwd and mount namespace only; rootless chroot/pivot_root is not yet enforced"
            .to_owned(),
    );
    Ok(())
}

#[cfg(target_os = "linux")]
fn install_network_seccomp() -> Result<(), SandboxError> {
    use std::convert::TryInto;

    use nix::libc;
    use seccompiler::{BpfProgram, SeccompAction, SeccompFilter, TargetArch};

    let denied = [
        libc::SYS_socket,
        libc::SYS_socketpair,
        libc::SYS_connect,
        libc::SYS_bind,
        libc::SYS_listen,
        libc::SYS_accept,
        libc::SYS_accept4,
        libc::SYS_ptrace,
        libc::SYS_mount,
        libc::SYS_umount2,
    ];
    let rules = denied
        .into_iter()
        .map(|syscall| (syscall, Vec::new()))
        .collect::<BTreeMap<_, _>>();
    let architecture = TargetArch::try_from(std::env::consts::ARCH)
        .map_err(|error| SandboxError::Seccomp(error.to_string()))?;
    let program: BpfProgram = SeccompFilter::new(
        rules,
        SeccompAction::Allow,
        SeccompAction::Errno(libc::EPERM as u32),
        architecture,
    )
    .map_err(|error| SandboxError::Seccomp(error.to_string()))?
    .try_into()
    .map_err(|error: seccompiler::BackendError| SandboxError::Seccomp(error.to_string()))?;
    seccompiler::apply_filter(&program)
        .map_err(|error| SandboxError::Seccomp(error.to_string()))?;
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn install_network_seccomp() -> Result<(), SandboxError> {
    Err(SandboxError::UnsupportedPlatform)
}

/// Read-only file broker that confines access to canonical allowlisted roots.
#[derive(Clone, Debug)]
pub struct FileAccessBroker {
    roots: Vec<PathBuf>,
    max_bytes: usize,
}

impl FileAccessBroker {
    pub fn new(
        roots: impl IntoIterator<Item = PathBuf>,
        max_bytes: usize,
    ) -> Result<Self, SandboxError> {
        let roots = roots
            .into_iter()
            .map(fs::canonicalize)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { roots, max_bytes })
    }

    pub fn read(&self, path: impl AsRef<Path>) -> Result<Vec<u8>, SandboxError> {
        let canonical = fs::canonicalize(path)?;
        if !self.roots.iter().any(|root| canonical.starts_with(root)) {
            return Err(SandboxError::FileDenied(canonical));
        }
        let metadata = fs::metadata(&canonical)?;
        let size = usize::try_from(metadata.len()).unwrap_or(usize::MAX);
        if size > self.max_bytes {
            return Err(SandboxError::FileTooLarge {
                actual: size,
                limit: self.max_bytes,
            });
        }
        Ok(fs::read(canonical)?)
    }
}

#[derive(Debug)]
pub enum SandboxError {
    Io(std::io::Error),
    Nix(nix::Error),
    Seccomp(String),
    FileDenied(PathBuf),
    FileTooLarge { actual: usize, limit: usize },
    UnsupportedPlatform,
}

impl fmt::Display for SandboxError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "sandbox I/O failed: {error}"),
            Self::Nix(error) => write!(formatter, "sandbox kernel control failed: {error}"),
            Self::Seccomp(error) => write!(formatter, "seccomp setup failed: {error}"),
            Self::FileDenied(path) => write!(
                formatter,
                "brokered file access denied outside allowlisted roots: {}",
                path.display()
            ),
            Self::FileTooLarge { actual, limit } => {
                write!(
                    formatter,
                    "brokered file is {actual} bytes, limit is {limit}"
                )
            }
            Self::UnsupportedPlatform => {
                formatter.write_str("Linux sandbox is unavailable on this platform")
            }
        }
    }
}

impl Error for SandboxError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Nix(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for SandboxError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<nix::Error> for SandboxError {
    fn from(error: nix::Error) -> Self {
        Self::Nix(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broker_denies_escape_and_size_overflow() {
        let root =
            std::env::temp_dir().join(format!("meow-file-broker-{}-{}", std::process::id(), 17));
        let outside =
            std::env::temp_dir().join(format!("meow-file-outside-{}-{}", std::process::id(), 17));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("ok.txt"), b"cat").unwrap();
        fs::write(&outside, b"outside").unwrap();
        let broker = FileAccessBroker::new([root.clone()], 4).unwrap();
        assert_eq!(broker.read(root.join("ok.txt")).unwrap(), b"cat");
        assert!(matches!(
            broker.read(&outside),
            Err(SandboxError::FileDenied(_))
        ));
        fs::write(root.join("large.txt"), b"12345").unwrap();
        assert!(matches!(
            broker.read(root.join("large.txt")),
            Err(SandboxError::FileTooLarge { .. })
        ));
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_file(outside);
    }
}
