use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::model::{NodeId, ValueType};
use crate::preflight::tools::verify_external_tool;
use crate::preflight::{PreparedNode, PreparedPlan, PreparedResource, verify_prepared_asset};
use crate::source::SourceSpan;

use super::artifact::verify_prepared_artifact;
use super::cache;
use super::execute::Executor;
use super::lock::{FileLock, sibling_lock_path};

pub(super) struct ProtectedResources {
    paths: HashMap<PathBuf, &'static str>,
}

impl ProtectedResources {
    pub(super) fn new(plan: &PreparedPlan) -> Self {
        let mut paths = HashMap::from([
            (
                plan.ffmpeg().executable().to_path_buf(),
                "FFmpeg executable",
            ),
            (
                plan.ffprobe().executable().to_path_buf(),
                "FFprobe executable",
            ),
        ]);
        for source in plan.source_files() {
            if let Some(path) = source.filesystem_path() {
                paths.entry(path.to_path_buf()).or_insert("source program");
            }
        }
        for node in plan.nodes() {
            let result: std::result::Result<(), std::convert::Infallible> = node
                .try_visit_resources(|resource| {
                    paths
                        .entry(resource.path().to_path_buf())
                        .or_insert(resource.role());
                    Ok(())
                });
            match result {
                Ok(()) => {}
                Err(never) => match never {},
            }
        }
        Self { paths }
    }

    pub(super) fn reject_existing_path(
        &self,
        path: &Path,
        role: &str,
        diagnostic: BuiltinDiagnostic,
    ) -> Result<()> {
        let identity = match fs::canonicalize(path) {
            Ok(identity) => identity,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(Diagnostic::builtin(
                    diagnostic,
                    format!(
                        "could not resolve {role} path `{}` before use: {error}",
                        path.display()
                    ),
                    SourceSpan::file_start(path),
                ));
            }
        };
        let Some(resource_role) = self.paths.get(&identity) else {
            return Ok(());
        };
        Err(Diagnostic::builtin(
            diagnostic,
            format!(
                "{role} path `{}` collides with {resource_role} path `{}`",
                path.display(),
                identity.display()
            ),
            SourceSpan::file_start(path),
        ))
    }

    fn reject_cache_entry(&self, artifact: &Path) -> Result<()> {
        self.reject_existing_path(artifact, "cache artifact", BuiltinDiagnostic::CacheIo)?;
        self.reject_existing_path(
            &cache::metadata_path(artifact),
            "cache metadata",
            BuiltinDiagnostic::CacheIo,
        )?;
        self.reject_existing_path(
            &sibling_lock_path(artifact, "cache"),
            "cache lock",
            BuiltinDiagnostic::CacheLock,
        )
    }
}

pub(super) struct ExecutionPlan {
    actions: Vec<ExecutionAction>,
    slot_count: usize,
}

struct ExecutionAction {
    node: NodeId,
    artifact: PathBuf,
    disposition: CacheDisposition,
}

#[derive(Clone, Copy)]
enum CacheDisposition {
    ReuseVerified,
    Render,
}

pub(super) struct ExecutionResult {
    artifacts: Vec<Option<PathBuf>>,
    cache_hits: usize,
    cache_misses: usize,
}

impl ExecutionPlan {
    pub(super) fn build(
        plan: &PreparedPlan,
        cache_directory: &Path,
        protected: &ProtectedResources,
    ) -> Result<Self> {
        validate_prepared_order(plan)?;
        let mut seen = vec![false; plan.nodes().len()];
        let mut pending = vec![plan.result()];
        let mut actions = Vec::new();

        while let Some(id) = pending.pop() {
            let index = id.get() as usize;
            if seen[index] {
                continue;
            }
            seen[index] = true;
            let node = &plan.nodes()[index];
            let artifact = cache_artifact_path(plan, cache_directory, node);
            protected.reject_cache_entry(&artifact)?;
            let disposition = if probe_cache(plan, node, &artifact)? {
                verify_node_resources(node)?;
                CacheDisposition::ReuseVerified
            } else {
                node.visit_inputs(|input| pending.push(input));
                CacheDisposition::Render
            };
            actions.push(ExecutionAction {
                node: id,
                artifact,
                disposition,
            });
        }

        actions.sort_unstable_by_key(|action| action.node.get());
        Ok(Self {
            actions,
            slot_count: plan.nodes().len(),
        })
    }

