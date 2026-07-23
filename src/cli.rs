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
    /// Parse, type-check, and infer source-independent video domains.
    Validate { workflow: PathBuf },
    /// Emit the canonical pure semantic compiled workflow.
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
            if let Some(domain) = compiled.root_domain() {
                println!(
                    "valid: {} semantic value(s), {} frame(s)",
                    compiled.nodes().len(),
                    domain.frames.0
                );
            } else {
                println!(
                    "valid: {} semantic value(s), duration resolves during preflight",
                    compiled.nodes().len()
                );
            }
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
