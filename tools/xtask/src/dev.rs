use std::{
    path::Path,
    process::{Command, ExitCode},
};

pub fn run(args: &[String]) -> ExitCode {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/dev.sh");
    let status = match Command::new("bash").arg(script).args(args).status() {
        Ok(status) => status,
        Err(error) => {
            eprintln!("failed to start MeowEngine dev process: {error}");
            return ExitCode::FAILURE;
        }
    };

    status
        .code()
        .and_then(|code| u8::try_from(code).ok())
        .map_or(ExitCode::FAILURE, ExitCode::from)
}
