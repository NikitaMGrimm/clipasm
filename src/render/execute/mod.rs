#![allow(clippy::trivially_copy_pass_by_ref)]

mod audio;
mod context;
mod effects;
mod export;
mod external;
mod filters;
mod media;
mod timeline;
mod transitions;

use std::path::{Path, PathBuf};

use crate::diagnostic::{Diagnostic, Result};
use crate::preflight::{
    PreparedAudioKind, PreparedNode, PreparedNodeMedia, PreparedPlan, PreparedVideoKind,
};

use context::{RenderContext, StagedArtifact};

pub(super) struct Executor<'a> {
    plan: &'a PreparedPlan,
}

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
            return Err(Diagnostic::new(
                "E_INVALID_PLAN",
                "prepared result is Audio, but rendering requires Video",
                result.origin().span.clone(),
            ));
        };
        export::stage_export(
            artifact,
            staged,
            self.plan.video(),
            self.plan.audio(),
            domain,
            has_audio,
            self.plan.ffmpeg().executable(),
            self.plan.ffprobe().executable(),
        )
    }

    pub(in crate::render) fn render_node(
        &self,
        node: &PreparedNode,
        artifacts: &[PathBuf],
        destination: &Path,
    ) -> Result<StagedArtifact> {
        let extension = match node.value_type() {
            crate::model::ValueType::Audio => "mka",
            crate::model::ValueType::Video => "mkv",
        };
        let staged = StagedArtifact::new(destination, extension)?;
        let context = RenderContext::new(self.plan, node, artifacts, staged.path());
        Self::render_into(node, &context)?;
        Ok(staged)
    }

    fn render_into(node: &PreparedNode, context: &RenderContext<'_>) -> Result<()> {
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
            PreparedNodeMedia::Audio {
                kind: PreparedAudioKind::AudioSource { asset },
                domain,
            } => audio::source(context, asset, domain),
            PreparedNodeMedia::Audio {
                kind: PreparedAudioKind::AudioSlice { input, range },
                ..
            } => audio::slice(context, *input, range.start(), range.end()),
            PreparedNodeMedia::Audio {
                kind: PreparedAudioKind::AudioRepeat { input, count },
                domain,
            } => audio::repeat(context, *input, count.get(), domain),
            PreparedNodeMedia::Audio {
                kind: PreparedAudioKind::AudioConcat { inputs },
                domain,
            } => audio::concat(context, inputs, domain),
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
                kind: PreparedVideoKind::Zoom { input, percent },
                domain,
                ..
            } => effects::zoom(context, *input, *percent, domain),
            PreparedNodeMedia::Video {
                kind: PreparedVideoKind::Wobble { input, pixels },
                domain,
                ..
            } => effects::wobble(context, *input, *pixels, domain),
            PreparedNodeMedia::Video {
                kind:
                    PreparedVideoKind::FlashJoin {
                        before,
                        after,
                        frames,
                    },
                domain,
                ..
            } => transitions::flash(context, *before, *after, *frames, domain),
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
            PreparedNodeMedia::Audio {
                kind: PreparedAudioKind::ExtractAudio { video },
                domain,
            } => audio::extract(context, *video, domain),
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
                kind:
                    PreparedVideoKind::ExternalVideo {
                        executable,
                        inputs,
                        parameters,
                        ..
                    },
                ..
            } => external::video(context, executable, inputs, parameters),
        }
    }
}
