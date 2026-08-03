use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::model::{AudioDomain, FrameCount, NodeId, TimelineRate, VideoDomain};
use crate::preflight::{AudioEncoding, PreparedNode};

use super::super::color::{linear_rgb_to_encoding, working_to_linear_rgb};
use super::super::filters::normalize_audio;
use super::super::graph::{GraphBuilder, NodePads, Pad, VideoPads};
use super::super::recipe::RecipeContext;

pub(in crate::render::execute) fn lower_video(
    graph: &mut GraphBuilder<'_, '_>,
    before: NodeId,
    after: NodeId,
    frames: FrameCount,
    domain: &VideoDomain,
) -> Result<NodePads> {
    let layout = CrossfadeLayout::new(graph.context(), before, after, frames, domain)?;
    let before = graph.video_input(before)?;
    let after = graph.video_input(after)?;
    let video = append_crossfade_video_filter(
        graph,
        &layout,
        before.video,
        after.video,
        graph.context().policy().working_video_encoding(),
    );
    let audio = append_crossfade_audio_filter(
        graph,
        &layout.audio_layout(),
        before.audio,
        after.audio,
        graph.context().audio(),
        graph.context().policy().working_audio_encoding(),
    );
    Ok(NodePads::Video(VideoPads { video, audio }))
}

pub(in crate::render::execute) fn lower_audio(
    graph: &mut GraphBuilder<'_, '_>,
    before: NodeId,
    after: NodeId,
    samples: u64,
    domain: &AudioDomain,
) -> Result<NodePads> {
    let layout = AudioCrossfadeLayout::new(graph.context(), before, after, samples, domain)?;
    let before = graph.audio_input(before)?;
    let after = graph.audio_input(after)?;
    let audio = append_crossfade_audio_filter(
        graph,
        &layout,
        before,
        after,
        graph.context().audio(),
        graph.context().policy().working_audio_encoding(),
    );
    Ok(NodePads::Audio(audio))
}

fn append_crossfade_video_filter(
    graph: &mut GraphBuilder<'_, '_>,
    layout: &CrossfadeLayout,
    before: Pad,
    after: Pad,
    working: crate::preflight::VideoEncoding,
) -> Pad {
    let (prefix, before_overlap_source) = before_video_pieces(graph, layout, before);
    let (after_overlap_source, suffix) = after_video_pieces(graph, layout, after);

    let before_overlap = graph.output_video_pad();
    graph.clause(format!(
        "{}trim=start_frame={}:end_frame={},setpts=PTS-STARTPTS{}",
        before_overlap_source.bracketed(),
        layout.prefix_frames,
        layout.before_frames,
        before_overlap.bracketed(),
    ));
    let after_overlap = graph.output_video_pad();
    graph.clause(format!(
        "{}trim=end_frame={},setpts=PTS-STARTPTS{}",
        after_overlap_source.bracketed(),
        layout.overlap_frames,
        after_overlap.bracketed(),
    ));
    let before_linear = graph.output_video_pad();
    graph.clause(format!(
        "{}{}{}",
        before_overlap.bracketed(),
        working_to_linear_rgb(graph.context().policy().working_video_encoding()),
        before_linear.bracketed(),
    ));
    let after_linear = graph.output_video_pad();
    graph.clause(format!(
        "{}{}{}",
        after_overlap.bracketed(),
        working_to_linear_rgb(graph.context().policy().working_video_encoding()),
        after_linear.bracketed(),
    ));
    let overlap = graph.output_video_pad();
    graph.clause(format!(
        "{}{}blend=all_expr='{}':shortest=1:repeatlast=0,trim=end_frame={},setpts=PTS-STARTPTS,{}{}",
        before_linear.bracketed(),
        after_linear.bracketed(),
        blend_expression(layout.overlap_frames),
        layout.overlap_frames,
        linear_rgb_to_encoding(working),
        overlap.bracketed(),
    ));

    let mut pieces = Vec::with_capacity(3);
    pieces.extend(prefix);
    pieces.push(overlap);
    pieces.extend(suffix);
    let joined = if pieces.len() == 1 {
        pieces.pop().expect("crossfade has one overlap piece")
    } else {
        let labels = pieces.iter().map(Pad::bracketed).collect::<String>();
        let joined = graph.output_video_pad();
        graph.clause(format!(
            "{labels}concat=n={}:v=1:a=0{}",
            pieces.len(),
            joined.bracketed(),
        ));
        joined
    };
    let output = graph.output_video_pad();
    graph.clause(format!(
        "{}trim=end_frame={},setpts=PTS-STARTPTS{}",
        joined.bracketed(),
        layout.output_frames,
        output.bracketed(),
    ));
    output
}

