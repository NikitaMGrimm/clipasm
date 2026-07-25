use std::collections::HashMap;

use crate::compiler::CompiledProgram;
use crate::diagnostic::{Diagnostic, Result};
use crate::model::{
    AudioDomain, FrameCount, FrameRange, NodeId, TimelineRate, ValueId, ValueRef, VideoDomain,
    VideoSpec,
};
use crate::semantic::{SemanticNodeKind, SourceOrigin};
use crate::source::SourceSpan;

use super::assets::{prepare_audio_asset, prepare_image_asset, prepare_video_asset};
use super::identity::node_fingerprint;
use super::tools::{ToolIdentity, inspect_external_tool};
use super::{PreparedAudioKind, PreparedMedia, PreparedNode, PreparedVideoKind};

pub(super) struct PreflightLowerer<'a> {
    pub(super) compiled: &'a CompiledProgram,
    pub(super) ffmpeg: &'a ToolIdentity,
    pub(super) ffprobe: &'a ToolIdentity,
    pub(super) nodes: Vec<PreparedNode>,
    pub(super) lowered: HashMap<ValueId, NodeId>,
}

impl PreflightLowerer<'_> {
    #[allow(clippy::too_many_lines)]
    pub(super) fn lower(&mut self, value: ValueRef) -> Result<NodeId> {
        if let Some(node) = self.lowered.get(&value.id()) {
            return Ok(*node);
        }
        let compiled_node = &self.compiled.nodes()[value.id().get() as usize];
        let result = match compiled_node.kind() {
            SemanticNodeKind::ImageVideo { path, frames, fit } => {
                let asset =
                    prepare_image_asset(path, compiled_node.origin(), self.ffmpeg, self.ffprobe)?;
                self.add_video_node(
                    PreparedVideoKind::ImageVideo {
                        asset,
                        frames: *frames,
                        fit: *fit,
                    },
                    *compiled_node.domain().expect("Video node domain"),
                    false,
                    compiled_node.semantic_version(),
                    compiled_node.origin().clone(),
                )?
            }
            SemanticNodeKind::VideoSource { path, fit } => {
                let (asset, frames, has_audio) = prepare_video_asset(
                    path,
                    self.compiled.video(),
                    compiled_node.origin(),
                    self.ffmpeg,
                    self.ffprobe,
                )?;
                self.add_video_node(
                    PreparedVideoKind::VideoSource {
                        asset,
                        frames,
                        fit: *fit,
                    },
                    project_domain(self.compiled.video(), frames),
                    has_audio,
                    compiled_node.semantic_version(),
                    compiled_node.origin().clone(),
                )?
            }
            SemanticNodeKind::AudioSource { path } => {
                let (asset, domain) = prepare_audio_asset(
                    path,
                    *self.compiled.audio(),
                    compiled_node.origin(),
                    self.ffmpeg,
                    self.ffprobe,
                )?;
                self.add_audio_node(
                    PreparedAudioKind::AudioSource { asset },
                    domain,
                    compiled_node.semantic_version(),
                    compiled_node.origin().clone(),
                )?
            }
            SemanticNodeKind::AudioSlice { input, range } => {
                let input = self.prepared_dependency(*input, compiled_node.origin())?;
                let input_domain = *self.audio_domain(input, compiled_node.origin())?;
                if range.end() > input_domain.samples() {
                    return Err(Diagnostic::new(
                        "E_RANGE_OUT_OF_BOUNDS",
                        format!(
                            "audio range {}..{} exceeds input duration of {} samples",
                            range.start(),
                            range.end(),
                            input_domain.samples()
                        ),
                        compiled_node.origin().span.clone(),
                    ));
                }
                self.add_audio_node(
                    PreparedAudioKind::AudioSlice {
                        input,
                        range: *range,
                    },
                    AudioDomain::new(range.samples(), input_domain.audio_spec()),
                    compiled_node.semantic_version(),
                    compiled_node.origin().clone(),
                )?
            }
            SemanticNodeKind::AudioRepeat { input, count } => {
                let input = self.prepared_dependency(*input, compiled_node.origin())?;
                let input_domain = *self.audio_domain(input, compiled_node.origin())?;
                let samples = input_domain
                    .samples()
                    .checked_mul(count.get())
                    .ok_or_else(|| {
                        Diagnostic::new(
                            "E_AUDIO_DURATION_OVERFLOW",
                            "repeated audio exceeds the supported sample count",
                            compiled_node.origin().span.clone(),
                        )
                    })?;
                self.add_audio_node(
                    PreparedAudioKind::AudioRepeat {
                        input,
                        count: *count,
                    },
                    AudioDomain::new(samples, input_domain.audio_spec()),
                    compiled_node.semantic_version(),
                    compiled_node.origin().clone(),
                )?
            }
            SemanticNodeKind::AudioConcat { inputs } => {
                let inputs = inputs
                    .iter()
                    .map(|input| self.prepared_dependency(*input, compiled_node.origin()))
                    .collect::<Result<Vec<_>>>()?;
                let mut samples = 0_u64;
                for input in &inputs {
                    samples = samples
                        .checked_add(self.audio_domain(*input, compiled_node.origin())?.samples())
                        .ok_or_else(|| {
                            Diagnostic::new(
                                "E_AUDIO_DURATION_OVERFLOW",
                                "concatenated audio exceeds the supported sample count",
                                compiled_node.origin().span.clone(),
                            )
                        })?;
                }
                self.add_audio_node(
                    PreparedAudioKind::AudioConcat { inputs },
                    AudioDomain::new(samples, *self.compiled.audio()),
                    compiled_node.semantic_version(),
                    compiled_node.origin().clone(),
                )?
            }
            SemanticNodeKind::Reference { symbol } => {
                let target = self.compiled.symbol_value(*symbol).ok_or_else(|| {
                    Diagnostic::new(
                        "E_MISSING_REFERENCE",
                        format!("reference names unknown symbol {}", symbol.index()),
                        compiled_node.origin().span.clone(),
                    )
                })?;
                self.prepared_dependency(target, compiled_node.origin())?
            }
            SemanticNodeKind::Repeat { input, count } => {
                let input = self.prepared_dependency(*input, compiled_node.origin())?;
                let (input_domain, input_has_audio) =
                    self.video_domain(input, compiled_node.origin())?;
                let frames = input_domain
                    .frames()
                    .checked_mul(count.get(), &compiled_node.origin().span)?;
                self.add_video_node(
                    PreparedVideoKind::Repeat {
                        input,
                        count: *count,
                        frames,
                    },
                    project_domain(self.compiled.video(), frames),
                    input_has_audio,
                    compiled_node.semantic_version(),
                    compiled_node.origin().clone(),
                )?
            }
            SemanticNodeKind::Zoom { input, percent } => {
                let input = self.prepared_dependency(*input, compiled_node.origin())?;
                let (input_domain, input_has_audio) =
                    self.video_domain(input, compiled_node.origin())?;
                self.add_video_node(
                    PreparedVideoKind::Zoom {
                        input,
                        percent: *percent,
                    },
                    *input_domain,
                    input_has_audio,
                    compiled_node.semantic_version(),
                    compiled_node.origin().clone(),
                )?
            }
            SemanticNodeKind::Wobble { input, pixels } => {
                let input = self.prepared_dependency(*input, compiled_node.origin())?;
                let (input_domain, input_has_audio) =
                    self.video_domain(input, compiled_node.origin())?;
                self.add_video_node(
                    PreparedVideoKind::Wobble {
                        input,
                        pixels: *pixels,
                    },
                    *input_domain,
                    input_has_audio,
                    compiled_node.semantic_version(),
                    compiled_node.origin().clone(),
                )?
            }
            SemanticNodeKind::FlashJoin {
                before,
                after,
                frames,
            } => {
                let before = self.prepared_dependency(*before, compiled_node.origin())?;
                let after = self.prepared_dependency(*after, compiled_node.origin())?;
                let (before_domain, before_has_audio) =
                    self.video_domain(before, compiled_node.origin())?;
                let (after_domain, after_has_audio) =
                    self.video_domain(after, compiled_node.origin())?;
                let after_frames = after_domain.frames();
                validate_flash_frames(*frames, after_frames, &compiled_node.origin().span)?;
                let total = before_domain
                    .frames()
                    .checked_add(after_frames, &compiled_node.origin().span)?;
                self.add_video_node(
                    PreparedVideoKind::FlashJoin {
                        before,
                        after,
                        frames: *frames,
                    },
                    project_domain(self.compiled.video(), total),
                    before_has_audio || after_has_audio,
                    compiled_node.semantic_version(),
                    compiled_node.origin().clone(),
                )?
            }
            SemanticNodeKind::Concat { inputs } => {
                let inputs = inputs
                    .iter()
                    .map(|input| self.prepared_dependency(*input, compiled_node.origin()))
                    .collect::<Result<Vec<_>>>()?;
                let domain = self.concat_domain(&inputs, compiled_node.origin())?;
                let has_audio = inputs.iter().try_fold(false, |has_audio, input| {
                    self.video_domain(*input, compiled_node.origin())
                        .map(|(_, input_has_audio)| has_audio || input_has_audio)
                })?;
                self.add_video_node(
                    PreparedVideoKind::Concat { inputs },
                    domain,
                    has_audio,
                    compiled_node.semantic_version(),
                    compiled_node.origin().clone(),
                )?
            }
            SemanticNodeKind::Slice { input, range } => {
                let input = self.prepared_dependency(*input, compiled_node.origin())?;
                let (input_domain, input_has_audio) =
                    self.video_domain(input, compiled_node.origin())?;
                validate_prepared_range(*range, input_domain, &compiled_node.origin().span)?;
                self.add_video_node(
                    PreparedVideoKind::Slice {
                        input,
                        range: *range,
                    },
                    project_domain(self.compiled.video(), range.frames()),
                    input_has_audio,
                    compiled_node.semantic_version(),
                    compiled_node.origin().clone(),
                )?
            }
            SemanticNodeKind::ReplaceRange {
                base,
                replacement,
                range,
            } => {
                let base_node = self.prepared_dependency(*base, compiled_node.origin())?;
                let replacement_node =
                    self.prepared_dependency(*replacement, compiled_node.origin())?;
                let (base_domain, base_has_audio) =
                    self.video_domain(base_node, compiled_node.origin())?;
                let base_domain = *base_domain;
                validate_prepared_range(*range, &base_domain, &compiled_node.origin().span)?;
                let mut pieces = Vec::new();
                if range.start() > 0 {
                    pieces.push(self.add_video_node(
                        PreparedVideoKind::Slice {
                            input: base_node,
                            range:
                                FrameRange::new(0, range.start()).expect("nonempty during prefix"),
                        },
                        project_domain(self.compiled.video(), FrameCount(range.start())),
                        base_has_audio,
                        compiled_node.semantic_version(),
                        compiled_node.origin().clone_with_construct("range prefix"),
                    )?);
                }
                pieces.push(replacement_node);
                if range.end() < base_domain.frames().0 {
                    pieces.push(
                        self.add_video_node(
                            PreparedVideoKind::Slice {
                                input: base_node,
                                range: FrameRange::new(range.end(), base_domain.frames().0)
                                    .expect("nonempty during suffix"),
                            },
                            project_domain(
                                self.compiled.video(),
                                FrameCount(base_domain.frames().0 - range.end()),
                            ),
                            base_has_audio,
                            compiled_node.semantic_version(),
                            compiled_node.origin().clone_with_construct("range suffix"),
                        )?,
                    );
                }
                if pieces.len() == 1 {
                    pieces[0]
                } else {
                    let domain = self.concat_domain(&pieces, compiled_node.origin())?;
                    let has_audio = pieces.iter().try_fold(false, |has_audio, piece| {
                        self.video_domain(*piece, compiled_node.origin())
                            .map(|(_, piece_has_audio)| has_audio || piece_has_audio)
                    })?;
                    self.add_video_node(
                        PreparedVideoKind::Concat { inputs: pieces },
                        domain,
                        has_audio,
                        compiled_node.semantic_version(),
                        compiled_node.origin().clone(),
                    )?
                }
            }
            SemanticNodeKind::ExtractAudio { video } => {
                let video = self.prepared_dependency(*video, compiled_node.origin())?;
                let samples = TimelineRate::new(*self.compiled.video(), *self.compiled.audio())
                    .samples_for_frames(
                        self.video_domain(video, compiled_node.origin())?.0.frames(),
                        &compiled_node.origin().span,
                    )?;
                self.add_audio_node(
                    PreparedAudioKind::ExtractAudio { video },
                    AudioDomain::new(samples, *self.compiled.audio()),
                    compiled_node.semantic_version(),
                    compiled_node.origin().clone(),
                )?
            }
            SemanticNodeKind::SetAudio { audio, video } => {
                let audio = self.prepared_dependency(*audio, compiled_node.origin())?;
                let video = self.prepared_dependency(*video, compiled_node.origin())?;
                self.add_video_node(
                    PreparedVideoKind::SetAudio { audio, video },
                    *self.video_domain(video, compiled_node.origin())?.0,
                    true,
                    compiled_node.semantic_version(),
                    compiled_node.origin().clone(),
                )?
            }
            SemanticNodeKind::AudioOnBlack { audio } => {
                let audio = self.prepared_dependency(*audio, compiled_node.origin())?;
                let frames = TimelineRate::new(*self.compiled.video(), *self.compiled.audio())
                    .frames_for_samples(
                        self.audio_domain(audio, compiled_node.origin())?.samples(),
                        &compiled_node.origin().span,
                    )?;
                self.add_video_node(
                    PreparedVideoKind::AudioOnBlack { audio },
                    project_domain(self.compiled.video(), frames),
                    true,
                    compiled_node.semantic_version(),
                    compiled_node.origin().clone(),
                )?
            }
            SemanticNodeKind::ExternalVideo { invocation } => {
                let inputs = invocation
                    .inputs
                    .iter()
                    .map(|(name, input)| {
                        self.prepared_dependency(*input, compiled_node.origin())
                            .map(|input| (name.clone(), input))
                    })
                    .collect::<Result<std::collections::BTreeMap<_, _>>>()?;
                let preserved = inputs[&invocation.preserve_input];
                let (preserved_domain, preserved_has_audio) =
                    self.video_domain(preserved, compiled_node.origin())?;
                let executable =
                    inspect_external_tool(&invocation.command.value, &invocation.command.span)?;
                self.add_video_node(
                    PreparedVideoKind::ExternalVideo {
                        executable,
                        inputs,
                        parameters: invocation.parameters.clone(),
                        preserve_input: invocation.preserve_input.clone(),
                    },
                    *preserved_domain,
                    preserved_has_audio,
                    compiled_node.semantic_version(),
                    compiled_node.origin().clone(),
                )?
            }
        };
        self.lowered.insert(value.id(), result);
        Ok(result)
    }

    fn prepared_dependency(&self, value: ValueRef, origin: &SourceOrigin) -> Result<NodeId> {
        self.lowered.get(&value.id()).copied().ok_or_else(|| {
            Diagnostic::new(
                "E_INVALID_GRAPH",
                format!(
                    "semantic dependency {} was not prepared before its consumer",
                    value.id().get()
                ),
                origin.span.clone(),
            )
        })
    }

    fn video_domain(&self, node: NodeId, origin: &SourceOrigin) -> Result<(&VideoDomain, bool)> {
        match &self.nodes[node.get() as usize].media {
            PreparedMedia::Video {
                domain, has_audio, ..
            } => Ok((domain, *has_audio)),
            PreparedMedia::Audio { .. } => Err(Diagnostic::new(
                "E_INVALID_GRAPH",
                format!(
                    "prepared dependency {} is Audio, but Video is required",
                    node.get()
                ),
                origin.span.clone(),
            )),
        }
    }

    fn audio_domain(&self, node: NodeId, origin: &SourceOrigin) -> Result<&AudioDomain> {
        match &self.nodes[node.get() as usize].media {
            PreparedMedia::Audio { domain, .. } => Ok(domain),
            PreparedMedia::Video { .. } => Err(Diagnostic::new(
                "E_INVALID_GRAPH",
                format!(
                    "prepared dependency {} is Video, but Audio is required",
                    node.get()
                ),
                origin.span.clone(),
            )),
        }
    }

    fn concat_domain(&self, inputs: &[NodeId], origin: &SourceOrigin) -> Result<VideoDomain> {
        let mut frames = FrameCount(0);
        for input in inputs {
            frames =
                frames.checked_add(self.video_domain(*input, origin)?.0.frames(), &origin.span)?;
        }
        Ok(project_domain(self.compiled.video(), frames))
    }

    fn add_video_node(
        &mut self,
        kind: PreparedVideoKind,
        domain: VideoDomain,
        has_audio: bool,
        semantic_version: u32,
        origin: SourceOrigin,
    ) -> Result<NodeId> {
        self.add_node(
            PreparedMedia::Video {
                kind,
                domain,
                has_audio,
            },
            semantic_version,
            origin,
        )
    }

    fn add_audio_node(
        &mut self,
        kind: PreparedAudioKind,
        domain: AudioDomain,
        semantic_version: u32,
        origin: SourceOrigin,
    ) -> Result<NodeId> {
        self.add_node(
            PreparedMedia::Audio { kind, domain },
            semantic_version,
            origin,
        )
    }

    fn add_node(
        &mut self,
        media: PreparedMedia,
        semantic_version: u32,
        origin: SourceOrigin,
    ) -> Result<NodeId> {
        let id = NodeId::new(u32::try_from(self.nodes.len()).map_err(|_| {
            Diagnostic::new(
                "E_GRAPH_TOO_LARGE",
                "prepared graph contains too many primitive nodes",
                origin.span.clone(),
            )
        })?);
        let fingerprint = node_fingerprint(&media, semantic_version, &self.nodes)?;
        self.nodes.push(PreparedNode {
            id,
            media,
            origin,
            fingerprint,
        });
        Ok(id)
    }
}

fn project_domain(video: &VideoSpec, frames: FrameCount) -> VideoDomain {
    VideoDomain::new(frames, *video)
}

fn validate_prepared_range(
    range: FrameRange,
    input: &VideoDomain,
    span: &SourceSpan,
) -> Result<()> {
    if range.end() > input.frames().0 {
        return Err(Diagnostic::new(
            "E_INVALID_TIME_RANGE",
            format!(
                "frame range {}..{} is outside the base Video domain of {} frames",
                range.start(),
                range.end(),
                input.frames().0
            ),
            span.clone(),
        ));
    }
    Ok(())
}

fn validate_flash_frames(frames: FrameCount, after: FrameCount, span: &SourceSpan) -> Result<()> {
    if frames > after {
        return Err(Diagnostic::new(
            "E_INVALID_FLASH_FRAMES",
            format!(
                "`flash.frames` is {} frames, but `after` contains only {} frames",
                frames.0, after.0
            ),
            span.clone(),
        ));
    }
    Ok(())
}
