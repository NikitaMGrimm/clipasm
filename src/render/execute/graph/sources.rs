use crate::diagnostic::Result;
use crate::model::{FrameCount, ValueType};

use super::super::filters::{
    image_filter, normalize_audio, samples_for_video, silence_source, video_filter,
};
use super::{GraphBuilder, NodePads};

impl GraphBuilder<'_, '_> {
    pub(super) fn image(
        &mut self,
        asset: &crate::preflight::PreparedAsset,
        color: &crate::preflight::PreparedSourceColor,
        fit: crate::model::ImageFit,
        frames: FrameCount,
    ) -> Result<NodePads> {
        let image = self.asset_input(
            &["-f", "image2", "-loop", "1", "-pattern_type", "none", "-i"],
            asset,
            ValueType::Video,
        );
        let NodePads::Video(image) = image else {
            unreachable!("Video asset input has Video pads")
        };
        let silence = self.lavfi_audio(silence_source(
            self.context.audio(),
            self.context.policy().working_audio_encoding(),
        ));
        let output = self.output_video_pads();
        let samples = samples_for_video(
            frames,
            self.context.video(),
            self.context.audio(),
            self.context.span(),
        )?;
        self.clause(format!(
            "{}{},trim=end_frame={},setpts=PTS-STARTPTS{}",
            image.video.bracketed(),
            image_filter(
                fit,
                self.context.video(),
                color,
                self.context.policy().working_video_encoding(),
            ),
            frames.0,
            output.video.bracketed(),
        ));
        self.clause(format!(
            "{}{}{}",
            silence.bracketed(),
            normalize_audio(
                samples,
                self.context.audio(),
                self.context.policy().working_audio_encoding(),
            ),
            output.audio.bracketed(),
        ));
        Ok(NodePads::Video(output))
    }

    pub(super) fn video_source(
        &mut self,
        asset: &crate::preflight::PreparedAsset,
        color: &crate::preflight::PreparedSourceColor,
        fit: crate::model::ImageFit,
        frames: FrameCount,
        has_audio: bool,
    ) -> Result<NodePads> {
        let source = self.asset_input(&["-i"], asset, ValueType::Video);
        let NodePads::Video(source) = source else {
            unreachable!("Video asset input has Video pads")
        };
        let audio = if has_audio {
            source.audio
        } else {
            self.lavfi_audio(silence_source(
                self.context.audio(),
                self.context.policy().working_audio_encoding(),
            ))
        };
        let output = self.output_video_pads();
        let samples = samples_for_video(
            frames,
            self.context.video(),
            self.context.audio(),
            self.context.span(),
        )?;
        self.clause(format!(
            "{}{}{}",
            source.video.bracketed(),
            video_filter(
                fit,
                frames,
                self.context.video(),
                color,
                self.context.policy().working_video_encoding(),
            ),
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

    pub(super) fn audio_source(
        &mut self,
        asset: &crate::preflight::PreparedAsset,
        samples: u64,
    ) -> NodePads {
        let source = self.asset_input(&["-i"], asset, ValueType::Audio);
        let NodePads::Audio(source) = source else {
            unreachable!("Audio asset input has an Audio pad")
        };
        let output = self.output_audio_pad();
        self.clause(format!(
            "{}{}{}",
            source.bracketed(),
            normalize_audio(
                samples,
                self.context.audio(),
                self.context.policy().working_audio_encoding(),
            ),
            output.bracketed(),
        ));
        NodePads::Audio(output)
    }
}