fn before_video_pieces(
    graph: &mut GraphBuilder<'_, '_>,
    layout: &CrossfadeLayout,
    before: Pad,
) -> (Option<Pad>, Pad) {
    if layout.prefix_frames > 0 {
        let prefix_source = graph.output_video_pad();
        let overlap_source = graph.output_video_pad();
        graph.clause(format!(
            "{}split=2{}{}",
            before.bracketed(),
            prefix_source.bracketed(),
            overlap_source.bracketed(),
        ));
        let prefix = graph.output_video_pad();
        graph.clause(format!(
            "{}trim=end_frame={},setpts=PTS-STARTPTS{}",
            prefix_source.bracketed(),
            layout.prefix_frames,
            prefix.bracketed(),
        ));
        (Some(prefix), overlap_source)
    } else {
        (None, before)
    }
}

fn after_video_pieces(
    graph: &mut GraphBuilder<'_, '_>,
    layout: &CrossfadeLayout,
    after: Pad,
) -> (Pad, Option<Pad>) {
    if layout.suffix_frames > 0 {
        let overlap_source = graph.output_video_pad();
        let suffix_source = graph.output_video_pad();
        graph.clause(format!(
            "{}split=2{}{}",
            after.bracketed(),
            overlap_source.bracketed(),
            suffix_source.bracketed(),
        ));
        let suffix = graph.output_video_pad();
        graph.clause(format!(
            "{}trim=start_frame={}:end_frame={},setpts=PTS-STARTPTS{}",
            suffix_source.bracketed(),
            layout.overlap_frames,
            layout.after_frames,
            suffix.bracketed(),
        ));
        (overlap_source, Some(suffix))
    } else {
        (after, None)
    }
}

fn append_crossfade_audio_filter(
    graph: &mut GraphBuilder<'_, '_>,
    layout: &AudioCrossfadeLayout,
    before: Pad,
    after: Pad,
    audio: crate::model::AudioSpec,
    audio_encoding: AudioEncoding,
) -> Pad {
    let sources = crossfade_audio_sources(graph, layout, before, after);
    let mut tracks = Vec::new();
    if let Some(source) = sources.before_prefix.as_ref() {
        tracks.push(append_prefix_audio_track(
            graph,
            layout,
            audio,
            audio_encoding,
            source,
        ));
    }
    if layout.overlap_samples > 0 {
        tracks.extend(append_overlap_audio_tracks(
            graph,
            layout,
            audio,
            audio_encoding,
            &sources,
        ));
    }
    if let Some(source) = sources.after_suffix.as_ref() {
        tracks.push(append_suffix_audio_track(
            graph,
            layout,
            audio,
            audio_encoding,
            source,
        ));
    }
    debug_assert!(!tracks.is_empty());
    let output = graph.output_audio_pad();
    if tracks.len() == 1 {
        graph.clause(format!(
            "{}{}{}",
            tracks[0].bracketed(),
            normalize_audio(layout.output_samples, audio, audio_encoding),
            output.bracketed(),
        ));
    } else {
        let labels = tracks.iter().map(Pad::bracketed).collect::<String>();
        let mixed = graph.output_audio_pad();
        graph.clause(format!(
            "{labels}amix=inputs={}:duration=longest:dropout_transition=0:normalize=0{}",
            tracks.len(),
            mixed.bracketed(),
        ));
        graph.clause(format!(
            "{}{}{}",
            mixed.bracketed(),
            normalize_audio(layout.output_samples, audio, audio_encoding),
            output.bracketed(),
        ));
    }
    output
}

struct CrossfadeAudioSources {
    before_prefix: Option<Pad>,
    before_overlap: Option<Pad>,
    after_overlap: Option<Pad>,
    after_suffix: Option<Pad>,
}

