mod app;
mod diagnostics;
mod session;

use std::{env, error::Error, ffi::OsString, io};

use app::{BrowserApp, PresentationBackend};
use session::BrowserSession;
use softbuffer::Context;
use winit::event_loop::{ControlFlow, EventLoop};

fn main() -> Result<(), Box<dyn Error>> {
    let arguments = env::args_os().skip(1).collect::<Vec<_>>();
    if run_internal_process(&arguments)? {
        return Ok(());
    }

    diagnostics::init()?;

    let options = Options::parse(arguments)?;
    let event_loop = build_event_loop(options.backend)?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let profile_dir = env::var_os("MEOW_PROFILE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("artifacts/profile"));
    let mut session = if options.single_process {
        BrowserSession::local(profile_dir)
    } else {
        BrowserSession::remote(env::current_exe()?, profile_dir, options.sandbox)
    }
    .map_err(io::Error::other)?;
    if options.initial_url != "about:blank" {
        session
            .navigate(&options.initial_url)
            .map_err(io::Error::other)?;
    }
    let current_url = session.current_url().map_err(io::Error::other)?;
    let process_model = if session.is_remote() {
        "multiprocess"
    } else {
        "single-process"
    };

    let dev_session = env::var("MEOW_DEV_SESSION").unwrap_or_else(|_| "direct".to_owned());
    tracing::info!(
        dev_session,
        engine = meow_embedder_api::ENGINE_NAME,
        version = meow_embedder_api::engine_version(),
        requested_backend = ?options.backend,
        renderer = ?options.renderer,
        initial_url = %current_url,
        process_model,
        sandbox = options.sandbox && !options.single_process,
        exit_after_first_frame = options.smoke_test,
        "starting browser shell"
    );

    let cpu_context = if options.renderer == PresentationBackend::Cpu {
        Some(Context::new(event_loop.owned_display_handle())?)
    } else {
        None
    };
    let mut app = BrowserApp::new(cpu_context, options.renderer, options.smoke_test, session);
    event_loop.run_app(&mut app)?;

    tracing::info!("browser shell stopped cleanly");
    Ok(())
}

fn run_internal_process(arguments: &[OsString]) -> Result<bool, Box<dyn Error>> {
    match arguments.first().and_then(|value| value.to_str()) {
        Some("--process-smoke-test") => {
            let profile = std::env::temp_dir()
                .join(format!("meow-browser-process-smoke-{}", std::process::id()));
            let mut supervisor =
                meow_process_model::ProcessSupervisor::spawn(env::current_exe()?, &profile, true)?;
            let sandbox = supervisor.client_mut().sandbox_status()?;
            let frame = supervisor.client_mut().render(640, 480)?;
            if frame.display_list().commands().is_empty() || !sandbox.seccomp_applied {
                return Err(
                    io::Error::other("multiprocess browser smoke precondition failed").into(),
                );
            }
            let crash = supervisor.crash_content_for_test()?;
            let recovered = supervisor.client_mut().render(640, 480)?;
            if recovered.display_list().commands().is_empty() {
                return Err(io::Error::other("content restart did not submit a frame").into());
            }
            println!(
                "{}",
                serde_json::json!({
                    "browser_binary_multiprocess": true,
                    "content_crash_contained": crash.message.contains("intentional content crash"),
                    "seccomp_applied": sandbox.seccomp_applied,
                })
            );
            let _ = std::fs::remove_dir_all(profile);
            Ok(true)
        }
        Some("--meow-network-process") => {
            let socket = required_internal_path(arguments, 1, "network socket")?;
            meow_process_model::run_network_process(socket)?;
            Ok(true)
        }
        Some("--meow-content-process") => {
            let content_socket = required_internal_path(arguments, 1, "content socket")?;
            let network_socket = required_internal_path(arguments, 2, "network socket")?;
            let profile = required_internal_path(arguments, 3, "profile directory")?;
            let sandbox_root = required_internal_path(arguments, 4, "sandbox root")?;
            let crash_report = required_internal_path(arguments, 5, "crash report")?;
            let sandbox_enabled = arguments.get(6).is_some_and(|value| value == "1");
            meow_process_model::run_content_process(
                content_socket,
                network_socket,
                profile,
                sandbox_root,
                crash_report,
                sandbox_enabled,
            )?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn required_internal_path(
    arguments: &[OsString],
    index: usize,
    name: &str,
) -> io::Result<std::path::PathBuf> {
    arguments
        .get(index)
        .map(std::path::PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("missing {name}")))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RequestedBackend {
    Auto,
    Wayland,
    X11,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Options {
    backend: RequestedBackend,
    renderer: PresentationBackend,
    smoke_test: bool,
    initial_url: String,
    single_process: bool,
    sandbox: bool,
}

impl Options {
    fn parse(arguments: impl IntoIterator<Item = OsString>) -> io::Result<Self> {
        let mut options = Self {
            backend: RequestedBackend::Auto,
            renderer: PresentationBackend::Gpu,
            smoke_test: false,
            initial_url: "about:blank".to_owned(),
            single_process: !cfg!(target_os = "linux"),
            sandbox: cfg!(target_os = "linux"),
        };
        let mut arguments = arguments.into_iter();
        let mut positional_url = false;

        while let Some(argument) = arguments.next() {
            let argument = argument.to_string_lossy();
            match argument.as_ref() {
                "--smoke-test" => options.smoke_test = true,
                "--single-process" => options.single_process = true,
                "--no-sandbox" => options.sandbox = false,
                "--backend=auto" => options.backend = RequestedBackend::Auto,
                "--backend=wayland" => options.backend = RequestedBackend::Wayland,
                "--backend=x11" => options.backend = RequestedBackend::X11,
                "--renderer=cpu" => options.renderer = PresentationBackend::Cpu,
                "--renderer=gpu" => options.renderer = PresentationBackend::Gpu,
                "--url" => {
                    options.initial_url = arguments
                        .next()
                        .ok_or_else(|| {
                            io::Error::new(io::ErrorKind::InvalidInput, "--url requires a URL")
                        })?
                        .into_string()
                        .map_err(|_| {
                            io::Error::new(io::ErrorKind::InvalidInput, "URL must be valid UTF-8")
                        })?;
                }
                value if value.starts_with("--url=") => {
                    let value = value.trim_start_matches("--url=");
                    if value.is_empty() {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "--url requires a URL",
                        ));
                    }
                    options.initial_url = value.to_owned();
                }
                value if !value.starts_with('-') && !positional_url => {
                    options.initial_url = value.to_owned();
                    positional_url = true;
                }
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!(
                            "unknown argument {argument:?}; expected [URL], --url URL, --smoke-test, --single-process, --no-sandbox, --backend=auto|wayland|x11, or --renderer=cpu|gpu"
                        ),
                    ));
                }
            }
        }

        Ok(options)
    }
}

