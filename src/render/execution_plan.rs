use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::model::{NodeId, ValueType};
use crate::preflight::tools::verify_external_tool;
use crate::preflight::{PreparedNode, PreparedPlan, PreparedResource, verify_prepared_asset};
use crate::source::SourceSpan;

use super::MaterializationMode;
use super::artifact::{verify_native_transient_artifact, verify_prepared_artifact};
use super::cache;
use super::execute::{ArtifactProducer, Executor};
use super::lock::{FileLock, sibling_lock_path};
use super::staging::StagingDirectory;

pub(super) enum ArtifactStorage {
    Persistent { directory: PathBuf },
    Transient { directory: StagingDirectory },
}

impl ArtifactStorage {
    pub(super) fn persistent(plan: &PreparedPlan, cache_root: &Path) -> Result<Self> {
        let directory = cache_root.join(plan.execution_namespace());
        fs::create_dir_all(&directory).map_err(|error| {
            Diagnostic::builtin(
                BuiltinDiagnostic::CacheIo,
                format!(
                    "could not create cache directory `{}`: {error}",
                    directory.display()
                ),
                SourceSpan::source_start(plan.entrypoint_source().clone()),
            )
        })?;
        Ok(Self::Persistent { directory })
    }

    pub(super) fn transient(output: &Path) -> Result<Self> {
        StagingDirectory::beside(output, "render", BuiltinDiagnostic::OutputIo)
            .map(|directory| Self::Transient { directory })
    }

    fn artifact_path(&self, plan: &PreparedPlan, node: &PreparedNode) -> PathBuf {
        let extension = match node.value_type() {
            ValueType::Video => plan.render_policy().working_video_extension(),
            ValueType::Audio => plan.render_policy().working_audio_extension(),
        };
        match self {
            Self::Persistent { directory } => {
                directory.join(format!("{}.{}", node.fingerprint(), extension))
            }
            Self::Transient { directory } => {
                directory.path(&format!("node-{}.{}", node.id().get(), extension))
            }
        }
    }

    const fn is_persistent(&self) -> bool {
        matches!(self, Self::Persistent { .. })
    }
}

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
        for path in plan.source_paths() {
            paths.entry(path.clone()).or_insert("source program");
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
    remaining_consumers: Option<Vec<usize>>,
}

struct ExecutionAction {
    node: NodeId,
    artifact: PathBuf,
    kind: ExecutionActionKind,
    frontier: Vec<NodeId>,
}

enum ExecutionActionKind {
    ReuseVerified,
    Render { region: Vec<NodeId> },
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum NodeState {
    Unreached,
    ReuseVerified,
    Render,
}

pub(super) struct ExecutionResult {
    artifacts: Vec<Option<PathBuf>>,
    reused_artifacts: usize,
    rendered_jobs: usize,
}

impl ExecutionPlan {
    pub(super) fn build(
        plan: &PreparedPlan,
        storage: &ArtifactStorage,
        protected: &ProtectedResources,
        materialization: MaterializationMode,
    ) -> Result<Self> {
        validate_prepared_order(plan)?;
        let mut states = vec![NodeState::Unreached; plan.nodes().len()];
        let mut pending = vec![plan.result()];

        while let Some(id) = pending.pop() {
            let index = id.get() as usize;
            if states[index] != NodeState::Unreached {
                continue;
            }
            let node = &plan.nodes()[index];
            let artifact = storage.artifact_path(plan, node);
            if storage.is_persistent() {
                protected.reject_cache_entry(&artifact)?;
            }
            states[index] = if storage.is_persistent() && probe_cache(plan, node, &artifact)? {
                verify_node_resources(node)?;
                NodeState::ReuseVerified
            } else {
                node.visit_inputs(|input| pending.push(input));
                NodeState::Render
            };
        }

        let fused_regions = (materialization == MaterializationMode::Fused)
            .then(|| plan_fused_regions(plan, &states));
        let mut actions = Vec::new();
        for node in plan.nodes() {
            let id = node.id();
            match states[id.get() as usize] {
                NodeState::Unreached => {}
                NodeState::ReuseVerified => actions.push(ExecutionAction {
                    node: id,
                    artifact: storage.artifact_path(plan, node),
                    kind: ExecutionActionKind::ReuseVerified,
                    frontier: Vec::new(),
                }),
                NodeState::Render => {
                    let region = match materialization {
                        MaterializationMode::All => vec![id],
                        MaterializationMode::Fused => {
                            let regions = fused_regions
                                .as_ref()
                                .expect("fused regions were planned for fused materialization");
                            let region = &regions[id.get() as usize];
                            if region.is_empty() {
                                continue;
                            }
                            region.clone()
                        }
                    };
                    let frontier = region_frontier(plan, &region);
                    actions.push(ExecutionAction {
                        node: id,
                        artifact: storage.artifact_path(plan, node),
                        kind: ExecutionActionKind::Render { region },
                        frontier,
                    });
                }
            }
        }
        actions.sort_unstable_by_key(|action| action.node.get());
        let remaining_consumers = (!storage.is_persistent()).then(|| {
            let mut counts = vec![0_usize; plan.nodes().len()];
            for action in &actions {
                for input in &action.frontier {
                    let input = *input;
                    counts[input.get() as usize] = counts[input.get() as usize]
                        .checked_add(1)
                        .expect("prepared graph consumer count fits in usize");
                }
            }
            counts
        });
        Ok(Self {
            actions,
            slot_count: plan.nodes().len(),
            remaining_consumers,
        })
    }

