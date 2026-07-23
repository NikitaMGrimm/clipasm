use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::compiler::{CompiledWorkflow, SemanticNodeKind, SourceOrigin};
use crate::diagnostic::{Diagnostic, Result, SourceSpan};
use crate::model::{
    FrameCount, FrameRange, ImageFit, NodeId, ValueId, ValueRef, VideoDomain, VideoSpec,
};

const PREPARED_FORMAT_VERSION: u32 = 1;
const REQUIRED_FFMPEG_FILTERS: &[&str] = &[
    "scale", "crop", "pad", "fps", "setsar", "format", "trim", "setpts", "concat",
];

#[derive(Serialize)]
struct PreparedNodeIdentity<'a> {
    semantic_version: u32,
    domain: &'a VideoDomain,
    operation: serde_json::Value,
    upstream: Vec<&'a str>,
}

#[derive(Serialize)]
struct PreparedPlanIdentity<'a> {
    format_version: u32,
    engine_version: &'a str,
    video: &'a VideoSpec,
    root: &'a str,
    names: BTreeMap<&'a str, &'a str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PreparedPlan {
    format_version: u32,
    engine_version: String,
    semantic_hash: String,
    video: VideoSpec,
    media_policy: RenderMediaPolicy,
    nodes: Vec<PreparedNode>,
    root: NodeId,
    named_values: BTreeMap<String, NodeId>,
    output: PathBuf,
    manifest: PathBuf,
    ffmpeg: ToolIdentity,
    ffprobe: ToolIdentity,
    execution_namespace: String,
    #[serde(skip)]
    workflow_path: PathBuf,
}

impl PreparedPlan {
    #[must_use]
    pub fn semantic_hash(&self) -> &str {
        &self.semantic_hash
    }

    #[must_use]
    pub fn video(&self) -> &VideoSpec {
        &self.video
    }

    #[must_use]
    pub fn nodes(&self) -> &[PreparedNode] {
        &self.nodes
    }

    #[must_use]
    pub const fn root(&self) -> NodeId {
        self.root
    }

    #[must_use]
    pub fn named_values(&self) -> &BTreeMap<String, NodeId> {
        &self.named_values
    }

    #[must_use]
    pub fn output(&self) -> &Path {
        &self.output
    }

    #[must_use]
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

