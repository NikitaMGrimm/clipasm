use crate::model::{ImageFit, NodeId};

use super::tools::FfmpegRequirements;
use super::{PreparedAudioKind, PreparedNode, PreparedNodeMedia, PreparedVideoKind};

pub(super) fn ffmpeg_requirements(nodes: &[PreparedNode], result: NodeId) -> FfmpegRequirements {
    let result = nodes
        .get(result.get() as usize)
        .expect("prepared result identifies an existing node");
    let mut requirements = FfmpegRequirements::for_render(result.has_audio());
    for node in nodes {
        match node.media() {
            PreparedNodeMedia::Video {
                kind, has_audio, ..
            } => add_video_requirements(kind, has_audio, &mut requirements),
            PreparedNodeMedia::Audio { kind, .. } => {
                add_audio_requirements(kind, &mut requirements);
            }
        }
    }
    requirements
}

fn add_video_requirements(
    kind: &PreparedVideoKind,
    has_audio: bool,
    requirements: &mut FfmpegRequirements,
) {
    match kind {
        PreparedVideoKind::ImageVideo { fit, .. } => {
            require_image_filter(*fit, requirements);
            requirements.require_filters(["trim", "setpts", "anullsrc"]);
            require_normalized_audio(requirements);
        }
        PreparedVideoKind::VideoSource { fit, .. } => {
            require_image_filter(*fit, requirements);
            requirements.require_filters(["setpts", "tpad", "trim"]);
            if !has_audio {
                requirements.require_filters(["anullsrc"]);
            }
            require_normalized_audio(requirements);
        }
        PreparedVideoKind::Slice { .. } => {
            requirements.require_filters(["trim", "setpts", "atrim", "asetpts"]);
        }
        PreparedVideoKind::Repeat { .. } => {
            requirements.require_filters(["trim", "setpts", "asetnsamples"]);
            require_normalized_audio(requirements);
        }
        PreparedVideoKind::Zoom { .. } => {
            requirements.require_filters(["perspective", "setpts"]);
            require_normalized_audio(requirements);
        }
        PreparedVideoKind::Wobble { .. } => {
            requirements.require_filters(["scale", "setsar", "crop", "setpts"]);
            require_normalized_audio(requirements);
        }
        PreparedVideoKind::FlashJoin { .. } => {
            requirements.require_filters(["fade", "concat"]);
            require_normalized_audio(requirements);
        }
        PreparedVideoKind::Crossfade { .. } => {
            requirements.require_filters([
                "split", "trim", "setpts", "blend", "concat", "asplit", "atrim", "asetpts",
                "afade", "adelay", "apad", "amix",
            ]);
            require_normalized_audio(requirements);
        }
        PreparedVideoKind::Concat { .. } => {
            requirements.require_filters(["concat"]);
            require_normalized_audio(requirements);
        }
        PreparedVideoKind::SetAudio { .. } => {
            requirements.require_filters(["trim", "setpts"]);
            require_normalized_audio(requirements);
        }
        PreparedVideoKind::AudioOnBlack { .. } => {
            requirements.require_filters(["color", "trim", "setpts", "format"]);
            require_normalized_audio(requirements);
        }
        PreparedVideoKind::ExternalVideo { .. } => {}
    }
}

fn add_audio_requirements(kind: &PreparedAudioKind, requirements: &mut FfmpegRequirements) {
    match kind {
        PreparedAudioKind::AudioSource { .. }
        | PreparedAudioKind::AudioRepeat { .. }
        | PreparedAudioKind::ExtractAudio { .. } => require_normalized_audio(requirements),
        PreparedAudioKind::AudioSlice { .. } => {
            requirements.require_filters(["atrim", "asetpts"]);
        }
        PreparedAudioKind::AudioConcat { .. } => {
            requirements.require_filters(["concat"]);
            require_normalized_audio(requirements);
        }
    }
}

fn require_image_filter(fit: ImageFit, requirements: &mut FfmpegRequirements) {
    requirements.require_filters(["scale", "fps", "setsar", "format"]);
    match fit {
        ImageFit::Cover => requirements.require_filters(["crop"]),
        ImageFit::Contain => requirements.require_filters(["pad"]),
        ImageFit::Stretch => {}
    }
}

fn require_normalized_audio(requirements: &mut FfmpegRequirements) {
    requirements.require_filters(["aresample", "aformat", "atrim", "apad", "asetpts"]);
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::model::FrameCount;

    use super::*;
    use crate::preflight::PreparedAsset;

    fn asset() -> PreparedAsset {
        PreparedAsset::new(PathBuf::from("asset"), "content".to_owned())
    }

    #[test]
    fn simple_image_requirements_exclude_unreachable_operations() {
        let mut requirements = FfmpegRequirements::for_render(false);
        add_video_requirements(
            &PreparedVideoKind::ImageVideo {
                asset: asset(),
                frames: FrameCount(1),
                fit: ImageFit::Stretch,
            },
            false,
            &mut requirements,
        );
        assert!(requirements.requires_encoder("libx264"));
        assert!(requirements.requires_filter("scale"));
        assert!(requirements.requires_filter("anullsrc"));
        assert!(!requirements.requires_filter("blend"));
        assert!(!requirements.requires_filter("fade"));
        assert!(!requirements.requires_filter("perspective"));
        assert!(!requirements.requires_encoder("aac"));
    }

    #[test]
    fn crossfade_and_audio_export_add_their_own_capabilities() {
        let mut requirements = FfmpegRequirements::for_render(true);
        add_video_requirements(
            &PreparedVideoKind::Crossfade {
                before: NodeId::new(0),
                after: NodeId::new(1),
                frames: FrameCount(1),
            },
            true,
            &mut requirements,
        );
        for filter in ["blend", "afade", "adelay", "amix", "split", "asplit"] {
            assert!(requirements.requires_filter(filter), "missing {filter}");
        }
        assert!(requirements.requires_encoder("aac"));
    }
}
