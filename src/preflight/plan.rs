use std::collections::BTreeMap;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};

use crate::diagnostic::Result;
use crate::model::{
    AudioDomain, AudioSpec, ExactNumber, FrameCount, FrameRange, ImageFit, NodeId, ValueType,
    VideoDomain, VideoSpec,
};
use crate::semantic::SourceOrigin;
use crate::source::SourceFile;

use super::policy::RenderPolicy;
use super::tools::{self, ExternalToolIdentity, ToolIdentity};

#[derive(Clone, Debug)]
#[non_exhaustive]
/// One external-program parameter after preflight resolution.
pub enum PreparedExternalParameterValue {
    /// Signed integer parameter.
    Integer(i64),
    /// One declared keyword value.
    Keyword(String),
    /// Verified file parameter and its content hash.
    File(PreparedAsset),
}

#[derive(Clone, Debug)]
#[non_exhaustive]
/// One external-process argument after preflight resolution.
pub enum PreparedExternalArgument {
    /// Literal argument text.
    Text(String),
    /// Verified file argument and its content hash.
    File(PreparedAsset),
}

#[derive(Clone, Debug)]
/// An exact, media-verified plan consumed by [`crate::render::render`].
///
/// Every Video node has an exact [`VideoDomain`], every Audio node has an
/// exact [`AudioDomain`], every source asset has a recorded content hash, and
/// tool/media policy has been incorporated into the private execution namespace.
pub struct PreparedPlan {
    format_version: u32,
    engine_version: String,
    semantic_hash: String,
    render_policy: RenderPolicy,
    video: VideoSpec,
    audio: AudioSpec,
    nodes: Vec<PreparedNode>,
    result: NodeId,
    named_values: BTreeMap<String, NodeId>,
    output: PathBuf,
    manifest: PathBuf,
    ffmpeg: ToolIdentity,
    ffprobe: ToolIdentity,
    execution_namespace: String,
    entrypoint_source: SourceFile,
}

impl PreparedPlan {
    #[expect(
        clippy::too_many_arguments,
        reason = "the sole plan constructor makes every identity-bearing and publication field explicit without a duplicate builder"
    )]
    pub(super) fn new(
        format_version: u32,
        engine_version: String,
        semantic_hash: String,
        render_policy: RenderPolicy,
        video: VideoSpec,
        audio: AudioSpec,
        nodes: Vec<PreparedNode>,
        result: NodeId,
        named_values: BTreeMap<String, NodeId>,
        output: PathBuf,
        manifest: PathBuf,
        ffmpeg: ToolIdentity,
        ffprobe: ToolIdentity,
        execution_namespace: String,
        entrypoint_source: SourceFile,
    ) -> Self {
        Self {
            format_version,
            engine_version,
            semantic_hash,
            render_policy,
            video,
            audio,
            nodes,
            result,
            named_values,
            output,
            manifest,
            ffmpeg,
            ffprobe,
            execution_namespace,
            entrypoint_source,
        }
    }

    pub(crate) const fn format_version(&self) -> u32 {
        self.format_version
    }

    pub(crate) fn engine_version(&self) -> &str {
        &self.engine_version
    }

    /// Serialize the explicit prepared-plan inspection document.
    ///
    /// This local inspection format includes resolved paths, tool identities,
    /// renderer primitives, and cache metadata. It is distinct from the
    /// path-free render manifest.
    ///
    /// # Errors
    ///
    /// Returns `E_PREPARED_JSON` when a field cannot be represented as JSON.
    pub fn prepared_json(&self) -> Result<String> {
        crate::format::prepared_json::prepared_plan(self)
    }

    #[must_use]
    /// Return the identity of prepared semantics and resolved source content.
    ///
    /// Renderer/tool compatibility is tracked separately and does not alter
    /// this hash.
    pub fn semantic_hash(&self) -> &str {
        &self.semantic_hash
    }

    #[must_use]
    /// Return the common video properties of the prepared plan.
    pub fn video(&self) -> &VideoSpec {
        &self.video
    }

    #[must_use]
    /// Return the canonical project audio properties.
    pub fn audio(&self) -> &AudioSpec {
        &self.audio
    }

    #[must_use]
    /// Return renderer nodes in stable topological order.
    pub fn nodes(&self) -> &[PreparedNode] {
        &self.nodes
    }

    #[must_use]
    /// Return the node that produces the exported video.
    pub const fn result(&self) -> NodeId {
        self.result
    }

    #[must_use]
    /// Return result-reachable user names and their prepared node IDs.
    pub fn named_values(&self) -> &BTreeMap<String, NodeId> {
        &self.named_values
    }

    #[must_use]
    /// Return the absolute or source-program-relative MP4 destination.
    pub fn output(&self) -> &Path {
        &self.output
    }

    #[must_use]
    /// Return the manifest path published beside the output.
    pub fn manifest(&self) -> &Path {
        &self.manifest
    }

    pub(crate) fn ffmpeg(&self) -> &ToolIdentity {
        &self.ffmpeg
    }

    pub(crate) const fn render_policy(&self) -> RenderPolicy {
        self.render_policy
    }

    pub(crate) fn ffprobe(&self) -> &ToolIdentity {
        &self.ffprobe
    }

    pub(crate) fn execution_namespace(&self) -> &str {
        &self.execution_namespace
    }

    pub(crate) fn verify_tool_identities(&self) -> Result<()> {
        tools::verify_tool_identity(&self.ffmpeg, "FFmpeg")?;
        tools::verify_tool_identity(&self.ffprobe, "FFprobe")
    }

    pub(crate) const fn entrypoint_source(&self) -> &SourceFile {
        &self.entrypoint_source
    }
}

