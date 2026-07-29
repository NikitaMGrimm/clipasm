use std::path::{Path, PathBuf};
use std::process::Command;

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::media_tool;
use crate::model::{AudioSpec, NodeId, VideoSpec};
use crate::preflight::{PreparedNode, PreparedPlan, RenderPolicy};
use crate::source::SourceSpan;

use super::recipe::{FfmpegRecipe, RecipeContext};

pub(super) struct RenderContext<'a> {
    plan: &'a PreparedPlan,
    node: &'a PreparedNode,
    artifacts: &'a [Option<PathBuf>],
    temporary: &'a Path,
}

impl<'a> RenderContext<'a> {
    pub(super) const fn new(
        plan: &'a PreparedPlan,
        node: &'a PreparedNode,
        artifacts: &'a [Option<PathBuf>],
        temporary: &'a Path,
    ) -> Self {
        Self {
            plan,
            node,
            artifacts,
            temporary,
        }
    }

    pub(super) const fn plan(&self) -> &'a PreparedPlan {
        self.plan
    }

    pub(super) fn nodes(&self) -> &'a [PreparedNode] {
        self.plan.nodes()
    }

    pub(super) fn video(&self) -> &'a VideoSpec {
        self.plan.video()
    }

    pub(super) fn audio(&self) -> &'a AudioSpec {
        self.plan.audio()
    }

    pub(super) const fn policy(&self) -> RenderPolicy {
        self.plan.render_policy()
    }

    pub(super) fn temporary(&self) -> &Path {
        self.temporary
    }

    pub(super) fn span(&self) -> &SourceSpan {
        &self.node.origin().span
    }

    pub(super) fn recipe_context(&self) -> RecipeContext<'_> {
        RecipeContext::new(
            self.video(),
            self.audio(),
            self.nodes(),
            self.policy(),
            self.span(),
        )
    }

    pub(super) fn artifact(&self, id: NodeId) -> Result<&'a Path> {
        self.artifacts
            .get(id.get() as usize)
            .and_then(Option::as_deref)
            .ok_or_else(|| {
                Diagnostic::builtin(
                    BuiltinDiagnostic::InvalidPlan,
                    format!("primitive input {} is not available", id.get()),
                    self.span().clone(),
                )
            })
    }

    pub(super) fn finish_ffmpeg(&self, recipe: &FfmpegRecipe) -> Result<()> {
        let command = recipe.materialize(
            self.plan.ffmpeg().executable(),
            self.temporary(),
            self.span(),
            |node| {
                self.artifacts
                    .get(node.get() as usize)
                    .and_then(Option::as_deref)
            },
        )?;
        run_command(command, BuiltinDiagnostic::Ffmpeg, self.span())
    }
}

pub(super) fn run_command(
    command: Command,
    diagnostic: BuiltinDiagnostic,
    span: &SourceSpan,
) -> Result<()> {
    media_tool::run(command, diagnostic, span)
}
