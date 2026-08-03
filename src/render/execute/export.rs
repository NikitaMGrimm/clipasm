#[cfg(feature = "native")]
use std::fs;
#[cfg(feature = "native")]
use std::path::Path;

#[cfg(feature = "native")]
use crate::diagnostic::{BuiltinDiagnostic, Result};
#[cfg(feature = "native")]
use crate::model::VideoDomain;
use crate::model::{AudioSpec, NodeId, VideoSpec};
use crate::preflight::RenderPolicy;
#[cfg(feature = "native")]
use crate::source::SourceSpan;

#[cfg(feature = "native")]
use super::super::artifact::verify_video_artifact;
use super::color::working_to_encoding;
#[cfg(feature = "native")]
use super::context::run_command;
use super::recipe::{FfmpegRecipe, append_video_encoding_metadata};

#[expect(
    clippy::too_many_arguments,
    reason = "the export transaction keeps its artifact contract, policy, and tool identities explicit at one call site"
)]
#[cfg(feature = "native")]
pub(super) fn stage_export(
    result: NodeId,
    artifact: &Path,
    staged: &Path,
    spec: &VideoSpec,
    audio: AudioSpec,
    domain: &VideoDomain,
    has_audio: bool,
    render_policy: RenderPolicy,
    ffmpeg: &Path,
    ffprobe: &Path,
) -> Result<()> {
    let result = export_video(
        result,
        artifact,
        staged,
        spec,
        audio,
        has_audio,
        render_policy,
        ffmpeg,
    )
    .and_then(|()| {
        verify_video_artifact(
            ffprobe,
            staged,
            domain,
            audio,
            has_audio,
            None,
            render_policy.export_video_encoding(),
            None,
        )
    });
    if let Err(error) = result {
        return Err(match fs::remove_file(staged) {
            Ok(()) => error,
            Err(cleanup_error) if cleanup_error.kind() == std::io::ErrorKind::NotFound => error,
            Err(cleanup_error) => error.note(format!(
                "could not remove failed staged export `{}`: {cleanup_error}",
                staged.display()
            )),
        });
    }
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "the private export step mirrors the complete recipe inputs without introducing a duplicate request type"
)]
#[cfg(feature = "native")]
fn export_video(
    result: NodeId,
    artifact: &Path,
    output: &Path,
    spec: &VideoSpec,
    audio: AudioSpec,
    has_audio: bool,
    render_policy: RenderPolicy,
    ffmpeg: &Path,
) -> Result<()> {
    let recipe = export_recipe(result, spec, audio, has_audio, render_policy);
    let command = recipe.materialize(ffmpeg, output, &SourceSpan::file_start(output), |node| {
        (node == result).then_some(artifact)
    })?;
    run_command(
        command,
        BuiltinDiagnostic::Ffmpeg,
        &SourceSpan::file_start(output),
    )
}

pub(crate) fn export_recipe(
    result: NodeId,
    spec: &VideoSpec,
    audio: AudioSpec,
    has_audio: bool,
    render_policy: RenderPolicy,
) -> FfmpegRecipe {
    let encoding = render_policy.export_video_encoding();
    let mut recipe = FfmpegRecipe::new();
    recipe
        .args(["-i"])
        .artifact(result)
        .args(["-map", "0:v:0", "-vf"])
        .arg(working_to_encoding(encoding))
        .args(["-c:v", render_policy.export_video_encoder(), "-pix_fmt"])
        .arg(encoding.pixel_format());
    append_video_encoding_metadata(&mut recipe, encoding);
    recipe.arg("-r").arg(format!(
        "{}/{}",
        spec.fps().numerator(),
        spec.fps().denominator()
    ));
    if has_audio {
        recipe.args([
            "-map",
            "0:a:0",
            "-c:a",
            render_policy.export_audio_encoder(),
            "-ar",
            &audio.sample_rate().to_string(),
            "-ac",
            &audio.channels().to_string(),
        ]);
    } else {
        recipe.arg("-an");
    }
    recipe.args([
        "-movflags",
        render_policy.export_movflags(),
        "-f",
        render_policy.export_container(),
    ]);
    recipe
}

#[cfg(all(test, feature = "native"))]
mod tests {
    use std::ffi::OsString;
    use std::process::Command;

