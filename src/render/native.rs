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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
/// Policy for materializing working artifacts between renderer primitives.
pub enum MaterializationMode {
    /// Materialize every reachable prepared node.
    #[default]
    All,
    /// Fuse compatible `FFmpeg` graph regions without duplicating physical streams.
    Fused,
}

impl MaterializationMode {
    /// Return the stable configuration and manifest label for this mode.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Fused => "fused",
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
/// Execution policies for one native render.
pub struct RenderOptions {
    cache_mode: CacheMode,
    materialization_mode: MaterializationMode,
    cache_root: Option<PathBuf>,
}

impl RenderOptions {
    /// Create render options with explicit cache and materialization policies.
    #[must_use]
    pub const fn new(cache_mode: CacheMode, materialization_mode: MaterializationMode) -> Self {
        Self {
            cache_mode,
            materialization_mode,
            cache_root: None,
        }
    }

    /// Select persistent caching with an explicit cache root.
    #[must_use]
    pub fn with_cache_root(mut self, cache_root: impl Into<PathBuf>) -> Self {
        self.cache_mode = CacheMode::Persistent;
        self.cache_root = Some(cache_root.into());
        self
    }

    /// Return the selected cache policy.
    #[must_use]
    pub const fn cache_mode(&self) -> CacheMode {
        self.cache_mode
    }

    /// Return the selected intermediate-materialization policy.
    #[must_use]
    pub const fn materialization_mode(&self) -> MaterializationMode {
        self.materialization_mode
    }
}

#[derive(Clone, Debug)]
#[non_exhaustive]
/// Paths and execution statistics from a completed render.
pub struct RenderReport {
    /// Published MP4 output path.
    output: PathBuf,
    /// Published JSON manifest path.
    manifest: PathBuf,
    /// Number of verified working artifacts actually reused.
    ///
    /// Nodes pruned behind a downstream cache hit are not counted.
    reused_artifacts: usize,
    /// Number of execution jobs actually rendered.
    ///
    /// Nodes pruned behind a downstream cache hit are not counted.
    rendered_jobs: usize,
    /// Cache policy used for the render.
    cache_mode: CacheMode,
    /// Intermediate-materialization policy used for the render.
    materialization_mode: MaterializationMode,
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
    pub const fn reused_artifacts(&self) -> usize {
        self.reused_artifacts
    }

    /// Return the number of execution jobs rendered during this run.
    #[must_use]
    pub const fn rendered_jobs(&self) -> usize {
        self.rendered_jobs
    }

    /// Return the cache policy used for this render.
    #[must_use]
    pub const fn cache_mode(&self) -> CacheMode {
        self.cache_mode
    }

    /// Return the intermediate-materialization policy used for this render.
    #[must_use]
    pub const fn materialization_mode(&self) -> MaterializationMode {
        self.materialization_mode
    }
}

/// Render an invariant-protected prepared plan and publish its MP4 and manifest.
///
/// Working intermediates use lossless FFV1 with explicitly tagged 10-bit,
/// non-subsampled BT.709 Video. The delivery profile is explicitly tagged
/// 8-bit BT.709 H.264 MP4 with `yuv420p`, square pixels, and AAC when the result
/// carries meaningful audio.
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
    render_with_options(plan, &RenderOptions::default())
}

/// Render an invariant-protected prepared plan with an explicit persistent cache root.
///
/// `ClipAsm` stores execution-namespace directories beneath `cache_root`.
///
/// # Errors
///
/// Returns the same diagnostics as [`render`].
pub fn render_with_cache_root(plan: &PreparedPlan, cache_root: &Path) -> Result<RenderReport> {
    render_with_options(plan, &RenderOptions::default().with_cache_root(cache_root))
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
    render_with_options(
        plan,
        &RenderOptions::new(CacheMode::None, MaterializationMode::All),
    )
}

/// Render an invariant-protected prepared plan with explicit execution policies.
///
/// # Errors
///
/// Returns the same diagnostics as [`render`].
pub fn render_with_options(plan: &PreparedPlan, options: &RenderOptions) -> Result<RenderReport> {
    plan.verify_tool_identities()?;
    create_output_directory(plan)?;
    let default_cache_root;
    let storage = match options.cache_mode {
        CacheMode::Persistent => {
            let cache_root = if let Some(cache_root) = options.cache_root.as_deref() {
                cache_root
            } else {
                let source_directory =
                    plan.entrypoint_source().base_directory().ok_or_else(|| {
                        Diagnostic::builtin(
                            BuiltinDiagnostic::InvalidPlan,
                            "prepared plan has no entrypoint base directory",
                            SourceSpan::source_start(plan.entrypoint_source().clone()),
                        )
                    })?;
                default_cache_root = source_directory.join(".clipasm").join("cache");
                &default_cache_root
            };
            ArtifactStorage::persistent(plan, cache_root)?
        }
        CacheMode::None => ArtifactStorage::transient(plan.output())?,
    };
    render_with_storage(
        plan,
        &storage,
        options.cache_mode,
        options.materialization_mode,
    )
}

fn render_with_storage(
    plan: &PreparedPlan,
    storage: &ArtifactStorage,
    cache_mode: CacheMode,
    materialization_mode: MaterializationMode,
) -> Result<RenderReport> {
    let protected = ProtectedResources::new(plan);
    let execution = ExecutionPlan::build(plan, storage, &protected, materialization_mode)?
        .execute(plan, storage, &protected)?;
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
        materialization_mode,
        execution.reused_artifacts(),
        execution.rendered_jobs(),
    )?;
    publication.stage_manifest(&manifest_json)?;
    publication.commit()?;

    Ok(RenderReport {
        output: plan.output().to_path_buf(),
        manifest: plan.manifest().to_path_buf(),
        reused_artifacts: execution.reused_artifacts(),
        rendered_jobs: execution.rendered_jobs(),
        cache_mode,
        materialization_mode,
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
