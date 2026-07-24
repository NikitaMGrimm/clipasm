//! Media-aware preparation of compiled programs for rendering.
//!
//! Preflight is the first pipeline phase that performs I/O. It resolves
//! result-reachable assets, verifies source contracts and tool capabilities,
//! derives exact media domains, and lowers semantic operations into a
//! [`PreparedPlan`] containing renderer primitives.
//!
//! ```no_run
//! use std::path::Path;
//!
//! let source = clipasm::frontend::yaml::parse_file(Path::new("program.yaml"))?;
//! let compiled = clipasm::compiler::compile(&source)?;
//! let plan = clipasm::preflight::preflight(&compiled)?;
//! let result = &plan.nodes()[plan.result().get() as usize];
//! println!("prepared {} frames", result.domain().frames.0);
//! # Ok::<(), clipasm::diagnostic::Diagnostic>(())
//! ```

use std::collections::{BTreeMap, HashMap};
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::compiler::CompiledProgram;
use crate::diagnostic::{Diagnostic, Result};
use crate::model::{
    AudioDomain, AudioSpec, FrameCount, FrameRange, ImageFit, NodeId, ValueType, VideoDomain,
    VideoSpec,
};
use crate::semantic::SourceOrigin;
use crate::source::{SourceFile, SourceSpan};

mod assets;
mod identity;
mod lower;
mod tools;

pub(crate) use assets::verify_prepared_asset;
use assets::{
    entrypoint_directory, manifest_path, prepare_output_path, reject_asset_collisions,
    reject_path_collision, validate_destination,
};
use identity::{cache_execution_namespace, prepared_semantic_hash};
use lower::PreflightLowerer;
use tools::{ToolIdentity, inspect_ffmpeg, inspect_ffprobe};

const PREPARED_FORMAT_VERSION: u32 = 5;
const CACHE_FORMAT_VERSION: u32 = 3;
const REQUIRED_FFMPEG_FILTERS: &[&str] = &[
    "scale",
    "crop",
    "pad",
    "fps",
    "setsar",
    "format",
    "trim",
    "setpts",
    "tpad",
    "concat",
    "fade",
    "perspective",
    "aresample",
    "aformat",
    "atrim",
    "apad",
    "anullsrc",
    "color",
];

#[derive(Clone, Debug, Serialize)]
/// An exact, media-verified plan consumed by [`crate::render::render`].
///
/// Every node has an exact [`VideoDomain`], every source asset has a recorded
/// content hash, and tool/media policy has been incorporated into the private
/// execution namespace.
pub struct PreparedPlan {
    format_version: u32,
    engine_version: String,
    semantic_hash: String,
    video: VideoSpec,
    audio: AudioSpec,
    media_policy: RenderMediaPolicy,
    nodes: Vec<PreparedNode>,
    result: NodeId,
    named_values: BTreeMap<String, NodeId>,
    output: PathBuf,
    manifest: PathBuf,
    ffmpeg: ToolIdentity,
    ffprobe: ToolIdentity,
    execution_namespace: String,
    #[serde(skip)]
    entrypoint_source: SourceFile,
}

impl PreparedPlan {
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

    pub(crate) const fn media_policy(&self) -> RenderMediaPolicy {
        self.media_policy
    }

