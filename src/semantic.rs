use std::num::NonZeroU64;
use std::path::PathBuf;

use serde::Serialize;

use crate::diagnostic::{Diagnostic, Result, SourceSpan};
use crate::model::{
    FrameCount, FrameRange, ImageFit, ValueId, ValueRef, ValueType, VideoDomain, VideoSpec,
};

#[derive(Clone, Debug)]
pub(crate) struct CompiledNode {
    id: ValueId,
    kind: SemanticNodeKind,
    value_type: ValueType,
    domain: Option<VideoDomain>,
    semantic_version: u32,
    origin: SourceOrigin,
}

impl CompiledNode {
    pub(crate) fn from_draft(id: ValueId, draft: &DraftNode, domain: Option<VideoDomain>) -> Self {
        Self {
            id,
            kind: draft.kind.clone(),
            value_type: draft.value_type,
            domain,
            semantic_version: draft.semantic_version,
            origin: draft.origin.clone(),
        }
    }

    pub(crate) const fn kind(&self) -> &SemanticNodeKind {
        &self.kind
    }

    pub(crate) const fn id(&self) -> ValueId {
        self.id
    }

    pub(crate) const fn value_type(&self) -> ValueType {
        self.value_type
    }

    pub(crate) const fn domain(&self) -> Option<&VideoDomain> {
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
    Repeat {
        input: ValueRef,
        count: NonZeroU64,
    },
    Zoom {
        input: ValueRef,
        percent: u32,
    },
    Wobble {
        input: ValueRef,
        pixels: u32,
    },
    FlashJoin {
        before: ValueRef,
        after: ValueRef,
        frames: FrameCount,
    },
    Concat {
        inputs: Vec<ValueRef>,
    },
    Slice {
        input: ValueRef,
        range: FrameRange,
    },
    ReplaceRange {
        base: ValueRef,
        replacement: ValueRef,
        range: FrameRange,
    },
}

#[derive(Clone, Debug, Serialize)]
/// Authored construct and source location responsible for a semantic value.
///
/// Program constructs are static registry names; compiler-generated labels
/// such as `reference` are also stable identifiers.
pub struct SourceOrigin {
    /// Registered program name or stable compiler-generated construct label.
    pub construct: String,
    /// Most relevant authored source location.
    pub span: SourceSpan,
}

impl SourceOrigin {
    #[must_use]
    pub(crate) fn new(construct: impl Into<String>, span: SourceSpan) -> Self {
        Self {
            construct: construct.into(),
            span,
        }
    }

    #[must_use]
    pub(crate) fn clone_with_construct(&self, construct: impl Into<String>) -> Self {
        Self::new(construct, self.span.clone())
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DraftNode {
    kind: SemanticNodeKind,
    value_type: ValueType,
    semantic_version: u32,
    origin: SourceOrigin,
}

impl DraftNode {
    pub(crate) const fn kind(&self) -> &SemanticNodeKind {
        &self.kind
    }

    pub(crate) const fn value_type(&self) -> ValueType {
        self.value_type
    }

    pub(crate) const fn semantic_version(&self) -> u32 {
        self.semantic_version
    }

    pub(crate) const fn origin(&self) -> &SourceOrigin {
        &self.origin
    }
}

pub(crate) struct GraphBuilder<'a> {
    nodes: &'a mut Vec<DraftNode>,
    video: &'a VideoSpec,
    semantic_version: u32,
    origin: SourceOrigin,
}

impl<'a> GraphBuilder<'a> {
    pub(crate) fn for_program(
        nodes: &'a mut Vec<DraftNode>,
        video: &'a VideoSpec,
        semantic_version: u32,
        origin: SourceOrigin,
    ) -> Self {
        Self {
            nodes,
            video,
            semantic_version,
            origin,
        }
    }

    #[must_use]
    pub(crate) const fn video_spec(&self) -> &VideoSpec {
        self.video
    }

    pub(crate) fn at_span(&mut self, span: SourceSpan) -> GraphBuilder<'_> {
        GraphBuilder {
            nodes: &mut *self.nodes,
            video: self.video,
            semantic_version: self.semantic_version,
            origin: SourceOrigin::new(self.origin.construct.clone(), span),
        }
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
    ) -> Result<ValueRef> {
        self.push(
            SemanticNodeKind::ImageVideo { path, frames, fit },
            ValueType::Video,
        )
    }

