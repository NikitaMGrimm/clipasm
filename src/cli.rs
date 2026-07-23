use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use crate::compiler;
use crate::diagnostic::{Diagnostic, Result, SourceSpan};

#[derive(Debug, Parser)]
#[command(name = "rhythmcut", version, about)]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Parse, type-check, and infer every video domain.
    Validate { workflow: PathBuf },
    /// Emit the canonical, fully lowered primitive plan.
    Compile {
        workflow: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Compile and render the workflow using `FFmpeg`.
    Render { workflow: PathBuf },
}

#[must_use]
pub fn run() -> ExitCode {
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
        Command::Validate { workflow } => {
            let compiled = compiler::compile_file(&workflow)?;
            let frames = compiled.root_domain().frames.0;
            println!(
                "valid: {} semantic value(s), {frames} frame(s)",
                compiled.nodes().len()
            );
        }
        Command::Compile { workflow, output } => {
            let compiled = compiler::compile_file(&workflow)?;
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
        Command::Render { workflow } => {
            let compiled = compiler::compile_file(&workflow)?;
            let prepared = crate::preflight::preflight(&compiled)?;
            let report = crate::render::render(&prepared)?;
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
