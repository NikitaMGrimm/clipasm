#![allow(clippy::trivially_copy_pass_by_ref)]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;

use crate::diagnostic::{Diagnostic, Result};
use crate::external::EXTERNAL_PROTOCOL_VERSION;
use crate::model::{
    AudioDomain, AudioSpec, FrameCount, ImageFit, NodeId, ValueType, VideoDomain, VideoSpec,
};
use crate::preflight::{
    EXPORT_PIXEL_FORMAT, PreparedAudioKind, PreparedNode, PreparedNodeMedia, PreparedPlan,
    PreparedVideoKind, WORKING_PIXEL_FORMAT,
};
use crate::source::SourceSpan;

use super::artifact::verify_video_artifact;

static TEMPORARY_COUNTER: AtomicU64 = AtomicU64::new(0);
const WOBBLE_FREQUENCY_NUMERATOR: u32 = 13;
const WOBBLE_FREQUENCY_DENOMINATOR: u32 = 2;

pub(super) struct Executor<'a> {
    plan: &'a PreparedPlan,
}

#[derive(Serialize)]
struct ExternalRunRequest<'a> {
    protocol_version: u32,
    inputs: BTreeMap<&'a str, ExternalRunInput<'a>>,
    parameters: &'a BTreeMap<String, crate::external::ExternalParameterValue>,
    output: &'a Path,
    project: ExternalRunProject<'a>,
    tools: ExternalRunTools<'a>,
}

#[derive(Serialize)]
struct ExternalRunInput<'a> {
    path: &'a Path,
    value_type: ValueType,
    domain: Option<&'a VideoDomain>,
    audio_domain: Option<&'a AudioDomain>,
    has_audio: bool,
}

#[derive(Serialize)]
struct ExternalRunProject<'a> {
    video: &'a VideoSpec,
    audio: &'a AudioSpec,
}

#[derive(Serialize)]
struct ExternalRunTools<'a> {
    ffmpeg: &'a Path,
    ffprobe: &'a Path,
}

impl<'a> Executor<'a> {
    pub(super) const fn new(plan: &'a PreparedPlan) -> Self {
        Self { plan }
    }

