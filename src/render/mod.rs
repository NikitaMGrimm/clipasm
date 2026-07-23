use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::diagnostic::{Diagnostic, Result, SourceSpan};
use crate::model::{FrameCount, ImageFit, NodeId, VideoDomain, VideoSpec};
use crate::preflight::{
    PreparedNode, PreparedNodeKind, PreparedPlan, RenderMediaPolicy, verify_prepared_asset,
};

static TEMPORARY_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Serialize)]
pub struct RenderReport {
    pub output: PathBuf,
    pub manifest: PathBuf,
    pub cache_hits: usize,
    pub cache_misses: usize,
}

#[derive(Serialize)]
struct Manifest<'a> {
    engine_version: &'static str,
    ffmpeg: &'a str,
    ffprobe: &'a str,
    cache_hits: usize,
    cache_misses: usize,
    plan: &'a PreparedPlan,
}

#[derive(Deserialize)]
struct ProbeDocument {
    streams: Vec<ProbeStream>,
}

#[derive(Deserialize)]
struct ProbeStream {
    codec_type: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    pix_fmt: Option<String>,
    r_frame_rate: Option<String>,
    nb_read_frames: Option<String>,
    start_time: Option<String>,
    sample_aspect_ratio: Option<String>,
}

/// Render an invariant-protected prepared plan and atomically publish its MP4
/// and manifest.
///
/// Working intermediates use lossless FFV1 with non-subsampled `yuv444p`.
/// The only delivery profile is H.264 MP4 with `yuv420p`, square pixels, and
/// no audio. This is the renderer's initial fixed color/media policy.
///
/// # Errors
///
/// Returns a diagnostic for changed assets, rendering/cache failures, contract
/// violations, or atomic publication failures.
#[allow(clippy::too_many_lines)]
pub fn render(plan: &PreparedPlan) -> Result<RenderReport> {
    let workflow_directory = plan
        .workflow_path()
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let cache_directory = workflow_directory
        .join(".rhythmcut")
        .join("cache")
        .join(plan.execution_namespace());
    fs::create_dir_all(&cache_directory).map_err(|error| {
        Diagnostic::new(
            "E_CACHE_IO",
            format!(
                "could not create cache directory `{}`: {error}",
                cache_directory.display()
            ),
            SourceSpan::file_start(plan.workflow_path()),
        )
    })?;

    let mut artifacts = Vec::<PathBuf>::with_capacity(plan.nodes().len());
    let mut cache_hits = 0;
    let mut cache_misses = 0;
    for node in plan.nodes() {
        if node.id().get() as usize != artifacts.len() {
            return Err(Diagnostic::new(
                "E_INVALID_PLAN",
                "prepared nodes are not in stable topological order",
                node.origin().span.clone(),
            ));
        }
        let artifact = cache_directory.join(format!("{}.mkv", node.fingerprint()));
        if let PreparedNodeKind::ImageVideo { asset, .. } = node.kind() {
            verify_prepared_asset(asset, &node.origin().span)?;
        }
        let hit = artifact.is_file()
            && verify_artifact(
                plan.ffprobe().executable(),
                &artifact,
                node.domain(),
                plan.media_policy().working_pixel_format(),
            )
            .is_ok();
        if hit {
            cache_hits += 1;
        } else {
            cache_misses += 1;
            if artifact.exists() {
                fs::remove_file(&artifact).map_err(|error| {
                    Diagnostic::new(
                        "E_CACHE_IO",
                        format!(
                            "could not replace invalid cache artifact `{}`: {error}",
                            artifact.display()
                        ),
                        node.origin().span.clone(),
                    )
                })?;
            }
            render_node(
                node,
                &artifacts,
                &artifact,
                plan.video(),
                plan.media_policy(),
                plan.ffmpeg().executable(),
            )?;
            verify_artifact(
                plan.ffprobe().executable(),
                &artifact,
                node.domain(),
                plan.media_policy().working_pixel_format(),
            )?;
        }
        artifacts.push(artifact);
    }

    let root_node = &plan.nodes()[plan.root().get() as usize];
    let root_artifact = artifacts.get(plan.root().get() as usize).ok_or_else(|| {
        Diagnostic::new(
            "E_INVALID_PLAN",
            "prepared root does not identify a primitive artifact",
            SourceSpan::file_start(plan.workflow_path()),
        )
    })?;
    if let Some(parent) = plan.output().parent() {
        fs::create_dir_all(parent).map_err(|error| {
            Diagnostic::new(
                "E_OUTPUT_IO",
                format!(
                    "could not create output directory `{}`: {error}",
                    parent.display()
                ),
                SourceSpan::file_start(plan.output()),
            )
        })?;
    }

    publish_export(
        root_artifact,
        plan.output(),
        plan.video(),
        root_node.domain(),
        plan.media_policy(),
        plan.ffmpeg().executable(),
        plan.ffprobe().executable(),
    )?;

    let manifest = Manifest {
        engine_version: env!("CARGO_PKG_VERSION"),
        ffmpeg: plan.ffmpeg().version(),
        ffprobe: plan.ffprobe().version(),
        cache_hits,
        cache_misses,
        plan,
    };
    let manifest_json = serde_json::to_vec_pretty(&manifest).map_err(|error| {
        Diagnostic::new(
            "E_MANIFEST",
            format!("could not serialize render manifest: {error}"),
            SourceSpan::file_start(plan.workflow_path()),
        )
    })?;
    write_atomically(
        plan.manifest(),
        &manifest_json,
        "manifest",
        "json",
        "E_MANIFEST",
    )?;

    Ok(RenderReport {
        output: plan.output().to_path_buf(),
        manifest: plan.manifest().to_path_buf(),
        cache_hits,
        cache_misses,
    })
}

