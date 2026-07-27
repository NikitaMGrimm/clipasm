use std::path::{Path, PathBuf};

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::model::{NodeId, ValueType};
use crate::preflight::tools::verify_external_tool;
use crate::preflight::{
    PreparedAudioKind, PreparedExternalArgument, PreparedExternalParameterValue, PreparedNode,
    PreparedNodeMedia, PreparedPlan, PreparedVideoKind, verify_prepared_asset,
};
use crate::source::SourceSpan;

use super::artifact::verify_prepared_artifact;
use super::cache;
use super::execute::Executor;
use super::lock::{FileLock, sibling_lock_path};

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
    pub(super) fn build(plan: &PreparedPlan, cache_directory: &Path) -> Result<Self> {
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

    pub(super) fn execute(self, plan: &PreparedPlan) -> Result<ExecutionResult> {
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
                    let cache_hit = cache_is_valid(plan, node, &action.artifact);
                    verify_node_resources(node)?;
                    if cache_hit {
                        cache_hits += 1;
                    } else {
                        cache::remove_entry(&action.artifact)?;
                        let staged = executor.render_node(node, &artifacts, &action.artifact)?;
                        verify_prepared_artifact(
                            plan.ffprobe().executable(),
                            staged.path(),
                            node,
                            *plan.audio(),
                            plan.render_policy().working_pixel_format(),
                        )?;
                        staged.commit(node.fingerprint())?;
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
    cache::verify_entry(artifact, node.fingerprint()).is_ok()
        && verify_prepared_artifact(
            plan.ffprobe().executable(),
            artifact,
            node,
            *plan.audio(),
            plan.render_policy().working_pixel_format(),
        )
        .is_ok()
}

fn verify_node_resources(node: &PreparedNode) -> Result<()> {
    match node.media() {
        PreparedNodeMedia::Video {
            kind:
                PreparedVideoKind::ImageVideo { asset, .. }
                | PreparedVideoKind::VideoSource { asset, .. },
            ..
        }
        | PreparedNodeMedia::Audio {
            kind: PreparedAudioKind::AudioSource { asset },
            ..
        } => verify_prepared_asset(asset, &node.origin().span),
        PreparedNodeMedia::Video {
            kind:
                PreparedVideoKind::ExternalVideo {
                    executable,
                    arguments,
                    parameters,
                    ..
                },
            ..
        } => {
            verify_external_tool(executable, &node.origin().span)?;
            for asset in arguments.iter().filter_map(|argument| match argument {
                PreparedExternalArgument::File(asset) => Some(asset),
                PreparedExternalArgument::Text(_) => None,
            }) {
                verify_prepared_asset(asset, &node.origin().span)?;
            }
            for asset in parameters.values().filter_map(|value| match value {
                PreparedExternalParameterValue::File(asset) => Some(asset),
                PreparedExternalParameterValue::Integer(_)
                | PreparedExternalParameterValue::Keyword(_) => None,
            }) {
                verify_prepared_asset(asset, &node.origin().span)?;
            }
            Ok(())
        }
        PreparedNodeMedia::Video { .. } | PreparedNodeMedia::Audio { .. } => Ok(()),
    }
}