    pub(super) fn stage_export(
        &self,
        artifact: &Path,
        staged: &Path,
        result: &PreparedNode,
    ) -> Result<()> {
        let PreparedNodeMedia::Video {
            domain, has_audio, ..
        } = result.media()
        else {
            return Err(Diagnostic::new(
                "E_INVALID_PLAN",
                "prepared result is Audio, but rendering requires Video",
                result.origin().span.clone(),
            ));
        };
        stage_export(
            artifact,
            staged,
            self.plan.video(),
            self.plan.audio(),
            domain,
            has_audio,
            self.plan.ffmpeg().executable(),
            self.plan.ffprobe().executable(),
        )
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn render_node(
        &self,
        node: &PreparedNode,
        artifacts: &[PathBuf],
        destination: &Path,
    ) -> Result<()> {
        let nodes = self.plan.nodes();
        let spec = self.plan.video();
        let audio = self.plan.audio();
        let ffmpeg = self.plan.ffmpeg().executable();
        let ffprobe = self.plan.ffprobe().executable();
        let extension = if node.value_type() == ValueType::Audio {
            "mka"
        } else {
            "mkv"
        };
        let temporary = temporary_sibling(destination, "cache", extension);
        let mut command = Command::new(ffmpeg);
        command.args(["-y", "-v", "error"]);
        match node.media() {
            PreparedNodeMedia::Video {
                kind: PreparedVideoKind::ImageVideo { asset, fit, frames },
                ..
            } => {
                let samples = samples_for_video(*frames, spec, audio, &node.origin().span)?;
                command.args(["-loop", "1", "-i"]).arg(asset.source_path());
                command
                    .args(["-f", "lavfi", "-i"])
                    .arg(silence_source(audio));
                let filter = format!(
                    "[0:v]{},trim=end_frame={},setpts=PTS-STARTPTS[v];[1:a]{}[a]",
                    image_filter(*fit, spec),
                    frames.0,
                    normalize_audio(samples, audio)
                );
                command.args(["-filter_complex", &filter, "-map", "[v]", "-map", "[a]"]);
                append_video_output(&mut command, *frames, spec, audio, &temporary);
            }
            PreparedNodeMedia::Video {
                kind: PreparedVideoKind::VideoSource { asset, fit, frames },
                has_audio,
                ..
            } => {
                let samples = samples_for_video(*frames, spec, audio, &node.origin().span)?;
                command.arg("-i").arg(asset.source_path());
                let audio_input = if has_audio {
                    "[0:a:0]".to_owned()
                } else {
                    command
                        .args(["-f", "lavfi", "-i"])
                        .arg(silence_source(audio));
                    "[1:a]".to_owned()
                };
                let filter = format!(
                    "[0:v]{}[v];{audio_input}{}[a]",
                    video_filter(*fit, *frames, spec),
                    normalize_audio(samples, audio)
                );
                command.args(["-filter_complex", &filter, "-map", "[v]", "-map", "[a]"]);
                append_video_output(&mut command, *frames, spec, audio, &temporary);
            }
            PreparedNodeMedia::Audio {
                kind: PreparedAudioKind::AudioSource { asset },
                domain,
            } => {
                command.arg("-i").arg(asset.source_path());
                let filter = format!("[0:a:0]{}[a]", normalize_audio(domain.samples, audio));
                command.args(["-filter_complex", &filter, "-map", "[a]"]);
                append_audio_output(&mut command, audio, &temporary);
            }
            PreparedNodeMedia::Audio {
                kind: PreparedAudioKind::AudioSlice { input, range },
                ..
            } => {
                command
                    .arg("-i")
                    .arg(artifact(artifacts, *input, &node.origin().span)?);
                let filter = format!(
                    "[0:a]atrim=start_sample={}:end_sample={},asetpts=PTS-STARTPTS[a]",
                    range.start(),
                    range.end()
                );
                command.args(["-filter_complex", &filter, "-map", "[a]"]);
                append_audio_output(&mut command, audio, &temporary);
            }
            PreparedNodeMedia::Audio {
                kind: PreparedAudioKind::AudioRepeat { input, count },
                domain,
            } => {
                command
                    .args(["-stream_loop", &(count.get() - 1).to_string(), "-i"])
                    .arg(artifact(artifacts, *input, &node.origin().span)?);
                let filter = format!("[0:a]{}[a]", normalize_audio(domain.samples, audio));
                command.args(["-filter_complex", &filter, "-map", "[a]"]);
                append_audio_output(&mut command, audio, &temporary);
            }
            PreparedNodeMedia::Audio {
                kind: PreparedAudioKind::AudioConcat { inputs },
                domain,
            } => {
                for input in inputs {
                    command
                        .arg("-i")
                        .arg(artifact(artifacts, *input, &node.origin().span)?);
                }
                let labels = (0..inputs.len()).fold(String::new(), |mut output, index| {
                    let _ = write!(output, "[{index}:a]");
                    output
                });
                let filter = format!(
                    "{labels}concat=n={}:v=0:a=1[joined];[joined]{}[a]",
                    inputs.len(),
                    normalize_audio(domain.samples, audio)
                );
                command.args(["-filter_complex", &filter, "-map", "[a]"]);
                append_audio_output(&mut command, audio, &temporary);
            }
            PreparedNodeMedia::Video {
                kind: PreparedVideoKind::Slice { input, range },
                ..
            } => {
                command
                    .arg("-i")
                    .arg(artifact(artifacts, *input, &node.origin().span)?);
                let start =
                    samples_for_video(FrameCount(range.start()), spec, audio, &node.origin().span)?;
                let end =
                    samples_for_video(FrameCount(range.end()), spec, audio, &node.origin().span)?;
                let filter = format!(
                    "[0:v]trim=start_frame={}:end_frame={},setpts=PTS-STARTPTS[v];[0:a]atrim=start_sample={start}:end_sample={end},asetpts=PTS-STARTPTS[a]",
                    range.start(),
                    range.end()
                );
                command.args(["-filter_complex", &filter, "-map", "[v]", "-map", "[a]"]);
                append_video_output(&mut command, range.frames(), spec, audio, &temporary);
            }
            PreparedNodeMedia::Video {
                kind:
                    PreparedVideoKind::Repeat {
                        input,
                        count,
                        frames,
                    },
                ..
            } => {
                command
                    .args(["-stream_loop", &(count.get() - 1).to_string(), "-i"])
                    .arg(artifact(artifacts, *input, &node.origin().span)?);
                let samples = samples_for_video(*frames, spec, audio, &node.origin().span)?;
                let filter = format!(
                    "[0:v]trim=end_frame={},setpts=PTS-STARTPTS[v];[0:a]{}[a]",
                    frames.0,
                    normalize_audio(samples, audio)
                );
                command.args(["-filter_complex", &filter, "-map", "[v]", "-map", "[a]"]);
                append_video_output(&mut command, *frames, spec, audio, &temporary);
            }
            PreparedNodeMedia::Video {
                kind: PreparedVideoKind::Zoom { input, percent },
                domain,
                ..
            } => {
                command
                    .arg("-i")
                    .arg(artifact(artifacts, *input, &node.origin().span)?);
                let samples = samples_for_video(domain.frames, spec, audio, &node.origin().span)?;
                let filter = format!(
                    "[0:v]{}[v];[0:a]{}[a]",
                    zoom_filter(*percent, domain.frames),
                    normalize_audio(samples, audio)
                );
                command.args(["-filter_complex", &filter, "-map", "[v]", "-map", "[a]"]);
                append_video_output(&mut command, domain.frames, spec, audio, &temporary);
            }
            PreparedNodeMedia::Video {
                kind: PreparedVideoKind::Wobble { input, pixels },
                domain,
                ..
            } => {
                command
                    .arg("-i")
                    .arg(artifact(artifacts, *input, &node.origin().span)?);
                let samples = samples_for_video(domain.frames, spec, audio, &node.origin().span)?;
                let filter = format!(
                    "[0:v]{}[v];[0:a]{}[a]",
                    wobble_filter(*pixels, spec),
                    normalize_audio(samples, audio)
                );
                command.args(["-filter_complex", &filter, "-map", "[v]", "-map", "[a]"]);
                append_video_output(&mut command, domain.frames, spec, audio, &temporary);
            }
            PreparedNodeMedia::Video {
                kind:
                    PreparedVideoKind::FlashJoin {
                        before,
                        after,
                        frames,
                    },
                domain,
                ..
            } => {
                command
                    .arg("-i")
                    .arg(artifact(artifacts, *before, &node.origin().span)?);
                command
                    .arg("-i")
                    .arg(artifact(artifacts, *after, &node.origin().span)?);
                let samples = samples_for_video(domain.frames, spec, audio, &node.origin().span)?;
                let filter = format!(
                    "[1:v]fade=t=in:start_frame=0:nb_frames={}:color=white[after];[0:v][0:a][after][1:a]concat=n=2:v=1:a=1[v][joined];[joined]{}[a]",
                    frames.0,
                    normalize_audio(samples, audio)
                );
                command.args(["-filter_complex", &filter, "-map", "[v]", "-map", "[a]"]);
                append_video_output(&mut command, domain.frames, spec, audio, &temporary);
            }
            PreparedNodeMedia::Video {
                kind: PreparedVideoKind::Concat { inputs },
                domain,
                ..
            } => {
                for input in inputs {
                    command
                        .arg("-i")
                        .arg(artifact(artifacts, *input, &node.origin().span)?);
                }
                let labels = (0..inputs.len()).fold(String::new(), |mut output, index| {
                    let _ = write!(output, "[{index}:v][{index}:a]");
                    output
                });
                let samples = samples_for_video(domain.frames, spec, audio, &node.origin().span)?;
                let filter = format!(
                    "{labels}concat=n={}:v=1:a=1[v][joined];[joined]{}[a]",
                    inputs.len(),
                    normalize_audio(samples, audio)
                );
                command.args(["-filter_complex", &filter, "-map", "[v]", "-map", "[a]"]);
                append_video_output(&mut command, domain.frames, spec, audio, &temporary);
            }
            PreparedNodeMedia::Audio {
                kind: PreparedAudioKind::ExtractAudio { video },
                domain,
            } => {
                command
                    .arg("-i")
                    .arg(artifact(artifacts, *video, &node.origin().span)?);
                let filter = format!("[0:a]{}[a]", normalize_audio(domain.samples, audio));
                command.args(["-filter_complex", &filter, "-map", "[a]"]);
                append_audio_output(&mut command, audio, &temporary);
            }
            PreparedNodeMedia::Video {
                kind:
                    PreparedVideoKind::SetAudio {
                        audio: audio_node,
                        video,
                    },
                domain,
                ..
            } => {
                command
                    .arg("-i")
                    .arg(artifact(artifacts, *audio_node, &node.origin().span)?);
                command
                    .arg("-i")
                    .arg(artifact(artifacts, *video, &node.origin().span)?);
                let samples = samples_for_video(domain.frames, spec, audio, &node.origin().span)?;
                let filter = format!(
                    "[1:v]trim=end_frame={},setpts=PTS-STARTPTS[v];[0:a]{}[a]",
                    domain.frames.0,
                    normalize_audio(samples, audio)
                );
                command.args(["-filter_complex", &filter, "-map", "[v]", "-map", "[a]"]);
                append_video_output(&mut command, domain.frames, spec, audio, &temporary);
            }
            PreparedNodeMedia::Video {
                kind: PreparedVideoKind::AudioOnBlack { audio: audio_node },
                domain,
                ..
            } => {
                command.args(["-f", "lavfi", "-i"]).arg(format!(
                    "color=c=black:s={}x{}:r={}/{}",
                    spec.width,
                    spec.height,
                    spec.fps.numerator(),
                    spec.fps.denominator()
                ));
                command
                    .arg("-i")
                    .arg(artifact(artifacts, *audio_node, &node.origin().span)?);
                let samples = samples_for_video(domain.frames, spec, audio, &node.origin().span)?;
                let filter = format!(
                    "[0:v]trim=end_frame={},setpts=PTS-STARTPTS,format={}[v];[1:a]{}[a]",
                    domain.frames.0,
                    WORKING_PIXEL_FORMAT,
                    normalize_audio(samples, audio)
                );
                command.args(["-filter_complex", &filter, "-map", "[v]", "-map", "[a]"]);
                append_video_output(&mut command, domain.frames, spec, audio, &temporary);
            }
            PreparedNodeMedia::Video {
                kind:
                    PreparedVideoKind::ExternalVideo {
                        executable,
                        inputs,
                        parameters,
                        ..
                    },
                ..
            } => {
                let inputs = inputs
                    .iter()
                    .map(|(name, id)| {
                        let input_node = &nodes[id.get() as usize];
                        Ok((
                            name.as_str(),
                            ExternalRunInput {
                                path: artifact(artifacts, *id, &node.origin().span)?,
                                value_type: input_node.value_type(),
                                domain: input_node.video_domain(),
                                audio_domain: input_node.audio_domain(),
                                has_audio: input_node.has_audio(),
                            },
                        ))
                    })
                    .collect::<Result<BTreeMap<_, _>>>()?;
                let request = ExternalRunRequest {
                    protocol_version: EXTERNAL_PROTOCOL_VERSION,
                    inputs,
                    parameters,
                    output: &temporary,
                    project: ExternalRunProject { video: spec, audio },
                    tools: ExternalRunTools { ffmpeg, ffprobe },
                };
                run_external(executable.executable(), &request, &node.origin().span)?;
                atomic_replace(&temporary, destination, "E_CACHE_IO")?;
                return Ok(());
            }
        }
        if let Err(error) = run_command(command, "E_FFMPEG", &node.origin().span) {
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }
        atomic_replace(&temporary, destination, "E_CACHE_IO")
    }
}

#[allow(clippy::too_many_arguments)]
fn stage_export(
    artifact: &Path,
    staged: &Path,
    spec: &VideoSpec,
    audio: &AudioSpec,
    domain: &VideoDomain,
    has_audio: bool,
    ffmpeg: &Path,
    ffprobe: &Path,
) -> Result<()> {
    let result = export_mp4(
        artifact,
        staged,
        spec,
        audio,
        domain.frames,
        has_audio,
        ffmpeg,
    )
    .and_then(|()| {
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
    frames: FrameCount,
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
            spec.fps.numerator(),
            spec.fps.denominator()
        ));
    if has_audio {
        command.args([
            "-map",
            "0:a:0",
            "-c:a",
            "aac",
            "-ar",
            &audio.sample_rate.to_string(),
            "-ac",
            &audio.channels.to_string(),
        ]);
    } else {
        command.arg("-an");
    }
    command
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

fn run_external(
    executable: &Path,
    request: &ExternalRunRequest<'_>,
    span: &SourceSpan,
) -> Result<()> {
    let mut child = Command::new(executable)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            Diagnostic::new(
                "E_EXTERNAL_EXECUTION",
                format!(
                    "could not start external program `{}`: {error}",
                    executable.display()
                ),
                span.clone(),
            )
        })?;
    let request = serde_json::to_vec(request).map_err(|error| {
        Diagnostic::new(
            "E_EXTERNAL_PROTOCOL",
            format!("could not serialize external program request: {error}"),
            span.clone(),
        )
    })?;
    child
        .stdin
        .take()
        .expect("piped external stdin")
        .write_all(&request)
        .map_err(|error| {
            Diagnostic::new(
                "E_EXTERNAL_EXECUTION",
                format!("could not write external program request: {error}"),
                span.clone(),
            )
        })?;
    let output = child.wait_with_output().map_err(|error| {
        Diagnostic::new(
            "E_EXTERNAL_EXECUTION",
            format!("could not wait for external program: {error}"),
            span.clone(),
        )
    })?;
    if output.status.success() {
        return Ok(());
    }
    Err(Diagnostic::new(
        "E_EXTERNAL_EXECUTION",
        format!(
            "external program `{}` failed with {}
{}",
            executable.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ),
        span.clone(),
    ))
}