    /// Add a pure semantic video-file source with a deferred frame domain.
    ///
    /// # Errors
    ///
    /// Returns a graph-size diagnostic.
    pub(crate) fn video_source(&mut self, path: PathBuf, fit: ImageFit) -> Result<ValueRef> {
        self.push(
            SemanticNodeKind::VideoSource { path, fit },
            ValueType::Video,
        )
    }

    /// Add a checked semantic Video slice.
    ///
    /// # Errors
    ///
    /// Returns a type or graph-size diagnostic.
    pub(crate) fn slice(&mut self, input: ValueRef, range: FrameRange) -> Result<ValueRef> {
        self.require_type(input, ValueType::Video, "input")?;
        self.push(SemanticNodeKind::Slice { input, range }, ValueType::Video)
    }

    /// Add a checked compact semantic repetition, aliasing a count of one.
    ///
    /// # Errors
    ///
    /// Returns a type or graph-size diagnostic.
    pub(crate) fn repeat(&mut self, input: ValueRef, count: NonZeroU64) -> Result<ValueRef> {
        self.require_type(input, ValueType::Video, "video")?;
        if count.get() == 1 {
            return Ok(input);
        }
        self.push(SemanticNodeKind::Repeat { input, count }, ValueType::Video)
    }

    /// Add a centered full-clip linear zoom that preserves the input domain.
    ///
    /// # Errors
    ///
    /// Returns a type or graph-size diagnostic.
    pub(crate) fn zoom(&mut self, input: ValueRef, percent: u32) -> Result<ValueRef> {
        self.require_type(input, ValueType::Video, "video")?;
        self.push(SemanticNodeKind::Zoom { input, percent }, ValueType::Video)
    }

    /// Add deterministic full-clip two-axis motion that preserves the input domain.
    ///
    /// # Errors
    ///
    /// Returns a type or graph-size diagnostic.
    pub(crate) fn wobble(&mut self, input: ValueRef, pixels: u32) -> Result<ValueRef> {
        self.require_type(input, ValueType::Video, "video")?;
        self.push(SemanticNodeKind::Wobble { input, pixels }, ValueType::Video)
    }

    /// Join two Videos without overlap while fading the start of the latter from white.
    ///
    /// # Errors
    ///
    /// Returns a type or graph-size diagnostic.
    pub(crate) fn flash_join(
        &mut self,
        before: ValueRef,
        after: ValueRef,
        frames: FrameCount,
    ) -> Result<ValueRef> {
        self.require_type(before, ValueType::Video, "before")?;
        self.require_type(after, ValueType::Video, "after")?;
        self.push(
            SemanticNodeKind::FlashJoin {
                before,
                after,
                frames,
            },
            ValueType::Video,
        )
    }

    /// Add a checked semantic concatenation, aliasing one input.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic for empty, mistyped, or oversized graphs.
    pub(crate) fn concat(&mut self, inputs: Vec<ValueRef>) -> Result<ValueRef> {
        if inputs.is_empty() {
            return Err(Diagnostic::new(
                "E_EMPTY_CONCAT",
                format!("`{}` requires at least one Video", self.origin.construct),
                self.origin.span.clone(),
            ));
        }
        for input in &inputs {
            self.require_type(*input, ValueType::Video, "videos")?;
        }
        if inputs.len() == 1 {
            return Ok(inputs[0]);
        }
        self.push(SemanticNodeKind::Concat { inputs }, ValueType::Video)
    }

    pub(crate) fn reference(&mut self, name: String, value_type: ValueType) -> Result<ValueRef> {
        self.push(SemanticNodeKind::Reference { name }, value_type)
    }

    pub(crate) fn replace_range(
        &mut self,
        base: ValueRef,
        range: FrameRange,
        replacement: ValueRef,
    ) -> Result<ValueRef> {
        self.require_type(base, ValueType::Video, "base")?;
        self.require_type(replacement, ValueType::Video, "replacement")?;
        self.push(
            SemanticNodeKind::ReplaceRange {
                base,
                replacement,
                range,
            },
            ValueType::Video,
        )
    }

    fn push(&mut self, kind: SemanticNodeKind, value_type: ValueType) -> Result<ValueRef> {
        let id = ValueId::new(u32::try_from(self.nodes.len()).map_err(|_| {
            Diagnostic::new(
                "E_GRAPH_TOO_LARGE",
                "semantic graph contains too many values",
                self.origin.span.clone(),
            )
        })?);
        self.nodes.push(DraftNode {
            kind,
            value_type,
            semantic_version: self.semantic_version,
            origin: self.origin.clone(),
        });
        Ok(ValueRef::new(id, value_type))
    }

