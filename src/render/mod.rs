//! Verified execution, caching, and rollback-capable publication of prepared plans.
//!
//! Rendering accepts only [`PreparedPlan`],
//! re-verifies source content, reuses compatible cached artifacts, executes
//! `FFmpeg` primitives, and publishes the MP4 and manifest as one in-process
//! transaction.

mod artifact;
mod execute;
mod lock;
mod publication;

use std::fs;
use std::path::PathBuf;

use serde::Serialize;

use crate::diagnostic::{Diagnostic, Result};
use crate::model::ValueType;
use crate::preflight::tools::verify_external_tool;
use crate::preflight::{PreparedNodeKind, PreparedPlan, verify_prepared_asset};
use crate::source::SourceSpan;
use artifact::verify_prepared_artifact;
use lock::{FileLock, sibling_lock_path};
use publication::PublicationTransaction;

#[derive(Clone, Debug, Serialize)]
/// Paths and cache statistics from a completed render.
pub struct RenderReport {
    /// Published MP4 output path.
    pub output: PathBuf,
    /// Published JSON manifest path.
    pub manifest: PathBuf,
    /// Number of prepared-node artifacts reused from verified cache entries.
    pub cache_hits: usize,
    /// Number of prepared-node artifacts rendered during this execution.
    pub cache_misses: usize,
}

#[derive(Serialize)]
struct Manifest<'a> {
    engine_version: &'static str,
    ffmpeg: &'a str,
    ffprobe: &'a str,
    cache_hits: usize,
    cache_misses: usize,
    plan: &'a PreparedPlan,
}

