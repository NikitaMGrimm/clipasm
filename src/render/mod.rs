use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::compiler::{CompiledPlan, PlanNode, PrimitiveNodeKind};
use crate::diagnostic::{Diagnostic, Result, SourceSpan};
use crate::model::{FrameCount, ImageFit, NodeId, VideoSpec};

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
    plan: &'a CompiledPlan,
}

#[derive(Deserialize)]
struct ProbeDocument {
    streams: Vec<ProbeStream>,
    #[serde(default)]
    frames: Vec<ProbeFrame>,
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

#[derive(Deserialize)]
struct ProbeFrame {
    best_effort_timestamp_time: Option<String>,
}

/// Execute a compiled plan, verify artifacts, export MP4, and write a manifest.
///
/// # Errors
///
/// Returns a diagnostic when tool preflight, rendering, cache I/O, artifact
/// verification, export, or manifest writing fails.
#[allow(clippy::too_many_lines)]
pub fn render(plan: &CompiledPlan, workflow_path: &Path) -> Result<RenderReport> {
    let output_path = plan.output.as_ref().ok_or_else(|| {
        Diagnostic::new(
            "E_MISSING_OUTPUT",
            "`render` requires the top-level `output` field",
            SourceSpan::file_start(workflow_path),
        )
    })?;
    let ffmpeg_version = tool_version("ffmpeg")?;
    let ffprobe_version = tool_version("ffprobe")?;
    let namespace = hex::encode(Sha256::digest(format!(
        "{ffmpeg_version}\n{ffprobe_version}\nrender-v1"
    )));
    let workflow_directory = workflow_path.parent().unwrap_or_else(|| Path::new("."));
    let cache_directory = workflow_directory
        .join(".rhythmcut")
        .join("cache")
        .join(namespace);
    fs::create_dir_all(&cache_directory).map_err(|error| {
        Diagnostic::new(
            "E_CACHE_IO",
            format!(
                "could not create cache directory `{}`: {error}",
                cache_directory.display()
            ),
            SourceSpan::file_start(workflow_path),
        )
    })?;

    let mut artifacts = Vec::<PathBuf>::with_capacity(plan.nodes.len());
    let mut cache_hits = 0;
    let mut cache_misses = 0;
    for node in &plan.nodes {
        if node.id.0 as usize != artifacts.len() {
            return Err(Diagnostic::new(
                "E_INVALID_PLAN",
                "primitive nodes are not in stable topological order",
                node.origin.span.clone(),
            ));
        }
        let artifact = cache_directory.join(format!("{}.mkv", node.fingerprint));
        let hit =
            artifact.is_file() && verify_artifact(&artifact, &plan.video, node.frames).is_ok();
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
                        node.origin.span.clone(),
                    )
                })?;
            }
            render_node(node, &artifacts, &artifact, &plan.video)?;
            verify_artifact(&artifact, &plan.video, node.frames)?;
        }
        artifacts.push(artifact);
    }

    let root_artifact = artifacts.get(plan.root.0 as usize).ok_or_else(|| {
        Diagnostic::new(
            "E_INVALID_PLAN",
            "compiled root does not identify a primitive artifact",
            SourceSpan::file_start(workflow_path),
        )
    })?;
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            Diagnostic::new(
                "E_OUTPUT_IO",
                format!(
                    "could not create output directory `{}`: {error}",
                    parent.display()
                ),
                SourceSpan::file_start(workflow_path),
            )
        })?;
    }
    export_mp4(
        root_artifact,
        output_path,
        &plan.video,
        plan.nodes[plan.root.0 as usize].frames,
    )?;
    verify_artifact(
        output_path,
        &plan.video,
        plan.nodes[plan.root.0 as usize].frames,
    )?;

    let manifest_path = PathBuf::from(format!("{}.manifest.json", output_path.display()));
    let manifest = Manifest {
        engine_version: env!("CARGO_PKG_VERSION"),
        ffmpeg: &ffmpeg_version,
        ffprobe: &ffprobe_version,
        cache_hits,
        cache_misses,
        plan,
    };
    let manifest_json = serde_json::to_string_pretty(&manifest).map_err(|error| {
        Diagnostic::new(
            "E_MANIFEST",
            format!("could not serialize render manifest: {error}"),
            SourceSpan::file_start(workflow_path),
        )
    })?;
    fs::write(&manifest_path, manifest_json).map_err(|error| {
        Diagnostic::new(
            "E_MANIFEST",
            format!(
                "could not write manifest `{}`: {error}",
                manifest_path.display()
            ),
            SourceSpan::file_start(workflow_path),
        )
    })?;

    Ok(RenderReport {
        output: output_path.clone(),
        manifest: manifest_path,
        cache_hits,
        cache_misses,
    })
}

