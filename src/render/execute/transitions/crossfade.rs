use std::fmt::Write as _;

use crate::diagnostic::Result;
use crate::model::{FrameCount, NodeId, VideoDomain};

use super::super::filters::normalize_audio;
use super::super::recipe::{FfmpegRecipe, RecipeContext};

pub(super) fn render(
    context: &RecipeContext<'_>,
    before: NodeId,
    after: NodeId,
    frames: FrameCount,
    domain: &VideoDomain,
) -> Result<FfmpegRecipe> {
    let layout = CrossfadeLayout::new(context, before, after, frames, domain)?;
    let mut filter = String::new();
    append_crossfade_video_filter(&mut filter, &layout);
    append_crossfade_audio_filter(
        &mut filter,
        &layout,
        context.audio(),
        context.policy().working_channel_layout(),
    );

    let mut recipe = FfmpegRecipe::new();
    recipe.args(["-i"]).artifact(before);
    recipe.args(["-i"]).artifact(after);
    recipe.args(["-filter_complex", &filter, "-map", "[v]", "-map", "[a]"]);
    context.append_video_output(&mut recipe);
    Ok(recipe)
}

fn append_crossfade_video_filter(filter: &mut String, layout: &CrossfadeLayout) {
    let before_overlap = if layout.prefix_frames > 0 {
        let _ = write!(
            filter,
            "[0:v]split=2[before_v_prefix_src][before_v_overlap_src];[before_v_prefix_src]trim=end_frame={},setpts=PTS-STARTPTS[v_prefix];",
            layout.prefix_frames
        );
        "[before_v_overlap_src]"
    } else {
        "[0:v]"
    };
    let after_overlap = if layout.suffix_frames > 0 {
        filter.push_str("[1:v]split=2[after_v_overlap_src][after_v_suffix_src];");
        "[after_v_overlap_src]"
    } else {
        "[1:v]"
    };
    let _ = write!(
        filter,
        "{before_overlap}trim=start_frame={}:end_frame={},setpts=PTS-STARTPTS[before_v_overlap];{after_overlap}trim=end_frame={},setpts=PTS-STARTPTS[after_v_overlap];",
        layout.prefix_frames, layout.before_frames, layout.overlap_frames,
    );
    let piece_count =
        usize::from(layout.prefix_frames > 0) + 1 + usize::from(layout.suffix_frames > 0);
    let overlap_output = if piece_count == 1 {
        "v_joined"
    } else {
        "v_overlap"
    };
    let _ = write!(
        filter,
        "[before_v_overlap][after_v_overlap]blend=all_expr='{}':shortest=1:repeatlast=0,trim=end_frame={},setpts=PTS-STARTPTS[{overlap_output}];",
        blend_expression(layout.overlap_frames),
        layout.overlap_frames,
    );
    if layout.suffix_frames > 0 {
        let _ = write!(
            filter,
            "[after_v_suffix_src]trim=start_frame={}:end_frame={},setpts=PTS-STARTPTS[v_suffix];",
            layout.overlap_frames, layout.after_frames,
        );
    }
    if piece_count > 1 {
        let mut labels = String::new();
        if layout.prefix_frames > 0 {
            labels.push_str("[v_prefix]");
        }
        labels.push_str("[v_overlap]");
        if layout.suffix_frames > 0 {
            labels.push_str("[v_suffix]");
        }
        let _ = write!(filter, "{labels}concat=n={piece_count}:v=1:a=0[v_joined];");
    }
    let _ = write!(
        filter,
        "[v_joined]trim=end_frame={},setpts=PTS-STARTPTS[v];",
        layout.output_frames,
    );
}

