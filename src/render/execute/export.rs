use std::fs;
use std::path::Path;
use std::process::Command;

use crate::diagnostic::Result;
use crate::model::{AudioSpec, VideoDomain, VideoSpec};
use crate::preflight::EXPORT_PIXEL_FORMAT;
use crate::source::SourceSpan;

use super::super::artifact::verify_video_artifact;
use super::context::run_command;

#[allow(clippy::too_many_arguments)]
pub(super) fn stage_export(
    artifact: &Path,
    staged: &Path,
    spec: &VideoSpec,
    audio: &AudioSpec,
    domain: &VideoDomain,
    has_audio: bool,
    ffmpeg: &Path,
    ffprobe: &Path,
) -> Result<()> {
    let result = export_mp4(artifact, staged, spec, audio, has_audio, ffmpeg).and_then(|()| {
        verify_video_artifact(
            ffprobe,
            staged,
            domain,
            audio,
            has_audio,
            false,
            EXPORT_PIXEL_FORMAT,
        )
    });
    if let Err(error) = result {
        let _ = fs::remove_file(staged);
        return Err(error);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn export_mp4(
    artifact: &Path,
    output: &Path,
    spec: &VideoSpec,
    audio: &AudioSpec,
    has_audio: bool,
    ffmpeg: &Path,
) -> Result<()> {
    let mut command = Command::new(ffmpeg);
    command
        .args(["-y", "-v", "error", "-i"])
        .arg(artifact)
        .args(["-map", "0:v:0", "-c:v", "libx264", "-pix_fmt"])
        .arg(EXPORT_PIXEL_FORMAT)
        .arg("-r")
        .arg(format!(
            "{}/{}",
            spec.fps().numerator(),
            spec.fps().denominator()
        ));
    if has_audio {
        command.args([
            "-map",
            "0:a:0",
            "-c:a",
            "aac",
            "-ar",
            &audio.sample_rate().to_string(),
            "-ac",
            &audio.channels().to_string(),
        ]);
    } else {
        command.arg("-an");
    }
    command
        .args(["-movflags", "+faststart", "-f", "mp4"])
        .arg(output);
    run_command(command, "E_FFMPEG", &SourceSpan::file_start(output))
}

#[cfg(test)]
mod tests {
    use super::super::super::publication::PublicationTransaction;
    use super::*;
    use crate::model::{FrameCount, FrameRate, VideoDomain, VideoSpec};

    #[test]
    fn failed_final_export_preserves_existing_pair() {
        if Command::new("ffmpeg").arg("-version").output().is_err() {
            return;
        }
        let directory = tempfile::tempdir().expect("temporary directory");
        let invalid_artifact = directory.path().join("invalid.mkv");
        let output = directory.path().join("final.mp4");
        let manifest = directory.path().join("final.mp4.manifest.json");
        fs::write(&invalid_artifact, b"not video").expect("invalid artifact");
        fs::write(&output, b"existing valid output").expect("existing output");
        fs::write(&manifest, b"existing manifest").expect("existing manifest");
        let spec =
            VideoSpec::new(64, 64, FrameRate::new(10, 1).expect("frame rate")).expect("video spec");
        let domain = VideoDomain::new(FrameCount(10), spec);
        let publication = PublicationTransaction::new(&output, &manifest);
        stage_export(
            &invalid_artifact,
            publication.staged_output(),
            &spec,
            &AudioSpec::default(),
            &domain,
            false,
            Path::new("ffmpeg"),
            Path::new("ffprobe"),
        )
        .expect_err("export failure");
        assert_eq!(
            fs::read(&output).expect("preserved output"),
            b"existing valid output"
        );
        assert_eq!(
            fs::read(&manifest).expect("preserved manifest"),
            b"existing manifest"
        );
    }
}