fn crossfade_audio_sources(
    graph: &mut GraphBuilder<'_, '_>,
    layout: &AudioCrossfadeLayout,
    before: Pad,
    after: Pad,
) -> CrossfadeAudioSources {
    let before_branches =
        usize::from(layout.prefix_samples > 0) + usize::from(layout.overlap_samples > 0);
    let (before_prefix, before_overlap) = match before_branches {
        2 => {
            let prefix = graph.output_audio_pad();
            let overlap = graph.output_audio_pad();
            graph.clause(format!(
                "{}asplit=2{}{}",
                before.bracketed(),
                prefix.bracketed(),
                overlap.bracketed(),
            ));
            (Some(prefix), Some(overlap))
        }
        1 if layout.prefix_samples > 0 => (Some(before), None),
        1 => (None, Some(before)),
        0 => (None, None),
        _ => unreachable!("crossfade has at most two before Audio branches"),
    };
    let after_branches =
        usize::from(layout.overlap_samples > 0) + usize::from(layout.suffix_samples > 0);
    let (after_overlap, after_suffix) = match after_branches {
        2 => {
            let overlap = graph.output_audio_pad();
            let suffix = graph.output_audio_pad();
            graph.clause(format!(
                "{}asplit=2{}{}",
                after.bracketed(),
                overlap.bracketed(),
                suffix.bracketed(),
            ));
            (Some(overlap), Some(suffix))
        }
        1 if layout.overlap_samples > 0 => (Some(after), None),
        1 => (None, Some(after)),
        0 => (None, None),
        _ => unreachable!("crossfade has at most two after Audio branches"),
    };
    CrossfadeAudioSources {
        before_prefix,
        before_overlap,
        after_overlap,
        after_suffix,
    }
}

fn append_prefix_audio_track(
    graph: &mut GraphBuilder<'_, '_>,
    layout: &AudioCrossfadeLayout,
    audio: crate::model::AudioSpec,
    audio_encoding: AudioEncoding,
    source: &Pad,
) -> Pad {
    let output = graph.output_audio_pad();
    graph.clause(format!(
        "{}atrim=end_sample={},asetpts=PTS-STARTPTS,{},apad=whole_len={},atrim=end_sample={},asetpts=PTS-STARTPTS{}",
        source.bracketed(),
        layout.prefix_samples,
        normalize_audio(layout.prefix_samples, audio, audio_encoding),
        layout.output_samples,
        layout.output_samples,
        output.bracketed(),
    ));
    output
}

fn append_overlap_audio_tracks(
    graph: &mut GraphBuilder<'_, '_>,
    layout: &AudioCrossfadeLayout,
    audio: crate::model::AudioSpec,
    audio_encoding: AudioEncoding,
    sources: &CrossfadeAudioSources,
) -> [Pad; 2] {
    let before = sources
        .before_overlap
        .as_ref()
        .expect("positive overlap has before Audio");
    let after = sources
        .after_overlap
        .as_ref()
        .expect("positive overlap has after Audio");
    let before_output = graph.output_audio_pad();
    graph.clause(format!(
        "{}atrim=start_sample={}:end_sample={},asetpts=PTS-STARTPTS,{},afade=t=out:start_sample=0:nb_samples={}:curve=qsin,adelay={}S:all=1,apad=whole_len={},atrim=end_sample={},asetpts=PTS-STARTPTS{}",
        before.bracketed(),
        layout.prefix_samples,
        layout.before_total_samples,
        normalize_audio(layout.overlap_samples, audio, audio_encoding),
        layout.overlap_samples,
        layout.prefix_samples,
        layout.output_samples,
        layout.output_samples,
        before_output.bracketed(),
    ));
    let after_output = graph.output_audio_pad();
    graph.clause(format!(
        "{}atrim=end_sample={},asetpts=PTS-STARTPTS,{},afade=t=in:start_sample=0:nb_samples={}:curve=qsin,adelay={}S:all=1,apad=whole_len={},atrim=end_sample={},asetpts=PTS-STARTPTS{}",
        after.bracketed(),
        layout.after_overlap_end_samples,
        normalize_audio(layout.overlap_samples, audio, audio_encoding),
        layout.overlap_samples,
        layout.prefix_samples,
        layout.output_samples,
        layout.output_samples,
        after_output.bracketed(),
    ));
    [before_output, after_output]
}

