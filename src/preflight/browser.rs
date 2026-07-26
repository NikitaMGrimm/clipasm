//! Browser preparation for virtual, in-memory media projects.
//!
//! This module performs no filesystem or process I/O. A browser host supplies
//! content hashes for the requested virtual assets, then executes the returned
//! render plan through its own `FFmpeg` runtime.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::compiler::CompiledProgram;
use crate::diagnostic::{Diagnostic, Result};
use crate::model::{AudioDomain, AudioSpec, FrameCount, NodeId, VideoSpec};
use crate::semantic::{SemanticNodeKind, SourceOrigin};
use crate::source::SourceSpan;

use super::identity::prepared_semantic_hash;
use super::lower::{PreflightLowerer, PreparationHost};
use super::tools::ExternalToolIdentity;
use super::{PreparedAsset, PreparedNode, RenderPolicy};

/// The media role of one virtual asset required for browser rendering.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserAssetKind {
    /// A still image used by the `image` program.
    Image,
}

/// One virtual asset required by the result-reachable semantic graph.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
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

/// One browser file bound to a normalized virtual path and SHA-256 digest.
#[derive(Clone, Debug, Deserialize)]
pub struct BrowserAsset {
    path: String,
    content_hash: String,
}

impl BrowserAsset {
    /// Construct one virtual asset fact supplied by a browser host.
    #[must_use]
    pub fn new(path: impl Into<String>, content_hash: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            content_hash: content_hash.into(),
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
/// Browser rendering currently accepts result-reachable `image` assets and
/// every native operation that can be prepared from them. Video files, Audio
/// files, and external programs return an explicit unsupported diagnostic.
///
/// # Errors
///
/// Returns a diagnostic when publication does not select exactly one Video,
/// a virtual path is unsafe or non-UTF-8, or the graph requires an unsupported
/// browser source.
pub fn required_assets(compiled: &CompiledProgram) -> Result<Vec<BrowserAssetRequest>> {
    let output = compiled.render_output()?;
    let order = crate::compiler::traversal::topological_order(
        compiled.nodes(),
        compiled.symbol_values(),
        [output],
    )?;
    let mut paths = BTreeSet::new();
    let mut requests = Vec::new();
    for value in order {
        let node = &compiled.nodes()[value.id().get() as usize];
        match node.kind() {
            SemanticNodeKind::ImageVideo { path, .. } => {
                let path = virtual_path(path, &node.origin().span)?;
                if paths.insert(path.clone()) {
                    requests.push(BrowserAssetRequest {
                        path,
                        kind: BrowserAssetKind::Image,
                    });
                }
            }
            SemanticNodeKind::VideoSource { .. } => {
                return Err(browser_unsupported(
                    "video-file sources are not yet supported in the browser",
                    &node.origin().span,
                ));
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
            | SemanticNodeKind::AudioRepeat { .. }
            | SemanticNodeKind::Zoom { .. }
            | SemanticNodeKind::Wobble { .. }
            | SemanticNodeKind::FlashJoin { .. }
            | SemanticNodeKind::Crossfade { .. }
            | SemanticNodeKind::Concat { .. }
            | SemanticNodeKind::AudioConcat { .. }
            | SemanticNodeKind::Slice { .. }
            | SemanticNodeKind::AudioSlice { .. }
            | SemanticNodeKind::ReplaceRange { .. }
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
        let Some(content_hash) = supplied.get(request.path()) else {
            return Err(Diagnostic::new(
                "E_BROWSER_MISSING_ASSET",
                format!("browser asset `{}` has not been supplied", request.path()),
                SourceSpan::file_start(request.path()),
            ));
        };
        prepared_assets.insert(
            PathBuf::from(request.path()),
            PreparedAsset::new(PathBuf::from(request.path()), content_hash.clone()),
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
    let order = crate::compiler::traversal::topological_order(
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

fn supplied_assets(assets: &[BrowserAsset]) -> Result<BTreeMap<String, String>> {
    let mut supplied = BTreeMap::new();
    for asset in assets {
        let span = SourceSpan::file_start(&asset.path);
        let path = normalize_relative(Path::new(&asset.path), &span)?;
        if asset.content_hash.len() != 64
            || !asset
                .content_hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(Diagnostic::new(
                "E_BROWSER_ASSET_HASH",
                format!("browser asset `{path}` does not have a valid SHA-256 digest"),
                span,
            ));
        }
        if supplied
            .insert(path.clone(), asset.content_hash.to_ascii_lowercase())
            .is_some()
        {
            return Err(Diagnostic::new(
                "E_BROWSER_DUPLICATE_ASSET",
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
    for component in Path::new(&portable).components() {
        match component {
            Component::CurDir => {}
            Component::Normal(value) => components.push(value),
            Component::ParentDir => {
                if components.pop().is_none() {
                    return Err(invalid_browser_path(path, span));
                }
            }
            Component::Prefix(_) | Component::RootDir => {
                return Err(invalid_browser_path(path, span));
            }
        }
    }
    if components.is_empty() {
        return Err(invalid_browser_path(path, span));
    }
    let mut normalized = PathBuf::new();
    for component in components {
        normalized.push(component);
    }
    normalized
        .to_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| invalid_browser_path(path, span))
}

fn invalid_browser_path(path: &Path, span: &SourceSpan) -> Diagnostic {
    Diagnostic::new(
        "E_BROWSER_ASSET_PATH",
        format!(
            "browser asset path `{}` must be a project-relative UTF-8 path without traversal",
            path.display()
        ),
        span.clone(),
    )
}

fn browser_unsupported(message: &str, span: &SourceSpan) -> Diagnostic {
    Diagnostic::new("E_BROWSER_RENDER_UNSUPPORTED", message, span.clone())
}

struct BrowserPreparationHost {
    assets: BTreeMap<PathBuf, PreparedAsset>,
}

impl PreparationHost for BrowserPreparationHost {
    fn prepare_image(&mut self, authored: &Path, origin: &SourceOrigin) -> Result<PreparedAsset> {
        let path = PathBuf::from(virtual_path(authored, &origin.span)?);
        self.assets.get(&path).cloned().ok_or_else(|| {
            Diagnostic::new(
                "E_BROWSER_MISSING_ASSET",
                format!("browser asset `{}` has not been supplied", path.display()),
                origin.span.clone(),
            )
        })
    }

    fn prepare_video(
        &mut self,
        _authored: &Path,
        _video: &VideoSpec,
        origin: &SourceOrigin,
    ) -> Result<(PreparedAsset, FrameCount, bool)> {
        Err(browser_unsupported(
            "video-file sources are not yet supported in the browser",
            &origin.span,
        ))
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

    fn compiled(source: &str) -> CompiledProgram {
        let package =
            crate::language::parse_str(Path::new("playground.clipasm"), source).expect("source");
        crate::compiler::compile(&package).expect("compiled")
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
    fn prepares_image_graph_from_supplied_hashes() {
        let compiled = compiled("clipasm 1\nimage(\"still.png\", 1s)\n");
        let plan = prepare(
            &compiled,
            &[BrowserAsset::new("still.png", "ab".repeat(32))],
        )
        .expect("browser plan");
        assert_eq!(plan.nodes.len(), 1);
        assert_eq!(
            plan.nodes[plan.result.get() as usize].value_type(),
            ValueType::Video
        );
        assert!(!plan.semantic_hash().is_empty());
    }

    #[test]
    fn rejects_missing_duplicate_and_traversing_assets() {
        let program = compiled("clipasm 1\nimage(\"still.png\", 1s)\n");
        assert_eq!(
            prepare(&program, &[]).expect_err("missing").code,
            "E_BROWSER_MISSING_ASSET"
        );
        let duplicate = [
            BrowserAsset::new("still.png", "ab".repeat(32)),
            BrowserAsset::new("./still.png", "cd".repeat(32)),
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
    fn rejects_external_and_media_file_sources_explicitly() {
        for (source, message) in [
            ("clipasm 1\nvideo(\"clip.mp4\")\n", "video-file sources"),
            (
                "clipasm 1\nset_audio(audio=audio(\"sound.wav\"), video=image(\"still.png\", 1s))\n",
                "audio-file sources",
            ),
        ] {
            let error = required_assets(&compiled(source)).expect_err("unsupported source");
            assert_eq!(error.code, "E_BROWSER_RENDER_UNSUPPORTED");
            assert!(error.message.contains(message));
        }
    }
}