    pub(crate) fn ffmpeg(&self) -> &ToolIdentity {
        &self.ffmpeg
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

#[derive(Clone, Debug, Serialize)]
/// One exact renderer primitive in a [`PreparedPlan`].
pub struct PreparedNode {
    id: NodeId,
    kind: PreparedNodeKind,
    value_type: ValueType,
    domain: Option<VideoDomain>,
    audio_domain: Option<AudioDomain>,
    has_audio: bool,
    origin: SourceOrigin,
    fingerprint: String,
}

impl PreparedNode {
    #[must_use]
    /// Return this node's engine-assigned prepared-plan identifier.
    pub const fn id(&self) -> NodeId {
        self.id
    }

    #[must_use]
    /// Return the renderer primitive and its upstream references.
    pub const fn kind(&self) -> &PreparedNodeKind {
        &self.kind
    }

    #[must_use]
    /// Return exact duration, dimensions, and frame rate.
    pub const fn domain(&self) -> &VideoDomain {
        self.domain.as_ref().expect("Video prepared node domain")
    }

    #[must_use]
    /// Return the exact domain of an Audio prepared node.
    pub const fn audio_domain(&self) -> &AudioDomain {
        self.audio_domain
            .as_ref()
            .expect("Audio prepared node domain")
    }

    #[must_use]
    /// Return whether this node produces Video or Audio.
    pub const fn value_type(&self) -> ValueType {
        self.value_type
    }

    #[must_use]
    /// Return whether a Video node contains meaningful attached audio.
    pub const fn has_audio(&self) -> bool {
        self.has_audio
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
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
/// The closed set of exact primitives understood by the renderer.
pub enum PreparedNodeKind {
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
    /// A normalized audio-file source.
    AudioSource {
        /// Verified source media and content hash.
        asset: PreparedAsset,
    },
    /// A closed-open frame range selected from one upstream node.
    Slice {
        /// Prepared node being sliced.
        input: NodeId,
        /// Exact selected frame range.
        range: FrameRange,
    },
    /// Compact repetition of one upstream artifact.
    Repeat {
        /// Prepared node repeated in sequence.
        input: NodeId,
        /// Total number of copies.
        count: NonZeroU64,
        /// Exact total output frames.
        frames: FrameCount,
    },
    /// A centered linear zoom over the complete upstream clip.
    Zoom {
        /// Prepared node being zoomed.
        input: NodeId,
        /// Final percentage increase over the source size.
        percent: u32,
    },
    /// Deterministic two-axis full-clip motion.
    Wobble {
        /// Prepared node being moved.
        input: NodeId,
        /// Maximum movement from center in pixels.
        pixels: u32,
    },
    /// Ordered join whose latter Video fades from white at its start.
    FlashJoin {
        /// Video rendered unchanged before the cut.
        before: NodeId,
        /// Video rendered after the cut with the flash fade.
        after: NodeId,
        /// Positive number of frames over which the flash clears.
        frames: FrameCount,
    },
    /// Ordered concatenation of prepared video nodes.
    Concat {
        /// Upstream nodes in output order.
        inputs: Vec<NodeId>,
    },
    /// The synchronized audio timeline extracted from one Video.
    ExtractAudio {
        /// Upstream audiovisual Video node.
        video: NodeId,
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
}

#[derive(Clone, Debug, Serialize)]
/// A source file verified during preflight and bound to its content hash.
pub struct PreparedAsset {
    source_path: PathBuf,
    content_hash: String,
}

impl PreparedAsset {
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

#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) struct RenderMediaPolicy {
    working_pixel_format: WorkingPixelFormat,
    export_pixel_format: ExportPixelFormat,
}

impl Default for RenderMediaPolicy {
    fn default() -> Self {
        Self {
            working_pixel_format: WorkingPixelFormat::Yuv444p,
            export_pixel_format: ExportPixelFormat::Yuv420p,
        }
    }
}

impl RenderMediaPolicy {
    pub(crate) const fn working_pixel_format(self) -> &'static str {
        match self.working_pixel_format {
            WorkingPixelFormat::Yuv444p => "yuv444p",
            #[cfg(test)]
            WorkingPixelFormat::Test => "test",
        }
    }

    pub(crate) const fn export_pixel_format(self) -> &'static str {
        match self.export_pixel_format {
            ExportPixelFormat::Yuv420p => "yuv420p",
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
enum WorkingPixelFormat {
    Yuv444p,
    #[cfg(test)]
    Test,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
enum ExportPixelFormat {
    Yuv420p,
}

/// Resolve and verify assets/tools, lower result-reachable primitives, and build
/// an invariant-protected renderer plan.
///
/// # Errors
///
/// Returns a diagnostic for invalid output configuration, unavailable tool
/// capabilities, inaccessible/undecodable assets, or preparation failures.
pub fn preflight(compiled: &CompiledProgram) -> Result<PreparedPlan> {
    let render_output = compiled.render_output()?;
    entrypoint_directory(compiled.entrypoint_source())?;
    let output = prepare_output_path(compiled)?;
    let manifest = manifest_path(&output);
    validate_destination(&output, "output", "E_INVALID_OUTPUT_DESTINATION")?;
    validate_destination(&manifest, "manifest", "E_INVALID_MANIFEST_DESTINATION")?;
    if let Some(source_path) = compiled.entrypoint_source().filesystem_path() {
        reject_path_collision(
            &output,
            "output",
            source_path,
            "source program",
            "E_OUTPUT_COLLISION",
        )?;
        reject_path_collision(
            &manifest,
            "manifest",
            source_path,
            "source program",
            "E_MANIFEST_COLLISION",
        )?;
    }
    let video = compiled.video().clone();
    let audio = *compiled.audio();
    if !video.width.is_multiple_of(2) || !video.height.is_multiple_of(2) {
        return Err(Diagnostic::new(
            "E_EXPORT_DIMENSIONS",
            "the MP4/H.264/yuv420p export profile requires even width and height",
            compiled.output().map_or_else(
                || SourceSpan::source_start(compiled.entrypoint_source().clone()),
                |output| output.span.clone(),
            ),
        ));
    }

    let ffmpeg = inspect_ffmpeg()?;
    let ffprobe = inspect_ffprobe()?;
    let media_policy = RenderMediaPolicy::default();
    let execution_namespace = cache_execution_namespace(&ffmpeg, &ffprobe, media_policy)?;
    let mut lowerer = PreflightLowerer {
        compiled,
        ffmpeg: &ffmpeg,
        ffprobe: &ffprobe,
        nodes: Vec::new(),
        lowered: HashMap::new(),
    };
    let order = crate::compiler::traversal::topological_order(
        compiled.nodes(),
        compiled.symbol_values(),
        [render_output],
    )?;
    for value in order {
        lowerer.lower(value)?;
    }
    let result = lowerer.lowered[&render_output.id()];
    let named_values = compiled
        .named_values()
        .iter()
        .filter_map(|(name, value)| {
            lowerer
                .lowered
                .get(&value.id())
                .copied()
                .map(|node| (name.clone(), node))
        })
        .collect::<BTreeMap<_, _>>();
    reject_asset_collisions(&output, &manifest, &lowerer.nodes)?;
    let semantic_hash =
        prepared_semantic_hash(&video, &audio, result, &named_values, &lowerer.nodes)?;

    Ok(PreparedPlan {
        format_version: PREPARED_FORMAT_VERSION,
        engine_version: env!("CARGO_PKG_VERSION").to_owned(),
        semantic_hash,
        video,
        audio,
        media_policy,
        nodes: lowerer.nodes,
        result,
        named_values,
        output,
        manifest,
        ffmpeg,
        ffprobe,
        execution_namespace,
        entrypoint_source: compiled.entrypoint_source().clone(),
    })
}