fn append_suffix_audio_track(
    graph: &mut GraphBuilder<'_, '_>,
    layout: &AudioCrossfadeLayout,
    audio: crate::model::AudioSpec,
    audio_encoding: AudioEncoding,
    source: &Pad,
) -> Pad {
    let output = graph.output_audio_pad();
    graph.clause(format!(
        "{}atrim=start_sample={}:end_sample={},asetpts=PTS-STARTPTS,{},adelay={}S:all=1,apad=whole_len={},atrim=end_sample={},asetpts=PTS-STARTPTS{}",
        source.bracketed(),
        layout.after_overlap_end_samples,
        layout.after_total_samples,
        normalize_audio(layout.suffix_samples, audio, audio_encoding),
        layout.before_total_samples,
        layout.output_samples,
        layout.output_samples,
        output.bracketed(),
    ));
    output
}

struct CrossfadeLayout {
    before_frames: u64,
    after_frames: u64,
    overlap_frames: u64,
    prefix_frames: u64,
    suffix_frames: u64,
    output_frames: u64,
    prefix_samples: u64,
    overlap_samples: u64,
    suffix_samples: u64,
    before_total_samples: u64,
    after_overlap_end_samples: u64,
    after_total_samples: u64,
    output_samples: u64,
}

impl CrossfadeLayout {
    fn audio_layout(&self) -> AudioCrossfadeLayout {
        AudioCrossfadeLayout {
            prefix_samples: self.prefix_samples,
            overlap_samples: self.overlap_samples,
            suffix_samples: self.suffix_samples,
            before_total_samples: self.before_total_samples,
            after_overlap_end_samples: self.after_overlap_end_samples,
            after_total_samples: self.after_total_samples,
            output_samples: self.output_samples,
        }
    }

    fn new(
        context: &RecipeContext<'_>,
        before: NodeId,
        after: NodeId,
        frames: FrameCount,
        domain: &VideoDomain,
    ) -> Result<Self> {
        let before_frames = context
            .nodes()
            .get(before.get() as usize)
            .and_then(PreparedNode::video_domain)
            .ok_or_else(|| {
                Diagnostic::builtin(
                    BuiltinDiagnostic::InvalidPlan,
                    format!("crossfade input {} is not an available Video", before.get()),
                    context.span().clone(),
                )
            })?
            .frames()
            .0;
        let after_frames = context
            .nodes()
            .get(after.get() as usize)
            .and_then(PreparedNode::video_domain)
            .ok_or_else(|| {
                Diagnostic::builtin(
                    BuiltinDiagnostic::InvalidPlan,
                    format!("crossfade input {} is not an available Video", after.get()),
                    context.span().clone(),
                )
            })?
            .frames()
            .0;
        if frames.0 == 0 || frames.0 > before_frames || frames.0 > after_frames {
            return Err(Diagnostic::builtin(
                BuiltinDiagnostic::InvalidPlan,
                "prepared crossfade overlap is outside its input domains",
                context.span().clone(),
            ));
        }
        let output_frames = before_frames
            .checked_add(after_frames)
            .and_then(|combined| combined.checked_sub(frames.0))
            .ok_or_else(|| {
                Diagnostic::builtin(
                    BuiltinDiagnostic::FrameOverflow,
                    "crossfade duration exceeds the supported frame count",
                    context.span().clone(),
                )
            })?;
        if output_frames != domain.frames().0 {
            return Err(Diagnostic::builtin(
                BuiltinDiagnostic::InvalidPlan,
                format!(
                    "prepared crossfade domain has {} frames, but its inputs and overlap require {output_frames}",
                    domain.frames().0
                ),
                context.span().clone(),
            ));
        }
        let prefix_frames = before_frames - frames.0;
        let suffix_frames = after_frames - frames.0;
        let timeline = TimelineRate::new(*context.video(), context.audio());
        let prefix_samples =
            timeline.samples_for_frames(FrameCount(prefix_frames), context.span())?;
        let overlap_samples =
            timeline.samples_between_frames(prefix_frames, before_frames, context.span())?;
        let suffix_samples =
            timeline.samples_between_frames(before_frames, output_frames, context.span())?;
        let before_total_samples =
            timeline.samples_for_frames(FrameCount(before_frames), context.span())?;
        let after_overlap_end_samples = timeline.samples_for_frames(frames, context.span())?;
        let after_total_samples =
            timeline.samples_for_frames(FrameCount(after_frames), context.span())?;
        let output_samples =
            timeline.samples_for_frames(FrameCount(output_frames), context.span())?;
        if overlap_samples > i64::MAX as u64 {
            return Err(Diagnostic::builtin(
                BuiltinDiagnostic::InvalidPlan,
                "prepared crossfade Audio overlap exceeds the renderer limit",
                context.span().clone(),
            ));
        }
        debug_assert_eq!(
            prefix_samples + overlap_samples + suffix_samples,
            output_samples
        );
        Ok(Self {
            before_frames,
            after_frames,
            overlap_frames: frames.0,
            prefix_frames,
            suffix_frames,
            output_frames,
            prefix_samples,
            overlap_samples,
            suffix_samples,
            before_total_samples,
            after_overlap_end_samples,
            after_total_samples,
            output_samples,
        })
    }
}

