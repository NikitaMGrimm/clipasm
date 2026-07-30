use std::path::Path;

use crate::diagnostic::Result;
use crate::model::{AudioDomain, AudioSpec, FrameCount, VideoSpec};
use crate::preflight::PreparedAsset;
use crate::preflight::tools::ExternalToolIdentity;
#[cfg(feature = "native")]
use crate::preflight::tools::ToolIdentity;
use crate::semantic::SourceOrigin;
use crate::source::SourceSpan;

pub(in crate::preflight) trait PreparationHost {
    fn prepare_image(&mut self, authored: &Path, origin: &SourceOrigin) -> Result<PreparedAsset>;

    fn prepare_video(
        &mut self,
        authored: &Path,
        video: &VideoSpec,
        origin: &SourceOrigin,
    ) -> Result<(PreparedAsset, FrameCount, bool)>;

    fn prepare_audio(
        &mut self,
        authored: &Path,
        audio: AudioSpec,
        origin: &SourceOrigin,
    ) -> Result<(PreparedAsset, AudioDomain)>;

    fn prepare_external_tool(
        &mut self,
        authored: &Path,
        span: &SourceSpan,
    ) -> Result<ExternalToolIdentity>;

    fn prepare_external_file(
        &mut self,
        authored: &Path,
        span: &SourceSpan,
    ) -> Result<PreparedAsset>;
}

#[cfg(feature = "native")]
pub(in crate::preflight) struct NativePreparationHost<'a> {
    ffmpeg: &'a ToolIdentity,
    ffprobe: &'a ToolIdentity,
}

#[cfg(feature = "native")]
impl<'a> NativePreparationHost<'a> {
    pub(in crate::preflight) const fn new(
        ffmpeg: &'a ToolIdentity,
        ffprobe: &'a ToolIdentity,
    ) -> Self {
        Self { ffmpeg, ffprobe }
    }
}

#[cfg(feature = "native")]
impl PreparationHost for NativePreparationHost<'_> {
    fn prepare_image(&mut self, authored: &Path, origin: &SourceOrigin) -> Result<PreparedAsset> {
        super::super::assets::prepare_image_asset(authored, origin, self.ffmpeg, self.ffprobe)
    }

    fn prepare_video(
        &mut self,
        authored: &Path,
        video: &VideoSpec,
        origin: &SourceOrigin,
    ) -> Result<(PreparedAsset, FrameCount, bool)> {
        super::super::assets::prepare_video_asset(
            authored,
            video,
            origin,
            self.ffmpeg,
            self.ffprobe,
        )
    }

    fn prepare_audio(
        &mut self,
        authored: &Path,
        audio: AudioSpec,
        origin: &SourceOrigin,
    ) -> Result<(PreparedAsset, AudioDomain)> {
        super::super::assets::prepare_audio_asset(
            authored,
            audio,
            origin,
            self.ffmpeg,
            self.ffprobe,
        )
    }

    fn prepare_external_tool(
        &mut self,
        authored: &Path,
        span: &SourceSpan,
    ) -> Result<ExternalToolIdentity> {
        super::super::tools::inspect_external_tool(authored, span)
    }

    fn prepare_external_file(
        &mut self,
        authored: &Path,
        span: &SourceSpan,
    ) -> Result<PreparedAsset> {
        super::super::assets::prepare_external_file_asset(authored, span)
    }
}