fn append_video_output(
    command: &mut Command,
    frames: FrameCount,
    spec: &VideoSpec,
    audio: &AudioSpec,
    destination: &Path,
) {
    command
        .args(["-frames:v", &frames.0.to_string(), "-c:v", "ffv1"])
        .args(["-level", "3", "-pix_fmt", WORKING_PIXEL_FORMAT, "-r"])
        .arg(format!(
            "{}/{}",
            spec.fps.numerator(),
            spec.fps.denominator()
        ))
        .args([
            "-c:a",
            "flac",
            "-ar",
            &audio.sample_rate.to_string(),
            "-ac",
            &audio.channels.to_string(),
        ])
        .arg(destination);
}

fn append_audio_output(command: &mut Command, audio: &AudioSpec, destination: &Path) {
    command
        .args([
            "-c:a",
            "flac",
            "-ar",
            &audio.sample_rate.to_string(),
            "-ac",
            &audio.channels.to_string(),
            "-f",
            "matroska",
        ])
        .arg(destination);
}

fn samples_for_video(
    frames: FrameCount,
    spec: &VideoSpec,
    audio: &AudioSpec,
    span: &SourceSpan,
) -> Result<u64> {
    audio.samples_for_frames(frames, spec.fps, span)
}

fn silence_source(audio: &AudioSpec) -> String {
    format!("anullsrc=r={}:cl=stereo", audio.sample_rate)
}

