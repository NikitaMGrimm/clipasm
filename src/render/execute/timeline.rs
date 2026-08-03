use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::model::{AudioSpec, FrameCount, FrameSampleStep, NodeId, TimelineRate, VideoSpec};
use crate::preflight::{AudioEncoding, PreparedNode};
use crate::source::SourceSpan;

use super::filters::normalize_audio;

pub(super) fn video_segment_sample_counts(
    inputs: &[NodeId],
    nodes: &[PreparedNode],
    video: &VideoSpec,
    audio: AudioSpec,
    span: &SourceSpan,
) -> Result<Vec<u64>> {
    let timeline = TimelineRate::new(*video, audio);
    let mut frame_boundary = 0_u64;
    inputs
        .iter()
        .map(|input| {
            let node = nodes.get(input.get() as usize).ok_or_else(|| {
                Diagnostic::builtin(
                    BuiltinDiagnostic::InvalidPlan,
                    format!("primitive input {} is not available", input.get()),
                    span.clone(),
                )
            })?;
            let domain = node.video_domain().ok_or_else(|| {
                Diagnostic::builtin(
                    BuiltinDiagnostic::InvalidPlan,
                    format!("primitive input {} is not Video", input.get()),
                    span.clone(),
                )
            })?;
            let start = frame_boundary;
            frame_boundary = frame_boundary
                .checked_add(domain.frames().0)
                .ok_or_else(|| {
                    Diagnostic::builtin(
                        BuiltinDiagnostic::FrameOverflow,
                        "video duration exceeds the supported frame count",
                        span.clone(),
                    )
                })?;
            timeline.samples_between_frames(start, frame_boundary, span)
        })
        .collect()
}

pub(super) fn repeat_audio_filter(
    input_frames: FrameCount,
    output_frames: FrameCount,
    video: &VideoSpec,
    audio: AudioSpec,
    audio_encoding: AudioEncoding,
    span: &SourceSpan,
) -> Result<String> {
    let timeline = TimelineRate::new(*video, audio);
    let output_samples = timeline.samples_for_frames(output_frames, span)?;
    let step = timeline.frame_sample_step(input_frames, span)?;
    if step.is_integral() {
        return Ok(normalize_audio(output_samples, audio, audio_encoding));
    }

    let input_samples = step.covering_samples().ok_or_else(|| {
        Diagnostic::builtin(
            BuiltinDiagnostic::AudioDurationOverflow,
            "repeated audio exceeds the supported sample count",
            span.clone(),
        )
    })?;
    let frame_samples = i32::try_from(input_samples).map_err(|_| {
        Diagnostic::builtin(
            BuiltinDiagnostic::RenderAudioTimeline,
            format!(
                "phase-aligned Video repeat requires at most {} audio samples per input segment, but this segment has {input_samples}",
                i32::MAX
            ),
            span.clone(),
        )
    })?;
    Ok(format!(
        "asetnsamples=n={frame_samples}:p=0,asetpts='{}',aresample={}:async={}:min_hard_comp=0.000001:first_pts=0,aformat=sample_fmts={}:sample_rates={}:channel_layouts={},atrim=end_sample={output_samples},apad=whole_len={output_samples},asetpts=PTS-STARTPTS",
        repeat_audio_pts_expression(step, input_samples),
        audio.sample_rate(),
        audio.sample_rate(),
        audio_encoding.sample_format(),
        audio.sample_rate(),
        audio_encoding.channel_layout(),
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

#[cfg(all(test, feature = "native"))]
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
        let filter = repeat_audio_filter(
            FrameCount(1),
            FrameCount(5),
            &video,
            audio,
            crate::preflight::RenderPolicy::CURRENT.working_audio_encoding(),
            &span,
        )
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
        run_command(command, BuiltinDiagnostic::Ffmpeg, &span).expect("phase-aligned repeat");

        let bytes = fs::read(output).expect("raw audio");
        assert_eq!(bytes.len(), 8 * 2 * size_of::<i16>());
    }
}
