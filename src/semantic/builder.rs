use std::num::NonZeroU64;
use std::path::PathBuf;

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::external::ExternalInvocation;
use crate::model::{
    AudioSpec, ExactNumber, FrameCount, ImageFit, NativeRange, TimelineRangeExpression, ValueId,
    ValueRef, ValueType, VideoSpec,
};
use crate::source::SourceSpan;

use super::{DraftNode, SemanticNodeKind, SourceOrigin, SymbolId};

pub(crate) struct GraphBuilder<'a> {
    nodes: &'a mut Vec<DraftNode>,
    video: &'a VideoSpec,
    audio: AudioSpec,
    semantic_version: u32,
    origin: SourceOrigin,
}

impl<'a> GraphBuilder<'a> {
    pub(crate) fn for_program(
        nodes: &'a mut Vec<DraftNode>,
        video: &'a VideoSpec,
        audio: AudioSpec,
        semantic_version: u32,
        origin: SourceOrigin,
    ) -> Self {
        Self {
            nodes,
            video,
            audio,
            semantic_version,
            origin,
        }
    }

    #[must_use]
    pub(crate) const fn video_spec(&self) -> &VideoSpec {
        self.video
    }

    #[must_use]
    pub(crate) const fn audio_spec(&self) -> &AudioSpec {
        &self.audio
    }

    pub(crate) fn at_span(&mut self, span: SourceSpan) -> GraphBuilder<'_> {
        GraphBuilder {
            nodes: &mut *self.nodes,
            video: self.video,
            audio: self.audio,
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
        self.push(SemanticNodeKind::ImageVideo { path, frames, fit })
    }

    pub(crate) fn deferred_image_video(
        &mut self,
        path: PathBuf,
        extent: crate::model::TimelineExpression,
        fit: ImageFit,
    ) -> Result<ValueRef> {
        self.push(SemanticNodeKind::DeferredImageVideo { path, extent, fit })
    }

    /// Add a pure semantic video-file source with a deferred frame domain.
    ///
    /// # Errors
    ///
    /// Returns a graph-size diagnostic.
    pub(crate) fn video_source(&mut self, path: PathBuf, fit: ImageFit) -> Result<ValueRef> {
        self.push(SemanticNodeKind::VideoSource { path, fit })
    }

    pub(crate) fn audio_source(&mut self, path: PathBuf) -> Result<ValueRef> {
        self.push(SemanticNodeKind::AudioSource { path })
    }

    pub(crate) fn extract_audio(&mut self, video: ValueRef) -> Result<ValueRef> {
        self.require_type(video, ValueType::Video, "video")?;
        self.push(SemanticNodeKind::ExtractAudio { video })
    }

    pub(crate) fn set_audio(&mut self, audio: ValueRef, video: ValueRef) -> Result<ValueRef> {
        self.require_type(audio, ValueType::Audio, "audio")?;
        self.require_type(video, ValueType::Video, "video")?;
        self.push(SemanticNodeKind::SetAudio { audio, video })
    }

    pub(crate) fn audio_on_black(&mut self, audio: ValueRef) -> Result<ValueRef> {
        self.require_type(audio, ValueType::Audio, "audio")?;
        self.push(SemanticNodeKind::AudioOnBlack { audio })
    }

    pub(crate) fn external_video(&mut self, invocation: ExternalInvocation) -> Result<ValueRef> {
        let preserved = invocation
            .inputs
            .get(&invocation.preserve_input)
            .copied()
            .ok_or_else(|| {
                Diagnostic::builtin(
                    BuiltinDiagnostic::InternalExternalProgram,
                    "external invocation is missing its preserved input",
                    self.origin.span.clone(),
                )
            })?;
        self.require_type(preserved, ValueType::Video, &invocation.preserve_input)?;
        self.push(SemanticNodeKind::ExternalVideo { invocation })
    }

    /// Add a checked semantic slice on the input's native frame or sample grid.
    ///
    /// # Errors
    ///
    /// Returns a type or graph-size diagnostic.
    pub(crate) fn slice(&mut self, input: ValueRef, range: NativeRange) -> Result<ValueRef> {
        self.require_type(input, range.value_type(), "input")?;
        self.push(SemanticNodeKind::Slice { input, range })
    }

