use std::fmt::Write as _;
use std::num::NonZeroU64;

use crate::diagnostic::{Diagnostic, Result};
use crate::model::{
    AudioSpec, FrameCount, FrameSampleStep, NodeId, TimelineRate, VideoDomain, VideoSpec,
};
use crate::preflight::PreparedNode;
use crate::source::SourceSpan;

use super::context::RenderContext;
use super::filters::{append_video_output, normalize_audio, samples_for_video};

pub(super) fn slice(
    context: &RenderContext<'_>,
    input: NodeId,
    start: u64,
    end: u64,
) -> Result<()> {
    let start_sample = samples_for_video(
        FrameCount(start),
        context.video(),
        context.audio(),
        context.span(),
    )?;
    let end_sample = samples_for_video(
        FrameCount(end),
        context.video(),
        context.audio(),
        context.span(),
    )?;
    let frames = FrameCount(end - start);
    let mut command = context.command();
    command.arg("-i").arg(context.artifact(input)?);
    let filter = format!(
        "[0:v]trim=start_frame={start}:end_frame={end},setpts=PTS-STARTPTS[v];[0:a]atrim=start_sample={start_sample}:end_sample={end_sample},asetpts=PTS-STARTPTS[a]"
    );
    command.args(["-filter_complex", &filter, "-map", "[v]", "-map", "[a]"]);
    append_video_output(
        &mut command,
        frames,
        context.video(),
        context.audio(),
        context.temporary(),
    );
    context.finish_ffmpeg(command)
}

pub(super) fn repeat(
    context: &RenderContext<'_>,
    input: NodeId,
    count: NonZeroU64,
    frames: FrameCount,
) -> Result<()> {
    let input_frames = context
        .nodes()
        .get(input.get() as usize)
        .and_then(PreparedNode::video_domain)
        .ok_or_else(|| {
            Diagnostic::new(
                "E_INVALID_PLAN",
                format!("repeat input {} is not an available Video", input.get()),
                context.span().clone(),
            )
        })?
        .frames();
    let audio_filter = repeat_audio_filter(
        input_frames,
        frames,
        context.video(),
        context.audio(),
        context.span(),
    )?;
    let mut command = context.command();
    command
        .args(["-stream_loop", &(count.get() - 1).to_string(), "-i"])
        .arg(context.artifact(input)?);
    let filter = format!(
        "[0:v]trim=end_frame={},setpts=PTS-STARTPTS[v];[0:a]{audio_filter}[a]",
        frames.0,
    );
    command.args(["-filter_complex", &filter, "-map", "[v]", "-map", "[a]"]);
    append_video_output(
        &mut command,
        frames,
        context.video(),
        context.audio(),
        context.temporary(),
    );
    context.finish_ffmpeg(command)
}

pub(super) fn concat(
    context: &RenderContext<'_>,
    inputs: &[NodeId],
    domain: &VideoDomain,
) -> Result<()> {
    let mut command = context.command();
    for input in inputs {
        command.arg("-i").arg(context.artifact(*input)?);
    }
    let segment_samples = video_segment_sample_counts(
        inputs,
        context.nodes(),
        context.video(),
        context.audio(),
        context.span(),
    )?;
    let mut audio_filters = String::new();
    let mut labels = String::new();
    for (index, samples) in segment_samples.into_iter().enumerate() {
        let _ = write!(
            audio_filters,
            "[{index}:a]{}[a{index}];",
            normalize_audio(samples, context.audio())
        );
        let _ = write!(labels, "[{index}:v][a{index}]");
    }
    let samples = samples_for_video(
        domain.frames(),
        context.video(),
        context.audio(),
        context.span(),
    )?;
    let filter = format!(
        "{audio_filters}{labels}concat=n={}:v=1:a=1[v][joined];[joined]{}[a]",
        inputs.len(),
        normalize_audio(samples, context.audio())
    );
    command.args(["-filter_complex", &filter, "-map", "[v]", "-map", "[a]"]);
    append_video_output(
        &mut command,
        domain.frames(),
        context.video(),
        context.audio(),
        context.temporary(),
    );
    context.finish_ffmpeg(command)
}

