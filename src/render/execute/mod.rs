mod color;
#[cfg(feature = "native")]
mod context;
mod effects;
mod export;
#[cfg(feature = "native")]
mod external;
mod filters;
mod graph;
mod recipe;
mod timeline;
mod transitions;

#[cfg(feature = "native")]
use std::path::{Path, PathBuf};

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::preflight::PreparedNode;
#[cfg(feature = "native")]
use crate::preflight::PreparedPlan;
#[cfg(feature = "native")]
use crate::preflight::{PreparedNodeMedia, PreparedVideoKind};

#[cfg(feature = "native")]
use super::cache::StagedArtifact;
#[cfg(feature = "native")]
use context::RenderContext;
pub(crate) use export::export_recipe;
pub(crate) use recipe::FfmpegArgument;
pub(crate) use recipe::{FfmpegRecipe, RecipeContext, validate_browser_arguments};

#[cfg(feature = "native")]
pub(super) struct Executor<'a> {
    plan: &'a PreparedPlan,
}

#[cfg(feature = "native")]
pub(super) enum ArtifactProducer {
    NativeFfmpeg,
    ExternalProgram,
}

#[cfg(feature = "native")]
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct FusedInputUse {
    pub(super) picture: usize,
    pub(super) audio: usize,
}

#[cfg(feature = "native")]
impl<'a> Executor<'a> {
    pub(super) const fn new(plan: &'a PreparedPlan) -> Self {
        Self { plan }
    }

    pub(super) fn stage_export(
        &self,
        artifact: &Path,
        staged: &Path,
        result: &PreparedNode,
    ) -> Result<()> {
        let PreparedNodeMedia::Video {
            domain, has_audio, ..
        } = result.media()
        else {
            return Err(Diagnostic::builtin(
                BuiltinDiagnostic::InvalidPlan,
                "prepared result is Audio, but rendering requires Video",
                result.origin().span.clone(),
            ));
        };
        export::stage_export(
            result.id(),
            artifact,
            staged,
            self.plan.video(),
            *self.plan.audio(),
            domain,
            has_audio,
            self.plan.render_policy(),
            self.plan.ffmpeg().executable(),
            self.plan.ffprobe().executable(),
        )
    }

    pub(in crate::render) fn stage_cache_region(
        &self,
        node: &PreparedNode,
        region: &[crate::model::NodeId],
        artifacts: &[Option<PathBuf>],
        destination: &Path,
    ) -> Result<StagedArtifact> {
        let extension = match node.value_type() {
            crate::model::ValueType::Audio => self.plan.render_policy().working_audio_extension(),
            crate::model::ValueType::Video => self.plan.render_policy().working_video_extension(),
        };
        let staged = StagedArtifact::new(destination, extension)?;
        self.render_region_to(node, region, artifacts, staged.path())?;
        Ok(staged)
    }

    pub(in crate::render) fn render_region_to(
        &self,
        node: &PreparedNode,
        region: &[crate::model::NodeId],
        artifacts: &[Option<PathBuf>],
        destination: &Path,
    ) -> Result<ArtifactProducer> {
        let context = RenderContext::new(self.plan, node, artifacts, destination);
        if region.len() == 1 {
            return Self::render_into(node, &context);
        }
        let recipe_context = context.recipe_context();
        context.finish_ffmpeg(&graph::recipe(&recipe_context, region, node.id())?)?;
        Ok(ArtifactProducer::NativeFfmpeg)
    }

    fn render_into(node: &PreparedNode, context: &RenderContext<'_>) -> Result<ArtifactProducer> {
        if let PreparedNodeMedia::Video {
            kind:
                PreparedVideoKind::ExternalVideo {
                    executable,
                    arguments,
                    inputs,
                    parameters,
                    ..
                },
            ..
        } = node.media()
        {
            external::video(context, executable, arguments, inputs, parameters)?;
            return Ok(ArtifactProducer::ExternalProgram);
        }
        let recipe_context = context.recipe_context();
        context.finish_ffmpeg(&ffmpeg_recipe(node, &recipe_context)?)?;
        Ok(ArtifactProducer::NativeFfmpeg)
    }
}

#[cfg(feature = "native")]
pub(super) fn is_graph_native(node: &PreparedNode) -> bool {
    graph::is_graph_native(node)
}

#[cfg(feature = "native")]
pub(super) fn accepts_fused_input(node: &PreparedNode, input: crate::model::NodeId) -> bool {
    graph::accepts_fused_input(node, input)
}

#[cfg(feature = "native")]
pub(super) fn visit_fused_inputs(
    node: &PreparedNode,
    visitor: impl FnMut(crate::model::NodeId, FusedInputUse),
) {
    graph::visit_fused_inputs(node, visitor);
}

#[cfg(feature = "native")]
pub(super) fn fused_region_fits(
    plan: &PreparedPlan,
    region: &[crate::model::NodeId],
    output: crate::model::NodeId,
    output_path: &Path,
    artifacts: &[PathBuf],
) -> Result<bool> {
    let node = &plan.nodes()[output.get() as usize];
    let context = RecipeContext::new(
        plan.video(),
        plan.audio(),
        plan.nodes(),
        plan.render_policy(),
        &node.origin().span,
    );
    graph::recipe(&context, region, output)?.materialized_command_fits(
        plan.ffmpeg().executable(),
        output_path,
        &node.origin().span,
        |input| artifacts.get(input.get() as usize).map(PathBuf::as_path),
    )
}

pub(crate) fn ffmpeg_recipe(
    node: &PreparedNode,
    context: &RecipeContext<'_>,
) -> Result<FfmpegRecipe> {
    if !graph::is_graph_native(node) {
        return Err(Diagnostic::builtin(
            BuiltinDiagnostic::InvalidPlan,
            "external programs do not have an FFmpeg recipe",
            node.origin().span.clone(),
        ));
    }
    graph::recipe(context, &[node.id()], node.id())
}