    pub(super) fn execute(
        self,
        plan: &PreparedPlan,
        protected: &ProtectedResources,
    ) -> Result<ExecutionResult> {
        let executor = Executor::new(plan);
        let mut artifacts = vec![None; self.slot_count];
        let mut cache_hits = 0;
        let mut cache_misses = 0;

        for action in self.actions {
            let index = action.node.get() as usize;
            let node = &plan.nodes()[index];
            match action.disposition {
                CacheDisposition::ReuseVerified => {
                    cache_hits += 1;
                }
                CacheDisposition::Render => {
                    let lock_path = sibling_lock_path(&action.artifact, "cache");
                    let _lock = FileLock::acquire(
                        &lock_path,
                        BuiltinDiagnostic::CacheLock,
                        "cache artifact",
                        &node.origin().span,
                    )?;
                    protected.reject_cache_entry(&action.artifact)?;
                    let cache_hit = cache_is_valid(plan, node, &action.artifact);
                    verify_node_resources(node)?;
                    if cache_hit {
                        cache_hits += 1;
                    } else {
                        cache::remove_entry(&action.artifact)?;
                        let staged = executor.render_node(node, &artifacts, &action.artifact)?;
                        let verified = staged.verify(|path| {
                            verify_prepared_artifact(
                                plan.ffprobe().executable(),
                                path,
                                &node.artifact_contract(),
                                plan.render_policy().working_pixel_format(),
                            )
                        })?;
                        verified.commit(cache_identity(plan, node))?;
                        cache_misses += 1;
                    }
                }
            }
            artifacts[index] = Some(action.artifact);
        }

        Ok(ExecutionResult {
            artifacts,
            cache_hits,
            cache_misses,
        })
    }
}

impl ExecutionResult {
    pub(super) fn artifact(&self, id: NodeId, span: &SourceSpan) -> Result<&Path> {
        self.artifacts
            .get(id.get() as usize)
            .and_then(Option::as_deref)
            .ok_or_else(|| {
                Diagnostic::builtin(
                    BuiltinDiagnostic::InvalidPlan,
                    "prepared result does not identify an executed artifact",
                    span.clone(),
                )
            })
    }

    pub(super) const fn cache_hits(&self) -> usize {
        self.cache_hits
    }

    pub(super) const fn cache_misses(&self) -> usize {
        self.cache_misses
    }
}

fn validate_prepared_order(plan: &PreparedPlan) -> Result<()> {
    for (index, node) in plan.nodes().iter().enumerate() {
        if node.id().get() as usize != index {
            return Err(Diagnostic::builtin(
                BuiltinDiagnostic::InvalidPlan,
                "prepared nodes are not in stable topological order",
                node.origin().span.clone(),
            ));
        }
        let mut invalid_input = None;
        node.visit_inputs(|input| {
            if input.get() as usize >= index {
                invalid_input = Some(input);
            }
        });
        if let Some(input) = invalid_input {
            return Err(Diagnostic::builtin(
                BuiltinDiagnostic::InvalidPlan,
                format!(
                    "prepared node {} has non-topological input {}",
                    node.id().get(),
                    input.get()
                ),
                node.origin().span.clone(),
            ));
        }
    }

    let result = plan
        .nodes()
        .get(plan.result().get() as usize)
        .ok_or_else(|| {
            Diagnostic::builtin(
                BuiltinDiagnostic::InvalidPlan,
                "prepared result does not identify an existing node",
                SourceSpan::source_start(plan.entrypoint_source().clone()),
            )
        })?;
    if result.value_type() != ValueType::Video {
        return Err(Diagnostic::builtin(
            BuiltinDiagnostic::InvalidPlan,
            "prepared result is Audio, but rendering requires Video",
            result.origin().span.clone(),
        ));
    }
    Ok(())
}

fn probe_cache(plan: &PreparedPlan, node: &PreparedNode, artifact: &Path) -> Result<bool> {
    let lock_path = sibling_lock_path(artifact, "cache");
    let _lock = FileLock::acquire(
        &lock_path,
        BuiltinDiagnostic::CacheLock,
        "cache artifact",
        &node.origin().span,
    )?;
    Ok(cache_is_valid(plan, node, artifact))
}

fn cache_artifact_path(
    plan: &PreparedPlan,
    cache_directory: &Path,
    node: &PreparedNode,
) -> PathBuf {
    let extension = match node.value_type() {
        ValueType::Video => plan.render_policy().working_video_extension(),
        ValueType::Audio => plan.render_policy().working_audio_extension(),
    };
    cache_directory.join(format!("{}.{}", node.fingerprint(), extension))
}

fn cache_is_valid(plan: &PreparedPlan, node: &PreparedNode, artifact: &Path) -> bool {
    cache::verify_entry(artifact, cache_identity(plan, node)).is_ok()
}

fn cache_identity<'a>(
    plan: &'a PreparedPlan,
    node: &'a PreparedNode,
) -> cache::CacheEntryIdentity<'a> {
    cache::CacheEntryIdentity::new(plan.execution_namespace(), node.fingerprint())
}

fn verify_node_resources(node: &PreparedNode) -> Result<()> {
    node.try_visit_resources(|resource| match resource {
        PreparedResource::Asset { asset, .. } => verify_prepared_asset(asset, &node.origin().span),
        PreparedResource::ExternalExecutable(executable) => {
            verify_external_tool(executable, &node.origin().span)
        }
    })
}
