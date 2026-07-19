mod dev;
mod doctor;
mod release;

use std::process::ExitCode;

pub fn run(args: impl IntoIterator<Item = String>) -> ExitCode {
    let args: Vec<String> = args.into_iter().collect();

    match args.as_slice() {
        [] => {
            print_usage();
            ExitCode::SUCCESS
        }
        [command] if matches!(command.as_str(), "help" | "--help" | "-h") => {
            print_usage();
            ExitCode::SUCCESS
        }
        [command] if command == "doctor" => doctor::run(),
        [command, rest @ ..] if command == "dev" => dev::run(rest),
        [command, rest @ ..]
            if matches!(
                command.as_str(),
                "wpt"
                    | "fuzz"
                    | "budgets"
                    | "package"
                    | "diagnostics"
                    | "release-check"
                    | "supply-chain"
                    | "v8-verify"
            ) =>
        {
            release::run(command, rest)
        }
        [command, ..] => {
            eprintln!("unknown xtask command: {command}\n");
            print_usage();
            ExitCode::from(2)
        }
    }
}

fn print_usage() {
    println!(
        "MeowEngine repository tasks\n\nusage:\n  cargo xtask doctor\n  cargo xtask dev [options]\n  cargo xtask supply-chain <validate|update|check>\n  cargo xtask v8-verify [--target <triple>|--all-targets]"
    );
}