/// Render an invariant-protected prepared plan and publish its MP4 and manifest.
///
/// Working intermediates use lossless FFV1 with non-subsampled `yuv444p`.
/// The only delivery profile is H.264 MP4 with `yuv420p`, square pixels, and
/// no audio. This is the renderer's initial fixed color/media policy.
///
/// Both files are staged before either destination is changed. If publication
/// fails, `ClipAsm` attempts to restore both previously published files. Each
/// final rename is atomic, but the pair is not crash-atomic across process
/// termination or power loss.
///
/// # Errors
///
/// Returns a diagnostic for changed assets, rendering/cache failures, contract
/// violations, or publication failures.
#[allow(clippy::too_many_lines)]
pub fn render(plan: &PreparedPlan) -> Result<RenderReport> {
    plan.verify_tool_identities()?;
    let source_directory = plan.entrypoint_source().base_directory().ok_or_else(|| {
        Diagnostic::new(
            "E_INVALID_PLAN",
            "prepared plan has no entrypoint base directory",
            SourceSpan::source_start(plan.entrypoint_source().clone()),
        )
    })?;
    let cache_directory = source_directory
        .join(".clipasm")
        .join("cache")
        .join(plan.execution_namespace());
    fs::create_dir_all(&cache_directory).map_err(|error| {
        Diagnostic::new(
            "E_CACHE_IO",
            format!(
                "could not create cache directory `{}`: {error}",
                cache_directory.display()
            ),
            SourceSpan::source_start(plan.entrypoint_source().clone()),
        )
    })?;

    let executor = execute::Executor::new(plan);
    let mut artifacts = Vec::<PathBuf>::with_capacity(plan.nodes().len());
    let mut cache_hits = 0;
    let mut cache_misses = 0;
    for node in plan.nodes() {
        if node.id().get() as usize != artifacts.len() {
            return Err(Diagnostic::new(
                "E_INVALID_PLAN",
                "prepared nodes are not in stable topological order",
                node.origin().span.clone(),
            ));
        }
        let extension = match node.value_type() {
            ValueType::Video => "mkv",
            ValueType::Audio => "mka",
            #[cfg(test)]
            ValueType::Test => "bin",
        };
        let artifact = cache_directory.join(format!("{}.{}", node.fingerprint(), extension));
        match node.kind() {
            PreparedNodeKind::ImageVideo { asset, .. }
            | PreparedNodeKind::VideoSource { asset, .. }
            | PreparedNodeKind::AudioSource { asset } => {
                verify_prepared_asset(asset, &node.origin().span)?;
            }
            PreparedNodeKind::Slice { .. }
            | PreparedNodeKind::AudioSlice { .. }
            | PreparedNodeKind::Repeat { .. }
            | PreparedNodeKind::AudioRepeat { .. }
            | PreparedNodeKind::Zoom { .. }
            | PreparedNodeKind::Wobble { .. }
            | PreparedNodeKind::FlashJoin { .. }
            | PreparedNodeKind::Concat { .. }
            | PreparedNodeKind::AudioConcat { .. }
            | PreparedNodeKind::ExtractAudio { .. }
            | PreparedNodeKind::SetAudio { .. }
            | PreparedNodeKind::AudioOnBlack { .. } => {}
            PreparedNodeKind::ExternalVideo { executable, .. } => {
                verify_external_tool(executable, &node.origin().span)?;
            }
        }
        let lock_path = sibling_lock_path(&artifact, "cache");
        let _lock = FileLock::acquire(
            &lock_path,
            "E_CACHE_LOCK",
            "cache artifact",
            &node.origin().span,
        )?;
        let hit = artifact.is_file()
            && verify_prepared_artifact(
                plan.ffprobe().executable(),
                &artifact,
                node,
                plan.audio(),
                plan.media_policy().working_pixel_format(),
            )
            .is_ok();
        if hit {
            cache_hits += 1;
        } else {
            cache_misses += 1;
            if artifact.exists() {
                fs::remove_file(&artifact).map_err(|error| {
                    Diagnostic::new(
                        "E_CACHE_IO",
                        format!(
                            "could not replace invalid cache artifact `{}`: {error}",
                            artifact.display()
                        ),
                        node.origin().span.clone(),
                    )
                })?;
            }
            executor.render_node(node, &artifacts, &artifact)?;
            verify_prepared_artifact(
                plan.ffprobe().executable(),
                &artifact,
                node,
                plan.audio(),
                plan.media_policy().working_pixel_format(),
            )?;
        }
        artifacts.push(artifact);
    }

    let result_node = &plan.nodes()[plan.result().get() as usize];
    let result_artifact = artifacts.get(plan.result().get() as usize).ok_or_else(|| {
        Diagnostic::new(
            "E_INVALID_PLAN",
            "prepared result does not identify a primitive artifact",
            SourceSpan::source_start(plan.entrypoint_source().clone()),
        )
    })?;
    if let Some(parent) = plan.output().parent() {
        fs::create_dir_all(parent).map_err(|error| {
            Diagnostic::new(
                "E_OUTPUT_IO",
                format!(
                    "could not create output directory `{}`: {error}",
                    parent.display()
                ),
                SourceSpan::file_start(plan.output()),
            )
        })?;
    }

    let publication_lock_path = sibling_lock_path(plan.output(), "publication");
    let _publication_lock = FileLock::acquire(
        &publication_lock_path,
        "E_PUBLICATION_LOCK",
        "publication",
        &SourceSpan::file_start(plan.output()),
    )?;
    let publication = PublicationTransaction::new(plan.output(), plan.manifest());
    executor.stage_export(result_artifact, publication.staged_output(), result_node)?;

    let manifest = Manifest {
        engine_version: env!("CARGO_PKG_VERSION"),
        ffmpeg: plan.ffmpeg().version(),
        ffprobe: plan.ffprobe().version(),
        cache_hits,
        cache_misses,
        plan,
    };
    let manifest_json = serde_json::to_vec_pretty(&manifest).map_err(|error| {
        Diagnostic::new(
            "E_MANIFEST",
            format!("could not serialize render manifest: {error}"),
            SourceSpan::source_start(plan.entrypoint_source().clone()),
        )
    })?;
    publication.stage_manifest(&manifest_json)?;
    publication.commit()?;

    Ok(RenderReport {
        output: plan.output().to_path_buf(),
        manifest: plan.manifest().to_path_buf(),
        cache_hits,
        cache_misses,
    })
}