#[derive(Clone, Debug)]
/// One exact renderer primitive in a [`PreparedPlan`].
pub struct PreparedNode {
    id: NodeId,
    media: PreparedMedia,
    origin: SourceOrigin,
    fingerprint: String,
}

#[derive(Clone, Debug)]
pub(super) enum PreparedMedia {
    Video {
        kind: PreparedVideoKind,
        domain: VideoDomain,
        has_audio: bool,
    },
    Audio {
        kind: PreparedAudioKind,
        domain: AudioDomain,
    },
}

#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
/// A borrowed, structurally typed view of one prepared node.
pub enum PreparedNodeMedia<'a> {
    /// A Video renderer primitive with its exact video domain.
    Video {
        /// The renderer operation.
        kind: &'a PreparedVideoKind,
        /// Exact duration, dimensions, and frame rate.
        domain: &'a VideoDomain,
        /// Whether the Video carries meaningful attached audio.
        has_audio: bool,
    },
    /// An Audio renderer primitive with its exact audio domain.
    Audio {
        /// The renderer operation.
        kind: &'a PreparedAudioKind,
        /// Exact duration and normalized audio format.
        domain: &'a AudioDomain,
    },
}

impl PreparedNode {
    pub(super) fn new(
        id: NodeId,
        media: PreparedMedia,
        origin: SourceOrigin,
        fingerprint: String,
    ) -> Self {
        Self {
            id,
            media,
            origin,
            fingerprint,
        }
    }

    pub(super) const fn prepared_media(&self) -> &PreparedMedia {
        &self.media
    }

    #[must_use]
    /// Return this node's engine-assigned prepared-plan identifier.
    pub const fn id(&self) -> NodeId {
        self.id
    }

