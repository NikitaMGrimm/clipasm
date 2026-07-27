//! Pure source-program compilation and semantic graph inspection.
//!
//! Compilation binds typed program calls, evaluates scoped stack frames, resolves
//! references, infers source-independent video domains, and computes semantic
//! identity. It never reads media files or invokes external tools.

mod check;
mod checked;
mod domain;
mod draft;
mod entrypoint;
mod evaluate;
mod finalize;
pub(crate) mod fingerprint;
mod ids;
mod link;
mod parameter;
mod scalar_scope;
mod stack;
pub(crate) mod traversal;
mod typecheck;

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::model::{AudioSpec, ValueRef, VideoDomain, VideoSpec};
#[cfg(test)]
use crate::program::ProgramRegistry;
use crate::semantic::{CompiledNode, SymbolId};
use crate::source::{SourceFile, SourceSpan, Spanned};
use crate::source::{SourcePackage, SourceUnit};

pub use crate::semantic::SourceOrigin;
pub use entrypoint::EntrypointBindings;

const COMPILED_FORMAT_VERSION: u32 = 20;

#[derive(Clone, Debug)]
/// A pure compiled program whose media-dependent facts may remain deferred.
///
/// Use [`result_domain`](Self::result_domain) to inspect a domain known from
/// authored data, or pass the program to [`crate::preflight::preflight`] to
/// resolve assets and exact renderer primitives.
pub struct CompiledProgram {
    format_version: u32,
    engine_version: String,
    structure_hash: String,
    video: VideoSpec,
    audio: AudioSpec,
    nodes: Vec<CompiledNode>,
    outputs: Vec<ValueRef>,
    named_values: BTreeMap<String, ValueRef>,
    symbol_values: Vec<ValueRef>,
    explain: Vec<ExplainEntry>,
    output: Option<Spanned<PathBuf>>,
    entrypoint_source: SourceFile,
}

impl CompiledProgram {
    /// Return the number of semantic values in the compiled graph.
    #[must_use]
    pub fn value_count(&self) -> usize {
        self.nodes.len()
    }

    /// Serialize the pure compiled structure as stable, pretty JSON.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic if serialization fails.
    pub fn compiled_json(&self) -> Result<String> {
        crate::format::json::compiled_program(self)
    }

    #[must_use]
    /// Return the stable hash of compiled language and graph semantics.
    ///
    /// Source locations, comments, project location, and the Cargo package
    /// version do not contribute to this identity.
    pub fn structure_hash(&self) -> &str {
        &self.structure_hash
    }

    #[must_use]
    /// Return the project-wide dimensions and canonical frame rate.
    pub fn video(&self) -> &VideoSpec {
        &self.video
    }

    #[must_use]
    /// Return the canonical project audio properties.
    pub fn audio(&self) -> &AudioSpec {
        &self.audio
    }

    #[must_use]
    /// Return the single Video output domain when it is knowable without reading media.
    ///
    /// Returns `None` for zero or multiple outputs, non-Video output, or a
    /// Video-file source whose duration remains deferred until preflight.
    ///
    /// ```
    /// use std::path::Path;
    ///
    /// let program = clipasm::language::parse_str(
    ///     Path::new("program.clipasm"),
    ///     "clipasm 1\nimage(\"missing.png\", 1s)\n",
    /// )?;
    /// let compiled = clipasm::compiler::compile(&program)?;
    ///
    /// assert_eq!(compiled.result_domain().expect("authored domain").frames().0, 30);
    /// # Ok::<(), clipasm::diagnostic::Diagnostic>(())
    /// ```
    pub fn result_domain(&self) -> Option<&VideoDomain> {
        let (result, count) = video_output(&self.outputs);
        let result = (count == 1).then_some(result?)?;
        self.nodes[result.id().get() as usize].domain()
    }

    #[must_use]
    /// Return the source program's ordered semantic outputs.
    pub fn outputs(&self) -> &[ValueRef] {
        &self.outputs
    }

    #[must_use]
    /// Return source-oriented entries for user-visible program constructs.
    pub fn explain(&self) -> &[ExplainEntry] {
        &self.explain
    }

    pub(crate) fn nodes(&self) -> &[CompiledNode] {
        &self.nodes
    }

    pub(crate) const fn format_version(&self) -> u32 {
        self.format_version
    }

    pub(crate) fn engine_version(&self) -> &str {
        &self.engine_version
    }

