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
        /// SourceProgram YAML file.
        workflow: PathBuf,
    },
    /// Emit the canonical pure semantic compiled workflow.
    Compile {
        /// SourceProgram YAML file.
        workflow: PathBuf,
        /// Write compiled JSON to this path instead of stdout.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Compile and render the workflow using `FFmpeg`.
    Render {
        /// SourceProgram YAML file. Relative media and output paths resolve from its directory.
        workflow: PathBuf,
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
        Command::Validate { workflow } => {
            let compiled = compiler::compile_file(&workflow)?;
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
