use std::fmt::Write as _;
use std::fs::OpenOptions;
use std::io;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};

use clipasm::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use clipasm::source::{SourceFile, SourceSpan};
use clipasm::{compiler, language, preflight, render};

mod explain;
mod init;
mod programs;
mod project;

#[derive(Debug, Parser)]
#[command(
    name = "clipasm",
    bin_name = "clipasm",
    version,
    about = "Compile and render typed Video and Audio graphs."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create a self-contained `ClipAsm` starter project.
    #[command(
        about = "Create a self-contained ClipAsm starter project",
        long_about = "Create a self-contained ClipAsm starter project.\n\n\
            PATH defaults to the current directory and is created when needed. \
            Existing directories are supported only when every starter path is available. \
            Existing files and incompatible directories are never replaced.",
        after_long_help = "Examples:\n  clipasm init hello-video\n  clipasm init"
    )]
    Init {
        /// Directory to initialize. Defaults to the current directory.
        path: Option<PathBuf>,
    },
    /// List built-in programs or show one built-in program reference.
    #[command(
        about = "List built-in programs or show one built-in program reference",
        long_about = "List ClipAsm's callable built-in programs or show the reference for one built-in program.\n\n\
            This command never inspects a project, source file, media asset, FFmpeg, or FFprobe."
    )]
    Programs {
        /// Built-in program name. Omit to list every built-in program.
        #[arg(value_name = "NAME")]
        name: Option<String>,
    },
    /// Explain a `ClipAsm` diagnostic code.
    #[command(
        about = "Explain a ClipAsm diagnostic code",
        long_about = "Explain one built-in ClipAsm diagnostic code.\n\n\
            This command uses the catalog built into the executable. It never inspects a project, \
            source file, media asset, FFmpeg, or FFprobe."
    )]
    Explain {
        /// Built-in diagnostic code such as `E_UNKNOWN_PROGRAM`.
        #[arg(value_name = "CODE")]
        code: String,
    },
    /// Parse, type-check, and infer source-independent Video and Audio domains.
    Validate {
        /// Native `.clipasm` source program. Omit to use the discovered project entrypoint.
        source: Option<PathBuf>,
        #[command(flatten)]
        bindings: BindingArgs,
    },
    /// Inspect compiled Video and Audio semantics as JSON.
    Inspect {
        /// Native `.clipasm` source program. Omit to use the discovered project entrypoint.
        source: Option<PathBuf>,
        /// Write inspection JSON to a new path instead of stdout. Existing files are preserved.
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[command(flatten)]
        bindings: BindingArgs,
    },
    /// Compile and render a Video, including attached Audio, using `FFmpeg` and `FFprobe`.
    #[command(
        about = "Compile and render a Video, including attached Audio, using FFmpeg and FFprobe"
    )]
    Render {
        /// Native `.clipasm` source program. Omit to use the discovered project entrypoint.
        source: Option<PathBuf>,
        /// Override `config.output`. Relative paths resolve from the caller's working directory.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Override the project's persistent working-artifact cache policy.
        #[arg(long, value_enum, value_name = "MODE")]
        cache: Option<project::CacheSetting>,
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
        Command::Init { path } => {
            let target = path.as_deref().unwrap_or_else(|| Path::new("."));
            let initialized_target = init::initialize(target)?;
            let displayed_target = displayed_init_target(target);
            print_init_success(
                &displayed_target,
                path.is_none() || target_is_current_directory(&initialized_target),
            );
        }
        Command::Programs { name } => programs::print(name.as_deref())?,
        Command::Explain { code } => explain::print(&code)?,
        Command::Validate { source, bindings } => {
            let selection = resolve_source(source)?;
            let authored = language::parse_file(selection.source())?;
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
            let selection = resolve_source(source)?;
            let authored = language::parse_file(selection.source())?;
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
            cache,
            bindings,
        } => {
            let selection = resolve_source(source)?;
            let authored = language::parse_file(selection.source())?;
            let bindings = entrypoint_bindings(bindings, output)?;
            let compiled = compiler::compile_with_bindings(&authored, &bindings)?;
            let prepared = preflight::preflight(&compiled)?;
            let cache = cache.unwrap_or_else(|| {
                selection
                    .project()
                    .map_or(project::CacheSetting::Persistent, project::Project::cache)
            });
            let report = if cache.render_mode() == render::CacheMode::None {
                render::render_without_cache(&prepared)?
            } else if let Some(project) = selection.project() {
                render::render_with_cache_root(&prepared, &project.cache_root())?
            } else {
                render::render(&prepared)?
            };
            println!(
                "rendered {} (cache: {}; {} hit(s), {} miss(es)); manifest: {}",
                report.output().display(),
                report.cache_mode().label(),
                report.cache_hits(),
                report.cache_misses(),
                report.manifest().display()
            );
        }
    }
    Ok(())
}

