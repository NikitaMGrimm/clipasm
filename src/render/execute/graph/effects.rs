use crate::diagnostic::Result;
use crate::model::{FrameCount, NodeId};

use super::super::color::{linear_rgb_to_encoding, working_to_linear_rgb};
use super::super::effects::zoom_filter;
use super::super::filters::{normalize_audio, samples_for_video};
use super::super::timeline::video_segment_sample_counts;
use super::{GraphBuilder, NodePads, VideoPads};

impl GraphBuilder<'_, '_> {
    pub(super) fn zoom(
        &mut self,
        input: NodeId,
        curve: &crate::preflight::PreparedZoomCurve,
        frames: FrameCount,
    ) -> Result<NodePads> {
        let input = self.video_input(input)?;
        let output = self.output_video_pads();
        let samples = samples_for_video(
            frames,
            self.context.video(),
            self.context.audio(),
            self.context.span(),
        )?;
        self.clause(format!(
            "{}{},{},{}{}",
            input.video.bracketed(),
            working_to_linear_rgb(self.context.policy().working_video_encoding()),
            zoom_filter(curve, frames),
            linear_rgb_to_encoding(self.context.policy().working_video_encoding()),
            output.video.bracketed(),
        ));
        self.clause(format!(
            "{}{}{}",
            input.audio.bracketed(),
            normalize_audio(
                samples,
                self.context.audio(),
                self.context.policy().working_audio_encoding(),
            ),
            output.audio.bracketed(),
        ));
        Ok(NodePads::Video(output))
    }

    pub(super) fn flash_cut(
        &mut self,
        before: NodeId,
        after: NodeId,
        fade_frames: FrameCount,
        output_frames: FrameCount,
    ) -> Result<NodePads> {
        let segment_samples = video_segment_sample_counts(
            &[before, after],
            self.context.nodes(),
            self.context.video(),
            self.context.audio(),
            self.context.span(),
        )?;
        let before = self.video_input(before)?;
        let after = self.video_input(after)?;
        let after_video = self.output_video_pad();
        self.clause(format!(
            "{}{},fade=t=in:start_frame=0:nb_frames={}:color=white,{}{}",
            after.video.bracketed(),
            working_to_linear_rgb(self.context.policy().working_video_encoding()),
            fade_frames.0,
            linear_rgb_to_encoding(self.context.policy().working_video_encoding()),
            after_video.bracketed(),
        ));
        let before_audio = self.output_audio_pad();
        let after_audio = self.output_audio_pad();
        self.clause(format!(
            "{}{}{}",
            before.audio.bracketed(),
            normalize_audio(
                segment_samples[0],
                self.context.audio(),
                self.context.policy().working_audio_encoding(),
            ),
            before_audio.bracketed(),
        ));
        self.clause(format!(
            "{}{}{}",
            after.audio.bracketed(),
            normalize_audio(
                segment_samples[1],
                self.context.audio(),
                self.context.policy().working_audio_encoding(),
            ),
            after_audio.bracketed(),
        ));
        let joined = self.output_video_pads();
        self.clause(format!(
            "{}{}concat=n=2:v=1:a=0{}",
            before.video.bracketed(),
            after_video.bracketed(),
            joined.video.bracketed(),
        ));
        self.clause(format!(
            "{}{}concat=n=2:v=0:a=1{}",
            before_audio.bracketed(),
            after_audio.bracketed(),
            joined.audio.bracketed(),
        ));
        let output_audio = self.output_audio_pad();
        let samples = samples_for_video(
            output_frames,
            self.context.video(),
            self.context.audio(),
            self.context.span(),
        )?;
        self.clause(format!(
            "{}{}{}",
            joined.audio.bracketed(),
            normalize_audio(
                samples,
                self.context.audio(),
                self.context.policy().working_audio_encoding(),
            ),
            output_audio.bracketed(),
        ));
        Ok(NodePads::Video(VideoPads {
            video: joined.video,
            audio: output_audio,
        }))
    }
}