fn publish_export(
    artifact: &Path,
    output: &Path,
    spec: &VideoSpec,
    domain: &VideoDomain,
    media_policy: RenderMediaPolicy,
    ffmpeg: &Path,
    ffprobe: &Path,
) -> Result<()> {
    let temporary = temporary_sibling(output, "export", "mp4");
    let result = export_mp4(
        artifact,
        &temporary,
        spec,
        domain.frames,
        media_policy,
        ffmpeg,
    )
    .and_then(|()| {
        verify_artifact(
            ffprobe,
            &temporary,
            domain,
            media_policy.export_pixel_format(),
        )
    });
    if let Err(error) = result {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    atomic_replace(&temporary, output, "E_OUTPUT_IO")
}

fn write_atomically(
    destination: &Path,
    contents: &[u8],
    role: &str,
    extension: &str,
    code: &'static str,
) -> Result<()> {
    let temporary = temporary_sibling(destination, role, extension);
    if let Err(error) = fs::write(&temporary, contents) {
        let _ = fs::remove_file(&temporary);
        return Err(Diagnostic::new(
            code,
            format!(
                "could not write temporary file `{}`: {error}",
                temporary.display()
            ),
            SourceSpan::file_start(destination),
        ));
    }
    atomic_replace(&temporary, destination, code)
}

fn render_node(
    node: &PreparedNode,
    artifacts: &[PathBuf],
    destination: &Path,
    spec: &VideoSpec,
    media_policy: RenderMediaPolicy,
    ffmpeg: &Path,
) -> Result<()> {
    let temporary = temporary_sibling(destination, "cache", "mkv");
    let mut command = Command::new(ffmpeg);
    command.args(["-y", "-v", "error"]);
    match node.kind() {
        PreparedNodeKind::ImageVideo { asset, fit, frames } => {
            command.args(["-loop", "1", "-i"]).arg(asset.source_path());
            command.args(["-vf", &image_filter(*fit, spec, media_policy)]);
            append_lossless_output(&mut command, *frames, spec, media_policy, &temporary);
        }
        PreparedNodeKind::Slice { input, range } => {
            command
                .arg("-i")
                .arg(artifact(artifacts, *input, &node.origin().span)?);
            command.args([
                "-vf",
                &format!(
                    "trim=start_frame={}:end_frame={},setpts=PTS-STARTPTS",
                    range.start(),
                    range.end()
                ),
            ]);
            append_lossless_output(&mut command, range.frames(), spec, media_policy, &temporary);
        }
        PreparedNodeKind::Concat { inputs } => {
            for input in inputs {
                command
                    .arg("-i")
                    .arg(artifact(artifacts, *input, &node.origin().span)?);
            }
            let labels = (0..inputs.len()).fold(String::new(), |mut output, index| {
                let _ = write!(output, "[{index}:v]");
                output
            });
            let filter = format!(
                "{labels}concat=n={}:v=1:a=0,setpts=PTS-STARTPTS[v]",
                inputs.len()
            );
            command.args(["-filter_complex", &filter, "-map", "[v]"]);
            append_lossless_output(
                &mut command,
                node.domain().frames,
                spec,
                media_policy,
                &temporary,
            );
        }
    }
    if let Err(error) = run_command(command, "E_FFMPEG", &node.origin().span) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    atomic_replace(&temporary, destination, "E_CACHE_IO")
}

fn append_lossless_output(
    command: &mut Command,
    frames: FrameCount,
    spec: &VideoSpec,
    media_policy: RenderMediaPolicy,
    destination: &Path,
) {
    command
        .args(["-frames:v", &frames.0.to_string(), "-an", "-c:v", "ffv1"])
        .args([
            "-level",
            "3",
            "-pix_fmt",
            media_policy.working_pixel_format(),
            "-r",
        ])
        .arg(format!(
            "{}/{}",
            spec.fps.numerator(),
            spec.fps.denominator()
        ))
        .arg(destination);
}

fn export_mp4(
    artifact: &Path,
    output: &Path,
    spec: &VideoSpec,
    frames: FrameCount,
    media_policy: RenderMediaPolicy,
    ffmpeg: &Path,
) -> Result<()> {
    let mut command = Command::new(ffmpeg);
    command
        .args(["-y", "-v", "error", "-i"])
        .arg(artifact)
        .args(["-an", "-c:v", "libx264", "-pix_fmt"])
        .arg(media_policy.export_pixel_format())
        .arg("-r")
        .arg(format!(
            "{}/{}",
            spec.fps.numerator(),
            spec.fps.denominator()
        ))
        .args([
            "-frames:v",
            &frames.0.to_string(),
            "-movflags",
            "+faststart",
            "-f",
            "mp4",
        ])
        .arg(output);
    run_command(command, "E_FFMPEG", &SourceSpan::file_start(output))
}

fn image_filter(fit: ImageFit, spec: &VideoSpec, media_policy: RenderMediaPolicy) -> String {
    let width = spec.width;
    let height = spec.height;
    let geometry = match fit {
        ImageFit::Cover => format!(
            "scale={width}:{height}:force_original_aspect_ratio=increase,crop={width}:{height}"
        ),
        ImageFit::Contain => format!(
            "scale={width}:{height}:force_original_aspect_ratio=decrease,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2"
        ),
        ImageFit::Stretch => format!("scale={width}:{height}"),
    };
    format!(
        "{geometry},fps={}/{},setsar=1,format={}",
        spec.fps.numerator(),
        spec.fps.denominator(),
        media_policy.working_pixel_format()
    )
}

fn artifact<'a>(artifacts: &'a [PathBuf], id: NodeId, span: &SourceSpan) -> Result<&'a Path> {
    artifacts
        .get(id.get() as usize)
        .map(PathBuf::as_path)
        .ok_or_else(|| {
            Diagnostic::new(
                "E_INVALID_PLAN",
                format!("primitive input {} is not available", id.get()),
                span.clone(),
            )
        })
}

