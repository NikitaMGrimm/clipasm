//! Browser preparation for virtual, in-memory media projects.
//!
//! This module performs no filesystem or process I/O. A browser host supplies
//! content hashes and `FFprobe` metadata for requested virtual assets, then
//! executes the returned render plan through its own `FFmpeg`
//! runtime.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use crate::compiler::CompiledProgram;
use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::model::{AudioDomain, AudioSpec, FrameCount, NodeId, VideoSpec};
use crate::semantic::{SemanticNodeKind, SourceOrigin};
use crate::source::SourceSpan;

use super::identity::prepared_semantic_hash;
use super::lower::{PreflightLowerer, PreparationHost};
use super::tools::ExternalToolIdentity;
use super::{PreparedAsset, PreparedNode, RenderPolicy};

const MAX_PROBE_JSON_BYTES: usize = 256 * 1024;
const MAX_TOTAL_PROBE_JSON_BYTES: usize = 1024 * 1024;

/// The media role of one virtual asset required for browser rendering.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BrowserAssetKind {
    /// A still image used by the `image` program.
    Image,
    /// A video file used by the `video` program.
    Video,
    /// One path used through both still-image and video-file source contracts.
    ImageAndVideo,
}

impl BrowserAssetKind {
    const fn merge(self, other: Self) -> Self {
        match (self, other) {
            (Self::Image, Self::Image) => Self::Image,
            (Self::Video, Self::Video) => Self::Video,
            (Self::Image, Self::Video)
            | (Self::Video, Self::Image)
            | (Self::ImageAndVideo, _)
            | (_, Self::ImageAndVideo) => Self::ImageAndVideo,
        }
    }
}

/// One virtual asset required by the result-reachable semantic graph.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserAssetRequest {
    path: String,
    kind: BrowserAssetKind,
}

impl BrowserAssetRequest {
    /// Return the normalized project-relative virtual path.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Return the media role expected at this path.
    #[must_use]
    pub const fn kind(&self) -> BrowserAssetKind {
        self.kind
    }
}

/// One browser file bound to a normalized virtual path, SHA-256 digest, and
/// bounded `FFprobe` stream document for the same bytes.
#[derive(Clone, Debug)]
pub struct BrowserAsset {
    path: String,
    content_hash: String,
    probe: String,
}

impl BrowserAsset {
    /// Construct one virtual asset fact supplied by a browser host.
    #[must_use]
    pub fn new(
        path: impl Into<String>,
        content_hash: impl Into<String>,
        probe: impl Into<String>,
    ) -> Self {
        Self {
            path: path.into(),
            content_hash: content_hash.into(),
            probe: probe.into(),
        }
    }

    /// Return the browser-visible project-relative path.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Return the SHA-256 digest supplied by the browser host.
    #[must_use]
    pub fn content_hash(&self) -> &str {
        &self.content_hash
    }
}

/// An exact prepared graph for browser rendering.
#[derive(Clone, Debug)]
pub struct BrowserPreparedPlan {
    video: VideoSpec,
    audio: AudioSpec,
    nodes: Vec<PreparedNode>,
    result: NodeId,
    semantic_hash: String,
}

impl BrowserPreparedPlan {
    /// Return the project video properties.
    #[must_use]
    pub const fn video(&self) -> &VideoSpec {
        &self.video
    }

    /// Return the project Audio properties.
    #[must_use]
    pub const fn audio(&self) -> &AudioSpec {
        &self.audio
    }

    /// Return the prepared semantic identity, including supplied asset hashes.
    #[must_use]
    pub fn semantic_hash(&self) -> &str {
        &self.semantic_hash
    }

    pub(crate) fn nodes(&self) -> &[PreparedNode] {
        &self.nodes
    }

    pub(crate) const fn result(&self) -> NodeId {
        self.result
    }
}

