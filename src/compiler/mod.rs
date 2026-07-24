//! Pure source-program compilation and semantic graph inspection.
//!
//! Compilation binds typed program calls, evaluates scoped stack frames, resolves
//! references, infers source-independent video domains, and computes semantic
//! identity. It never reads media files or invokes external tools.

mod bind;
mod evaluate;
pub(crate) mod fingerprint;
mod infer;
mod resolve;
mod stack;
pub(crate) mod traversal;

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::diagnostic::{Diagnostic, Result};
use crate::model::{ValueRef, VideoDomain, VideoSpec};
use crate::program::ProgramRegistry;
use crate::semantic::{CompiledNode, SymbolId};
use crate::source::{SourceFile, SourceSpan, Spanned};
use crate::source::{SourcePackage, SourceUnit};

pub use crate::semantic::SourceOrigin;

const COMPILED_FORMAT_VERSION: u32 = 9;

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
    nodes: Vec<CompiledNode>,
    outputs: Vec<ValueRef>,
    named_values: BTreeMap<String, ValueRef>,
    symbol_values: BTreeMap<SymbolId, ValueRef>,
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
    pub fn canonical_json(&self) -> Result<String> {
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
    /// Return the single Video output domain when it is knowable without reading media.
    ///
    /// Returns `None` for zero or multiple outputs, non-Video output, or a
    /// Video-file source whose duration remains deferred until preflight.
    ///
    /// ```
    /// use std::path::Path;
    ///
    /// let program = clipasm::frontend::yaml::parse_str(
    ///     Path::new("program.yaml"),
    ///     "- program:\n    version: 1\n\n- image: {path: missing.png, duration: 1s}\n",
    /// )?;
    /// let compiled = clipasm::compiler::compile(&program)?;
    ///
    /// assert_eq!(compiled.result_domain().expect("authored domain").frames.0, 30);
    /// # Ok::<(), clipasm::diagnostic::Diagnostic>(())
    /// ```
    pub fn result_domain(&self) -> Option<&VideoDomain> {
        let [result] = self.outputs.as_slice() else {
            return None;
        };
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
        let [output] = self.outputs.as_slice() else {
            return Err(Diagnostic::new(
                "E_ENTRYPOINT_OUTPUT_COUNT",
                format!(
                    "rendering requires exactly one source output, but {} values were produced",
                    self.outputs.len()
                ),
                self.output.as_ref().map_or_else(
                    || SourceSpan::source_start(self.entrypoint_source.clone()),
                    |output| output.span.clone(),
                ),
            ));
        };
        if output.value_type() != crate::model::ValueType::Video {
            return Err(Diagnostic::new(
                "E_ENTRYPOINT_OUTPUT_TYPE",
                format!(
                    "rendering requires one Video output, but the source produced {}",
                    output.value_type()
                ),
                self.output.as_ref().map_or_else(
                    || SourceSpan::source_start(self.entrypoint_source.clone()),
                    |output| output.span.clone(),
                ),
            ));
        }
        Ok(*output)
    }

    pub(crate) fn named_values(&self) -> &BTreeMap<String, ValueRef> {
        &self.named_values
    }

    pub(crate) fn symbol_value(&self, symbol: SymbolId) -> Option<ValueRef> {
        self.symbol_values.get(&symbol).copied()
    }

    pub(crate) fn symbol_values(&self) -> &BTreeMap<SymbolId, ValueRef> {
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
/// let program = clipasm::frontend::yaml::parse_str(
///     Path::new("program.yaml"),
///     "- program:\n    version: 1\n\n- video: unavailable.mp4\n",
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
    compile_with_registry(package, infer::build_catalog(package)?)
}

pub(crate) fn compile_with_registry(
    package: &SourcePackage,
    registry: ProgramRegistry,
) -> Result<CompiledProgram> {
    let entrypoint = package.root();
    let video = resolve_video_spec(entrypoint)?;
    let evaluation = evaluate::evaluate(package, &video, registry)?;
    validate_publication_output(entrypoint, &evaluation)?;
    resolve::finalize(entrypoint, video, evaluation, COMPILED_FORMAT_VERSION)
}

fn validate_publication_output(
    entrypoint: &SourceUnit,
    evaluation: &evaluate::Evaluation,
) -> Result<()> {
    if entrypoint.output().is_none() {
        return Ok(());
    }
    let [output] = evaluation.outputs.as_slice() else {
        return Err(Diagnostic::new(
            "E_ENTRYPOINT_OUTPUT_COUNT",
            format!(
                "a source program with `output` must produce exactly one value, but {} values remain",
                evaluation.outputs.len()
            ),
            entrypoint.program().span().clone(),
        ));
    };
    if output.value_type() != crate::model::ValueType::Video {
        return Err(Diagnostic::new(
            "E_ENTRYPOINT_OUTPUT_TYPE",
            format!(
                "a source program with `output` must produce one Video, but produced {}",
                output.value_type()
            ),
            entrypoint.program().span().clone(),
        ));
    }
    Ok(())
}

fn resolve_video_spec(entrypoint: &SourceUnit) -> Result<VideoSpec> {
    let mut spec = VideoSpec::default();
    let Some(project) = entrypoint.project() else {
        return Ok(spec);
    };
    if let Some(width) = &project.value.video.width {
        spec.width = width.value;
    }
    if let Some(height) = &project.value.video.height {
        spec.height = height.value;
    }
    if let Some(fps) = &project.value.video.fps {
        spec.fps = crate::model::FrameRate::parse(&fps.value, &fps.span)?;
    }
    Ok(spec)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;

    use super::*;
    use crate::program::StackAccess;
    use crate::source::{
        ArgumentValue, Invocation, Item, ItemKind, Literal, OutputBindings, ProgramBody,
        ProjectSettings, SourcePackage, SourceProgram, SourceUnit, SourceUnitId,
    };
    use crate::source::{SourceFile, SourceSpan, Spanned};

    #[test]
    fn compilation_is_independent_of_the_yaml_frontend() {
        let text = "- program:\n    version: 1\n\n- image: {path: card.png, duration: 1s}\n";
        let yaml =
            crate::frontend::yaml::parse_str(Path::new("program.yaml"), text).expect("YAML source");

        let source = SourceFile::new("program.yaml", text);
        let program_span = SourceSpan::at(source.clone(), 1, 9);
        let item_span = SourceSpan::at(source.clone(), 4, 8);
        let arguments = BTreeMap::from([
            (
                "duration".to_owned(),
                ArgumentValue::Literal(Literal::String(
                    "1s".to_owned(),
                    SourceSpan::at(source.clone(), 4, 42),
                )),
            ),
            (
                "path".to_owned(),
                ArgumentValue::Literal(Literal::String(
                    "card.png".to_owned(),
                    SourceSpan::at(source.clone(), 4, 22),
                )),
            ),
        ]);
        let direct = SourcePackage {
            root: SourceUnitId(0),
            units: vec![SourceUnit {
                source,
                imports: Vec::new(),
                project: Some(Spanned::new(
                    ProjectSettings::default(),
                    program_span.clone(),
                )),
                program: SourceProgram {
                    inputs: Vec::new(),
                    parameters: Vec::new(),
                    clips: Vec::new(),
                    body: ProgramBody {
                        items: vec![Item {
                            kind: ItemKind::Invocation(Invocation {
                                program: Spanned::new("image".to_owned(), item_span.clone()),
                                stack_access: None,
                                arguments,
                                body: None,
                            }),
                            output_bindings: OutputBindings::None,
                            span: item_span,
                        }],
                        span: SourceSpan::at(program_span.source().clone(), 1, 1),
                    },
                    span: program_span,
                    stack_access: StackAccess::Owned,
                },
                output: None,
            }],
        };

        assert_eq!(
            compile(&yaml).expect("YAML compile").structure_hash(),
            compile(&direct).expect("direct compile").structure_hash()
        );
    }
}