    #[must_use]
    /// Return a structurally typed view of this prepared node.
    pub const fn media(&self) -> PreparedNodeMedia<'_> {
        match &self.media {
            PreparedMedia::Video {
                kind,
                domain,
                has_audio,
            } => PreparedNodeMedia::Video {
                kind,
                domain,
                has_audio: *has_audio,
            },
            PreparedMedia::Audio { kind, domain } => PreparedNodeMedia::Audio { kind, domain },
        }
    }

    #[must_use]
    /// Return the Video operation, or `None` for Audio.
    pub const fn video_kind(&self) -> Option<&PreparedVideoKind> {
        match &self.media {
            PreparedMedia::Video { kind, .. } => Some(kind),
            PreparedMedia::Audio { .. } => None,
        }
    }

    #[must_use]
    /// Return the Audio operation, or `None` for Video.
    pub const fn audio_kind(&self) -> Option<&PreparedAudioKind> {
        match &self.media {
            PreparedMedia::Video { .. } => None,
            PreparedMedia::Audio { kind, .. } => Some(kind),
        }
    }

    #[must_use]
    /// Return whether this node produces Video or Audio.
    pub const fn value_type(&self) -> ValueType {
        match self.media {
            PreparedMedia::Video { .. } => ValueType::Video,
            PreparedMedia::Audio { .. } => ValueType::Audio,
        }
    }

    #[must_use]
    /// Return this node's exact Video domain, or `None` for Audio.
    pub const fn video_domain(&self) -> Option<&VideoDomain> {
        match &self.media {
            PreparedMedia::Video { domain, .. } => Some(domain),
            PreparedMedia::Audio { .. } => None,
        }
    }

    #[must_use]
    /// Return this node's exact Audio domain, or `None` for Video.
    pub const fn audio_domain(&self) -> Option<&AudioDomain> {
        match &self.media {
            PreparedMedia::Video { .. } => None,
            PreparedMedia::Audio { domain, .. } => Some(domain),
        }
    }

    #[must_use]
    /// Return whether a Video node contains meaningful attached audio.
    ///
    /// Audio nodes return `false` because their audio is the value itself, not
    /// an attachment to a picture timeline.
    pub const fn has_audio(&self) -> bool {
        match self.media {
            PreparedMedia::Video { has_audio, .. } => has_audio,
            PreparedMedia::Audio { .. } => false,
        }
    }

    #[must_use]
    /// Return the authored construct and source location responsible for the node.
    pub const fn origin(&self) -> &SourceOrigin {
        &self.origin
    }

    #[must_use]
    /// Return the content fingerprint used to address this node's artifact.
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    pub(crate) fn visit_inputs(&self, visitor: impl FnMut(NodeId)) {
        match &self.media {
            PreparedMedia::Video { kind, .. } => kind.visit_inputs(visitor),
            PreparedMedia::Audio { kind, .. } => kind.visit_inputs(visitor),
        }
    }
}

#[derive(Clone, Debug)]
#[non_exhaustive]
/// The closed set of exact Video primitives understood by the renderer.
pub enum PreparedVideoKind {
    /// A still image expanded to an exact frame count.
    ImageVideo {
        /// Verified source image and its preflight content hash.
        asset: PreparedAsset,
        /// Number of output frames.
        frames: FrameCount,
        /// Source-to-project fitting policy.
        fit: ImageFit,
    },
    /// A video-file source normalized to the project video domain.
    VideoSource {
        /// Verified source video and its preflight content hash.
        asset: PreparedAsset,
        /// Exact source duration converted to project frames.
        frames: FrameCount,
        /// Source-to-project fitting policy.
        fit: ImageFit,
    },
    /// A closed-open frame range selected from one upstream Video node.
    Slice {
        /// Prepared Video node being sliced.
        input: NodeId,
        /// Exact selected frame range.
        range: FrameRange,
    },
    /// Compact repetition of one upstream Video artifact.
    Repeat {
        /// Prepared Video node repeated in sequence.
        input: NodeId,
        /// Total number of copies.
        count: NonZeroU64,
        /// Exact total output frames.
        frames: FrameCount,
    },
    /// A centered linear `zoom_in` over the complete upstream clip.
    ZoomIn {
        /// Prepared Video node being zoomed.
        input: NodeId,
        /// Exact final fractional increase over the source size.
        by: ExactNumber,
    },
    /// Ordered cut whose latter Video fades from white at its start.
    FlashCut {
        /// Video rendered unchanged before the cut.
        before: NodeId,
        /// Video rendered after the cut with the flash fade.
        after: NodeId,
        /// Positive number of frames over which the flash clears.
        frames: FrameCount,
    },
    /// Two Videos overlapped with a linear picture and Audio fade.
    Crossfade {
        /// Video supplying the prefix and first overlap side.
        before: NodeId,
        /// Video supplying the second overlap side and suffix.
        after: NodeId,
        /// Positive exact number of overlapping project frames.
        frames: FrameCount,
    },
    /// Ordered concatenation of prepared Video nodes.
    Concat {
        /// Upstream Video nodes in output order.
        inputs: Vec<NodeId>,
    },
    /// A Video whose attached audio is replaced from time zero.
    SetAudio {
        /// Replacement Audio node.
        audio: NodeId,
        /// Video supplying the picture timeline and output duration.
        video: NodeId,
    },
    /// A project-sized black Video carrying one Audio timeline.
    AudioOnBlack {
        /// Audio used as the output timeline.
        audio: NodeId,
    },
    /// A Video produced by one registered external executable.
    ExternalVideo {
        /// Prepared external executable identity.
        executable: ExternalToolIdentity,
        /// Ordered process arguments.
        arguments: Vec<PreparedExternalArgument>,
        /// Named prepared inputs.
        inputs: BTreeMap<String, NodeId>,
        /// Bound scalar parameters.
        parameters: BTreeMap<String, PreparedExternalParameterValue>,
        /// Input whose exact domain and audio presence are preserved.
        preserve_input: String,
    },
}