fn render_node(
    node: &PlanNode,
    artifacts: &[PathBuf],
    destination: &Path,
    spec: &VideoSpec,
) -> Result<()> {
    let temporary = destination.with_extension(format!("{}.tmp.mkv", std::process::id()));
    if temporary.exists() {
        fs::remove_file(&temporary).map_err(|error| {
            Diagnostic::new(
                "E_CACHE_IO",
                format!(
                    "could not clear temporary artifact `{}`: {error}",
                    temporary.display()
                ),
                node.origin.span.clone(),
            )
        })?;
    }
    let mut command = Command::new("ffmpeg");
    command.args(["-y", "-v", "error"]);
    match &node.kind {
        PrimitiveNodeKind::ImageVideo {
            path, fit, frames, ..
        } => {
            command.args(["-loop", "1", "-i"]).arg(path);
            command.args(["-vf", &image_filter(*fit, spec)]);
            append_lossless_output(&mut command, *frames, spec, &temporary);
        }
        PrimitiveNodeKind::Slice { input, range } => {
            command
                .arg("-i")
                .arg(artifact(artifacts, *input, &node.origin.span)?);
            command.args([
                "-vf",
                &format!(
                    "trim=start_frame={}:end_frame={},setpts=PTS-STARTPTS",
                    range.start, range.end
                ),
            ]);
            append_lossless_output(&mut command, range.frames(), spec, &temporary);
        }
        PrimitiveNodeKind::Concat { inputs } => {
            for input in inputs {
                command
                    .arg("-i")
                    .arg(artifact(artifacts, *input, &node.origin.span)?);
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
            append_lossless_output(&mut command, node.frames, spec, &temporary);
        }
    }
    run_command(command, "E_FFMPEG", &node.origin.span)?;
    fs::rename(&temporary, destination).map_err(|error| {
        Diagnostic::new(
            "E_CACHE_IO",
            format!(
                "could not store cache artifact `{}`: {error}",
                destination.display()
            ),
            node.origin.span.clone(),
        )
    })
}

fn append_lossless_output(
    command: &mut Command,
    frames: FrameCount,
    spec: &VideoSpec,
    destination: &Path,
) {
    command
        .args(["-frames:v", &frames.0.to_string(), "-an", "-c:v", "ffv1"])
        .args(["-level", "3", "-pix_fmt", "yuv420p", "-r"])
        .arg(format!("{}/{}", spec.fps.numerator, spec.fps.denominator))
        .arg(destination);
}

fn export_mp4(artifact: &Path, output: &Path, spec: &VideoSpec, frames: FrameCount) -> Result<()> {
    let mut command = Command::new("ffmpeg");
    command
        .args(["-y", "-v", "error", "-i"])
        .arg(artifact)
        .args(["-an", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-r"])
        .arg(format!("{}/{}", spec.fps.numerator, spec.fps.denominator))
        .args([
            "-frames:v",
            &frames.0.to_string(),
            "-movflags",
            "+faststart",
        ])
        .arg(output);
    run_command(command, "E_FFMPEG", &SourceSpan::file_start(output))
}

fn image_filter(fit: ImageFit, spec: &VideoSpec) -> String {
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
        "{geometry},fps={}/{},setsar=1,format=yuv420p",
        spec.fps.numerator, spec.fps.denominator
    )
}

fn artifact<'a>(artifacts: &'a [PathBuf], id: NodeId, span: &SourceSpan) -> Result<&'a Path> {
    artifacts
        .get(id.0 as usize)
        .map(PathBuf::as_path)
        .ok_or_else(|| {
            Diagnostic::new(
                "E_INVALID_PLAN",
                format!("primitive input {} is not available", id.0),
                span.clone(),
            )
        })
}

#[allow(clippy::too_many_lines)]
fn verify_artifact(path: &Path, spec: &VideoSpec, frames: FrameCount) -> Result<()> {
    let mut command = Command::new("ffprobe");
    command
        .args([
            "-v",
            "error",
            "-count_frames",
            "-show_streams",
            "-show_frames",
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
    if video.width != Some(spec.width) || video.height != Some(spec.height) {
        return Err(contract_error(
            path,
            &format!(
                "expected {}x{}, found {:?}x{:?}",
                spec.width, spec.height, video.width, video.height
            ),
        ));
    }
    if video.pix_fmt.as_deref() != Some("yuv420p") {
        return Err(contract_error(
            path,
            &format!("expected yuv420p, found {:?}", video.pix_fmt),
        ));
    }
    let expected_rate = format!("{}/{}", spec.fps.numerator, spec.fps.denominator);
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
    if actual_frames != Some(frames.0) {
        return Err(contract_error(
            path,
            &format!(
                "expected {} frames, FFprobe counted {:?}",
                frames.0, actual_frames
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
    let timestamps = document
        .frames
        .iter()
        .filter_map(|frame| {
            frame
                .best_effort_timestamp_time
                .as_deref()
                .and_then(|value| value.parse::<f64>().ok())
        })
        .collect::<Vec<_>>();
    if u64::try_from(timestamps.len()) != Ok(frames.0)
        || timestamps.windows(2).any(|pair| pair[0] >= pair[1])
    {
        return Err(contract_error(
            path,
            "decoded frame timestamps are missing or are not strictly monotonic",
        ));
    }
    Ok(())
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

fn tool_version(tool: &str) -> Result<String> {
    let mut command = Command::new(tool);
    command.arg("-version");
    let output = run_output(
        command,
        if tool == "ffmpeg" {
            "E_FFMPEG"
        } else {
            "E_FFPROBE"
        },
        &SourceSpan::file_start(tool),
    )?;
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or_default()
        .to_owned())
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
