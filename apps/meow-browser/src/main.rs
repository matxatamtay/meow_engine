mod app;
mod diagnostics;

use std::{env, error::Error, io};

use app::{BrowserApp, PresentationBackend};
use softbuffer::Context;
use winit::event_loop::{ControlFlow, EventLoop};

fn main() -> Result<(), Box<dyn Error>> {
    diagnostics::init()?;

    let options = Options::parse(env::args_os().skip(1))?;
    let event_loop = build_event_loop(options.backend)?;
    event_loop.set_control_flow(ControlFlow::Wait);

    tracing::info!(
        engine = meow_embedder_api::ENGINE_NAME,
        version = meow_embedder_api::engine_version(),
        requested_backend = ?options.backend,
        renderer = ?options.renderer,
        exit_after_first_frame = options.smoke_test,
        "starting browser shell"
    );

    let cpu_context = if options.renderer == PresentationBackend::Cpu {
        Some(Context::new(event_loop.owned_display_handle())?)
    } else {
        None
    };
    let mut app = BrowserApp::new(cpu_context, options.renderer, options.smoke_test);
    event_loop.run_app(&mut app)?;

    tracing::info!("browser shell stopped cleanly");
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RequestedBackend {
    Auto,
    Wayland,
    X11,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Options {
    backend: RequestedBackend,
    renderer: PresentationBackend,
    smoke_test: bool,
}

impl Options {
    fn parse(arguments: impl IntoIterator<Item = std::ffi::OsString>) -> io::Result<Self> {
        let mut options = Self {
            backend: RequestedBackend::Auto,
            renderer: PresentationBackend::Gpu,
            smoke_test: false,
        };

        for argument in arguments {
            let argument = argument.to_string_lossy();
            match argument.as_ref() {
                "--smoke-test" => options.smoke_test = true,
                "--backend=auto" => options.backend = RequestedBackend::Auto,
                "--backend=wayland" => options.backend = RequestedBackend::Wayland,
                "--backend=x11" => options.backend = RequestedBackend::X11,
                "--renderer=cpu" => options.renderer = PresentationBackend::Cpu,
                "--renderer=gpu" => options.renderer = PresentationBackend::Gpu,
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!(
                            "unknown argument {argument:?}; expected --smoke-test, --backend=auto|wayland|x11, or --renderer=cpu|gpu"
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
    fn parses_backend_and_smoke_test_options() {
        let options = Options::parse([
            std::ffi::OsString::from("--backend=x11"),
            std::ffi::OsString::from("--renderer=cpu"),
            std::ffi::OsString::from("--smoke-test"),
        ])
        .expect("options should parse");

        assert_eq!(options.backend, RequestedBackend::X11);
        assert_eq!(options.renderer, PresentationBackend::Cpu);
        assert!(options.smoke_test);
    }

    #[test]
    fn rejects_unknown_options() {
        let error = Options::parse([std::ffi::OsString::from("--cats")])
            .expect_err("unknown options should be rejected");

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }
}
