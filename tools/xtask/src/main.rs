use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    xtask::run(env::args().skip(1))
}