#[expect(
    clippy::struct_field_names,
    reason = "explicit sample units distinguish this layout from the adjacent frame layout"
)]
struct AudioCrossfadeLayout {
    prefix_samples: u64,
    overlap_samples: u64,
    suffix_samples: u64,
    before_total_samples: u64,
    after_overlap_end_samples: u64,
    after_total_samples: u64,
    output_samples: u64,
}

impl AudioCrossfadeLayout {
    fn new(
        context: &RecipeContext<'_>,
        before: NodeId,
        after: NodeId,
        overlap_samples: u64,
        domain: &AudioDomain,
    ) -> Result<Self> {
        let before_total_samples = context
            .nodes()
            .get(before.get() as usize)
            .and_then(PreparedNode::audio_domain)
            .ok_or_else(|| {
                Diagnostic::builtin(
                    BuiltinDiagnostic::InvalidPlan,
                    format!("crossfade input {} is not an available Audio", before.get()),
                    context.span().clone(),
                )
            })?
            .samples();
        let after_total_samples = context
            .nodes()
            .get(after.get() as usize)
            .and_then(PreparedNode::audio_domain)
            .ok_or_else(|| {
                Diagnostic::builtin(
                    BuiltinDiagnostic::InvalidPlan,
                    format!("crossfade input {} is not an available Audio", after.get()),
                    context.span().clone(),
                )
            })?
            .samples();
        if overlap_samples == 0
            || overlap_samples > before_total_samples
            || overlap_samples > after_total_samples
            || overlap_samples > i64::MAX as u64
        {
            return Err(Diagnostic::builtin(
                BuiltinDiagnostic::InvalidPlan,
                "prepared Audio crossfade overlap is outside its input domains",
                context.span().clone(),
            ));
        }
        let output_samples = before_total_samples
            .checked_add(after_total_samples)
            .and_then(|combined| combined.checked_sub(overlap_samples))
            .ok_or_else(|| {
                Diagnostic::builtin(
                    BuiltinDiagnostic::AudioDurationOverflow,
                    "Audio crossfade duration exceeds the supported sample count",
                    context.span().clone(),
                )
            })?;
        if output_samples != domain.samples() {
            return Err(Diagnostic::builtin(
                BuiltinDiagnostic::InvalidPlan,
                format!(
                    "prepared Audio crossfade domain has {} samples, but its inputs and overlap require {output_samples}",
                    domain.samples()
                ),
                context.span().clone(),
            ));
        }
        Ok(Self {
            prefix_samples: before_total_samples - overlap_samples,
            overlap_samples,
            suffix_samples: after_total_samples - overlap_samples,
            before_total_samples,
            after_overlap_end_samples: overlap_samples,
            after_total_samples,
            output_samples,
        })
    }
}

fn blend_expression(frames: u64) -> String {
    if frames == 1 {
        "(A+B)/2".to_owned()
    } else {
        let last = frames - 1;
        format!("A*(1-(N-1)/{last})+B*(N-1)/{last}")
    }
}