fn normalize_audio(samples: u64, audio: &AudioSpec) -> String {
    format!(
        "aresample={},aformat=sample_rates={}:channel_layouts=stereo,atrim=end_sample={samples},apad=whole_len={samples},asetpts=PTS-STARTPTS",
        audio.sample_rate, audio.sample_rate
    )
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
        "{geometry},fps={}/{},setsar=1,format={}",
        spec.fps.numerator(),
        spec.fps.denominator(),
        WORKING_PIXEL_FORMAT
    )
}

fn video_filter(fit: ImageFit, frames: FrameCount, spec: &VideoSpec) -> String {
    format!(
        "setpts=PTS-STARTPTS,{},tpad=stop_mode=clone:stop=1,trim=end_frame={},setpts=PTS-STARTPTS",
        image_filter(fit, spec),
        frames.0
    )
}

fn zoom_filter(percent: u32, frames: FrameCount) -> String {
    let last_frame = frames.0.saturating_sub(1).max(1);
    let zoom = format!("(1+{percent}*(in-1)/(100*{last_frame}))");
    let x_margin = format!("W*(1-1/{zoom})/2");
    let y_margin = format!("H*(1-1/{zoom})/2");
    format!(
        "perspective=x0='{x_margin}':y0='{y_margin}':x1='W-{x_margin}':y1='{y_margin}':x2='{x_margin}':y2='H-{y_margin}':x3='W-{x_margin}':y3='H-{y_margin}':sense=source:eval=frame:interpolation=cubic,setpts=PTS-STARTPTS"
    )
}

fn wobble_filter(pixels: u32, spec: &VideoSpec) -> String {
    let padding = pixels * 2;
    let scaled_width = spec
        .width
        .checked_add(padding)
        .expect("wobble dimensions were validated during compilation");
    let scaled_height = spec
        .height
        .checked_add(padding)
        .expect("wobble dimensions were validated during compilation");
    let phase = format!(
        "2*PI*{}*n*{}/({}*{})",
        WOBBLE_FREQUENCY_NUMERATOR,
        spec.fps.denominator(),
        WOBBLE_FREQUENCY_DENOMINATOR,
        spec.fps.numerator()
    );
    format!(
        "scale={scaled_width}:{scaled_height},setsar=1,crop={}:{}:x='{pixels}*(1+sin({phase}))':y='{pixels}*(1+sin({phase}+PI/2))',setpts=PTS-STARTPTS",
        spec.width, spec.height
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

pub(super) fn run_command(command: Command, code: &'static str, span: &SourceSpan) -> Result<()> {
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
    use super::super::publication::PublicationTransaction;
    use super::*;
    use crate::model::FrameRate;

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