/// Discover virtual assets required for browser rendering.
///
/// Browser rendering accepts result-reachable still-image and video-file
/// assets, plus every native operation that can be prepared from them. Audio
/// files and external programs return an explicit unsupported diagnostic.
///
/// # Errors
///
/// Returns a diagnostic when publication does not select exactly one Video,
/// a virtual path is unsafe or non-UTF-8, or the graph requires an unsupported
/// browser source.
pub fn required_assets(compiled: &CompiledProgram) -> Result<Vec<BrowserAssetRequest>> {
    let output = compiled.render_output()?;
    let order =
        crate::semantic::topological_order(compiled.nodes(), compiled.symbol_values(), [output])?;
    let mut paths = BTreeMap::<String, usize>::new();
    let mut requests = Vec::<BrowserAssetRequest>::new();
    for value in order {
        let node = &compiled.nodes()[value.id().get() as usize];
        match node.kind() {
            SemanticNodeKind::ImageVideo { path, .. }
            | SemanticNodeKind::DeferredImageVideo { path, .. } => {
                let path = virtual_path(path, &node.origin().span)?;
                if let Some(index) = paths.get(&path) {
                    requests[*index].kind = requests[*index].kind.merge(BrowserAssetKind::Image);
                } else {
                    paths.insert(path.clone(), requests.len());
                    requests.push(BrowserAssetRequest {
                        path,
                        kind: BrowserAssetKind::Image,
                    });
                }
            }
            SemanticNodeKind::VideoSource { path, .. } => {
                let path = virtual_path(path, &node.origin().span)?;
                if let Some(index) = paths.get(&path) {
                    requests[*index].kind = requests[*index].kind.merge(BrowserAssetKind::Video);
                } else {
                    paths.insert(path.clone(), requests.len());
                    requests.push(BrowserAssetRequest {
                        path,
                        kind: BrowserAssetKind::Video,
                    });
                }
            }
            SemanticNodeKind::AudioSource { .. } => {
                return Err(browser_unsupported(
                    "audio-file sources are not yet supported in the browser",
                    &node.origin().span,
                ));
            }
            SemanticNodeKind::ExternalVideo { .. } => {
                return Err(browser_unsupported(
                    "external programs cannot run in the browser",
                    &node.origin().span,
                ));
            }
            SemanticNodeKind::Reference { .. }
            | SemanticNodeKind::Repeat { .. }
            | SemanticNodeKind::ZoomIn { .. }
            | SemanticNodeKind::FlashCut { .. }
            | SemanticNodeKind::Crossfade { .. }
            | SemanticNodeKind::Concat { .. }
            | SemanticNodeKind::Slice { .. }
            | SemanticNodeKind::DeferredSlice { .. }
            | SemanticNodeKind::ReplaceRange { .. }
            | SemanticNodeKind::DeferredReplaceRange { .. }
            | SemanticNodeKind::ExtractAudio { .. }
            | SemanticNodeKind::SetAudio { .. }
            | SemanticNodeKind::AudioOnBlack { .. } => {}
        }
    }
    Ok(requests)
}

/// Prepare an exact browser graph from supplied virtual asset facts.
///
/// The browser host is responsible for hashing the same bytes it will mount in
/// the virtual filesystem. Preparation does not open or decode those bytes.
///
/// # Errors
///
/// Returns a diagnostic for missing, duplicate, unsafe, or invalid asset facts,
/// unsupported source kinds, or ordinary prepared-domain failures.
pub fn prepare(compiled: &CompiledProgram, assets: &[BrowserAsset]) -> Result<BrowserPreparedPlan> {
    let requests = required_assets(compiled)?;
    let supplied = supplied_assets(assets)?;
    let mut prepared_assets = BTreeMap::new();
    for request in requests {
        let Some(facts) = supplied.get(request.path()) else {
            return Err(Diagnostic::builtin(
                BuiltinDiagnostic::BrowserMissingAsset,
                format!("browser asset `{}` has not been supplied", request.path()),
                SourceSpan::file_start(request.path()),
            ));
        };
        prepared_assets.insert(
            PathBuf::from(request.path()),
            BrowserPreparedAsset {
                asset: PreparedAsset::new(
                    PathBuf::from(request.path()),
                    facts.content_hash.clone(),
                ),
                probe: facts.probe.clone(),
            },
        );
    }

    let render_output = compiled.render_output()?;
    let video = *compiled.video();
    let audio = *compiled.audio();
    RenderPolicy::CURRENT.validate_video_spec(
        &video,
        &SourceSpan::source_start(compiled.entrypoint_source().clone()),
    )?;
    let mut host = BrowserPreparationHost {
        assets: prepared_assets,
    };
    let mut lowerer = PreflightLowerer {
        compiled,
        host: &mut host,
        nodes: Vec::new(),
        lowered: HashMap::new(),
    };
    let order = crate::semantic::topological_order(
        compiled.nodes(),
        compiled.symbol_values(),
        [render_output],
    )?;
    for value in order {
        lowerer.lower(value)?;
    }
    let result = lowerer.lowered[&render_output.id()];
    let names = compiled
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
    let semantic_hash = prepared_semantic_hash(&video, audio, result, &names, &lowerer.nodes)?;

    Ok(BrowserPreparedPlan {
        video,
        audio,
        nodes: lowerer.nodes,
        result,
        semantic_hash,
    })
}