    pub(super) fn execute(
        self,
        plan: &PreparedPlan,
        storage: &ArtifactStorage,
        protected: &ProtectedResources,
    ) -> Result<ExecutionResult> {
        let executor = Executor::new(plan);
        let mut artifacts = vec![None; self.slot_count];
        let mut reused_artifacts = 0;
        let mut rendered_jobs = 0;

        let mut remaining_consumers = self.remaining_consumers;
        for action in self.actions {
            let index = action.node.get() as usize;
            let node = &plan.nodes()[index];
            match &action.kind {
                ExecutionActionKind::ReuseVerified => {
                    reused_artifacts += 1;
                }
                ExecutionActionKind::Render { region } => {
                    let reused = match storage {
                        ArtifactStorage::Persistent { .. } => render_persistent_region(
                            plan,
                            protected,
                            &executor,
                            node,
                            region,
                            &artifacts,
                            &action.artifact,
                        )?,
                        ArtifactStorage::Transient { .. } => {
                            render_transient_region(
                                plan,
                                &executor,
                                node,
                                region,
                                &artifacts,
                                &action.artifact,
                            )?;
                            false
                        }
                    };
                    if reused {
                        reused_artifacts += 1;
                    } else {
                        rendered_jobs += 1;
                    }
                }
            }
            artifacts[index] = Some(action.artifact);
            if let Some(remaining_consumers) = &mut remaining_consumers {
                release_consumed_artifacts(
                    node,
                    &action.frontier,
                    remaining_consumers,
                    &mut artifacts,
                )?;
            }
        }

        Ok(ExecutionResult {
            artifacts,
            reused_artifacts,
            rendered_jobs,
        })
    }
}

fn render_persistent_region(
    plan: &PreparedPlan,
    protected: &ProtectedResources,
    executor: &Executor<'_>,
    node: &PreparedNode,
    region: &[NodeId],
    artifacts: &[Option<PathBuf>],
    artifact: &Path,
) -> Result<bool> {
    let lock_path = sibling_lock_path(artifact, "cache");
    let _lock = FileLock::acquire(
        &lock_path,
        BuiltinDiagnostic::CacheLock,
        "cache artifact",
        &node.origin().span,
    )?;
    protected.reject_cache_entry(artifact)?;
    let cache_hit = cache_is_valid(plan, node, artifact);
    if cache_hit {
        verify_node_resources(node)?;
        return Ok(true);
    }
    cache::remove_entry(artifact)?;
    verify_region_resources(plan, region)?;
    let staged = executor.stage_cache_region(node, region, artifacts, artifact)?;
    let verified = staged.verify(|path| verify_node_artifact(plan, node, path))?;
    verified.commit(cache_identity(plan, node))?;
    Ok(false)
}

fn render_transient_region(
    plan: &PreparedPlan,
    executor: &Executor<'_>,
    node: &PreparedNode,
    region: &[NodeId],
    artifacts: &[Option<PathBuf>],
    artifact: &Path,
) -> Result<()> {
    verify_region_resources(plan, region)?;
    let producer = executor.render_region_to(node, region, artifacts, artifact)?;
    match producer {
        // A successful closed native recipe already fixes the finite prepared
        // domain. Keep structural evidence here without decoding the temporary
        // a second time solely to recount it.
        ArtifactProducer::NativeFfmpeg => verify_native_transient_artifact(
            plan.ffprobe().executable(),
            artifact,
            &node.artifact_contract(),
            plan.render_policy().working_video_encoding(),
            plan.render_policy().working_audio_encoding(),
        ),
        ArtifactProducer::ExternalProgram => verify_node_artifact(plan, node, artifact),
    }
}

fn verify_node_artifact(plan: &PreparedPlan, node: &PreparedNode, path: &Path) -> Result<()> {
    verify_prepared_artifact(
        plan.ffprobe().executable(),
        path,
        &node.artifact_contract(),
        plan.render_policy().working_video_encoding(),
        plan.render_policy().working_audio_encoding(),
    )
}

fn release_consumed_artifacts(
    node: &PreparedNode,
    frontier: &[NodeId],
    remaining_consumers: &mut [usize],
    artifacts: &mut [Option<PathBuf>],
) -> Result<()> {
    for input in frontier {
        let input = *input;
        let index = input.get() as usize;
        let remaining = remaining_consumers.get_mut(index).ok_or_else(|| {
            Diagnostic::builtin(
                BuiltinDiagnostic::InvalidPlan,
                "prepared input does not identify an execution slot",
                node.origin().span.clone(),
            )
        })?;
        *remaining = remaining.checked_sub(1).ok_or_else(|| {
            Diagnostic::builtin(
                BuiltinDiagnostic::InvalidPlan,
                "temporary artifact consumer count underflowed",
                node.origin().span.clone(),
            )
        })?;
        if *remaining == 0 {
            let path = artifacts[index].take().ok_or_else(|| {
                Diagnostic::builtin(
                    BuiltinDiagnostic::InvalidPlan,
                    "consumed temporary artifact is not available",
                    node.origin().span.clone(),
                )
            })?;
            fs::remove_file(&path).map_err(|error| {
                Diagnostic::builtin(
                    BuiltinDiagnostic::OutputIo,
                    format!(
                        "could not remove temporary render artifact `{}` after its final use: {error}",
                        path.display()
                    ),
                    SourceSpan::file_start(path),
                )
            })?;
        }
    }
    Ok(())
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

    pub(super) const fn reused_artifacts(&self) -> usize {
        self.reused_artifacts
    }

    pub(super) const fn rendered_jobs(&self) -> usize {
        self.rendered_jobs
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

fn verify_region_resources(plan: &PreparedPlan, region: &[NodeId]) -> Result<()> {
    for id in region {
        verify_node_resources(&plan.nodes()[id.get() as usize])?;
    }
    Ok(())
}

fn plan_fused_regions(plan: &PreparedPlan, states: &[NodeState]) -> Vec<Vec<NodeId>> {
    let mut consumers = vec![Vec::new(); plan.nodes().len()];
    for node in plan.nodes() {
        if states[node.id().get() as usize] != NodeState::Render {
            continue;
        }
        super::execute::visit_fused_inputs(node, |input, streams| {
            if states[input.get() as usize] == NodeState::Render {
                consumers[input.get() as usize].push((node.id(), streams));
            }
        });
    }

    // Each render node belongs to the downstream materialized endpoint that owns
    // it. Reverse topological order ensures every consumer already has an owner.
    let mut owners = vec![None; plan.nodes().len()];
    for node in plan.nodes().iter().rev() {
        let id = node.id();
        let index = id.get() as usize;
        if states[index] != NodeState::Render {
            continue;
        }

        let downstream = &consumers[index];
        let adopted_owner = super::execute::is_graph_native(node)
            .then(|| {
                let (first, _) = *downstream.first()?;
                let owner = owners[first.get() as usize]?;
                let mut picture_uses = 0_usize;
                let mut audio_uses = 0_usize;
                let compatible = downstream.iter().copied().all(|(consumer, streams)| {
                    picture_uses = picture_uses
                        .checked_add(streams.picture)
                        .expect("prepared picture-stream use count fits in usize");
                    audio_uses = audio_uses
                        .checked_add(streams.audio)
                        .expect("prepared audio-stream use count fits in usize");
                    let consumer_node = &plan.nodes()[consumer.get() as usize];
                    super::execute::is_graph_native(consumer_node)
                        && super::execute::accepts_fused_input(consumer_node, id)
                        && owners[consumer.get() as usize] == Some(owner)
                });
                (compatible && picture_uses <= 1 && audio_uses <= 1).then_some(owner)
            })
            .flatten();
        owners[index] = Some(adopted_owner.unwrap_or(id));
    }

    let mut regions = vec![Vec::new(); plan.nodes().len()];
    for node in plan.nodes() {
        let index = node.id().get() as usize;
        if let Some(owner) = owners[index] {
            regions[owner.get() as usize].push(node.id());
        }
    }
    regions
}

fn region_frontier(plan: &PreparedPlan, region: &[NodeId]) -> Vec<NodeId> {
    let included = region.iter().copied().collect::<BTreeSet<_>>();
    let mut frontier = Vec::new();
    for id in region {
        plan.nodes()[id.get() as usize].visit_inputs(|input| {
            if !included.contains(&input) {
                frontier.push(input);
            }
        });
    }
    frontier
}