    pub(crate) fn render_output(&self) -> Result<ValueRef> {
        let (output, count) = video_output(&self.outputs);
        let Some(output) = output.filter(|_| count == 1) else {
            return Err(Diagnostic::builtin(
                BuiltinDiagnostic::EntrypointOutputCount,
                format!(
                    "rendering requires exactly one Video output, but {count} Video values were produced"
                ),
                self.output.as_ref().map_or_else(
                    || SourceSpan::source_start(self.entrypoint_source.clone()),
                    |output| output.span.clone(),
                ),
            ));
        };
        Ok(output)
    }

    pub(crate) fn named_values(&self) -> &BTreeMap<String, ValueRef> {
        &self.named_values
    }

    pub(crate) fn symbol_value(&self, symbol: SymbolId) -> Option<ValueRef> {
        self.symbol_values.get(symbol.index()).copied()
    }

    pub(crate) fn symbol_values(&self) -> &[ValueRef] {
        &self.symbol_values
    }

    pub(crate) fn output(&self) -> Option<&Spanned<PathBuf>> {
        self.output.as_ref()
    }

    pub(crate) const fn entrypoint_source(&self) -> &SourceFile {
        &self.entrypoint_source
    }
}

#[derive(Clone, Debug)]
/// A user-visible source construct and the semantic value it produced.
///
/// Explain entries preserve authoring constructs even when their lowering
/// becomes one or more semantic operations.
pub struct ExplainEntry {
    construct: String,
    outputs: Vec<ExplainOutput>,
    span: SourceSpan,
}

#[derive(Clone, Debug)]
/// One ordered semantic output from an authored construct.
pub struct ExplainOutput {
    value: ValueRef,
    id: Option<String>,
}

impl ExplainEntry {
    #[must_use]
    /// Return the registered program name or reference/declaration label.
    pub fn construct(&self) -> &str {
        &self.construct
    }

    #[must_use]
    /// Return the ordered semantic values produced by this construct.
    pub fn outputs(&self) -> &[ExplainOutput] {
        &self.outputs
    }

    #[must_use]
    /// Return the source location of this authored construct.
    pub const fn span(&self) -> &SourceSpan {
        &self.span
    }
}

impl ExplainOutput {
    #[must_use]
    /// Return the semantic value produced at this output position.
    pub const fn value(&self) -> ValueRef {
        self.value
    }

    #[must_use]
    /// Return the optional user-authored name attached to this output.
    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }
}

/// Purely compile an already parsed source program.
///
/// Compilation can validate a video source even when the asset is unavailable:
///
/// ```
/// use std::path::Path;
///
/// let program = clipasm::language::parse_str(
///     Path::new("program.clipasm"),
///     "clipasm 1\nvideo(\"unavailable.mp4\")\n",
/// )?;
/// let compiled = clipasm::compiler::compile(&program)?;
///
/// assert!(compiled.result_domain().is_none());
/// # Ok::<(), clipasm::diagnostic::Diagnostic>(())
/// ```
///
/// # Errors
///
/// Returns a diagnostic for invalid programs, stack behavior, references,
/// types, cycles, or frame domains.
pub fn compile(package: &SourcePackage) -> Result<CompiledProgram> {
    compile_with_bindings(package, &EntrypointBindings::new())
}

/// Purely compile a parsed source package with external root-program bindings.
///
/// Bindings are matched against the root program's declared `inputs` and
/// `parameters`. Relative file paths use the source base carried by each
/// binding span. An optional output binding overrides `config.output` for this
/// compilation without changing the authored package.
///
/// # Errors
///
/// Returns a diagnostic for unknown, duplicate, missing, or ill-typed root
/// bindings, or for any ordinary compilation failure.
pub fn compile_with_bindings(
    package: &SourcePackage,
    bindings: &EntrypointBindings,
) -> Result<CompiledProgram> {
    compile_checked(package, &check::check(package)?, bindings)
}

#[cfg(test)]
pub(crate) fn compile_with_registry(
    package: &SourcePackage,
    registry: &ProgramRegistry,
) -> Result<CompiledProgram> {
    let checked = check::check_with_registry(package, registry)?;
    compile_checked(package, &checked, &EntrypointBindings::new())
}