#[allow(clippy::too_many_lines)]
fn verify_artifact(
    ffprobe: &Path,
    path: &Path,
    domain: &VideoDomain,
    pixel_format: &str,
) -> Result<()> {
    let mut command = Command::new(ffprobe);
    command
        .args([
            "-v",
            "error",
            "-count_frames",
            "-show_streams",
            "-of",
            "json",
        ])
        .arg(path);
    let output = run_output(command, "E_FFPROBE", &SourceSpan::file_start(path))?;
    let document: ProbeDocument = serde_json::from_slice(&output.stdout).map_err(|error| {
        Diagnostic::new(
            "E_ARTIFACT_CONTRACT",
            format!(
                "FFprobe returned invalid JSON for `{}`: {error}",
                path.display()
            ),
            SourceSpan::file_start(path),
        )
    })?;
    let videos = document
        .streams
        .iter()
        .filter(|stream| stream.codec_type.as_deref() == Some("video"))
        .collect::<Vec<_>>();
    let audio_count = document
        .streams
        .iter()
        .filter(|stream| stream.codec_type.as_deref() == Some("audio"))
        .count();
    if videos.len() != 1 || audio_count != 0 {
        return Err(contract_error(
            path,
            &format!(
                "expected one video stream and no audio, found {} video and {audio_count} audio streams",
                videos.len()
            ),
        ));
    }
    let video = videos[0];
    if video.width != Some(domain.width) || video.height != Some(domain.height) {
        return Err(contract_error(
            path,
            &format!(
                "expected {}x{}, found {:?}x{:?}",
                domain.width, domain.height, video.width, video.height
            ),
        ));
    }
    if video.pix_fmt.as_deref() != Some(pixel_format) {
        return Err(contract_error(
            path,
            &format!("expected {pixel_format}, found {:?}", video.pix_fmt),
        ));
    }
    let expected_rate = format!(
        "{}/{}",
        domain.frame_rate.numerator(),
        domain.frame_rate.denominator()
    );
    if video.r_frame_rate.as_deref() != Some(expected_rate.as_str()) {
        return Err(contract_error(
            path,
            &format!(
                "expected frame rate {expected_rate}, found {:?}",
                video.r_frame_rate
            ),
        ));
    }
    let actual_frames = video
        .nb_read_frames
        .as_deref()
        .and_then(|value| value.parse::<u64>().ok());
    if actual_frames != Some(domain.frames.0) {
        return Err(contract_error(
            path,
            &format!(
                "expected {} frames, FFprobe counted {:?}",
                domain.frames.0, actual_frames
            ),
        ));
    }
    let start = video
        .start_time
        .as_deref()
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(0.0);
    if start.abs() > 0.000_001 {
        return Err(contract_error(
            path,
            &format!("timestamps must begin at zero, found {start}"),
        ));
    }
    if video.sample_aspect_ratio.as_deref() != Some("1:1") {
        return Err(contract_error(
            path,
            &format!(
                "expected square pixels (1:1), found {:?}",
                video.sample_aspect_ratio
            ),
        ));
    }
    Ok(())
}