    pub(crate) fn deferred_slice(
        &mut self,
        input: ValueRef,
        range: TimelineRangeExpression,
    ) -> Result<ValueRef> {
        self.push(SemanticNodeKind::DeferredSlice { input, range })
    }

    /// Add a checked compact semantic repetition, aliasing a count of one.
    ///
    /// # Errors
    ///
    /// Returns a type or graph-size diagnostic.
    pub(crate) fn repeat(&mut self, input: ValueRef, count: NonZeroU64) -> Result<ValueRef> {
        if count.get() == 1 {
            return Ok(input);
        }
        self.push(SemanticNodeKind::Repeat { input, count })
    }

    /// Add a centered full-clip linear `zoom_in` that preserves the input domain.
    ///
    /// # Errors
    ///
    /// Returns a type or graph-size diagnostic.
    pub(crate) fn zoom_in(&mut self, input: ValueRef, by: ExactNumber) -> Result<ValueRef> {
        self.require_type(input, ValueType::Video, "video")?;
        self.push(SemanticNodeKind::ZoomIn { input, by })
    }

    /// Join two Videos without overlap while fading the start of the latter from white.
    ///
    /// # Errors
    ///
    /// Returns a type or graph-size diagnostic.
    pub(crate) fn flash_cut(
        &mut self,
        before: ValueRef,
        after: ValueRef,
        frames: FrameCount,
    ) -> Result<ValueRef> {
        self.require_type(before, ValueType::Video, "before")?;
        self.require_type(after, ValueType::Video, "after")?;
        self.push(SemanticNodeKind::FlashCut {
            before,
            after,
            frames,
        })
    }

    /// Overlap the end of one Video with the start of another.
    ///
    /// # Errors
    ///
    /// Returns a type or graph-size diagnostic.
    pub(crate) fn crossfade(
        &mut self,
        before: ValueRef,
        after: ValueRef,
        frames: FrameCount,
    ) -> Result<ValueRef> {
        self.require_type(before, ValueType::Video, "before")?;
        self.require_type(after, ValueType::Video, "after")?;
        self.push(SemanticNodeKind::Crossfade {
            before,
            after,
            frames,
        })
    }

    /// Add a checked semantic concatenation, aliasing one input.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic for empty, mistyped, or oversized graphs.
    pub(crate) fn concat(&mut self, inputs: Vec<ValueRef>) -> Result<ValueRef> {
        let Some(first) = inputs.first().copied() else {
            return Err(Diagnostic::builtin(
                BuiltinDiagnostic::EmptyConcat,
                format!(
                    "`{}` requires at least one Video or Audio value",
                    self.origin.construct
                ),
                self.origin.span.clone(),
            ));
        };
        let value_type = first.value_type();
        for input in &inputs {
            if input.value_type() != value_type {
                return Err(Diagnostic::builtin(
                    BuiltinDiagnostic::TypeMismatch,
                    format!(
                        "program `{}` concat inputs must all be {value_type}",
                        self.origin.construct
                    ),
                    self.origin.span.clone(),
                ));
            }
        }
        if inputs.len() == 1 {
            return Ok(first);
        }
        self.push(SemanticNodeKind::Concat { inputs })
    }

    pub(crate) fn reference(
        &mut self,
        symbol: SymbolId,
        value_type: ValueType,
    ) -> Result<ValueRef> {
        self.push(SemanticNodeKind::Reference { symbol, value_type })
    }

    pub(crate) fn replace_range(
        &mut self,
        base: ValueRef,
        range: NativeRange,
        replacement: ValueRef,
    ) -> Result<ValueRef> {
        let value_type = range.value_type();
        self.require_type(base, value_type, "base")?;
        self.require_type(replacement, value_type, "replacement")?;
        self.push(SemanticNodeKind::ReplaceRange {
            base,
            replacement,
            range,
        })
    }

    pub(crate) fn deferred_replace_range(
        &mut self,
        base: ValueRef,
        range: TimelineRangeExpression,
        replacement: ValueRef,
    ) -> Result<ValueRef> {
        if base.value_type() != replacement.value_type() {
            return Err(Diagnostic::builtin(
                BuiltinDiagnostic::TypeMismatch,
                format!(
                    "program `{}` replacement must have the same type as its base timeline",
                    self.origin.construct
                ),
                self.origin.span.clone(),
            ));
        }
        self.push(SemanticNodeKind::DeferredReplaceRange {
            base,
            replacement,
            range,
        })
    }