fn compile_checked(
    package: &SourcePackage,
    checked: &checked::CheckedPackage,
    bindings: &EntrypointBindings,
) -> Result<CompiledProgram> {
    let entrypoint = package.root();
    let video = resolve_video_spec(entrypoint)?;
    let audio = resolve_audio_spec(entrypoint)?;
    let evaluation = evaluate::evaluate(&video, audio, entrypoint.program(), checked, bindings)?;
    let output = bindings.output.as_ref().or_else(|| entrypoint.output());
    validate_publication_output(entrypoint, output, &evaluation)?;
    finalize::finalize(
        entrypoint,
        output.cloned(),
        video,
        audio,
        evaluation,
        COMPILED_FORMAT_VERSION,
    )
}

fn validate_publication_output(
    entrypoint: &SourceUnit,
    output: Option<&Spanned<PathBuf>>,
    evaluation: &evaluate::Evaluation,
) -> Result<()> {
    if output.is_none() {
        return Ok(());
    }
    let (output, count) = video_output(&evaluation.outputs);
    let Some(output) = output.filter(|_| count == 1) else {
        return Err(Diagnostic::builtin(
            BuiltinDiagnostic::EntrypointOutputCount,
            format!(
                "a source program with `output` must produce exactly one Video, but {count} Video values remain"
            ),
            entrypoint.program().span().clone(),
        ));
    };
    debug_assert_eq!(output.value_type(), crate::model::ValueType::Video);
    Ok(())
}

fn video_output(outputs: &[ValueRef]) -> (Option<ValueRef>, usize) {
    outputs
        .iter()
        .copied()
        .fold((None, 0), |(first, count), value| {
            if value.value_type() == crate::model::ValueType::Video {
                (first.or(Some(value)), count + 1)
            } else {
                (first, count)
            }
        })
}

fn resolve_video_spec(entrypoint: &SourceUnit) -> Result<VideoSpec> {
    let defaults = VideoSpec::default();
    let Some(project) = entrypoint.project() else {
        return Ok(defaults);
    };
    let width = resolve_dimension(
        project.value.video.width.as_ref(),
        defaults.width(),
        "width",
    )?;
    let height = resolve_dimension(
        project.value.video.height.as_ref(),
        defaults.height(),
        "height",
    )?;
    let fps = match &project.value.video.fps {
        Some(fps) => crate::model::FrameRate::parse(&fps.value, &fps.span)?,
        None => defaults.fps(),
    };
    Ok(VideoSpec::new(width, height, fps)
        .expect("positive dimensions and frame rate form a valid VideoSpec"))
}

fn resolve_audio_spec(entrypoint: &SourceUnit) -> Result<AudioSpec> {
    let defaults = AudioSpec::default();
    let Some(project) = entrypoint.project() else {
        return Ok(defaults);
    };
    let sample_rate = match &project.value.audio.sample_rate {
        Some(sample_rate) if sample_rate.value == 0 => {
            return Err(Diagnostic::builtin(
                BuiltinDiagnostic::InvalidAudioSpec,
                "`sample_rate` must be greater than zero",
                sample_rate.span.clone(),
            ));
        }
        Some(sample_rate) => sample_rate.value,
        None => defaults.sample_rate(),
    };
    Ok(AudioSpec::new(sample_rate, defaults.channels())
        .expect("positive project audio settings form a valid AudioSpec"))
}

