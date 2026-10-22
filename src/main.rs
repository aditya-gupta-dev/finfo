use std::process::ExitCode;

const HELP: &str = "finfo - Detailed file information

Usage: finfo [OPTIONS] <PATH>...

Options:
  -h, --help  Show this help
  --          Treat all remaining arguments as paths

Examples:
  finfo Cargo.toml /usr/bin/env src
  finfo -- -filename.txt

Author:     https://github.com/aditya-gupta-dev
Repository: https://github.com/aditya-gupta-dev/finfo";

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1).peekable();
    if args.peek().is_none() {
        println!("{HELP}");
        return ExitCode::SUCCESS;
    }

    let mut files = Vec::new();
    let mut paths_only = false;
    for arg in args {
        if !paths_only && arg == "--" {
            paths_only = true;
        } else if !paths_only && (arg == "-h" || arg == "--help") {
            println!("{HELP}");
            return ExitCode::SUCCESS;
        } else if !paths_only && arg.as_encoded_bytes().starts_with(b"-") {
            eprintln!(
                "Unknown option: {}\nUse 'finfo --help' for usage.",
                arg.to_string_lossy()
            );
            return ExitCode::FAILURE;
        } else {
            files.push(arg);
        }
    }
    if files.is_empty() {
        eprintln!("No paths provided.\nUse 'finfo --help' for usage.");
        return ExitCode::FAILURE;
    }
    match finfo::files::run(&files) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}