pub(super) fn video_segment_sample_counts(
    inputs: &[NodeId],
    nodes: &[PreparedNode],
    video: &VideoSpec,
    audio: &AudioSpec,
    span: &SourceSpan,
) -> Result<Vec<u64>> {
    let timeline = TimelineRate::new(*video, *audio);
    let mut frame_boundary = 0_u64;
    inputs
        .iter()
        .map(|input| {
            let node = nodes.get(input.get() as usize).ok_or_else(|| {
                Diagnostic::new(
                    "E_INVALID_PLAN",
                    format!("primitive input {} is not available", input.get()),
                    span.clone(),
                )
            })?;
            let domain = node.video_domain().ok_or_else(|| {
                Diagnostic::new(
                    "E_INVALID_PLAN",
                    format!("primitive input {} is not Video", input.get()),
                    span.clone(),
                )
            })?;
            let start = frame_boundary;
            frame_boundary = frame_boundary
                .checked_add(domain.frames().0)
                .ok_or_else(|| {
                    Diagnostic::new(
                        "E_FRAME_OVERFLOW",
                        "video duration exceeds the supported frame count",
                        span.clone(),
                    )
                })?;
            timeline.samples_between_frames(start, frame_boundary, span)
        })
        .collect()
}

fn repeat_audio_filter(
    input_frames: FrameCount,
    output_frames: FrameCount,
    video: &VideoSpec,
    audio: &AudioSpec,
    span: &SourceSpan,
) -> Result<String> {
    let timeline = TimelineRate::new(*video, *audio);
    let output_samples = timeline.samples_for_frames(output_frames, span)?;
    let step = timeline.frame_sample_step(input_frames, span)?;
    if step.is_integral() {
        return Ok(normalize_audio(output_samples, audio));
    }

    let input_samples = step.covering_samples().ok_or_else(|| {
        Diagnostic::new(
            "E_AUDIO_DURATION_OVERFLOW",
            "repeated audio exceeds the supported sample count",
            span.clone(),
        )
    })?;
    let frame_samples = i32::try_from(input_samples).map_err(|_| {
        Diagnostic::new(
            "E_RENDER_AUDIO_TIMELINE",
            format!(
                "phase-aligned Video repeat requires at most {} audio samples per input segment, but this segment has {input_samples}",
                i32::MAX
            ),
            span.clone(),
        )
    })?;
    Ok(format!(
        "asetnsamples=n={frame_samples}:p=0,asetpts='{}',aresample={}:async={}:min_hard_comp=0.000001:first_pts=0,aformat=sample_rates={}:channel_layouts=stereo,atrim=end_sample={output_samples},apad=whole_len={output_samples},asetpts=PTS-STARTPTS",
        repeat_audio_pts_expression(step, input_samples),
        audio.sample_rate(),
        audio.sample_rate(),
        audio.sample_rate(),
    ))
}

fn repeat_audio_pts_expression(step: FrameSampleStep, input_samples: u64) -> String {
    let segment = format!("(N/{input_samples})");
    format!(
        "({segment}*{}+ceil({segment}*{}/{}))/(SR*TB)",
        step.whole(),
        step.remainder(),
        step.denominator(),
    )
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::mem::size_of;
    use std::process::Command;

    use super::super::context::run_command;
    use super::*;
    use crate::model::FrameRate;

    #[test]
    fn repeated_audio_uses_cumulative_frame_boundaries() {
        if Command::new("ffmpeg").arg("-version").output().is_err() {
            return;
        }
        let video =
            VideoSpec::new(64, 64, FrameRate::new(25, 4).expect("frame rate")).expect("video spec");
        let audio = AudioSpec::new(10, 2).expect("audio spec");
        let span = SourceSpan::file_start("repeat-test");
        let filter = repeat_audio_filter(FrameCount(1), FrameCount(5), &video, &audio, &span)
            .expect("repeat filter");
        assert!(filter.contains("asetnsamples=n=2:p=0"));
        assert!(filter.contains("((N/2)*1+ceil((N/2)*3/5))/(SR*TB)"));

        let directory = tempfile::tempdir().expect("temporary directory");
        let output = directory.path().join("repeat.raw");
        let mut command = Command::new("ffmpeg");
        command
            .args(["-y", "-v", "error", "-f", "lavfi", "-i"])
            .arg("aevalsrc=exprs='if(eq(n,0),0.1,0.2)|if(eq(n,0),0.1,0.2)':s=10:d=0.2,aloop=loop=4:size=2")
            .args(["-filter_complex", &format!("[0:a]{filter}[a]"), "-map", "[a]"])
            .args(["-c:a", "pcm_s16le", "-f", "s16le"])
            .arg(&output);
        run_command(command, "E_TEST_FFMPEG", &span).expect("phase-aligned repeat");

        let bytes = fs::read(output).expect("raw audio");
        assert_eq!(bytes.len(), 8 * 2 * size_of::<i16>());
    }
}