fn resolve_dimension(setting: Option<&Spanned<u32>>, default: u32, name: &str) -> Result<u32> {
    match setting {
        Some(setting) if setting.value == 0 => Err(Diagnostic::builtin(
            BuiltinDiagnostic::InvalidVideoSpec,
            format!("`{name}` must be greater than zero"),
            setting.span.clone(),
        )),
        Some(setting) => Ok(setting.value),
        None => Ok(default),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;

    use super::*;
    use crate::program::StackAccess;
    use crate::source::{
        ArgumentValue, Invocation, Item, ItemKind, ItemOrigin, Literal, OutputBindings,
        ProgramBody, ProjectSettings, ScalarExpression, SourcePackage, SourceProgram,
        SourceProgramImplementation, SourceUnit, SourceUnitId, StackBlock,
    };
    use crate::source::{SourceFile, SourceSpan, Spanned};

    #[test]
    fn native_lowering_matches_direct_canonical_source() {
        let text = "clipasm 1\nimage(\"card.png\", 1s)\n";
        let lowered =
            crate::language::parse_str(Path::new("program.clipasm"), text).expect("native source");

        let source = SourceFile::new("program.clipasm", text);
        let program_span = SourceSpan::at(source.clone(), 1, 9);
        let item_span = SourceSpan::at(source.clone(), 2, 1);
        let arguments = BTreeMap::from([
            (
                "duration".to_owned(),
                ArgumentValue::Scalar(ScalarExpression::Literal(Literal::String(
                    "1s".to_owned(),
                    SourceSpan::at(source.clone(), 2, 19),
                ))),
            ),
            (
                "path".to_owned(),
                ArgumentValue::Scalar(ScalarExpression::Literal(Literal::String(
                    "card.png".to_owned(),
                    SourceSpan::at(source.clone(), 2, 7),
                ))),
            ),
        ]);
        let direct = SourcePackage {
            root: SourceUnitId(0),
            units: vec![SourceUnit {
                source,
                imports: Vec::new(),
                project: None,
                program: SourceProgram {
                    inputs: Vec::new(),
                    parameters: Vec::new(),
                    implementation: SourceProgramImplementation::Body(ProgramBody {
                        items: vec![Item {
                            kind: ItemKind::Invocation(Invocation {
                                program: Spanned::new("image".to_owned(), item_span.clone()),
                                type_argument: None,
                                stack_access: None,
                                arguments,
                                body: None,
                            }),
                            output_bindings: OutputBindings::None,
                            origin: ItemOrigin::authored("image", item_span),
                        }],
                        span: SourceSpan::at(program_span.source().clone(), 1, 1),
                    }),
                    span: program_span,
                    stack_access: StackAccess::Owned,
                },
                output: None,
            }],
        };

        assert_eq!(
            compile(&lowered).expect("lowered compile").structure_hash(),
            compile(&direct).expect("direct compile").structure_hash()
        );
    }

    #[test]
    fn stack_blocks_return_all_owned_outputs_in_order() {
        let source = SourceFile::new("program.clipasm", "{ image audio }");
        let span = SourceSpan::source_start(source.clone());
        let invocation = |name: &str, arguments: &[(&str, &str)]| Item {
            kind: ItemKind::Invocation(Invocation {
                program: Spanned::new(name.to_owned(), span.clone()),
                type_argument: None,
                stack_access: None,
                arguments: arguments
                    .iter()
                    .map(|(name, value)| {
                        (
                            (*name).to_owned(),
                            ArgumentValue::Scalar(ScalarExpression::Literal(Literal::String(
                                (*value).to_owned(),
                                span.clone(),
                            ))),
                        )
                    })
                    .collect(),
                body: None,
            }),
            output_bindings: OutputBindings::None,
            origin: ItemOrigin::authored(name, span.clone()),
        };
        let package = SourcePackage {
            root: SourceUnitId(0),
            units: vec![SourceUnit {
                source,
                imports: Vec::new(),
                project: Some(Spanned::new(ProjectSettings::default(), span.clone())),
                program: SourceProgram {
                    inputs: Vec::new(),
                    parameters: Vec::new(),
                    implementation: SourceProgramImplementation::Body(ProgramBody {
                        items: vec![Item {
                            kind: ItemKind::StackBlock(StackBlock {
                                stack_access: StackAccess::Owned,
                                body: ProgramBody {
                                    items: vec![
                                        invocation(
                                            "image",
                                            &[("path", "card.png"), ("duration", "1s")],
                                        ),
                                        invocation("audio", &[("path", "sound.wav")]),
                                    ],
                                    span: span.clone(),
                                },
                            }),
                            output_bindings: OutputBindings::Many(
                                vec![
                                    Spanned::new("picture".to_owned(), span.clone()),
                                    Spanned::new("sound".to_owned(), span.clone()),
                                ],
                                span.clone(),
                            ),
                            origin: ItemOrigin::authored("stack block", span.clone()),
                        }],
                        span: span.clone(),
                    }),
                    span,
                    stack_access: StackAccess::Owned,
                },
                output: None,
            }],
        };

        let compiled = compile(&package).expect("stack block compile");
        assert_eq!(
            compiled
                .outputs()
                .iter()
                .map(|output| output.value_type())
                .collect::<Vec<_>>(),
            vec![
                crate::model::ValueType::Video,
                crate::model::ValueType::Audio
            ]
        );
        assert_eq!(
            compiled.named_values()["picture"].value_type(),
            crate::model::ValueType::Video
        );
        assert_eq!(
            compiled.named_values()["sound"].value_type(),
            crate::model::ValueType::Audio
        );
    }
}