impl PreparedVideoKind {
    pub(crate) fn visit_inputs(&self, mut visitor: impl FnMut(NodeId)) {
        match self {
            Self::ImageVideo { .. } | Self::VideoSource { .. } => {}
            Self::Slice { input, .. }
            | Self::Repeat { input, .. }
            | Self::ZoomIn { input, .. }
            | Self::AudioOnBlack { audio: input } => visitor(*input),
            Self::FlashCut { before, after, .. } | Self::Crossfade { before, after, .. } => {
                visitor(*before);
                visitor(*after);
            }
            Self::Concat { inputs } => inputs.iter().copied().for_each(&mut visitor),
            Self::SetAudio { audio, video } => {
                visitor(*audio);
                visitor(*video);
            }
            Self::ExternalVideo { inputs, .. } => {
                inputs.values().copied().for_each(visitor);
            }
        }
    }
}

#[derive(Clone, Debug)]
#[non_exhaustive]
/// The closed set of exact Audio primitives understood by the renderer.
pub enum PreparedAudioKind {
    /// A normalized audio-file source.
    AudioSource {
        /// Verified source media and content hash.
        asset: PreparedAsset,
    },
    /// A closed-open sample range selected from one Audio node.
    AudioSlice {
        /// Prepared Audio node being sliced.
        input: NodeId,
        /// Exact selected sample range.
        range: crate::model::SampleRange,
    },
    /// Compact repetition of one Audio node.
    AudioRepeat {
        /// Prepared Audio node repeated in sequence.
        input: NodeId,
        /// Total number of copies.
        count: NonZeroU64,
    },
    /// Ordered concatenation of prepared Audio nodes.
    AudioConcat {
        /// Upstream Audio nodes in output order.
        inputs: Vec<NodeId>,
    },
    /// The synchronized audio timeline extracted from one Video.
    ExtractAudio {
        /// Upstream audiovisual Video node.
        video: NodeId,
    },
}

impl PreparedAudioKind {
    pub(crate) fn visit_inputs(&self, mut visitor: impl FnMut(NodeId)) {
        match self {
            Self::AudioSource { .. } => {}
            Self::AudioSlice { input, .. } | Self::AudioRepeat { input, .. } => visitor(*input),
            Self::AudioConcat { inputs } => inputs.iter().copied().for_each(visitor),
            Self::ExtractAudio { video } => visitor(*video),
        }
    }
}

#[derive(Clone, Debug)]
/// A source file verified during preflight and bound to its content hash.
pub struct PreparedAsset {
    source_path: PathBuf,
    content_hash: String,
}

impl PreparedAsset {
    pub(super) fn new(source_path: PathBuf, content_hash: String) -> Self {
        Self {
            source_path,
            content_hash,
        }
    }

    #[must_use]
    /// Return the resolved path that was inspected during preflight.
    pub fn source_path(&self) -> &Path {
        &self.source_path
    }

    #[must_use]
    /// Return the SHA-256 hash recorded for later change detection.
    pub fn content_hash(&self) -> &str {
        &self.content_hash
    }
}