fn temporary_sibling(path: &Path, role: &str, extension: &str) -> PathBuf {
    let counter = TEMPORARY_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut name = std::ffi::OsString::from(".");
    name.push(path.file_name().unwrap_or_default());
    name.push(format!(
        ".{role}-{}-{counter}.{extension}",
        std::process::id()
    ));
    path.parent().unwrap_or_else(|| Path::new(".")).join(name)
}

fn atomic_replace(source: &Path, destination: &Path, code: &'static str) -> Result<()> {
    fs::rename(source, destination).map_err(|error| {
        Diagnostic::new(
            code,
            format!(
                "could not atomically replace `{}` from `{}`: {error}",
                destination.display(),
                source.display()
            ),
            SourceSpan::file_start(destination),
        )
    })
}

fn contract_error(path: &Path, message: &str) -> Diagnostic {
    Diagnostic::new(
        "E_ARTIFACT_CONTRACT",
        format!(
            "artifact `{}` violates its contract: {message}",
            path.display()
        ),
        SourceSpan::file_start(path),
    )
}

fn run_command(command: Command, code: &'static str, span: &SourceSpan) -> Result<()> {
    run_output(command, code, span).map(|_| ())
}

fn run_output(mut command: Command, code: &'static str, span: &SourceSpan) -> Result<Output> {
    let debug = format!("{command:?}");
    let output = command.output().map_err(|error| {
        Diagnostic::new(
            code,
            format!("could not start external tool: {error}"),
            span.clone(),
        )
        .note(debug.clone())
    })?;
    if !output.status.success() {
        return Err(Diagnostic::new(
            code,
            format!(
                "external tool exited with {}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ),
            span.clone(),
        )
        .note(debug));
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::FrameRate;

    #[test]
    fn failed_final_export_preserves_existing_destination() {
        if Command::new("ffmpeg").arg("-version").output().is_err() {
            return;
        }
        let directory = tempfile::tempdir().expect("temporary directory");
        let invalid_artifact = directory.path().join("invalid.mkv");
        let output = directory.path().join("final.mp4");
        fs::write(&invalid_artifact, b"not video").expect("invalid artifact");
        fs::write(&output, b"existing valid output").expect("existing output");
        let spec = VideoSpec {
            width: 64,
            height: 64,
            fps: FrameRate::new(10, 1).expect("frame rate"),
        };
        let domain = VideoDomain {
            frames: FrameCount(10),
            width: 64,
            height: 64,
            frame_rate: spec.fps,
        };
        publish_export(
            &invalid_artifact,
            &output,
            &spec,
            &domain,
            RenderMediaPolicy::default(),
            Path::new("ffmpeg"),
            Path::new("ffprobe"),
        )
        .expect_err("export failure");
        assert_eq!(
            fs::read(&output).expect("preserved output"),
            b"existing valid output"
        );
    }

    #[test]
    fn atomic_manifest_write_replaces_existing_destination() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let manifest = directory.path().join("final.mp4.manifest.json");
        fs::write(&manifest, b"old").expect("old manifest");
        write_atomically(
            &manifest,
            b"{\"new\":true}",
            "manifest",
            "json",
            "E_MANIFEST",
        )
        .expect("atomic write");
        assert_eq!(
            fs::read(&manifest).expect("new manifest"),
            b"{\"new\":true}"
        );
    }
}
