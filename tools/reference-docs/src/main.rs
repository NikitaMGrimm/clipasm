//! Command-line entry point for checked-in `ClipAsm` reference generation.

use std::process::ExitCode;

fn main() -> ExitCode {
    match clipasm_reference_docs::run(std::env::args_os().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
