use std::fs::OpenOptions;
use std::io;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};

use clipasm::diagnostic::{Diagnostic, Result};
use clipasm::source::{SourceFile, SourceSpan};
use clipasm::{compiler, language, preflight, render};

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
        /// Native `.clipasm` source program.
        source: PathBuf,
        #[command(flatten)]
        bindings: BindingArgs,
    },
    /// Inspect the compiled semantic program as JSON.
    Inspect {
        /// Native `.clipasm` source program.
        source: PathBuf,
        /// Write inspection JSON to a new path instead of stdout. Existing files are preserved.
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[command(flatten)]
        bindings: BindingArgs,
    },
    /// Compile and render the source program using `FFmpeg`.
    Render {
        /// Native `.clipasm` source program. Relative paths resolve from its directory.
        source: PathBuf,
        /// Override `config.output`. Relative paths resolve from the caller's working directory.
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[command(flatten)]
        bindings: BindingArgs,
    },
}

#[derive(Clone, Debug, Args)]
struct BindingArgs {
    /// Bind a declared root Video input as `NAME=VIDEO_PATH`. May be repeated.
    #[arg(long = "video-input", value_name = "NAME=VIDEO_PATH")]
    video_inputs: Vec<String>,
    /// Bind a declared root Audio input as `NAME=AUDIO_PATH`. May be repeated.
    #[arg(long = "audio-input", value_name = "NAME=AUDIO_PATH")]
    audio_inputs: Vec<String>,
    /// Bind a declared root scalar parameter as `NAME=VALUE`. May be repeated.
    #[arg(long = "arg", value_name = "NAME=VALUE")]
    arguments: Vec<String>,
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
        Command::Validate { source, bindings } => {
            let authored = language::parse_file(&source)?;
            let bindings = entrypoint_bindings(bindings, None)?;
            let compiled = compiler::compile_with_bindings(&authored, &bindings)?;
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
                        domain.frames().0
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
        Command::Inspect {
            source,
            output,
            bindings,
        } => {
            let authored = language::parse_file(&source)?;
            let bindings = entrypoint_bindings(bindings, None)?;
            let compiled = compiler::compile_with_bindings(&authored, &bindings)?;
            let json = compiled.compiled_json()?;
            if let Some(output) = output {
                write_new_inspection(&output, json.as_bytes())?;
            } else {
                println!("{json}");
            }
        }
        Command::Render {
            source,
            output,
            bindings,
        } => {
            let authored = language::parse_file(&source)?;
            let bindings = entrypoint_bindings(bindings, output)?;
            let compiled = compiler::compile_with_bindings(&authored, &bindings)?;
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

fn entrypoint_bindings(
    arguments: BindingArgs,
    output: Option<PathBuf>,
) -> Result<compiler::EntrypointBindings> {
    let current_directory = std::env::current_dir().map_err(|error| {
        Diagnostic::new(
            "E_PATH_RESOLUTION",
            format!("could not determine the caller's working directory: {error}"),
            SourceSpan::file_start("<command-line>"),
        )
    })?;
    let source = SourceFile::with_base("<command-line>", Some(current_directory), "");
    let mut bindings = compiler::EntrypointBindings::new();

    for argument in arguments.video_inputs {
        let (name, value) = split_binding(&argument, "--video-input", &source)?;
        bindings.bind_video_input(
            name,
            PathBuf::from(value),
            SourceSpan::source_start(source.clone()),
        )?;
    }
    for argument in arguments.audio_inputs {
        let (name, value) = split_binding(&argument, "--audio-input", &source)?;
        bindings.bind_audio_input(
            name,
            PathBuf::from(value),
            SourceSpan::source_start(source.clone()),
        )?;
    }
    for argument in arguments.arguments {
        let (name, value) = split_binding(&argument, "--arg", &source)?;
        bindings.bind_parameter(name, value, SourceSpan::source_start(source.clone()))?;
    }
    if let Some(output) = output {
        bindings.set_output(output, SourceSpan::source_start(source));
    }
    Ok(bindings)
}

fn split_binding<'a>(
    value: &'a str,
    option: &str,
    source: &SourceFile,
) -> Result<(&'a str, &'a str)> {
    let Some((name, value)) = value.split_once('=') else {
        return Err(invalid_cli_binding(option, "expected NAME=VALUE", source));
    };
    if name.is_empty() {
        return Err(invalid_cli_binding(
            option,
            "binding name must not be empty",
            source,
        ));
    }
    if value.is_empty() {
        return Err(invalid_cli_binding(
            option,
            "binding value must not be empty",
            source,
        ));
    }
    Ok((name, value))
}

fn invalid_cli_binding(option: &str, message: &str, source: &SourceFile) -> Diagnostic {
    Diagnostic::new(
        "E_INVALID_CLI_BINDING",
        format!("invalid {option} binding: {message}"),
        SourceSpan::source_start(source.clone()),
    )
}

fn write_new_inspection(path: &Path, contents: &[u8]) -> Result<()> {
    let mut file = match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            return Err(Diagnostic::new(
                "E_INSPECTION_EXISTS",
                format!(
                    "refusing to replace existing inspection destination `{}`",
                    path.display()
                ),
                SourceSpan::file_start(path),
            ));
        }
        Err(error) => {
            return Err(Diagnostic::new(
                "E_INSPECTION_IO",
                format!("could not create inspection `{}`: {error}", path.display()),
                SourceSpan::file_start(path),
            ));
        }
    };
    if let Err(error) = file.write_all(contents) {
        let diagnostic = Diagnostic::new(
            "E_INSPECTION_IO",
            format!("could not write inspection `{}`: {error}", path.display()),
            SourceSpan::file_start(path),
        );
        return match std::fs::remove_file(path) {
            Ok(()) => Err(diagnostic),
            Err(cleanup_error) => Err(diagnostic.note(format!(
                "could not remove incomplete inspection `{}`: {cleanup_error}",
                path.display()
            ))),
        };
    }
    Ok(())
}