    fn push(&mut self, kind: SemanticNodeKind) -> Result<ValueRef> {
        let value_type = kind.value_type();
        let id = ValueId::new(u32::try_from(self.nodes.len()).map_err(|_| {
            Diagnostic::builtin(
                BuiltinDiagnostic::GraphTooLarge,
                "semantic graph contains too many values",
                self.origin.span.clone(),
            )
        })?);
        self.nodes.push(DraftNode {
            kind,
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
    Err(Diagnostic::builtin(
        BuiltinDiagnostic::TypeMismatch,
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
        SourceOrigin::new(construct, SourceSpan::new("test.clipasm", line, 1))
    }

    #[test]
    fn builder_propagates_version_and_owner() {
        let video = VideoSpec::default();
        let mut nodes = Vec::new();
        GraphBuilder::for_program(
            &mut nodes,
            &video,
            crate::model::AudioSpec::default(),
            7,
            origin("image", 2),
        )
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
        let mut builder = GraphBuilder::for_program(
            &mut nodes,
            &video,
            crate::model::AudioSpec::default(),
            3,
            origin("during", 4),
        );
        let source = builder
            .image_video("base.png".into(), FrameCount(2), ImageFit::Cover)
            .expect("source");
        {
            let mut selection = builder.at_span(SourceSpan::new("test.clipasm", 9, 3));
            selection
                .slice(
                    source,
                    NativeRange::Frames(crate::model::FrameRange::new(0, 1).expect("range")),
                )
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
        let mut builder = GraphBuilder::for_program(
            &mut nodes,
            &video,
            crate::model::AudioSpec::default(),
            1,
            origin("concat", 1),
        );
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
        let mut builder = GraphBuilder::for_program(
            &mut nodes,
            &video,
            crate::model::AudioSpec::default(),
            2,
            origin("repeat", 1),
        );
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
    fn native_ranges_must_match_their_media_values() {
        let video = VideoSpec::default();
        let mut nodes = Vec::new();
        let mut builder = GraphBuilder::for_program(
            &mut nodes,
            &video,
            crate::model::AudioSpec::default(),
            1,
            origin("trim", 8),
        );
        let picture = builder
            .image_video("picture.png".into(), FrameCount(2), ImageFit::Cover)
            .expect("Video source");
        let sound = builder
            .audio_source("sound.wav".into())
            .expect("Audio source");
        let frames = NativeRange::Frames(crate::model::FrameRange::new(0, 1).expect("frames"));
        let samples = NativeRange::Samples(crate::model::SampleRange::new(0, 1).expect("samples"));

        let video_error = builder
            .slice(picture, samples)
            .expect_err("sample range cannot slice Video");
        assert_eq!(video_error.code, "E_TYPE_MISMATCH");

        let audio_error = builder
            .slice(sound, frames)
            .expect_err("frame range cannot slice Audio");
        assert_eq!(audio_error.code, "E_TYPE_MISMATCH");

        let replacement_error = builder
            .replace_range(picture, frames, sound)
            .expect_err("replacement must match the native range and base");
        assert_eq!(replacement_error.code, "E_TYPE_MISMATCH");
    }

    #[test]
    fn concat_errors_use_the_scoped_owner() {
        let video = VideoSpec::default();
        let mut nodes = Vec::new();
        let mut builder = GraphBuilder::for_program(
            &mut nodes,
            &video,
            crate::model::AudioSpec::default(),
            1,
            origin("join", 6),
        );

        let empty = builder.concat(Vec::new()).expect_err("empty concat");
        assert_eq!(empty.code, "E_EMPTY_CONCAT");
        assert!(empty.message.contains("`join`"));

        let video_value = ValueRef::new(ValueId::new(0), ValueType::Video);
        let audio_value = ValueRef::new(ValueId::new(1), ValueType::Audio);
        let wrong = builder
            .concat(vec![video_value, audio_value])
            .expect_err("mixed types");
        assert_eq!(wrong.code, "E_TYPE_MISMATCH");
        assert!(wrong.message.contains("program `join`"));
        assert_eq!(wrong.span.line, 6);
    }
}
