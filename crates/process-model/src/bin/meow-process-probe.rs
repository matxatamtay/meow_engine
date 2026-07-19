use std::{
    error::Error,
    io::{self, Read, Write},
    net::TcpListener,
    path::PathBuf,
    thread,
};

use meow_process_model::{
    ProcessError, ProcessSupervisor, run_content_process, run_network_process,
};

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        let code = if matches!(error, ProcessError::ContentCrashed(_)) {
            86
        } else {
            1
        };
        std::process::exit(code);
    }
}

fn run() -> Result<(), ProcessError> {
    let args = std::env::args_os().collect::<Vec<_>>();
    match args.get(1).and_then(|value| value.to_str()) {
        Some("--meow-network-process") => {
            let socket = required_path(&args, 2, "network socket")?;
            run_network_process(socket)
        }
        Some("--meow-content-process") => {
            let content_socket = required_path(&args, 2, "content socket")?;
            let network_socket = required_path(&args, 3, "network socket")?;
            let profile = required_path(&args, 4, "profile directory")?;
            let sandbox_root = required_path(&args, 5, "sandbox root")?;
            let crash_report = required_path(&args, 6, "crash report")?;
            let sandbox_enabled = args.get(7).is_some_and(|value| value == "1");
            run_content_process(
                content_socket,
                network_socket,
                profile,
                sandbox_root,
                crash_report,
                sandbox_enabled,
            )
        }
        _ => run_parent_probe().map_err(|error| ProcessError::Protocol(error.to_string())),
    }
}

fn run_parent_probe() -> Result<(), Box<dyn Error>> {
    let executable = std::env::current_exe()?;
    let profile =
        std::env::temp_dir().join(format!("meow-process-probe-profile-{}", std::process::id()));
    let mut supervisor = ProcessSupervisor::spawn(executable, &profile, true)?;
    let sandbox = supervisor.client_mut().sandbox_status()?;
    if !sandbox.seccomp_applied {
        return Err("content seccomp was not applied".into());
    }
    if supervisor.client_mut().current_url()? != "about:blank" {
        return Err("content process did not own about:blank".into());
    }
    let frame = supervisor.client_mut().render(640, 480)?;
    if frame.display_list().commands().is_empty() {
        return Err("content frame submission was empty".into());
    }
    let (url, server) = one_shot_http_server()?;
    let committed_url = supervisor.client_mut().navigate(url)?;
    server.join().map_err(|_| "HTTP smoke server panicked")??;
    if !committed_url.starts_with("http://127.0.0.1:") {
        return Err(
            format!("brokered navigation committed unexpected URL: {committed_url}").into(),
        );
    }
    let title = supervisor.client_mut().title(640, 480)?;
    if !title.contains("Brokered Cat") {
        return Err(format!("brokered document title missing: {title}").into());
    }
    let report = supervisor.crash_content_for_test()?;
    if !report.message.contains("intentional content crash") {
        return Err(format!("unexpected crash report: {}", report.message).into());
    }
    let recovered = supervisor.client_mut().render(640, 480)?;
    if recovered.display_list().commands().is_empty() {
        return Err("restarted content process did not submit a frame".into());
    }
    println!(
        "{}",
        serde_json::json!({
            "content_crash_contained": true,
            "network_process_survived": true,
            "brokered_http": true,
            "frame_commands": recovered.display_list().commands().len(),
            "seccomp_applied": sandbox.seccomp_applied,
            "namespace_gaps": sandbox.gaps,
        })
    );
    let _ = std::fs::remove_dir_all(profile);
    Ok(())
}

type SmokeServer = thread::JoinHandle<Result<(), io::Error>>;

fn one_shot_http_server() -> Result<(String, SmokeServer), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept()?;
        let mut request = [0_u8; 2048];
        let _ = stream.read(&mut request)?;
        let body = b"<!doctype html><title>Brokered Cat</title><main>network process</main>";
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )?;
        stream.write_all(body)?;
        stream.flush()?;
        Ok(())
    });
    Ok((format!("http://{address}/"), handle))
}

fn required_path(
    args: &[std::ffi::OsString],
    index: usize,
    name: &str,
) -> Result<PathBuf, ProcessError> {
    args.get(index)
        .map(PathBuf::from)
        .ok_or_else(|| ProcessError::Protocol(format!("missing {name}")))
}