#[derive(Clone)]
struct BrowserAssetFacts {
    content_hash: String,
    probe: String,
}

fn supplied_assets(assets: &[BrowserAsset]) -> Result<BTreeMap<String, BrowserAssetFacts>> {
    let mut supplied = BTreeMap::new();
    let mut total_probe_bytes = 0_usize;
    for asset in assets {
        let span = SourceSpan::file_start(&asset.path);
        let path = normalize_relative(Path::new(&asset.path), &span)?;
        if asset.probe.len() > MAX_PROBE_JSON_BYTES {
            return Err(Diagnostic::builtin(
                BuiltinDiagnostic::BrowserAssetFacts,
                format!(
                    "browser asset `{path}` probe metadata exceeds the {MAX_PROBE_JSON_BYTES}-byte limit"
                ),
                span,
            ));
        }
        total_probe_bytes = total_probe_bytes
            .checked_add(asset.probe.len())
            .filter(|total| *total <= MAX_TOTAL_PROBE_JSON_BYTES)
            .ok_or_else(|| {
                Diagnostic::builtin(
                    BuiltinDiagnostic::BrowserAssetFacts,
                    format!(
                        "browser asset probe metadata exceeds the {MAX_TOTAL_PROBE_JSON_BYTES}-byte aggregate limit"
                    ),
                    span.clone(),
                )
            })?;
        if asset.content_hash.len() != 64
            || !asset
                .content_hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(Diagnostic::builtin(
                BuiltinDiagnostic::BrowserAssetHash,
                format!("browser asset `{path}` does not have a valid SHA-256 digest"),
                span,
            ));
        }
        if supplied
            .insert(
                path.clone(),
                BrowserAssetFacts {
                    content_hash: asset.content_hash.to_ascii_lowercase(),
                    probe: asset.probe.clone(),
                },
            )
            .is_some()
        {
            return Err(Diagnostic::builtin(
                BuiltinDiagnostic::BrowserDuplicateAsset,
                format!("browser asset path `{path}` was supplied more than once"),
                SourceSpan::file_start(path),
            ));
        }
    }
    Ok(supplied)
}

fn virtual_path(authored: &Path, span: &SourceSpan) -> Result<String> {
    if authored.is_absolute() {
        return Err(invalid_browser_path(authored, span));
    }
    let base = span
        .source()
        .base_directory()
        .unwrap_or_else(|| Path::new(""));
    if base.is_absolute() {
        return Err(invalid_browser_path(authored, span));
    }
    normalize_relative(&base.join(authored), span)
}

fn normalize_relative(path: &Path, span: &SourceSpan) -> Result<String> {
    let Some(path_text) = path.to_str() else {
        return Err(invalid_browser_path(path, span));
    };
    let portable = path_text.replace('\\', "/");
    let bytes = portable.as_bytes();
    if portable.starts_with('/')
        || (bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':')
    {
        return Err(invalid_browser_path(path, span));
    }

    let mut components = Vec::new();
    for component in portable.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                if components.pop().is_none() {
                    return Err(invalid_browser_path(path, span));
                }
            }
            value => components.push(value),
        }
    }
    if components.is_empty() {
        return Err(invalid_browser_path(path, span));
    }
    Ok(components.join("/"))
}

fn invalid_browser_path(path: &Path, span: &SourceSpan) -> Diagnostic {
    Diagnostic::builtin(
        BuiltinDiagnostic::BrowserAssetPath,
        format!(
            "browser asset path `{}` must be a project-relative UTF-8 path without traversal",
            path.display()
        ),
        span.clone(),
    )
}

fn browser_unsupported(message: &str, span: &SourceSpan) -> Diagnostic {
    Diagnostic::builtin(
        BuiltinDiagnostic::BrowserRenderUnsupported,
        message,
        span.clone(),
    )
}

struct BrowserPreparationHost {
    assets: BTreeMap<PathBuf, BrowserPreparedAsset>,
}

struct BrowserPreparedAsset {
    asset: PreparedAsset,
    probe: String,
}

impl PreparationHost for BrowserPreparationHost {
    fn prepare_image(
        &mut self,
        authored: &Path,
        origin: &SourceOrigin,
    ) -> Result<(PreparedAsset, super::PreparedSourceColor)> {
        let path = PathBuf::from(virtual_path(authored, &origin.span)?);
        let prepared = self.assets.get(&path).ok_or_else(|| {
            Diagnostic::builtin(
                BuiltinDiagnostic::BrowserMissingAsset,
                format!("browser asset `{}` has not been supplied", path.display()),
                origin.span.clone(),
            )
        })?;
        let color = super::tools::validate_image_probe_json(&path, &origin.span, &prepared.probe)?;
        Ok((prepared.asset.clone(), color))
    }