    fn require_type(&self, value: ValueRef, expected: ValueType, port: &str) -> Result<()> {
        require_value_type(
            value,
            expected,
            &self.origin.construct,
            port,
            &self.origin.span,
        )
    }

    #[cfg(test)]
    pub(crate) fn test_value(&mut self) -> Result<ValueRef> {
        self.push(
            SemanticNodeKind::ImageVideo {
                path: PathBuf::from("test.value"),
                frames: FrameCount(1),
                fit: ImageFit::Cover,
            },
            ValueType::Test,
        )
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

#[cfg(test)]
mod tests {
    use super::*;

    fn origin(construct: &'static str, line: usize) -> SourceOrigin {
        SourceOrigin::new(construct, SourceSpan::new("test.yaml", line, 1))
    }

    #[test]
    fn builder_propagates_version_and_owner() {
        let video = VideoSpec::default();
        let mut nodes = Vec::new();
        GraphBuilder::for_program(&mut nodes, &video, 7, origin("image", 2))
            .image_video("card.png".into(), FrameCount(1), ImageFit::Cover)
            .expect("image");

        assert_eq!(nodes[0].semantic_version(), 7);
        assert_eq!(nodes[0].origin().construct, "image");
        assert_eq!(nodes[0].origin().span.line, 2);
    }

    #[test]
    fn derived_span_does_not_mutate_parent_builder() {
        let video = VideoSpec::default();
        let mut nodes = Vec::new();
        let mut builder = GraphBuilder::for_program(&mut nodes, &video, 3, origin("during", 4));
        let source = builder
            .image_video("base.png".into(), FrameCount(2), ImageFit::Cover)
            .expect("source");
        {
            let mut selection = builder.at_span(SourceSpan::new("test.yaml", 9, 3));
            selection
                .slice(source, FrameRange::new(0, 1).expect("range"))
                .expect("slice");
        }
        builder
            .image_video("replacement.png".into(), FrameCount(1), ImageFit::Cover)
            .expect("replacement");

        assert_eq!(nodes[0].origin().span.line, 4);
        assert_eq!(nodes[1].origin().span.line, 9);
        assert_eq!(nodes[2].origin().span.line, 4);
        assert!(nodes.iter().all(|node| node.origin().construct == "during"));
    }

    #[test]
    fn single_concat_aliases_without_adding_a_node() {
        let video = VideoSpec::default();
        let mut nodes = Vec::new();
        let mut builder = GraphBuilder::for_program(&mut nodes, &video, 1, origin("concat", 1));
        let source = builder
            .image_video("source.png".into(), FrameCount(1), ImageFit::Cover)
            .expect("source");
        let alias = builder.concat(vec![source]).expect("concat alias");

        assert_eq!(alias, source);
        assert_eq!(nodes.len(), 1);
    }

    #[test]
    fn repeat_one_aliases_and_larger_counts_stay_compact() {
        let video = VideoSpec::default();
        let mut nodes = Vec::new();
        let mut builder = GraphBuilder::for_program(&mut nodes, &video, 2, origin("repeat", 1));
        let source = builder
            .image_video("source.png".into(), FrameCount(1), ImageFit::Cover)
            .expect("source");
        let alias = builder
            .repeat(source, NonZeroU64::new(1).expect("nonzero"))
            .expect("repeat one");
        let repeated = builder
            .repeat(source, NonZeroU64::new(1_000_000).expect("nonzero"))
            .expect("compact repeat");

        assert_eq!(alias, source);
        assert_eq!(nodes.len(), 2);
        assert!(matches!(
            nodes[repeated.id().get() as usize].kind(),
            SemanticNodeKind::Repeat { input, count }
                if *input == source && count.get() == 1_000_000
        ));
    }

    #[test]
    fn concat_errors_use_the_scoped_owner() {
        let video = VideoSpec::default();
        let mut nodes = Vec::new();
        let mut builder = GraphBuilder::for_program(&mut nodes, &video, 1, origin("join", 6));

        let empty = builder.concat(Vec::new()).expect_err("empty concat");
        assert_eq!(empty.code, "E_EMPTY_CONCAT");
        assert!(empty.message.contains("`join`"));

        let wrong = builder
            .concat(vec![ValueRef::new(ValueId::new(0), ValueType::Test)])
            .expect_err("wrong type");
        assert_eq!(wrong.code, "E_TYPE_MISMATCH");
        assert!(wrong.message.contains("program `join`"));
        assert_eq!(wrong.span.line, 6);
    }
}