fn print_init_success(target: &Path, initializes_current_directory: bool) {
    println!(
        "Created ClipAsm project at `{}`.",
        safe_display_path(target)
    );
    println!("\nNext:");
    if !initializes_current_directory {
        if let Some(target) = portable_shell_path(target) {
            println!("  cd \"{target}\"");
        } else {
            println!("  In the created project directory, run:");
        }
    }
    println!("  clipasm render");
    println!("\nOptional source check:");
    println!("  clipasm validate");
}

enum SourceSelection {
    Explicit(PathBuf),
    Project(project::Project),
}

impl SourceSelection {
    fn source(&self) -> &Path {
        match self {
            Self::Explicit(source) => source,
            Self::Project(project) => project.entrypoint(),
        }
    }

    const fn project(&self) -> Option<&project::Project> {
        match self {
            Self::Explicit(_) => None,
            Self::Project(project) => Some(project),
        }
    }
}

fn resolve_source(source: Option<PathBuf>) -> Result<SourceSelection> {
    source.map_or_else(
        || project::discover().map(SourceSelection::Project),
        |source| Ok(SourceSelection::Explicit(source)),
    )
}

fn safe_display_path(path: &Path) -> String {
    let path = path.to_string_lossy();
    let mut displayed = String::with_capacity(path.len());
    for character in path.chars() {
        if character.is_control() {
            write!(displayed, "\\u{{{:04X}}}", character as u32)
                .expect("writing to a String cannot fail");
        } else {
            displayed.push(character);
        }
    }
    displayed
}

fn portable_shell_path(path: &Path) -> Option<&str> {
    let path = path.to_str()?;
    if path.ends_with('\\')
        || path.chars().any(|character| {
            character.is_control() || matches!(character, '"' | '$' | '`' | '%' | '!')
        })
    {
        return None;
    }
    Some(path)
}

fn displayed_init_target(requested: &Path) -> PathBuf {
    requested.to_path_buf()
}

fn target_is_current_directory(target: &Path) -> bool {
    let Ok(target) = std::fs::canonicalize(target) else {
        return target == Path::new(".");
    };
    let Ok(current_directory) = std::env::current_dir().and_then(std::fs::canonicalize) else {
        return false;
    };
    target == current_directory
}

fn entrypoint_bindings(
    arguments: BindingArgs,
    output: Option<PathBuf>,
) -> Result<compiler::EntrypointBindings> {
    let current_directory = std::env::current_dir().map_err(|error| {
        Diagnostic::builtin(
            BuiltinDiagnostic::PathResolution,
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
    Diagnostic::builtin(
        BuiltinDiagnostic::InvalidCliBinding,
        format!("invalid {option} binding: {message}"),
        SourceSpan::source_start(source.clone()),
    )
}

fn write_new_inspection(path: &Path, contents: &[u8]) -> Result<()> {
    let mut file = match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            return Err(Diagnostic::builtin(
                BuiltinDiagnostic::InspectionExists,
                format!(
                    "refusing to replace existing inspection destination `{}`",
                    path.display()
                ),
                SourceSpan::file_start(path),
            ));
        }
        Err(error) => {
            return Err(Diagnostic::builtin(
                BuiltinDiagnostic::InspectionIo,
                format!("could not create inspection `{}`: {error}", path.display()),
                SourceSpan::file_start(path),
            ));
        }
    };
    if let Err(error) = file.write_all(contents) {
        let diagnostic = Diagnostic::builtin(
            BuiltinDiagnostic::InspectionIo,
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
