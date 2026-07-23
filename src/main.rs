#![allow(missing_docs)]

use std::process::ExitCode;

mod cli;

fn main() -> ExitCode {
    cli::run()
}