fn append_crossfade_audio_filter(
    filter: &mut String,
    layout: &CrossfadeLayout,
    audio: &crate::model::AudioSpec,
    channel_layout: &str,
) {
    let sources = crossfade_audio_sources(filter, layout);
    let mut tracks = Vec::new();
    if let Some(source) = sources.before_prefix {
        append_prefix_audio_track(filter, layout, audio, channel_layout, source);
        tracks.push("[a_prefix_track]");
    }
    if layout.overlap_samples > 0 {
        append_overlap_audio_tracks(filter, layout, audio, channel_layout, &sources);
        tracks.push("[a_before_overlap_track]");
        tracks.push("[a_after_overlap_track]");
    }
    if let Some(source) = sources.after_suffix {
        append_suffix_audio_track(filter, layout, audio, channel_layout, source);
        tracks.push("[a_suffix_track]");
    }
    debug_assert!(!tracks.is_empty());
    let labels = tracks.concat();
    if tracks.len() == 1 {
        let _ = write!(
            filter,
            "{labels}{}[a]",
            normalize_audio(layout.output_samples, audio, channel_layout),
        );
    } else {
        let _ = write!(
            filter,
            "{labels}amix=inputs={}:duration=longest:dropout_transition=0:normalize=0[mixed_a];[mixed_a]{}[a]",
            tracks.len(),
            normalize_audio(layout.output_samples, audio, channel_layout),
        );
    }
}

struct CrossfadeAudioSources {
    before_prefix: Option<&'static str>,
    before_overlap: Option<&'static str>,
    after_overlap: Option<&'static str>,
    after_suffix: Option<&'static str>,
}

