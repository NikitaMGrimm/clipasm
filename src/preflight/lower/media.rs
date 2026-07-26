use std::path::Path;

use crate::diagnostic::Result;
use crate::model::{AudioDomain, FrameCount, ImageFit, NodeId, TimelineRate, ValueRef};
use crate::semantic::CompiledNode;

use super::super::{PreparedAudioKind, PreparedVideoKind};
use super::{PreflightLowerer, project_domain};

pub(super) fn image(
    lowerer: &mut PreflightLowerer<'_>,
    node: &CompiledNode,
    path: &Path,
    frames: FrameCount,
    fit: ImageFit,
) -> Result<NodeId> {
    let asset = lowerer.host.prepare_image(path, node.origin())?;
    lowerer.add_video_node(
        PreparedVideoKind::ImageVideo { asset, frames, fit },
        *node.domain().expect("Video node domain"),
        false,
        node.semantic_version(),
        node.origin().clone(),
    )
}

pub(super) fn video_source(
    lowerer: &mut PreflightLowerer<'_>,
    node: &CompiledNode,
    path: &Path,
    fit: ImageFit,
) -> Result<NodeId> {
    let (asset, frames, has_audio) =
        lowerer
            .host
            .prepare_video(path, lowerer.compiled.video(), node.origin())?;
    lowerer.add_video_node(
        PreparedVideoKind::VideoSource { asset, frames, fit },
        project_domain(lowerer.compiled.video(), frames),
        has_audio,
        node.semantic_version(),
        node.origin().clone(),
    )
}

pub(super) fn audio_source(
    lowerer: &mut PreflightLowerer<'_>,
    node: &CompiledNode,
    path: &Path,
) -> Result<NodeId> {
    let (asset, domain) =
        lowerer
            .host
            .prepare_audio(path, *lowerer.compiled.audio(), node.origin())?;
    lowerer.add_audio_node(
        PreparedAudioKind::AudioSource { asset },
        domain,
        node.semantic_version(),
        node.origin().clone(),
    )
}

pub(super) fn extract_audio(
    lowerer: &mut PreflightLowerer<'_>,
    node: &CompiledNode,
    video: ValueRef,
) -> Result<NodeId> {
    let video = lowerer.prepared_dependency(video, node.origin())?;
    let samples = TimelineRate::new(*lowerer.compiled.video(), *lowerer.compiled.audio())
        .samples_for_frames(
            lowerer.video_domain(video, node.origin())?.0.frames(),
            &node.origin().span,
        )?;
    lowerer.add_audio_node(
        PreparedAudioKind::ExtractAudio { video },
        AudioDomain::new(samples, *lowerer.compiled.audio()),
        node.semantic_version(),
        node.origin().clone(),
    )
}

pub(super) fn set_audio(
    lowerer: &mut PreflightLowerer<'_>,
    node: &CompiledNode,
    audio: ValueRef,
    video: ValueRef,
) -> Result<NodeId> {
    let audio = lowerer.prepared_dependency(audio, node.origin())?;
    let video = lowerer.prepared_dependency(video, node.origin())?;
    lowerer.add_video_node(
        PreparedVideoKind::SetAudio { audio, video },
        *lowerer.video_domain(video, node.origin())?.0,
        true,
        node.semantic_version(),
        node.origin().clone(),
    )
}

pub(super) fn audio_on_black(
    lowerer: &mut PreflightLowerer<'_>,
    node: &CompiledNode,
    audio: ValueRef,
) -> Result<NodeId> {
    let audio = lowerer.prepared_dependency(audio, node.origin())?;
    let frames = TimelineRate::new(*lowerer.compiled.video(), *lowerer.compiled.audio())
        .frames_for_samples(
            lowerer.audio_domain(audio, node.origin())?.samples(),
            &node.origin().span,
        )?;
    lowerer.add_video_node(
        PreparedVideoKind::AudioOnBlack { audio },
        project_domain(lowerer.compiled.video(), frames),
        true,
        node.semantic_version(),
        node.origin().clone(),
    )
}
