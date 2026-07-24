use std::fs::OpenOptions;
use std::io;
use std::io::Write as _;
use std::path::{Path, PathBuf};
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
        /// Write compiled JSON to a new path instead of stdout. Existing files are preserved.
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
            if let [output] = compiled.outputs() {
                if output.value_type() != clipasm::model::ValueType::Video {
                    println!(
                        "valid: {} semantic value(s), 1 output ({})",
                        compiled.value_count(),
                        output.value_type()
                    );
                } else if let Some(domain) = compiled.result_domain() {
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
            } else {
                println!(
                    "valid: {} semantic value(s), {} output(s)",
                    compiled.value_count(),
                    compiled.outputs().len()
                );
            }
        }
        Command::Compile { source, output } => {
            let compiled = compiler::compile_file(&source)?;
            let json = compiled.canonical_json()?;
            if let Some(output) = output {
                write_new_plan(&output, json.as_bytes())?;
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

fn write_new_plan(path: &Path, contents: &[u8]) -> Result<()> {
    let mut file = match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            return Err(Diagnostic::new(
                "E_PLAN_EXISTS",
                format!(
                    "refusing to replace existing plan destination `{}`",
                    path.display()
                ),
                SourceSpan::file_start(path),
            ));
        }
        Err(error) => {
            return Err(Diagnostic::new(
                "E_PLAN_IO",
                format!("could not create plan `{}`: {error}", path.display()),
                SourceSpan::file_start(path),
            ));
        }
    };
    if let Err(error) = file.write_all(contents) {
        let diagnostic = Diagnostic::new(
            "E_PLAN_IO",
            format!("could not write plan `{}`: {error}", path.display()),
            SourceSpan::file_start(path),
        );
        return match std::fs::remove_file(path) {
            Ok(()) => Err(diagnostic),
            Err(cleanup_error) => Err(diagnostic.note(format!(
                "could not remove incomplete plan `{}`: {cleanup_error}",
                path.display()
            ))),
        };
    }
    Ok(())
}
