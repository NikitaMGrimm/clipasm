use std::num::NonZeroU64;

use crate::diagnostic::Result;
use crate::model::{FrameCount, NodeId};

use super::super::color::{linear_rgb_to_encoding, working_to_linear_rgb};
use super::super::filters::{normalize_audio, samples_for_video};
use super::super::timeline::video_segment_sample_counts;
use super::{GraphBuilder, NodePads, VideoPads, invalid_plan};

impl GraphBuilder<'_, '_> {
    pub(super) fn video_repeat(
        &mut self,
        input: NodeId,
        count: NonZeroU64,
        frames: FrameCount,
    ) -> Result<NodePads> {
        let input_frames = self
            .node(input)?
            .video_domain()
            .ok_or_else(|| invalid_plan(self.context, "repeat input is not Video"))?
            .frames();
        let audio_filter = super::super::timeline::repeat_audio_filter(
            input_frames,
            frames,
            self.context.video(),
            self.context.audio(),
            self.context.policy().working_audio_encoding(),
            self.context.span(),
        )?;
        let loops = (count.get() - 1).to_string();
        let input = self.artifact_input_with(input, &["-stream_loop", &loops, "-i"])?;
        let NodePads::Video(input) = input else {
            return Err(invalid_plan(self.context, "repeat input resolved to Audio"));
        };
        let output = self.output_video_pads();
        self.clause(format!(
            "{}trim=end_frame={},setpts=PTS-STARTPTS{}",
            input.video.bracketed(),
            frames.0,
            output.video.bracketed(),
        ));
        self.clause(format!(
            "{}{}{}",
            input.audio.bracketed(),
            audio_filter,
            output.audio.bracketed(),
        ));
        Ok(NodePads::Video(output))
    }

    pub(super) fn audio_repeat(
        &mut self,
        input: NodeId,
        count: NonZeroU64,
        samples: u64,
    ) -> Result<NodePads> {
        let loops = (count.get() - 1).to_string();
        let input = self.artifact_input_with(input, &["-stream_loop", &loops, "-i"])?;
        let NodePads::Audio(input) = input else {
            return Err(invalid_plan(self.context, "repeat input resolved to Video"));
        };
        let output = self.output_audio_pad();
        self.clause(format!(
            "{}{}{}",
            input.bracketed(),
            normalize_audio(
                samples,
                self.context.audio(),
                self.context.policy().working_audio_encoding(),
            ),
            output.bracketed(),
        ));
        Ok(NodePads::Audio(output))
    }

    pub(super) fn video_slice(&mut self, input: NodeId, start: u64, end: u64) -> Result<NodePads> {
        let input = self.video_input(input)?;
        let output = self.output_video_pads();
        let start_sample = samples_for_video(
            FrameCount(start),
            self.context.video(),
            self.context.audio(),
            self.context.span(),
        )?;
        let end_sample = samples_for_video(
            FrameCount(end),
            self.context.video(),
            self.context.audio(),
            self.context.span(),
        )?;
        self.clause(format!(
            "{}trim=start_frame={start}:end_frame={end},setpts=PTS-STARTPTS{}",
            input.video.bracketed(),
            output.video.bracketed(),
        ));
        self.clause(format!(
            "{}atrim=start_sample={start_sample}:end_sample={end_sample},asetpts=PTS-STARTPTS{}",
            input.audio.bracketed(),
            output.audio.bracketed(),
        ));
        Ok(NodePads::Video(output))
    }

    pub(super) fn audio_slice(&mut self, input: NodeId, start: u64, end: u64) -> Result<NodePads> {
        let input = self.audio_input(input)?;
        let output = self.output_audio_pad();
        self.clause(format!(
            "{}atrim=start_sample={start}:end_sample={end},asetpts=PTS-STARTPTS{}",
            input.bracketed(),
            output.bracketed(),
        ));
        Ok(NodePads::Audio(output))
    }