#[cfg(target_os = "linux")]
fn build_event_loop(backend: RequestedBackend) -> Result<EventLoop<()>, Box<dyn Error>> {
    use winit::platform::{wayland::EventLoopBuilderExtWayland, x11::EventLoopBuilderExtX11};

    let mut builder = EventLoop::<()>::builder();
    match backend {
        RequestedBackend::Auto => {}
        RequestedBackend::Wayland => {
            builder.with_wayland();
        }
        RequestedBackend::X11 => {
            builder.with_x11();
        }
    }

    Ok(builder.build()?)
}

#[cfg(not(target_os = "linux"))]
fn build_event_loop(_backend: RequestedBackend) -> Result<EventLoop<()>, Box<dyn Error>> {
    Ok(EventLoop::new()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_backend_renderer_smoke_and_url_options() {
        let options = Options::parse([
            OsString::from("--backend=x11"),
            OsString::from("--renderer=cpu"),
            OsString::from("--smoke-test"),
            OsString::from("https://example.test/docs"),
        ])
        .expect("options should parse");

        assert_eq!(options.backend, RequestedBackend::X11);
        assert_eq!(options.renderer, PresentationBackend::Cpu);
        assert_eq!(options.initial_url, "https://example.test/docs");
        assert!(options.smoke_test);
        assert_eq!(options.single_process, !cfg!(target_os = "linux"));
        assert_eq!(options.sandbox, cfg!(target_os = "linux"));
    }

    #[test]
    fn parses_explicit_url_option() {
        let options = Options::parse([
            OsString::from("--url"),
            OsString::from("http://127.0.0.1:8000/"),
        ])
        .unwrap();
        assert_eq!(options.initial_url, "http://127.0.0.1:8000/");
    }

    #[test]
    fn parses_process_model_options() {
        let options = Options::parse([
            OsString::from("--single-process"),
            OsString::from("--no-sandbox"),
        ])
        .unwrap();
        assert!(options.single_process);
        assert!(!options.sandbox);
    }

    #[test]
    fn rejects_unknown_options() {
        let error = Options::parse([OsString::from("--cats")])
            .expect_err("unknown options should be rejected");

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }
}