    pub(crate) fn workflow_path(&self) -> &Path {
        &self.workflow_path
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct PreparedNode {
    id: NodeId,
    kind: PreparedNodeKind,
    domain: VideoDomain,
    origin: SourceOrigin,
    fingerprint: String,
}

impl PreparedNode {
    #[must_use]
    pub const fn id(&self) -> NodeId {
        self.id
    }

    #[must_use]
    pub const fn kind(&self) -> &PreparedNodeKind {
        &self.kind
    }

    #[must_use]
    pub const fn domain(&self) -> &VideoDomain {
        &self.domain
    }

    #[must_use]
    pub const fn origin(&self) -> &SourceOrigin {
        &self.origin
    }

    #[must_use]
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum PreparedNodeKind {
    ImageVideo {
        asset: PreparedAsset,
        frames: FrameCount,
        fit: ImageFit,
    },
    Slice {
        input: NodeId,
        range: FrameRange,
    },
    Concat {
        inputs: Vec<NodeId>,
    },
}

#[derive(Clone, Debug, Serialize)]
pub struct PreparedAsset {
    source_path: PathBuf,
    content_hash: String,
}

impl PreparedAsset {
    #[must_use]
    pub fn source_path(&self) -> &Path {
        &self.source_path
    }

    #[must_use]
    pub fn content_hash(&self) -> &str {
        &self.content_hash
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct RenderMediaPolicy {
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
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
enum ExportPixelFormat {
    Yuv420p,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ToolIdentity {
    executable: PathBuf,
    version: String,
}

impl ToolIdentity {
    pub(crate) fn executable(&self) -> &Path {
        &self.executable
    }

    pub(crate) fn version(&self) -> &str {
        &self.version
    }
}

/// Resolve and verify assets/tools, lower root-reachable primitives, and build
/// an invariant-protected renderer plan.
///
/// # Errors
///
/// Returns a diagnostic for invalid output configuration, unavailable tool
/// capabilities, inaccessible/undecodable assets, or preparation failures.
pub fn preflight(compiled: &CompiledWorkflow) -> Result<PreparedPlan> {
    let output = prepare_output_path(compiled)?;
    let manifest = manifest_path(&output);
    let video = compiled.video().clone();
    if !video.width.is_multiple_of(2) || !video.height.is_multiple_of(2) {
        return Err(Diagnostic::new(
            "E_EXPORT_DIMENSIONS",
            "the MP4/H.264/yuv420p export profile requires even width and height",
            compiled.output().map_or_else(
                || SourceSpan::file_start(compiled.source_path()),
                |output| output.span.clone(),
            ),
        ));
    }

    let ffmpeg = inspect_ffmpeg()?;
    let ffprobe = inspect_ffprobe()?;
    let execution_namespace = hex::encode(Sha256::digest(format!(
        "{}\n{}\n{}\n{}\nprepared-v1",
        ffmpeg.executable.display(),
        ffmpeg.version,
        ffprobe.executable.display(),
        ffprobe.version
    )));
    let mut lowerer = PreflightLowerer {
        compiled,
        ffmpeg: &ffmpeg,
        ffprobe: &ffprobe,
        nodes: Vec::new(),
        lowered: HashMap::new(),
    };
    let root = lowerer.lower(compiled.root())?;
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
    let semantic_hash = prepared_semantic_hash(&video, root, &named_values, &lowerer.nodes)?;

    Ok(PreparedPlan {
        format_version: PREPARED_FORMAT_VERSION,
        engine_version: env!("CARGO_PKG_VERSION").to_owned(),
        semantic_hash,
        video,
        media_policy: RenderMediaPolicy::default(),
        nodes: lowerer.nodes,
        root,
        named_values,
        output,
        manifest,
        ffmpeg,
        ffprobe,
        execution_namespace,
        workflow_path: compiled.source_path().to_path_buf(),
    })
}

struct PreflightLowerer<'a> {
    compiled: &'a CompiledWorkflow,
    ffmpeg: &'a ToolIdentity,
    ffprobe: &'a ToolIdentity,
    nodes: Vec<PreparedNode>,
    lowered: HashMap<ValueId, NodeId>,
}

impl PreflightLowerer<'_> {
    #[allow(clippy::too_many_lines)]
    fn lower(&mut self, value: ValueRef) -> Result<NodeId> {
        if let Some(node) = self.lowered.get(&value.id()) {
            return Ok(*node);
        }
        let compiled_node = &self.compiled.nodes()[value.id().0 as usize];
        let result = match compiled_node.kind() {
            SemanticNodeKind::ImageVideo { path, frames, fit } => {
                let asset = prepare_asset(
                    self.compiled.source_path(),
                    path,
                    compiled_node.origin(),
                    self.ffmpeg,
                    self.ffprobe,
                )?;
                self.add_node(
                    PreparedNodeKind::ImageVideo {
                        asset,
                        frames: *frames,
                        fit: *fit,
                    },
                    compiled_node.domain().expect("Video node domain").clone(),
                    compiled_node.semantic_version(),
                    compiled_node.origin().clone(),
                )?
            }
            SemanticNodeKind::Reference { name } => {
                let target = self.compiled.named_values()[name];
                self.lower(target)?
            }
            SemanticNodeKind::Concat { inputs } => {
                let inputs = inputs
                    .iter()
                    .map(|input| self.lower(*input))
                    .collect::<Result<Vec<_>>>()?;
                self.add_node(
                    PreparedNodeKind::Concat { inputs },
                    compiled_node.domain().expect("Video node domain").clone(),
                    compiled_node.semantic_version(),
                    compiled_node.origin().clone(),
                )?
            }
            SemanticNodeKind::Slice { input, range } => {
                let input = self.lower(*input)?;
                self.add_node(
                    PreparedNodeKind::Slice {
                        input,
                        range: *range,
                    },
                    compiled_node.domain().expect("Video node domain").clone(),
                    compiled_node.semantic_version(),
                    compiled_node.origin().clone(),
                )?
            }
            SemanticNodeKind::During {
                base,
                processed,
                range,
            } => {
                let base_node = self.lower(*base)?;
                let processed_node = self.lower(*processed)?;
                let base_domain = self.compiled.nodes()[base.id().0 as usize]
                    .domain()
                    .expect("during base domain");
                let mut pieces = Vec::new();
                if range.start > 0 {
                    pieces.push(self.add_node(
                        PreparedNodeKind::Slice {
                            input: base_node,
                            range: FrameRange {
                                start: 0,
                                end: range.start,
                            },
                        },
                        VideoDomain {
                            frames: FrameCount(range.start),
                            width: base_domain.width,
                            height: base_domain.height,
                            frame_rate: base_domain.frame_rate,
                        },
                        1,
                        compiled_node.origin().clone_with_construct("during prefix"),
                    )?);
                }
                pieces.push(processed_node);
                if range.end < base_domain.frames.0 {
                    pieces.push(self.add_node(
                        PreparedNodeKind::Slice {
                            input: base_node,
                            range: FrameRange {
                                start: range.end,
                                end: base_domain.frames.0,
                            },
                        },
                        VideoDomain {
                            frames: FrameCount(base_domain.frames.0 - range.end),
                            width: base_domain.width,
                            height: base_domain.height,
                            frame_rate: base_domain.frame_rate,
                        },
                        1,
                        compiled_node.origin().clone_with_construct("during suffix"),
                    )?);
                }
                if pieces.len() == 1 {
                    pieces[0]
                } else {
                    self.add_node(
                        PreparedNodeKind::Concat { inputs: pieces },
                        compiled_node.domain().expect("during domain").clone(),
                        compiled_node.semantic_version(),
                        compiled_node.origin().clone(),
                    )?
                }
            }
        };
        self.lowered.insert(value.id(), result);
        Ok(result)
    }

    fn add_node(
        &mut self,
        kind: PreparedNodeKind,
        domain: VideoDomain,
        semantic_version: u32,
        origin: SourceOrigin,
    ) -> Result<NodeId> {
        let id = NodeId(u32::try_from(self.nodes.len()).map_err(|_| {
            Diagnostic::new(
                "E_GRAPH_TOO_LARGE",
                "prepared graph contains too many primitive nodes",
                origin.span.clone(),
            )
        })?);
        let fingerprint = node_fingerprint(&kind, &domain, semantic_version, &self.nodes)?;
        self.nodes.push(PreparedNode {
            id,
            kind,
            domain,
            origin,
            fingerprint,
        });
        Ok(id)
    }
}

fn prepare_output_path(compiled: &CompiledWorkflow) -> Result<PathBuf> {
    let output = compiled.output().ok_or_else(|| {
        Diagnostic::new(
            "E_MISSING_OUTPUT",
            "`render` requires the top-level `output` field",
            SourceSpan::file_start(compiled.source_path()),
        )
    })?;
    if output
        .value
        .extension()
        .and_then(|extension| extension.to_str())
        .is_none_or(|extension| !extension.eq_ignore_ascii_case("mp4"))
    {
        return Err(Diagnostic::new(
            "E_INVALID_OUTPUT_EXTENSION",
            "the foundation export profile requires an `.mp4` output path",
            output.span.clone(),
        ));
    }
    Ok(resolve_path(compiled.source_path(), &output.value))
}

fn prepare_asset(
    workflow: &Path,
    authored: &Path,
    origin: &SourceOrigin,
    ffmpeg: &ToolIdentity,
    ffprobe: &ToolIdentity,
) -> Result<PreparedAsset> {
    let source_path = resolve_path(workflow, authored);
    let metadata = fs::metadata(&source_path).map_err(|error| {
        Diagnostic::new(
            "E_MISSING_IMAGE_FILE",
            format!(
                "image file `{}` is not accessible: {error}",
                source_path.display()
            ),
            origin.span.clone(),
        )
    })?;
    if !metadata.is_file() {
        return Err(Diagnostic::new(
            "E_MISSING_IMAGE_FILE",
            format!("image path `{}` is not a file", source_path.display()),
            origin.span.clone(),
        ));
    }
    let source_path = fs::canonicalize(&source_path).unwrap_or(source_path);
    let content_hash = hash_file(&source_path, &origin.span)?;
    verify_image_decodable(&source_path, &origin.span, ffmpeg, ffprobe)?;
    Ok(PreparedAsset {
        source_path,
        content_hash,
    })
}

pub(crate) fn verify_prepared_asset(asset: &PreparedAsset, span: &SourceSpan) -> Result<()> {
    let actual = hash_file(asset.source_path(), span)?;
    if actual == asset.content_hash() {
        Ok(())
    } else {
        Err(Diagnostic::new(
            "E_ASSET_CHANGED",
            format!(
                "asset `{}` changed after preflight",
                asset.source_path().display()
            ),
            span.clone(),
        ))
    }
}

fn hash_file(path: &Path, span: &SourceSpan) -> Result<String> {
    let file = fs::File::open(path).map_err(|error| {
        Diagnostic::new(
            "E_INPUT_HASH",
            format!("could not read asset `{}`: {error}", path.display()),
            span.clone(),
        )
    })?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let read = reader.read(&mut buffer).map_err(|error| {
            Diagnostic::new(
                "E_INPUT_HASH",
                format!("could not hash asset `{}`: {error}", path.display()),
                span.clone(),
            )
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn node_fingerprint(
    kind: &PreparedNodeKind,
    domain: &VideoDomain,
    semantic_version: u32,
    existing: &[PreparedNode],
) -> Result<String> {
    let (operation, inputs) = match kind {
        PreparedNodeKind::ImageVideo { asset, frames, fit } => (
            serde_json::json!({
                "operation": "image_video",
                "content_hash": asset.content_hash,
                "frames": frames,
                "fit": fit,
            }),
            Vec::new(),
        ),
        PreparedNodeKind::Slice { input, range } => (
            serde_json::json!({
                "operation": "slice",
                "range": range,
            }),
            vec![*input],
        ),
        PreparedNodeKind::Concat { inputs } => {
            (serde_json::json!({"operation": "concat"}), inputs.clone())
        }
    };
    let upstream = inputs
        .iter()
        .map(|input| existing[input.0 as usize].fingerprint.as_str())
        .collect::<Vec<_>>();
    crate::compiler::fingerprint::hash_serializable(&PreparedNodeIdentity {
        semantic_version,
        domain,
        operation,
        upstream,
    })
}

fn prepared_semantic_hash(
    video: &VideoSpec,
    root: NodeId,
    names: &BTreeMap<String, NodeId>,
    nodes: &[PreparedNode],
) -> Result<String> {
    let names = names
        .iter()
        .map(|(name, id)| (name.as_str(), nodes[id.0 as usize].fingerprint.as_str()))
        .collect::<BTreeMap<_, _>>();
    crate::compiler::fingerprint::hash_serializable(&PreparedPlanIdentity {
        format_version: PREPARED_FORMAT_VERSION,
        engine_version: env!("CARGO_PKG_VERSION"),
        video,
        root: &nodes[root.0 as usize].fingerprint,
        names,
    })
}

fn inspect_ffmpeg() -> Result<ToolIdentity> {
    inspect_ffmpeg_at(&resolve_executable("ffmpeg", "E_FFMPEG")?)
}

fn inspect_ffmpeg_at(tool: &Path) -> Result<ToolIdentity> {
    let tool = fs::canonicalize(tool).map_err(|error| {
        Diagnostic::new(
            "E_FFMPEG",
            format!(
                "could not resolve FFmpeg executable `{}`: {error}",
                tool.display()
            ),
            SourceSpan::file_start(tool),
        )
    })?;
    let version = tool_version(&tool, "E_FFMPEG")?;
    let encoders = tool_output(&tool, &["-hide_banner", "-encoders"], "E_FFMPEG")?;
    for encoder in ["libx264", "ffv1"] {
        if capability_missing(&encoders, encoder) {
            return Err(Diagnostic::new(
                "E_FFMPEG_CAPABILITY",
                format!("installed FFmpeg does not provide the required `{encoder}` encoder"),
                SourceSpan::file_start(&tool),
            ));
        }
    }
    let muxers = tool_output(&tool, &["-hide_banner", "-muxers"], "E_FFMPEG")?;
    for (muxer, display) in [("mp4", "MP4"), ("matroska", "Matroska")] {
        if capability_missing(&muxers, muxer) {
            return Err(Diagnostic::new(
                "E_FFMPEG_CAPABILITY",
                format!("installed FFmpeg does not provide the required {display} muxer"),
                SourceSpan::file_start(&tool),
            ));
        }
    }
    let filters = tool_output(&tool, &["-hide_banner", "-filters"], "E_FFMPEG")?;
    for filter in REQUIRED_FFMPEG_FILTERS {
        if capability_missing(&filters, filter) {
            return Err(Diagnostic::new(
                "E_FFMPEG_CAPABILITY",
                format!("installed FFmpeg does not provide the required `{filter}` filter"),
                SourceSpan::file_start(&tool),
            ));
        }
    }
    Ok(ToolIdentity {
        executable: tool,
        version,
    })
}

fn capability_missing(output: &str, capability: &str) -> bool {
    !output
        .lines()
        .any(|line| line.split_whitespace().any(|token| token == capability))
}

fn inspect_ffprobe() -> Result<ToolIdentity> {
    let executable = resolve_executable("ffprobe", "E_FFPROBE")?;
    Ok(ToolIdentity {
        version: tool_version(&executable, "E_FFPROBE")?,
        executable,
    })
}

fn resolve_executable(name: &str, code: &'static str) -> Result<PathBuf> {
    let authored = Path::new(name);
    let candidates = if authored.components().count() > 1 {
        vec![authored.to_path_buf()]
    } else {
        let path = std::env::var_os("PATH").ok_or_else(|| {
            Diagnostic::new(
                code,
                format!("could not resolve `{name}` because PATH is not set"),
                SourceSpan::file_start(authored),
            )
        })?;
        std::env::split_paths(&path)
            .flat_map(|directory| executable_candidates(&directory, name))
            .collect()
    };
    candidates
        .into_iter()
        .find(|candidate| is_executable_file(candidate))
        .and_then(|candidate| fs::canonicalize(candidate).ok())
        .ok_or_else(|| {
            Diagnostic::new(
                code,
                format!("could not resolve executable `{name}` on PATH"),
                SourceSpan::file_start(authored),
            )
        })
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn executable_candidates(directory: &Path, name: &str) -> Vec<PathBuf> {
    let candidate = directory.join(name);
    #[cfg(windows)]
    {
        let mut candidates = vec![candidate.clone()];
        if candidate.extension().is_none() {
            candidates.push(candidate.with_extension("exe"));
        }
        candidates
    }
    #[cfg(not(windows))]
    {
        vec![candidate]
    }
}

#[derive(Deserialize)]
struct ImageProbeDocument {
    #[serde(default)]
    streams: Vec<ImageProbeStream>,
}

#[derive(Deserialize)]
struct ImageProbeStream {
    codec_type: Option<String>,
    nb_read_frames: Option<String>,
}

fn verify_image_decodable(
    path: &Path,
    span: &SourceSpan,
    ffmpeg: &ToolIdentity,
    ffprobe: &ToolIdentity,
) -> Result<()> {
    let output = Command::new(ffprobe.executable())
        .args([
            "-v",
            "error",
            "-count_frames",
            "-show_entries",
            "stream=codec_type,nb_read_frames",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .map_err(|error| {
            Diagnostic::new(
                "E_FFPROBE",
                format!("could not inspect image `{}`: {error}", path.display()),
                span.clone(),
            )
        })?;
    if !output.status.success() {
        return Err(Diagnostic::new(
            "E_SOURCE_DECODABILITY",
            format!(
                "image `{}` is not decodable by FFprobe\n{}",
                path.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            ),
            span.clone(),
        ));
    }
    let document: ImageProbeDocument = serde_json::from_slice(&output.stdout).map_err(|error| {
        Diagnostic::new(
            "E_SOURCE_DECODABILITY",
            format!(
                "FFprobe returned invalid image metadata for `{}`: {error}",
                path.display()
            ),
            span.clone(),
        )
    })?;
    let videos = document
        .streams
        .iter()
        .filter(|stream| stream.codec_type.as_deref() == Some("video"))
        .collect::<Vec<_>>();
    let audio_count = document
        .streams
        .iter()
        .filter(|stream| stream.codec_type.as_deref() == Some("audio"))
        .count();
    let frame_count = videos
        .first()
        .and_then(|stream| stream.nb_read_frames.as_deref())
        .and_then(|frames| frames.parse::<u64>().ok());
    if videos.len() != 1 || audio_count != 0 || frame_count != Some(1) {
        return Err(Diagnostic::new(
            "E_SOURCE_CONTRACT",
            format!(
                "image `{}` must contain exactly one video stream, no audio, and one decoded frame; found {} video stream(s), {audio_count} audio stream(s), and {frame_count:?} decoded frame(s)",
                path.display(),
                videos.len()
            ),
            span.clone(),
        ));
    }
    let decode = Command::new(ffmpeg.executable())
        .args(["-v", "error", "-loop", "1", "-i"])
        .arg(path)
        .args(["-map", "0:v:0", "-frames:v", "1", "-an", "-f", "null", "-"])
        .output()
        .map_err(|error| {
            Diagnostic::new(
                "E_FFMPEG",
                format!("could not decode image `{}`: {error}", path.display()),
                span.clone(),
            )
        })?;
    if !decode.status.success() {
        return Err(Diagnostic::new(
            "E_SOURCE_DECODABILITY",
            format!(
                "image `{}` is not compatible with the renderer's still-image input mode\n{}",
                path.display(),
                String::from_utf8_lossy(&decode.stderr).trim()
            ),
            span.clone(),
        ));
    }
    Ok(())
}

fn tool_version(tool: &Path, code: &'static str) -> Result<String> {
    Ok(tool_output(tool, &["-version"], code)?
        .lines()
        .next()
        .unwrap_or_default()
        .to_owned())
}

fn tool_output(tool: &Path, arguments: &[&str], code: &'static str) -> Result<String> {
    let output = Command::new(tool)
        .args(arguments)
        .output()
        .map_err(|error| {
            Diagnostic::new(
                code,
                format!("could not start `{}`: {error}", tool.display()),
                SourceSpan::file_start(tool),
            )
        })?;
    if !output.status.success() {
        return Err(Diagnostic::new(
            code,
            format!(
                "`{}` exited with {}\n{}",
                tool.display(),
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ),
            SourceSpan::file_start(tool),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned()
        + &String::from_utf8_lossy(&output.stderr))
}

fn resolve_path(workflow: &Path, value: &Path) -> PathBuf {
    if value.is_absolute() {
        value.to_path_buf()
    } else {
        workflow
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(value)
    }
}

fn manifest_path(output: &Path) -> PathBuf {
    let mut value = output.as_os_str().to_os_string();
    value.push(".manifest.json");
    PathBuf::from(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn executable_script(contents: &str) -> (tempfile::TempDir, PathBuf) {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("fake-ffmpeg");
        fs::write(&path, contents).expect("script");
        let mut permissions = fs::metadata(&path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).expect("permissions");
        (directory, path)
    }

    #[cfg(unix)]
    #[test]
    fn ffmpeg_preflight_requires_all_render_encoders_and_muxers() {
        let (_directory, no_encoder) = executable_script(
            "#!/bin/sh\nif [ \"$1\" = \"-version\" ]; then echo fake; else echo none; fi\n",
        );
        let encoder_error = inspect_ffmpeg_at(&no_encoder).expect_err("missing encoder");
        assert_eq!(encoder_error.code, "E_FFMPEG_CAPABILITY");
        assert!(encoder_error.message.contains("libx264"));

        let (_directory, no_ffv1) = executable_script(
            "#!/bin/sh\nif [ \"$1\" = \"-version\" ]; then echo fake; elif [ \"$2\" = \"-encoders\" ]; then echo libx264; else echo none; fi\n",
        );
        let encoder_error = inspect_ffmpeg_at(&no_ffv1).expect_err("missing FFV1");
        assert_eq!(encoder_error.code, "E_FFMPEG_CAPABILITY");
        assert!(encoder_error.message.contains("ffv1"));

        let (_directory, no_matroska) = executable_script(
            "#!/bin/sh\nif [ \"$1\" = \"-version\" ]; then echo fake; elif [ \"$2\" = \"-encoders\" ]; then echo 'libx264 ffv1'; else echo mp4; fi\n",
        );
        let container_error = inspect_ffmpeg_at(&no_matroska).expect_err("missing Matroska");
        assert_eq!(container_error.code, "E_FFMPEG_CAPABILITY");
        assert!(container_error.message.contains("Matroska"));
    }

    #[cfg(unix)]
    #[test]
    fn ffmpeg_preflight_requires_every_render_filter() {
        let (_directory, no_filters) = executable_script(
            "#!/bin/sh\nif [ \"$1\" = \"-version\" ]; then echo fake; elif [ \"$2\" = \"-encoders\" ]; then echo 'libx264 ffv1'; elif [ \"$2\" = \"-muxers\" ]; then echo 'mp4 matroska'; else echo none; fi\n",
        );
        let error = inspect_ffmpeg_at(&no_filters).expect_err("missing filters");
        assert_eq!(error.code, "E_FFMPEG_CAPABILITY");
        assert!(error.message.contains("scale"));
    }

    #[cfg(unix)]
    #[test]
    fn manifest_path_preserves_non_utf8_output_bytes() {
        use std::ffi::OsString;
        use std::os::unix::ffi::{OsStrExt, OsStringExt};

        let output = PathBuf::from(OsString::from_vec(b"video-\xFF.mp4".to_vec()));
        let manifest = manifest_path(&output);
        assert_eq!(
            manifest.as_os_str().as_bytes(),
            b"video-\xFF.mp4.manifest.json"
        );
    }

    #[test]
    fn image_contract_rejects_video_and_animated_sources() {
        if Command::new("ffmpeg").arg("-version").output().is_err()
            || Command::new("ffprobe").arg("-version").output().is_err()
        {
            return;
        }
        let directory = tempfile::tempdir().expect("temporary directory");
        let video = directory.path().join("video.mp4");
        let animated = directory.path().join("animated.gif");
        let png = directory.path().join("still.png");
        let jpeg = directory.path().join("still.jpg");
        let ppm = directory.path().join("still.ppm");
        let span = SourceSpan::file_start("workflow.yaml");
        let ffmpeg = inspect_ffmpeg().expect("FFmpeg");
        let ffprobe = inspect_ffprobe().expect("FFprobe");
        assert!(ffmpeg.executable().is_absolute());
        assert!(ffprobe.executable().is_absolute());

        let status = Command::new("ffmpeg")
            .args([
                "-y",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=red:size=16x16:rate=2:duration=1",
                "-c:v",
                "libx264",
            ])
            .arg(&video)
            .status()
            .expect("create video");
        assert!(status.success());
        let status = Command::new("ffmpeg")
            .args([
                "-y",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=red:size=16x16:rate=2:duration=1",
            ])
            .arg(&animated)
            .status()
            .expect("create animation");
        assert!(status.success());

        assert!(verify_image_decodable(&video, &span, &ffmpeg, &ffprobe).is_err());
        assert!(verify_image_decodable(&animated, &span, &ffmpeg, &ffprobe).is_err());

        for still in [&png, &jpeg] {
            let status = Command::new(ffmpeg.executable())
                .args([
                    "-y",
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "color=red:size=16x16",
                    "-frames:v",
                    "1",
                ])
                .arg(still)
                .status()
                .expect("create still image");
            assert!(status.success());
        }
        fs::write(&ppm, b"P3\n1 1\n255\n255 0 0\n").expect("PPM");
        for still in [&png, &jpeg, &ppm] {
            verify_image_decodable(still, &span, &ffmpeg, &ffprobe).expect("supported still image");
        }
    }
}