    fn prepare_video(
        &mut self,
        authored: &Path,
        video: &VideoSpec,
        origin: &SourceOrigin,
    ) -> Result<(PreparedAsset, FrameCount, bool, super::PreparedSourceColor)> {
        let path = PathBuf::from(virtual_path(authored, &origin.span)?);
        let prepared = self.assets.get(&path).ok_or_else(|| {
            Diagnostic::builtin(
                BuiltinDiagnostic::BrowserMissingAsset,
                format!("browser asset `{}` has not been supplied", path.display()),
                origin.span.clone(),
            )
        })?;
        let (frames, has_audio, color) =
            super::tools::validate_video_probe_json(&path, video, &origin.span, &prepared.probe)?;
        Ok((prepared.asset.clone(), frames, has_audio, color))
    }

    fn prepare_audio(
        &mut self,
        _authored: &Path,
        _audio: AudioSpec,
        origin: &SourceOrigin,
    ) -> Result<(PreparedAsset, AudioDomain)> {
        Err(browser_unsupported(
            "audio-file sources are not yet supported in the browser",
            &origin.span,
        ))
    }

    fn prepare_external_tool(
        &mut self,
        _authored: &Path,
        span: &SourceSpan,
    ) -> Result<ExternalToolIdentity> {
        Err(browser_unsupported(
            "external programs cannot run in the browser",
            span,
        ))
    }

