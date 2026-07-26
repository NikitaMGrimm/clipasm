//! Verified execution, caching, and rollback-capable publication of prepared plans.
//!
//! Native rendering accepts only [`PreparedPlan`], re-verifies source content
//! reached by cache-aware execution, reuses compatible cached artifacts,
//! executes `FFmpeg` primitives, and publishes the MP4 and manifest as one
//! in-process transaction. The [`browser`] adapter serializes the same closed
//! recipes for an isolated WebAssembly host.

mod artifact;
pub mod browser;
mod cache;
mod execute;
mod execution_plan;
mod lock;
mod manifest;
mod publication;
mod staging;

use std::fs;
use std::path::PathBuf;

use serde::Serialize;

use crate::diagnostic::{Diagnostic, Result};
use crate::preflight::PreparedPlan;
use crate::source::SourceSpan;
use execution_plan::ExecutionPlan;
use lock::{FileLock, sibling_lock_path};
use publication::PublicationTransaction;

#[derive(Clone, Debug, Serialize)]
/// Paths and cache statistics from a completed render.
pub struct RenderReport {
    /// Published MP4 output path.
    pub output: PathBuf,
    /// Published JSON manifest path.
    pub manifest: PathBuf,
    /// Number of verified cache artifacts actually reused.
    ///
    /// Nodes pruned behind a downstream cache hit are not counted.
    pub cache_hits: usize,
    /// Number of prepared-node artifacts actually rendered.
    ///
    /// Nodes pruned behind a downstream cache hit are not counted.
    pub cache_misses: usize,
}

/// Render an invariant-protected prepared plan and publish its MP4 and manifest.
///
/// Working intermediates use lossless FFV1 with non-subsampled `yuv444p`.
/// The only delivery profile is H.264 MP4 with `yuv420p`, square pixels, and
/// AAC when the result carries meaningful audio. This is the renderer's initial
/// fixed color/media policy.
///
/// Both files are staged before either destination is changed. If publication
/// fails, `ClipAsm` attempts to restore both previously published files. Each
/// final rename is atomic, but the pair is not crash-atomic across process
/// termination or power loss.
///
/// # Errors
///
/// Returns a diagnostic for changed execution-frontier assets,
/// rendering/cache failures, contract violations, or publication failures.
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

    let execution = ExecutionPlan::build(plan, &cache_directory)?.execute(plan)?;
    let result_node = &plan.nodes()[plan.result().get() as usize];
    let result_artifact = execution.artifact(plan.result(), &result_node.origin().span)?;
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
    let publication = PublicationTransaction::new(plan.output(), plan.manifest())?;
    let executor = execute::Executor::new(plan);
    executor.stage_export(result_artifact, publication.staged_output(), result_node)?;

    let manifest_json = manifest::serialize(
        plan,
        result_node,
        execution.cache_hits(),
        execution.cache_misses(),
    )?;
    publication.stage_manifest(&manifest_json)?;
    publication.commit()?;

    Ok(RenderReport {
        output: plan.output().to_path_buf(),
        manifest: plan.manifest().to_path_buf(),
        cache_hits: execution.cache_hits(),
        cache_misses: execution.cache_misses(),
    })
}
