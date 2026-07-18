use std::{backtrace::Backtrace, env, error::Error, panic, thread};

use tracing_subscriber::EnvFilter;

const DEFAULT_FILTER: &str = "meow_browser=debug,meow_embedder_api=debug,meow_engine=debug,meow_renderer=debug,winit=info,softbuffer=info,vello=info,wgpu_core=warn,wgpu_hal=warn";

pub fn init() -> Result<(), Box<dyn Error>> {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_ansi(env::var_os("NO_COLOR").is_none())
        .compact()
        .try_init()
        .map_err(|error| std::io::Error::other(error.to_string()))?;

    install_panic_hook();
    Ok(())
}

fn install_panic_hook() {
    let previous_hook = panic::take_hook();

    panic::set_hook(Box::new(move |panic_info| {
        let current_thread = thread::current();
        let thread_name = current_thread.name().unwrap_or("<unnamed>");
        let panic_message = panic_payload(panic_info);
        let panic_location = panic_info.location().map_or_else(
            || "<unknown>".to_owned(),
            |location| {
                format!(
                    "{}:{}:{}",
                    location.file(),
                    location.line(),
                    location.column()
                )
            },
        );
        let backtrace = Backtrace::force_capture();

        tracing::error!(
            target: "meow_browser::crash",
            %panic_message,
            %panic_location,
            %thread_name,
            %backtrace,
            "fatal panic"
        );

        previous_hook(panic_info);
    }));
}

fn panic_payload(panic_info: &panic::PanicHookInfo<'_>) -> String {
    if let Some(message) = panic_info.payload().downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = panic_info.payload().downcast_ref::<String>() {
        message.clone()
    } else {
        "non-string panic payload".to_owned()
    }
}