fn crossfade_audio_sources(filter: &mut String, layout: &CrossfadeLayout) -> CrossfadeAudioSources {
    let before_branches =
        usize::from(layout.prefix_samples > 0) + usize::from(layout.overlap_samples > 0);
    let (before_prefix, before_overlap) = match before_branches {
        2 => {
            filter.push_str("[0:a]asplit=2[before_a_prefix_src][before_a_overlap_src];");
            (
                Some("[before_a_prefix_src]"),
                Some("[before_a_overlap_src]"),
            )
        }
        1 if layout.prefix_samples > 0 => (Some("[0:a]"), None),
        1 => (None, Some("[0:a]")),
        0 => (None, None),
        _ => unreachable!("crossfade has at most two before Audio branches"),
    };
    let after_branches =
        usize::from(layout.overlap_samples > 0) + usize::from(layout.suffix_samples > 0);
    let (after_overlap, after_suffix) = match after_branches {
        2 => {
            filter.push_str("[1:a]asplit=2[after_a_overlap_src][after_a_suffix_src];");
            (Some("[after_a_overlap_src]"), Some("[after_a_suffix_src]"))
        }
        1 if layout.overlap_samples > 0 => (Some("[1:a]"), None),
        1 => (None, Some("[1:a]")),
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
    filter: &mut String,
    layout: &CrossfadeLayout,
    audio: &crate::model::AudioSpec,
    channel_layout: &str,
    source: &str,
) {
    let _ = write!(
        filter,
        "{source}atrim=end_sample={},asetpts=PTS-STARTPTS,{},apad=whole_len={},atrim=end_sample={},asetpts=PTS-STARTPTS[a_prefix_track];",
        layout.prefix_samples,
        normalize_audio(layout.prefix_samples, audio, channel_layout),
        layout.output_samples,
        layout.output_samples,
    );
}

fn append_overlap_audio_tracks(
    filter: &mut String,
    layout: &CrossfadeLayout,
    audio: &crate::model::AudioSpec,
    channel_layout: &str,
    sources: &CrossfadeAudioSources,
) {
    let before = sources
        .before_overlap
        .expect("positive overlap has before Audio");
    let after = sources
        .after_overlap
        .expect("positive overlap has after Audio");
    let _ = write!(
        filter,
        "{before}atrim=start_sample={}:end_sample={},asetpts=PTS-STARTPTS,{},afade=t=out:start_sample=0:nb_samples={}:curve=tri,adelay={}S:all=1,apad=whole_len={},atrim=end_sample={},asetpts=PTS-STARTPTS[a_before_overlap_track];",
        layout.prefix_samples,
        layout.before_total_samples,
        normalize_audio(layout.overlap_samples, audio, channel_layout),
        layout.overlap_samples,
        layout.prefix_samples,
        layout.output_samples,
        layout.output_samples,
    );
    let _ = write!(
        filter,
        "{after}atrim=end_sample={},asetpts=PTS-STARTPTS,{},afade=t=in:start_sample=0:nb_samples={}:curve=tri,adelay={}S:all=1,apad=whole_len={},atrim=end_sample={},asetpts=PTS-STARTPTS[a_after_overlap_track];",
        layout.after_overlap_end_samples,
        normalize_audio(layout.overlap_samples, audio, channel_layout),
        layout.overlap_samples,
        layout.prefix_samples,
        layout.output_samples,
        layout.output_samples,
    );
}

fn append_suffix_audio_track(
    filter: &mut String,
    layout: &CrossfadeLayout,
    audio: &crate::model::AudioSpec,
    channel_layout: &str,
    source: &str,
) {
    let _ = write!(
        filter,
        "{source}atrim=start_sample={}:end_sample={},asetpts=PTS-STARTPTS,{},adelay={}S:all=1,apad=whole_len={},atrim=end_sample={},asetpts=PTS-STARTPTS[a_suffix_track];",
        layout.after_overlap_end_samples,
        layout.after_total_samples,
        normalize_audio(layout.suffix_samples, audio, channel_layout),
        layout.before_total_samples,
        layout.output_samples,
        layout.output_samples,
    );
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
    fn new(
        context: &RecipeContext<'_>,
        before: NodeId,
        after: NodeId,
        frames: FrameCount,
        domain: &VideoDomain,
    ) -> Result<Self> {
        use crate::diagnostic::Diagnostic;
        use crate::model::TimelineRate;

        let before_frames = context
            .nodes()
            .get(before.get() as usize)
            .and_then(crate::preflight::PreparedNode::video_domain)
            .ok_or_else(|| {
                Diagnostic::new(
                    "E_INVALID_PLAN",
                    format!("crossfade input {} is not an available Video", before.get()),
                    context.span().clone(),
                )
            })?
            .frames()
            .0;
        let after_frames = context
            .nodes()
            .get(after.get() as usize)
            .and_then(crate::preflight::PreparedNode::video_domain)
            .ok_or_else(|| {
                Diagnostic::new(
                    "E_INVALID_PLAN",
                    format!("crossfade input {} is not an available Video", after.get()),
                    context.span().clone(),
                )
            })?
            .frames()
            .0;
        if frames.0 == 0 || frames.0 > before_frames || frames.0 > after_frames {
            return Err(Diagnostic::new(
                "E_INVALID_PLAN",
                "prepared crossfade overlap is outside its input domains",
                context.span().clone(),
            ));
        }
        let output_frames = before_frames
            .checked_add(after_frames)
            .and_then(|combined| combined.checked_sub(frames.0))
            .ok_or_else(|| {
                Diagnostic::new(
                    "E_FRAME_OVERFLOW",
                    "crossfade duration exceeds the supported frame count",
                    context.span().clone(),
                )
            })?;
        if output_frames != domain.frames().0 {
            return Err(Diagnostic::new(
                "E_INVALID_PLAN",
                format!(
                    "prepared crossfade domain has {} frames, but its inputs and overlap require {output_frames}",
                    domain.frames().0
                ),
                context.span().clone(),
            ));
        }
        let prefix_frames = before_frames - frames.0;
        let suffix_frames = after_frames - frames.0;
        let timeline = TimelineRate::new(*context.video(), *context.audio());
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
            return Err(Diagnostic::new(
                "E_INVALID_PLAN",
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

fn blend_expression(frames: u64) -> String {
    if frames == 1 {
        "(A+B)/2".to_owned()
    } else {
        let last = frames - 1;
        format!("A*(1-N/{last})+B*N/{last}")
    }
}