    use super::super::super::publication::PublicationTransaction;
    use super::*;
    use crate::model::{FrameCount, FrameRate, VideoDomain, VideoSpec};

    fn export_arguments(has_audio: bool) -> Vec<OsString> {
        let spec =
            VideoSpec::new(64, 64, FrameRate::new(10, 1).expect("frame rate")).expect("video spec");
        let audio = AudioSpec::default();
        let recipe = export_recipe(
            NodeId::new(4),
            &spec,
            audio,
            has_audio,
            RenderPolicy::CURRENT,
        );
        let artifact = Path::new("cache/result.mkv");
        let command = recipe
            .materialize(
                Path::new("ffmpeg"),
                Path::new("final.mp4"),
                &SourceSpan::file_start("final.mp4"),
                |node| (node == NodeId::new(4)).then_some(artifact),
            )
            .expect("export command");
        command
            .get_args()
            .map(std::ffi::OsStr::to_os_string)
            .collect()
    }

    #[test]
    fn video_only_export_recipe_preserves_the_native_arguments() {
        assert_eq!(
            export_arguments(false),
            [
                "-y",
                "-v",
                "error",
                "-i",
                "cache/result.mkv",
                "-map",
                "0:v:0",
                "-vf",
                "zscale=matrixin=bt709:transferin=bt709:primariesin=bt709:rangein=limited:matrix=bt709:transfer=bt709:primaries=bt709:range=limited:chromal=left:npl=100:agamma=0:dither=error_diffusion,format=yuv420p,setparams=range=limited:color_primaries=bt709:color_trc=bt709:colorspace=bt709",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-color_primaries",
                "bt709",
                "-color_trc",
                "bt709",
                "-colorspace",
                "bt709",
                "-color_range",
                "tv",
                "-chroma_sample_location",
                "left",
                "-r",
                "10/1",
                "-an",
                "-movflags",
                "+faststart",
                "-f",
                "mp4",
                "final.mp4",
            ]
            .map(OsString::from)
        );
    }

    #[test]
    fn audiovisual_export_recipe_preserves_the_native_arguments() {
        assert_eq!(
            export_arguments(true),
            [
                "-y",
                "-v",
                "error",
                "-i",
                "cache/result.mkv",
                "-map",
                "0:v:0",
                "-vf",
                "zscale=matrixin=bt709:transferin=bt709:primariesin=bt709:rangein=limited:matrix=bt709:transfer=bt709:primaries=bt709:range=limited:chromal=left:npl=100:agamma=0:dither=error_diffusion,format=yuv420p,setparams=range=limited:color_primaries=bt709:color_trc=bt709:colorspace=bt709",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-color_primaries",
                "bt709",
                "-color_trc",
                "bt709",
                "-colorspace",
                "bt709",
                "-color_range",
                "tv",
                "-chroma_sample_location",
                "left",
                "-r",
                "10/1",
                "-map",
                "0:a:0",
                "-c:a",
                "aac",
                "-ar",
                "48000",
                "-ac",
                "2",
                "-movflags",
                "+faststart",
                "-f",
                "mp4",
                "final.mp4",
            ]
            .map(OsString::from)
        );
    }

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
        let publication =
            PublicationTransaction::new(&output, &manifest).expect("publication transaction");
        stage_export(
            NodeId::new(0),
            &invalid_artifact,
            publication.staged_output(),
            &spec,
            AudioSpec::default(),
            &domain,
            false,
            RenderPolicy::CURRENT,
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

    #[test]
    fn failed_export_reports_staging_cleanup_failure() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let staged = directory.path().join("staged.mp4");
        fs::create_dir(&staged).expect("staged directory");
        let spec =
            VideoSpec::new(64, 64, FrameRate::new(10, 1).expect("frame rate")).expect("video spec");
        let domain = VideoDomain::new(FrameCount(10), spec);

        let error = stage_export(
            NodeId::new(0),
            Path::new("missing-artifact.mkv"),
            &staged,
            &spec,
            AudioSpec::default(),
            &domain,
            false,
            RenderPolicy::CURRENT,
            &directory.path().join("missing-ffmpeg"),
            &directory.path().join("missing-ffprobe"),
        )
        .expect_err("export failure");

        assert!(error.notes.iter().any(|note| {
            note.contains("could not remove failed staged export")
                && note.contains(&staged.display().to_string())
        }));
        assert!(staged.is_dir());
    }
}
