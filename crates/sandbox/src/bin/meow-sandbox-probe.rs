use std::{error::Error, net::TcpStream};

use meow_sandbox::{SandboxPolicy, apply_content_sandbox};

fn main() -> Result<(), Box<dyn Error>> {
    let root = std::env::temp_dir().join(format!("meow-sandbox-probe-{}", std::process::id()));
    let mut policy = SandboxPolicy::content(&root);
    policy.attempt_namespaces = true;
    let report = apply_content_sandbox(&policy)?;
    let socket_error =
        TcpStream::connect("127.0.0.1:9").expect_err("seccomp must deny creation of a TCP socket");
    if socket_error.raw_os_error() != Some(nix::libc::EPERM) {
        return Err(format!("expected EPERM from seccomp, got {socket_error}").into());
    }
    println!("{}", serde_json::to_string(&report)?);
    Ok(())
}
