mod evaluate;
pub(crate) mod fingerprint;
mod resolve;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::diagnostic::{Diagnostic, Result, SourceSpan, Spanned};
use crate::model::{
    FrameCount, FrameRange, ImageFit, ValueId, ValueRef, ValueType, VideoDomain, VideoSpec,
};
use crate::program::{ProgramDefinition, ProgramRegistry};
use crate::syntax::{Argument, Workflow};

const COMPILED_FORMAT_VERSION: u32 = 3;

#[derive(Clone, Debug, Serialize)]
pub struct CompiledWorkflow {
    format_version: u32,
    workflow_version: u64,
    engine_version: String,
    structure_hash: String,
    video: VideoSpec,
    nodes: Vec<CompiledNode>,
    root: ValueRef,
    named_values: BTreeMap<String, ValueRef>,
    explain: Vec<ExplainEntry>,
    output: Option<Spanned<PathBuf>>,
    #[serde(skip)]
    source_path: PathBuf,
}

impl CompiledWorkflow {
    /// Serialize the pure compiled structure as stable, pretty JSON.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic if serialization fails.
    pub fn canonical_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(|error| {
            Diagnostic::new(
                "E_PLAN_SERIALIZATION",
                format!("could not serialize compiled workflow: {error}"),
                SourceSpan::file_start("<compiled-workflow>"),
            )
        })
    }

    #[must_use]
    pub fn structure_hash(&self) -> &str {
        &self.structure_hash
    }

    #[must_use]
    pub fn video(&self) -> &VideoSpec {
        &self.video
    }

    #[must_use]
    /// Return the root Video domain when it is knowable without reading media.
    ///
    /// Video-file source durations remain deferred until preflight.
    pub fn root_domain(&self) -> Option<&VideoDomain> {
        self.nodes[self.root.id().get() as usize].domain.as_ref()
    }

    #[must_use]
    pub fn explain(&self) -> &[ExplainEntry] {
        &self.explain
    }

    pub(crate) fn nodes(&self) -> &[CompiledNode] {
        &self.nodes
    }

    pub(crate) const fn root(&self) -> ValueRef {
        self.root
    }

    pub(crate) fn named_values(&self) -> &BTreeMap<String, ValueRef> {
        &self.named_values
    }

    pub(crate) fn output(&self) -> Option<&Spanned<PathBuf>> {
        self.output.as_ref()
    }

    pub(crate) fn source_path(&self) -> &Path {
        &self.source_path
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ExplainEntry {
    construct: String,
    output: ValueRef,
    id: Option<String>,
    span: SourceSpan,
}

impl ExplainEntry {
    #[must_use]
    pub fn construct(&self) -> &str {
        &self.construct
    }

    #[must_use]
    pub const fn output(&self) -> ValueRef {
        self.output
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct CompiledNode {
    id: ValueId,
    kind: SemanticNodeKind,
    value_type: ValueType,
    domain: Option<VideoDomain>,
    semantic_version: u32,
    origin: SourceOrigin,
}

impl CompiledNode {
    pub(crate) const fn kind(&self) -> &SemanticNodeKind {
        &self.kind
    }

    pub(crate) fn domain(&self) -> Option<&VideoDomain> {
        self.domain.as_ref()
    }

    pub(crate) const fn semantic_version(&self) -> u32 {
        self.semantic_version
    }

    pub(crate) const fn origin(&self) -> &SourceOrigin {
        &self.origin
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub(crate) enum SemanticNodeKind {
    ImageVideo {
        path: PathBuf,
        frames: FrameCount,
        fit: ImageFit,
    },
    VideoSource {
        path: PathBuf,
        fit: ImageFit,
    },
    Reference {
        name: String,
    },
    Concat {
        inputs: Vec<ValueRef>,
    },
    Slice {
        input: ValueRef,
        range: FrameRange,
    },
    During {
        base: ValueRef,
        processed: ValueRef,
        range: FrameRange,
    },
}

#[derive(Clone, Debug, Serialize)]
pub struct SourceOrigin {
    pub construct: String,
    pub span: SourceSpan,
}

impl SourceOrigin {
    #[must_use]
    pub fn clone_with_construct(&self, construct: impl Into<String>) -> Self {
        Self {
            construct: construct.into(),
            span: self.span.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct DraftNode {
    kind: SemanticNodeKind,
    value_type: ValueType,
    semantic_version: u32,
    origin: SourceOrigin,
}

#[derive(Clone, Debug)]
pub(super) enum DeclaredValueType {
    Known(ValueType),
    Alias(String),
}

#[derive(Clone, Debug)]
pub(super) struct Symbol {
    declared_at: SourceSpan,
    value: Option<ValueRef>,
    declared_type: DeclaredValueType,
    value_type: Option<ValueType>,
}

#[derive(Clone, Debug)]
pub(super) struct SurfaceRecord {
    construct: String,
    value: ValueRef,
    id: Option<String>,
    span: SourceSpan,
}

pub(super) struct Evaluation {
    nodes: Vec<DraftNode>,
    symbols: BTreeMap<String, Symbol>,
    symbol_order: Vec<String>,
    surface: Vec<SurfaceRecord>,
    root: ValueRef,
}

pub(crate) struct ResolvedCall<'a> {
    definition: &'static ProgramDefinition,
    inputs: BTreeMap<&'static str, Vec<ValueRef>>,
    parameters: &'a BTreeMap<String, Argument>,
    requested_frames: Option<FrameCount>,
    origin: SourceOrigin,
}

impl ResolvedCall<'_> {
    #[must_use]
    pub(crate) const fn definition(&self) -> &'static ProgramDefinition {
        self.definition
    }

    #[must_use]
    pub(crate) const fn requested_frames(&self) -> Option<u64> {
        match self.requested_frames {
            Some(frames) => Some(frames.0),
            None => None,
        }
    }

    #[must_use]
    pub(crate) const fn origin(&self) -> &SourceOrigin {
        &self.origin
    }

    /// Return one already bound input.
    ///
    /// # Errors
    ///
    /// Returns an internal binding diagnostic if the named port is absent.
    pub(crate) fn one_input(&self, name: &str) -> Result<ValueRef> {
        self.inputs
            .get(name)
            .and_then(|values| values.first())
            .copied()
            .ok_or_else(|| {
                Diagnostic::new(
                    "E_INTERNAL_BINDING",
                    format!("resolved call is missing input `{name}`"),
                    self.origin.span.clone(),
                )
            })
    }

    /// Return an already bound variadic input.
    ///
    /// # Errors
    ///
    /// Returns an internal binding diagnostic if the named port is absent.
    pub(crate) fn variadic_input(&self, name: &str) -> Result<&[ValueRef]> {
        self.inputs.get(name).map(Vec::as_slice).ok_or_else(|| {
            Diagnostic::new(
                "E_INTERNAL_BINDING",
                format!("resolved call is missing variadic input `{name}`"),
                self.origin.span.clone(),
            )
        })
    }

    /// Return a required string parameter.
    ///
    /// # Errors
    ///
    /// Returns a parameter diagnostic when missing or mistyped.
    pub(crate) fn string_parameter(&self, name: &str) -> Result<(&str, &SourceSpan)> {
        let argument = self.parameters.get(name).ok_or_else(|| {
            Diagnostic::new(
                "E_MISSING_ARGUMENT",
                format!("missing required parameter `{name}`"),
                self.origin.span.clone(),
            )
        })?;
        match argument {
            Argument::String(value, span) => Ok((value, span)),
            _ => Err(parameter_type_error(name, "string", argument)),
        }
    }

    /// Return an optional string parameter.
    ///
    /// # Errors
    ///
    /// Returns a parameter diagnostic when present with the wrong type.
    pub(crate) fn optional_string_parameter(
        &self,
        name: &str,
    ) -> Result<Option<(&str, &SourceSpan)>> {
        self.parameters
            .get(name)
            .map(|argument| match argument {
                Argument::String(value, span) => Ok((value.as_str(), span)),
                _ => Err(parameter_type_error(name, "string", argument)),
            })
            .transpose()
    }

    /// Return a required integer parameter.
    ///
    /// # Errors
    ///
    /// Returns a parameter diagnostic when missing or mistyped.
    pub(crate) fn integer_parameter(&self, name: &str) -> Result<(i64, &SourceSpan)> {
        let argument = self.parameters.get(name).ok_or_else(|| {
            Diagnostic::new(
                "E_MISSING_ARGUMENT",
                format!("missing required parameter `{name}`"),
                self.origin.span.clone(),
            )
        })?;
        match argument {
            Argument::Integer(value, span) => Ok((*value, span)),
            _ => Err(parameter_type_error(name, "integer", argument)),
        }
    }
}

fn parameter_type_error(name: &str, expected: &str, argument: &Argument) -> Diagnostic {
    Diagnostic::new(
        "E_INVALID_ARGUMENT_TYPE",
        format!("parameter `{name}` must be a {expected}"),
        argument.span().clone(),
    )
}

pub(crate) struct GraphBuilder<'a> {
    nodes: &'a mut Vec<DraftNode>,
    video: &'a VideoSpec,
}

impl<'a> GraphBuilder<'a> {
    pub(super) fn new(nodes: &'a mut Vec<DraftNode>, video: &'a VideoSpec) -> Self {
        Self { nodes, video }
    }

    #[must_use]
    pub(crate) const fn video_spec(&self) -> &VideoSpec {
        self.video
    }

    /// Add a pure semantic still-image Video source.
    ///
    /// # Errors
    ///
    /// Returns a graph-size diagnostic.
    pub(crate) fn image_video(
        &mut self,
        path: PathBuf,
        frames: FrameCount,
        fit: ImageFit,
        semantic_version: u32,
        origin: SourceOrigin,
    ) -> Result<ValueRef> {
        self.push(
            SemanticNodeKind::ImageVideo { path, frames, fit },
            ValueType::Video,
            semantic_version,
            origin,
        )
    }

    /// Add a pure semantic video-file source with an authored frame domain.
    ///
    /// # Errors
    ///
    /// Returns a graph-size diagnostic.
    pub(crate) fn video_source(
        &mut self,
        path: PathBuf,
        fit: ImageFit,
        semantic_version: u32,
        origin: SourceOrigin,
    ) -> Result<ValueRef> {
        self.push(
            SemanticNodeKind::VideoSource { path, fit },
            ValueType::Video,
            semantic_version,
            origin,
        )
    }

    /// Add a checked semantic Video slice.
    ///
    /// # Errors
    ///
    /// Returns a type or graph-size diagnostic.
    pub(crate) fn slice(
        &mut self,
        input: ValueRef,
        range: FrameRange,
        origin: SourceOrigin,
    ) -> Result<ValueRef> {
        require_value_type(input, ValueType::Video, "slice", "input", &origin.span)?;
        self.push(
            SemanticNodeKind::Slice { input, range },
            ValueType::Video,
            1,
            origin,
        )
    }

    /// Add a checked semantic concatenation, aliasing one input.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic for empty, mistyped, or oversized graphs.
    pub(crate) fn concat(
        &mut self,
        inputs: Vec<ValueRef>,
        semantic_version: u32,
        origin: SourceOrigin,
    ) -> Result<ValueRef> {
        if inputs.is_empty() {
            return Err(Diagnostic::new(
                "E_EMPTY_CONCAT",
                "`concat` requires at least one Video",
                origin.span,
            ));
        }
        for input in &inputs {
            require_value_type(*input, ValueType::Video, "concat", "videos", &origin.span)?;
        }
        if inputs.len() == 1 {
            return Ok(inputs[0]);
        }
        self.push(
            SemanticNodeKind::Concat { inputs },
            ValueType::Video,
            semantic_version,
            origin,
        )
    }

    pub(super) fn reference(
        &mut self,
        name: String,
        value_type: ValueType,
        origin: SourceOrigin,
    ) -> Result<ValueRef> {
        self.push(SemanticNodeKind::Reference { name }, value_type, 1, origin)
    }

    pub(super) fn during(
        &mut self,
        base: ValueRef,
        processed: ValueRef,
        range: FrameRange,
        origin: SourceOrigin,
    ) -> Result<ValueRef> {
        require_value_type(base, ValueType::Video, "during", "base", &origin.span)?;
        require_value_type(
            processed,
            ValueType::Video,
            "during",
            "processed",
            &origin.span,
        )?;
        self.push(
            SemanticNodeKind::During {
                base,
                processed,
                range,
            },
            ValueType::Video,
            1,
            origin,
        )
    }

    fn push(
        &mut self,
        kind: SemanticNodeKind,
        value_type: ValueType,
        semantic_version: u32,
        origin: SourceOrigin,
    ) -> Result<ValueRef> {
        let id = ValueId::new(u32::try_from(self.nodes.len()).map_err(|_| {
            Diagnostic::new(
                "E_GRAPH_TOO_LARGE",
                "semantic graph contains too many values",
                origin.span.clone(),
            )
        })?);
        self.nodes.push(DraftNode {
            kind,
            value_type,
            semantic_version,
            origin,
        });
        Ok(ValueRef::new(id, value_type))
    }
}

pub(crate) fn require_value_type(
    actual: ValueRef,
    expected: ValueType,
    program: &str,
    port: &str,
    span: &SourceSpan,
) -> Result<()> {
    if actual.value_type() == expected {
        return Ok(());
    }
    Err(Diagnostic::new(
        "E_TYPE_MISMATCH",
        format!(
            "program `{program}` port `{port}` expected {expected}, but the bound value is {}",
            actual.value_type()
        ),
        span.clone(),
    ))
}

/// Parse and purely compile a workflow file without reading media assets or
/// invoking external tools.
///
/// # Errors
///
/// Returns a source-located syntax or compilation diagnostic.
pub fn compile_file(path: &Path) -> Result<CompiledWorkflow> {
    let workflow = crate::syntax::parse_file(path)?;
    compile(&workflow)
}

/// Purely compile an already parsed workflow.
///
/// # Errors
///
/// Returns a diagnostic for invalid programs, stack behavior, references,
/// types, cycles, or frame domains.
pub fn compile(workflow: &Workflow) -> Result<CompiledWorkflow> {
    compile_with_registry(workflow, ProgramRegistry::default())
}

pub(crate) fn compile_with_registry(
    workflow: &Workflow,
    registry: ProgramRegistry,
) -> Result<CompiledWorkflow> {
    let video = resolve_video_spec(workflow)?;
    let evaluation = evaluate::evaluate(workflow, &video, registry)?;
    resolve::finalize(workflow, video, evaluation, COMPILED_FORMAT_VERSION)
}

fn resolve_video_spec(workflow: &Workflow) -> Result<VideoSpec> {
    let mut spec = VideoSpec::default();
    if let Some(width) = &workflow.video().width {
        spec.width = width.value;
    }
    if let Some(height) = &workflow.video().height {
        spec.height = height.value;
    }
    if let Some(fps) = &workflow.video().fps {
        spec.fps = crate::model::FrameRate::parse(&fps.value, &fps.span)?;
    }
    Ok(spec)
}