    pub(super) fn video_concat(
        &mut self,
        inputs: &[NodeId],
        frames: FrameCount,
    ) -> Result<NodePads> {
        let samples = video_segment_sample_counts(
            inputs,
            self.context.nodes(),
            self.context.video(),
            self.context.audio(),
            self.context.span(),
        )?;
        let mut video_labels = String::new();
        let mut audio_labels = String::new();
        for (input, samples) in inputs.iter().zip(samples) {
            let input = self.video_input(*input)?;
            let normalized = self.output_audio_pad();
            self.clause(format!(
                "{}{}{}",
                input.audio.bracketed(),
                normalize_audio(
                    samples,
                    self.context.audio(),
                    self.context.policy().working_audio_encoding(),
                ),
                normalized.bracketed(),
            ));
            video_labels.push_str(&input.video.bracketed());
            audio_labels.push_str(&normalized.bracketed());
        }
        let joined = self.output_video_pads();
        self.clause(format!(
            "{video_labels}concat=n={}:v=1:a=0{}",
            inputs.len(),
            joined.video.bracketed(),
        ));
        self.clause(format!(
            "{audio_labels}concat=n={}:v=0:a=1{}",
            inputs.len(),
            joined.audio.bracketed(),
        ));
        let output_audio = self.output_audio_pad();
        let output_samples = samples_for_video(
            frames,
            self.context.video(),
            self.context.audio(),
            self.context.span(),
        )?;
        self.clause(format!(
            "{}{}{}",
            joined.audio.bracketed(),
            normalize_audio(
                output_samples,
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

    pub(super) fn audio_concat(&mut self, inputs: &[NodeId], samples: u64) -> Result<NodePads> {
        let mut labels = String::new();
        for input in inputs {
            labels.push_str(&self.audio_input(*input)?.bracketed());
        }
        let joined = self.output_audio_pad();
        self.clause(format!(
            "{labels}concat=n={}:v=0:a=1{}",
            inputs.len(),
            joined.bracketed(),
        ));
        let output = self.output_audio_pad();
        self.clause(format!(
            "{}{}{}",
            joined.bracketed(),
            normalize_audio(
                samples,
                self.context.audio(),
                self.context.policy().working_audio_encoding(),
            ),
            output.bracketed(),
        ));
        Ok(NodePads::Audio(output))
    }

    pub(super) fn set_audio(
        &mut self,
        audio: NodeId,
        video: NodeId,
        frames: FrameCount,
    ) -> Result<NodePads> {
        let audio = self.audio_input(audio)?;
        let video = self.video_picture_input(video)?;
        let output = self.output_video_pads();
        let samples = samples_for_video(
            frames,
            self.context.video(),
            self.context.audio(),
            self.context.span(),
        )?;
        self.clause(format!(
            "{}trim=end_frame={},setpts=PTS-STARTPTS{}",
            video.bracketed(),
            frames.0,
            output.video.bracketed(),
        ));
        self.clause(format!(
            "{}{}{}",
            audio.bracketed(),
            normalize_audio(
                samples,
                self.context.audio(),
                self.context.policy().working_audio_encoding(),
            ),
            output.audio.bracketed(),
        ));
        Ok(NodePads::Video(output))
    }

    pub(super) fn audio_on_black(&mut self, audio: NodeId, frames: FrameCount) -> Result<NodePads> {
        let audio = self.audio_input(audio)?;
        let black = self.lavfi_video(format!(
            "color=c=black:s={}x{}:r={}/{}",
            self.context.video().width(),
            self.context.video().height(),
            self.context.video().fps().numerator(),
            self.context.video().fps().denominator(),
        ));
        let output = self.output_video_pads();
        let samples = samples_for_video(
            frames,
            self.context.video(),
            self.context.audio(),
            self.context.span(),
        )?;
        self.clause(format!(
            "{}trim=end_frame={},setpts=PTS-STARTPTS,{},{}{}",
            black.bracketed(),
            frames.0,
            working_to_linear_rgb(self.context.policy().working_video_encoding()),
            linear_rgb_to_encoding(self.context.policy().working_video_encoding()),
            output.video.bracketed(),
        ));
        self.clause(format!(
            "{}{}{}",
            audio.bracketed(),
            normalize_audio(
                samples,
                self.context.audio(),
                self.context.policy().working_audio_encoding(),
            ),
            output.audio.bracketed(),
        ));
        Ok(NodePads::Video(output))
    }

    pub(super) fn extract_audio(&mut self, video: NodeId, samples: u64) -> Result<NodePads> {
        let audio = self.video_audio_input(video)?;
        let output = self.output_audio_pad();
        self.clause(format!(
            "{}{}{}",
            audio.bracketed(),
            normalize_audio(
                samples,
                self.context.audio(),
                self.context.policy().working_audio_encoding(),
            ),
            output.bracketed(),
        ));
        Ok(NodePads::Audio(output))
    }
}
