use std::fs;
use std::path::{Path, PathBuf};

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::preflight::PreparedPlan;
use crate::source::SourceSpan;

use super::execution_plan::{ArtifactStorage, ExecutionPlan, ProtectedResources};
use super::lock::{FileLock, sibling_lock_path};
use super::publication::PublicationTransaction;
use super::{execute, manifest};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
/// Retention policy for working render artifacts.
pub enum CacheMode {
    /// Reuse verified artifacts and retain newly rendered artifacts for future renders.
    #[default]
    Persistent,
    /// Render into private temporary storage without reading or writing persistent cache entries.
    None,
}

impl CacheMode {
    /// Return the stable configuration and manifest label for this mode.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Persistent => "persistent",
            Self::None => "none",
        }
    }
}

#[derive(Clone, Debug)]
#[non_exhaustive]
/// Paths and cache statistics from a completed render.
pub struct RenderReport {
    /// Published MP4 output path.
    output: PathBuf,
    /// Published JSON manifest path.
    manifest: PathBuf,
    /// Number of verified cache artifacts actually reused.
    ///
    /// Nodes pruned behind a downstream cache hit are not counted.
    cache_hits: usize,
    /// Number of prepared-node artifacts actually rendered.
    ///
    /// Nodes pruned behind a downstream cache hit are not counted.
    cache_misses: usize,
    /// Cache policy used for the render.
    cache_mode: CacheMode,
}

impl RenderReport {
    /// Return the published MP4 output path.
    #[must_use]
    pub fn output(&self) -> &Path {
        &self.output
    }

    /// Return the published JSON manifest path.
    #[must_use]
    pub fn manifest(&self) -> &Path {
        &self.manifest
    }

    /// Return the number of verified working artifacts actually reused.
    #[must_use]
    pub const fn cache_hits(&self) -> usize {
        self.cache_hits
    }

    /// Return the number of working artifacts rendered during this run.
    #[must_use]
    pub const fn cache_misses(&self) -> usize {
        self.cache_misses
    }

    /// Return the cache policy used for this render.
    #[must_use]
    pub const fn cache_mode(&self) -> CacheMode {
        self.cache_mode
    }
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
    let source_directory = plan.entrypoint_source().base_directory().ok_or_else(|| {
        Diagnostic::builtin(
            BuiltinDiagnostic::InvalidPlan,
            "prepared plan has no entrypoint base directory",
            SourceSpan::source_start(plan.entrypoint_source().clone()),
        )
    })?;
    render_with_cache_root(plan, &source_directory.join(".clipasm").join("cache"))
}

/// Render an invariant-protected prepared plan with an explicit persistent cache root.
///
/// `ClipAsm` stores execution-namespace directories beneath `cache_root`.
///
/// # Errors
///
/// Returns the same diagnostics as [`render`].
pub fn render_with_cache_root(plan: &PreparedPlan, cache_root: &Path) -> Result<RenderReport> {
    plan.verify_tool_identities()?;
    create_output_directory(plan)?;
    let storage = ArtifactStorage::persistent(plan, cache_root)?;
    render_with_storage(plan, &storage, CacheMode::Persistent)
}

/// Render an invariant-protected prepared plan without persistent cache access.
///
/// Working artifacts use private temporary storage and are removed after their
/// final consumer. Existing persistent cache entries are neither read nor
/// changed.
///
/// # Errors
///
/// Returns the same diagnostics as [`render`].
pub fn render_without_cache(plan: &PreparedPlan) -> Result<RenderReport> {
    plan.verify_tool_identities()?;
    create_output_directory(plan)?;
    let storage = ArtifactStorage::transient(plan.output())?;
    render_with_storage(plan, &storage, CacheMode::None)
}

fn render_with_storage(
    plan: &PreparedPlan,
    storage: &ArtifactStorage,
    cache_mode: CacheMode,
) -> Result<RenderReport> {
    let protected = ProtectedResources::new(plan);
    let execution =
        ExecutionPlan::build(plan, storage, &protected)?.execute(plan, storage, &protected)?;
    let result_node = &plan.nodes()[plan.result().get() as usize];
    let result_artifact = execution.artifact(plan.result(), &result_node.origin().span)?;

    let publication_lock_path = sibling_lock_path(plan.output(), "publication");
    protected.reject_existing_path(
        &publication_lock_path,
        "publication lock",
        BuiltinDiagnostic::PublicationLock,
    )?;
    let _publication_lock = FileLock::acquire(
        &publication_lock_path,
        BuiltinDiagnostic::PublicationLock,
        "publication",
        &SourceSpan::file_start(plan.output()),
    )?;
    let publication = PublicationTransaction::new(plan.output(), plan.manifest())?;
    let executor = execute::Executor::new(plan);
    executor.stage_export(result_artifact, publication.staged_output(), result_node)?;

    let manifest_json = manifest::serialize(
        plan,
        result_node,
        cache_mode,
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
        cache_mode,
    })
}

fn create_output_directory(plan: &PreparedPlan) -> Result<()> {
    let Some(parent) = plan.output().parent() else {
        return Ok(());
    };
    fs::create_dir_all(parent).map_err(|error| {
        Diagnostic::builtin(
            BuiltinDiagnostic::OutputIo,
            format!(
                "could not create output directory `{}`: {error}",
                parent.display()
            ),
            SourceSpan::file_start(plan.output()),
        )
    })
}
