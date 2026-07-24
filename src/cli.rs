use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use clipasm::diagnostic::{Diagnostic, Result, SourceSpan};
use clipasm::{compiler, preflight, render};

#[derive(Debug, Parser)]
#[command(name = "clipasm", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Parse, type-check, and infer source-independent video domains.
    Validate {
        /// Source program YAML file.
        source: PathBuf,
    },
    /// Emit the canonical pure semantic compiled program.
    Compile {
        /// Source program YAML file.
        source: PathBuf,
        /// Write compiled JSON to this path instead of stdout.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Compile and render the source program using `FFmpeg`.
    Render {
        /// Source program YAML file. Relative media and output paths resolve from its directory.
        source: PathBuf,
    },
}

#[must_use]
pub(crate) fn run() -> ExitCode {
    match execute(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn execute(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Validate { source } => {
            let compiled = compiler::compile_file(&source)?;
            if let Some(domain) = compiled.result_domain() {
                println!(
                    "valid: {} semantic value(s), {} frame(s)",
                    compiled.value_count(),
                    domain.frames.0
                );
            } else {
                println!(
                    "valid: {} semantic value(s), duration resolves during preflight",
                    compiled.value_count()
                );
            }
        }
        Command::Compile { source, output } => {
            let compiled = compiler::compile_file(&source)?;
            let json = compiled.canonical_json()?;
            if let Some(output) = output {
                fs::write(&output, json).map_err(|error| {
                    Diagnostic::new(
                        "E_PLAN_IO",
                        format!("could not write plan `{}`: {error}", output.display()),
                        SourceSpan::file_start(&output),
                    )
                })?;
            } else {
                println!("{json}");
            }
        }
        Command::Render { source } => {
            let compiled = compiler::compile_file(&source)?;
            let prepared = preflight::preflight(&compiled)?;
            let report = render::render(&prepared)?;
            println!(
                "rendered {} (cache: {} hit(s), {} miss(es)); manifest: {}",
                report.output.display(),
                report.cache_hits,
                report.cache_misses,
                report.manifest.display()
            );
        }
    }
    Ok(())
}
