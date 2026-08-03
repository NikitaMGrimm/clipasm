mod audio;
#[cfg(feature = "native")]
mod context;
mod effects;
mod export;
#[cfg(feature = "native")]
mod external;
mod filters;
mod media;
mod recipe;
mod timeline;
mod transitions;

#[cfg(feature = "native")]
use std::path::{Path, PathBuf};

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::model::AudioDomain;
#[cfg(feature = "native")]
use crate::preflight::PreparedPlan;
use crate::preflight::{PreparedAudioKind, PreparedNode, PreparedNodeMedia, PreparedVideoKind};

#[cfg(feature = "native")]
use super::cache::StagedArtifact;
#[cfg(feature = "native")]
use context::RenderContext;
pub(crate) use export::export_recipe;
pub(crate) use recipe::FfmpegArgument;
pub(crate) use recipe::{FfmpegRecipe, RecipeContext};

#[cfg(feature = "native")]
pub(super) struct Executor<'a> {
    plan: &'a PreparedPlan,
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

    pub(in crate::render) fn stage_cache_node(
        &self,
        node: &PreparedNode,
        artifacts: &[Option<PathBuf>],
        destination: &Path,
    ) -> Result<StagedArtifact> {
        let extension = match node.value_type() {
            crate::model::ValueType::Audio => self.plan.render_policy().working_audio_extension(),
            crate::model::ValueType::Video => self.plan.render_policy().working_video_extension(),
        };
        let staged = StagedArtifact::new(destination, extension)?;
        self.render_node_to(node, artifacts, staged.path())?;
        Ok(staged)
    }

    pub(in crate::render) fn render_node_to(
        &self,
        node: &PreparedNode,
        artifacts: &[Option<PathBuf>],
        destination: &Path,
    ) -> Result<()> {
        let context = RenderContext::new(self.plan, node, artifacts, destination);
        Self::render_into(node, &context)
    }

    fn render_into(node: &PreparedNode, context: &RenderContext<'_>) -> Result<()> {
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
            return external::video(context, executable, arguments, inputs, parameters);
        }
        let recipe_context = context.recipe_context();
        context.finish_ffmpeg(&ffmpeg_recipe(node, &recipe_context)?)
    }
}

pub(crate) fn ffmpeg_recipe(
    node: &PreparedNode,
    context: &RecipeContext<'_>,
) -> Result<FfmpegRecipe> {
    match node.media() {
        PreparedNodeMedia::Video {
            kind: PreparedVideoKind::ImageVideo { asset, fit, frames },
            ..
        } => media::image(context, asset, *fit, *frames),
        PreparedNodeMedia::Video {
            kind: PreparedVideoKind::VideoSource { asset, fit, frames },
            has_audio,
            ..
        } => media::video_source(context, asset, *fit, *frames, has_audio),
        PreparedNodeMedia::Audio { kind, domain } => audio_recipe(context, kind, domain),
        PreparedNodeMedia::Video {
            kind: PreparedVideoKind::Slice { input, range },
            ..
        } => timeline::slice(context, *input, range.start(), range.end()),
        PreparedNodeMedia::Video {
            kind:
                PreparedVideoKind::Repeat {
                    input,
                    count,
                    frames,
                },
            ..
        } => timeline::repeat(context, *input, *count, *frames),
        PreparedNodeMedia::Video {
            kind: PreparedVideoKind::ZoomIn { input, by },
            domain,
            ..
        } => effects::zoom_in(context, *input, by, domain),
        PreparedNodeMedia::Video {
            kind:
                PreparedVideoKind::FlashCut {
                    before,
                    after,
                    frames,
                },
            domain,
            ..
        } => transitions::flash_cut(context, *before, *after, *frames, domain),
        PreparedNodeMedia::Video {
            kind:
                PreparedVideoKind::Crossfade {
                    before,
                    after,
                    frames,
                },
            domain,
            ..
        } => transitions::crossfade(context, *before, *after, *frames, domain),
        PreparedNodeMedia::Video {
            kind: PreparedVideoKind::Concat { inputs },
            domain,
            ..
        } => timeline::concat(context, inputs, domain),
        PreparedNodeMedia::Video {
            kind: PreparedVideoKind::SetAudio { audio, video },
            domain,
            ..
        } => media::set_audio(context, *audio, *video, domain),
        PreparedNodeMedia::Video {
            kind: PreparedVideoKind::AudioOnBlack { audio },
            domain,
            ..
        } => media::audio_on_black(context, *audio, domain),
        PreparedNodeMedia::Video {
            kind: PreparedVideoKind::ExternalVideo { .. },
            ..
        } => Err(Diagnostic::builtin(
            BuiltinDiagnostic::InvalidPlan,
            "external programs do not have an FFmpeg recipe",
            node.origin().span.clone(),
        )),
    }
}

fn audio_recipe(
    context: &RecipeContext<'_>,
    kind: &PreparedAudioKind,
    domain: &AudioDomain,
) -> Result<FfmpegRecipe> {
    match kind {
        PreparedAudioKind::AudioSource { asset } => Ok(audio::source(context, asset, domain)),
        PreparedAudioKind::AudioSlice { input, range } => {
            Ok(audio::slice(context, *input, range.start(), range.end()))
        }
        PreparedAudioKind::AudioRepeat { input, count } => {
            Ok(audio::repeat(context, *input, count.get(), domain))
        }
        PreparedAudioKind::AudioConcat { inputs } => Ok(audio::concat(context, inputs, domain)),
        PreparedAudioKind::Crossfade {
            before,
            after,
            samples,
        } => transitions::audio_crossfade(context, *before, *after, *samples, domain),
        PreparedAudioKind::ExtractAudio { video } => Ok(audio::extract(context, *video, domain)),
    }
}