    fn prepare_external_file(
        &mut self,
        _authored: &Path,
        span: &SourceSpan,
    ) -> Result<PreparedAsset> {
        Err(browser_unsupported(
            "external programs cannot run in the browser",
            span,
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::model::ValueType;
    use crate::preflight::{PreparedNodeMedia, PreparedVideoKind};

    const IMAGE_PROBE: &str = r#"{"streams":[{"codec_type":"video","nb_read_frames":"1","pix_fmt":"rgb24","color_range":"pc","color_space":"gbr","color_transfer":"unknown","color_primaries":"unknown"}]}"#;
    const VIDEO_PROBE: &str = r#"{"streams":[{"codec_type":"video","nb_read_frames":"48","avg_frame_rate":"24/1","pix_fmt":"yuv444p","color_range":"tv","color_space":"bt709","color_transfer":"bt709","color_primaries":"bt709"},{"codec_type":"audio","sample_rate":"48000"}]}"#;

    fn compiled(source: &str) -> CompiledProgram {
        let package =
            crate::language::parse_str(Path::new("playground.clipasm"), source).expect("source");
        crate::compiler::compile(&package).expect("compiled")
    }

    fn asset(path: &str, hash: &str, probe: &str) -> BrowserAsset {
        BrowserAsset::new(path, hash.repeat(32), probe)
    }

    #[test]
    fn requests_each_reachable_image_path_once() {
        let compiled = compiled(
            "clipasm 1\nimage(\"assets/still.png\", 1s)\nimage(\"assets/still.png\", 1s)\nconcat\n",
        );
        let requests = required_assets(&compiled).expect("requests");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].path(), "assets/still.png");
        assert_eq!(requests[0].kind(), BrowserAssetKind::Image);
    }

    #[test]
    fn requests_video_sources_for_browser_probing() {
        let compiled = compiled("clipasm 1\nvideo(\"clips/scene.mkv\")\n");
        let requests = required_assets(&compiled).expect("requests");

        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].path(), "clips/scene.mkv");
        assert_eq!(requests[0].kind(), BrowserAssetKind::Video);
    }

    #[test]
    fn one_path_can_require_both_source_contracts() {
        let compiled =
            compiled("clipasm 1\nimage(\"shared.mkv\", 1s)\nvideo(\"shared.mkv\")\nconcat\n");
        let requests = required_assets(&compiled).expect("requests");

        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].path(), "shared.mkv");
        assert_eq!(requests[0].kind(), BrowserAssetKind::ImageAndVideo);
    }

    #[test]
    fn prepares_video_sources_from_validated_browser_probe_metadata() {
        let compiled = compiled(
            "clipasm 1\nconfig {\nvideo {\nwidth = 320\nheight = 180\nfps = 24\n}\n}\nvideo(\"scene.mkv\")\n",
        );
        let plan =
            prepare(&compiled, &[asset("scene.mkv", "ab", VIDEO_PROBE)]).expect("browser plan");

        match plan.nodes[plan.result.get() as usize].media() {
            PreparedNodeMedia::Video {
                kind: PreparedVideoKind::VideoSource { frames, .. },
                has_audio,
                ..
            } => {
                assert_eq!(*frames, FrameCount(48));
                assert!(has_audio);
            }
            media => panic!("unexpected prepared media: {media:?}"),
        }
    }

    #[test]
    fn rejects_invalid_browser_video_probe_metadata() {
        let compiled = compiled("clipasm 1\nvideo(\"scene.mkv\")\n");
        let error = prepare(&compiled, &[asset("scene.mkv", "ab", r#"{"streams":[]}"#)])
            .expect_err("invalid video probe");

        assert_eq!(error.code, "E_SOURCE_CONTRACT");
        assert!(error.message.contains("exactly one video stream"));
    }

    #[test]
    fn prepares_image_graph_from_validated_probe_metadata() {
        let compiled = compiled("clipasm 1\nimage(\"still.png\", 1s)\n");
        let plan =
            prepare(&compiled, &[asset("still.png", "ab", IMAGE_PROBE)]).expect("browser plan");
        assert_eq!(plan.nodes.len(), 1);
        assert_eq!(
            plan.nodes[plan.result.get() as usize].value_type(),
            ValueType::Video
        );
        assert!(!plan.semantic_hash().is_empty());
    }

    #[test]
    fn rejects_animated_or_audio_bearing_browser_images() {
        let compiled = compiled("clipasm 1\nimage(\"still.png\", 1s)\n");
        let animated = asset(
            "still.png",
            "ab",
            r#"{"streams":[{"codec_type":"video","nb_read_frames":"2"}]}"#,
        );
        let error = prepare(&compiled, &[animated]).expect_err("animated image");

        assert_eq!(error.code, "E_SOURCE_CONTRACT");
        assert!(error.message.contains("one decoded frame"));
    }

    #[test]
    fn rejects_excessive_browser_probe_metadata() {
        let compiled = compiled("clipasm 1\nimage(\"still.png\", 1s)\n");
        let oversized = BrowserAsset::new(
            "still.png",
            "ab".repeat(32),
            "x".repeat(MAX_PROBE_JSON_BYTES + 1),
        );
        let error = prepare(&compiled, &[oversized]).expect_err("oversized probe");

        assert_eq!(error.code, "E_BROWSER_ASSET_FACTS");
        assert!(error.message.contains("exceeds"));
    }

    #[test]
    fn rejects_missing_duplicate_and_traversing_assets() {
        let program = compiled("clipasm 1\nimage(\"still.png\", 1s)\n");
        assert_eq!(
            prepare(&program, &[]).expect_err("missing").code,
            "E_BROWSER_MISSING_ASSET"
        );
        let duplicate = [
            asset("still.png", "ab", IMAGE_PROBE),
            asset("./still.png", "cd", IMAGE_PROBE),
        ];
        assert_eq!(
            prepare(&program, &duplicate).expect_err("duplicate").code,
            "E_BROWSER_DUPLICATE_ASSET"
        );
        let traversal = compiled("clipasm 1\nimage(\"../still.png\", 1s)\n");
        assert_eq!(
            required_assets(&traversal).expect_err("traversal").code,
            "E_BROWSER_ASSET_PATH"
        );
    }

    #[test]
    fn treats_slashes_and_backslashes_as_virtual_path_separators() {
        let span = SourceSpan::file_start("playground.clipasm");

        assert_eq!(
            normalize_relative(Path::new("a\\..\\secret.png"), &span).expect("normalized"),
            "secret.png"
        );
        assert_eq!(
            normalize_relative(Path::new("assets\\scene\\..\\still.png"), &span)
                .expect("normalized"),
            "assets/still.png"
        );

        for path in [
            "..\\secret.png",
            "C:\\secret.png",
            "C:/secret.png",
            "C:secret.png",
            "\\\\server\\share\\secret.png",
            "\\secret.png",
        ] {
            assert_eq!(
                normalize_relative(Path::new(path), &span)
                    .expect_err("unsafe browser path")
                    .code,
                "E_BROWSER_ASSET_PATH",
                "{path}"
            );
        }
    }

    #[test]
    fn rejects_external_and_audio_file_sources_explicitly() {
        let source =
            "clipasm 1\nset_audio(audio=audio(\"sound.wav\"), video=image(\"still.png\", 1s))\n";
        let error = required_assets(&compiled(source)).expect_err("unsupported source");

        assert_eq!(error.code, "E_BROWSER_RENDER_UNSUPPORTED");
        assert!(error.message.contains("audio-file sources"));
    }
}
