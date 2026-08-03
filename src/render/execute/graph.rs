use std::collections::{BTreeMap, BTreeSet};

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::model::{AudioDomain, NodeId, ValueType, VideoDomain};
use crate::preflight::{PreparedAudioKind, PreparedNode, PreparedNodeMedia, PreparedVideoKind};

#[cfg(feature = "native")]
use super::FusedInputUse;
use super::recipe::{FfmpegRecipe, RecipeContext};
use super::transitions;

mod effects;
mod sources;
mod timeline;

#[derive(Clone, Debug)]
pub(super) struct Pad(String);

impl Pad {
    pub(super) fn bracketed(&self) -> String {
        format!("[{}]", self.0)
    }
}

#[derive(Clone, Debug)]
pub(super) struct VideoPads {
    pub(super) video: Pad,
    pub(super) audio: Pad,
}

#[derive(Clone, Debug)]
pub(super) enum NodePads {
    Video(VideoPads),
    Audio(Pad),
}

#[derive(Clone, Debug)]
struct StoredPad {
    pad: Pad,
    consumed: bool,
}

#[derive(Clone, Debug)]
enum StoredNodePads {
    Video { video: StoredPad, audio: StoredPad },
    Audio(StoredPad),
}

pub(super) fn is_graph_native(node: &PreparedNode) -> bool {
    !matches!(
        node.media(),
        PreparedNodeMedia::Video {
            kind: PreparedVideoKind::ExternalVideo { .. },
            ..
        }
    )
}

#[cfg(feature = "native")]
pub(super) fn accepts_fused_input(node: &PreparedNode, input: NodeId) -> bool {
    match node.media() {
        PreparedNodeMedia::Video {
            kind: PreparedVideoKind::Repeat {
                input: artifact, ..
            },
            ..
        } if *artifact == input => false,
        PreparedNodeMedia::Audio {
            kind: PreparedAudioKind::AudioRepeat {
                input: artifact, ..
            },
            ..
        } if *artifact == input => false,
        PreparedNodeMedia::Video {
            kind:
                PreparedVideoKind::Concat { .. }
                | PreparedVideoKind::FlashCut { .. }
                | PreparedVideoKind::Crossfade { .. }
                | PreparedVideoKind::ExternalVideo { .. },
            ..
        }
        | PreparedNodeMedia::Audio {
            kind: PreparedAudioKind::AudioConcat { .. } | PreparedAudioKind::Crossfade { .. },
            ..
        } => false,
        _ => true,
    }
}

#[cfg(feature = "native")]
pub(super) fn visit_fused_inputs(
    node: &PreparedNode,
    mut visitor: impl FnMut(NodeId, FusedInputUse),
) {
    // This exhaustive map must match the physical pads taken by lower_video and
    // lower_audio. The planner admits a region only while each pad has one user.
    let video = FusedInputUse {
        picture: 1,
        audio: 1,
    };
    let picture = FusedInputUse {
        picture: 1,
        audio: 0,
    };
    let audio = FusedInputUse {
        picture: 0,
        audio: 1,
    };
    match node.media() {
        PreparedNodeMedia::Video { kind, .. } => match kind {
            PreparedVideoKind::ImageVideo { .. } | PreparedVideoKind::VideoSource { .. } => {}
            PreparedVideoKind::Slice { input, .. }
            | PreparedVideoKind::Repeat { input, .. }
            | PreparedVideoKind::ZoomIn { input, .. } => visitor(*input, video),
            PreparedVideoKind::Concat { inputs } => {
                for input in inputs {
                    visitor(*input, video);
                }
            }
            PreparedVideoKind::SetAudio {
                audio: audio_input,
                video: video_input,
            } => {
                visitor(*audio_input, audio);
                visitor(*video_input, picture);
            }
            PreparedVideoKind::AudioOnBlack { audio: input } => visitor(*input, audio),
            PreparedVideoKind::FlashCut { before, after, .. }
            | PreparedVideoKind::Crossfade { before, after, .. } => {
                visitor(*before, video);
                visitor(*after, video);
            }
            PreparedVideoKind::ExternalVideo { inputs, .. } => {
                for input in inputs.values() {
                    visitor(*input, FusedInputUse::default());
                }
            }
        },
        PreparedNodeMedia::Audio { kind, .. } => match kind {
            PreparedAudioKind::AudioSource { .. } => {}
            PreparedAudioKind::AudioSlice { input, .. }
            | PreparedAudioKind::AudioRepeat { input, .. }
            | PreparedAudioKind::ExtractAudio { video: input } => visitor(*input, audio),
            PreparedAudioKind::AudioConcat { inputs } => {
                for input in inputs {
                    visitor(*input, audio);
                }
            }
            PreparedAudioKind::Crossfade { before, after, .. } => {
                visitor(*before, audio);
                visitor(*after, audio);
            }
        },
    }
}

pub(super) fn recipe(
    context: &RecipeContext<'_>,
    node_ids: &[NodeId],
    output: NodeId,
) -> Result<FfmpegRecipe> {
    let included = node_ids.iter().copied().collect::<BTreeSet<_>>();
    if !included.contains(&output) {
        return Err(invalid_plan(
            context,
            "fused region does not contain its output",
        ));
    }
    let mut graph = GraphBuilder::new(context, included);
    for id in node_ids {
        graph.lower(*id)?;
    }
    graph.finish(output)
}

pub(super) struct GraphBuilder<'a, 'b> {
    context: &'a RecipeContext<'b>,
    included: BTreeSet<NodeId>,
    recipe: FfmpegRecipe,
    filter: String,
    next_input: usize,
    next_pad: usize,
    nodes: BTreeMap<NodeId, StoredNodePads>,
}

impl<'a, 'b> GraphBuilder<'a, 'b> {
    fn new(context: &'a RecipeContext<'b>, included: BTreeSet<NodeId>) -> Self {
        Self {
            context,
            included,
            recipe: FfmpegRecipe::new(),
            filter: String::new(),
            next_input: 0,
            next_pad: 0,
            nodes: BTreeMap::new(),
        }
    }

    fn lower(&mut self, id: NodeId) -> Result<()> {
        let node = self.node(id)?.clone();
        if !is_graph_native(&node) {
            return Err(invalid_plan(
                self.context,
                "an external primitive was placed inside an FFmpeg graph",
            ));
        }
        let output = self.lower_node(&node)?;
        let previous = self.nodes.insert(id, store(output));
        if previous.is_some() {
            return Err(invalid_plan(
                self.context,
                "fused region contains one primitive more than once",
            ));
        }
        Ok(())
    }

    fn lower_node(&mut self, node: &PreparedNode) -> Result<NodePads> {
        match node.media() {
            PreparedNodeMedia::Video {
                kind,
                domain,
                has_audio,
            } => self.lower_video(kind, domain, has_audio),
            PreparedNodeMedia::Audio { kind, domain } => self.lower_audio(kind, domain),
        }
    }

    fn lower_video(
        &mut self,
        kind: &PreparedVideoKind,
        domain: &VideoDomain,
        has_audio: bool,
    ) -> Result<NodePads> {
        match kind {
            PreparedVideoKind::ImageVideo {
                asset,
                color,
                fit,
                frames,
            } => self.image(asset, color, *fit, *frames),
            PreparedVideoKind::VideoSource {
                asset,
                color,
                fit,
                frames,
            } => self.video_source(asset, color, *fit, *frames, has_audio),
            PreparedVideoKind::Slice { input, range } => {
                self.video_slice(*input, range.start(), range.end())
            }
            PreparedVideoKind::ZoomIn { input, by } => self.zoom(*input, by, domain.frames()),
            PreparedVideoKind::Concat { inputs } => self.video_concat(inputs, domain.frames()),
            PreparedVideoKind::SetAudio { audio, video } => {
                self.set_audio(*audio, *video, domain.frames())
            }
            PreparedVideoKind::AudioOnBlack { audio } => {
                self.audio_on_black(*audio, domain.frames())
            }
            PreparedVideoKind::FlashCut {
                before,
                after,
                frames,
            } => self.flash_cut(*before, *after, *frames, domain.frames()),
            PreparedVideoKind::Crossfade {
                before,
                after,
                frames,
            } => transitions::lower_video(self, *before, *after, *frames, domain),
            PreparedVideoKind::Repeat {
                input,
                count,
                frames,
            } => self.video_repeat(*input, *count, *frames),
            PreparedVideoKind::ExternalVideo { .. } => {
                unreachable!("external primitives were rejected above")
            }
        }
    }

    fn lower_audio(&mut self, kind: &PreparedAudioKind, domain: &AudioDomain) -> Result<NodePads> {
        match kind {
            PreparedAudioKind::AudioSource { asset } => {
                Ok(self.audio_source(asset, domain.samples()))
            }
            PreparedAudioKind::AudioSlice { input, range } => {
                self.audio_slice(*input, range.start(), range.end())
            }
            PreparedAudioKind::AudioConcat { inputs } => {
                self.audio_concat(inputs, domain.samples())
            }
            PreparedAudioKind::ExtractAudio { video } => {
                self.extract_audio(*video, domain.samples())
            }
            PreparedAudioKind::Crossfade {
                before,
                after,
                samples,
            } => transitions::lower_audio(self, *before, *after, *samples, domain),
            PreparedAudioKind::AudioRepeat { input, count } => {
                self.audio_repeat(*input, *count, domain.samples())
            }
        }
    }

    fn finish(mut self, output: NodeId) -> Result<FfmpegRecipe> {
        let output_type = self.node(output)?.value_type();
        let output = self.take_node(output)?;
        let mut mapped = Vec::new();
        match output {
            NodePads::Video(video) => {
                mapped.push(video.video.bracketed());
                mapped.push(video.audio.bracketed());
            }
            NodePads::Audio(audio) => mapped.push(audio.bracketed()),
        }
        let unused = self
            .nodes
            .values()
            .flat_map(unused_pads)
            .collect::<Vec<_>>();
        for (pad, value_type) in unused {
            self.clause(format!(
                "{}{}",
                pad.bracketed(),
                match value_type {
                    ValueType::Video => "nullsink",
                    ValueType::Audio => "anullsink",
                }
            ));
        }
        if self.filter.ends_with(';') {
            self.filter.pop();
        }
        self.recipe.args(["-filter_complex", &self.filter]);
        for pad in &mapped {
            self.recipe.args(["-map", pad]);
        }
        match output_type {
            ValueType::Video => self.context.append_video_output(&mut self.recipe),
            ValueType::Audio => self.context.append_audio_output(&mut self.recipe),
        }
        Ok(self.recipe)
    }

    fn node(&self, id: NodeId) -> Result<&PreparedNode> {
        self.context.nodes().get(id.get() as usize).ok_or_else(|| {
            invalid_plan(
                self.context,
                &format!("fused primitive {} is unavailable", id.get()),
            )
        })
    }

    pub(super) fn context(&self) -> &RecipeContext<'_> {
        self.context
    }

    pub(super) fn video_input(&mut self, id: NodeId) -> Result<VideoPads> {
        match self.take_node(id)? {
            NodePads::Video(video) => Ok(video),
            NodePads::Audio(_) => Err(invalid_plan(self.context, "Video input resolved to Audio")),
        }
    }

    fn video_picture_input(&mut self, id: NodeId) -> Result<Pad> {
        if self.included.contains(&id) {
            let stored = self.nodes.get_mut(&id).ok_or_else(|| {
                invalid_plan(
                    self.context,
                    "fused Video input was not lowered before its consumer",
                )
            })?;
            return match stored {
                StoredNodePads::Video { video, .. } => take_pad(video, self.context),
                StoredNodePads::Audio(_) => {
                    Err(invalid_plan(self.context, "Video input resolved to Audio"))
                }
            };
        }
        match self.artifact_input(id)? {
            NodePads::Video(video) => Ok(video.video),
            NodePads::Audio(_) => Err(invalid_plan(self.context, "Video input resolved to Audio")),
        }
    }

    fn video_audio_input(&mut self, id: NodeId) -> Result<Pad> {
        if self.included.contains(&id) {
            let stored = self.nodes.get_mut(&id).ok_or_else(|| {
                invalid_plan(
                    self.context,
                    "fused Video input was not lowered before its consumer",
                )
            })?;
            return match stored {
                StoredNodePads::Video { audio, .. } => take_pad(audio, self.context),
                StoredNodePads::Audio(_) => {
                    Err(invalid_plan(self.context, "Video input resolved to Audio"))
                }
            };
        }
        match self.artifact_input(id)? {
            NodePads::Video(video) => Ok(video.audio),
            NodePads::Audio(_) => Err(invalid_plan(self.context, "Video input resolved to Audio")),
        }
    }

    pub(super) fn audio_input(&mut self, id: NodeId) -> Result<Pad> {
        match self.take_node(id)? {
            NodePads::Audio(audio) => Ok(audio),
            NodePads::Video(_) => Err(invalid_plan(self.context, "Audio input resolved to Video")),
        }
    }

    fn take_node(&mut self, id: NodeId) -> Result<NodePads> {
        if self.included.contains(&id) {
            let stored = self.nodes.get_mut(&id).ok_or_else(|| {
                invalid_plan(
                    self.context,
                    "fused input was not lowered before its consumer",
                )
            })?;
            return match stored {
                StoredNodePads::Video { video, audio } => Ok(NodePads::Video(VideoPads {
                    video: take_pad(video, self.context)?,
                    audio: take_pad(audio, self.context)?,
                })),
                StoredNodePads::Audio(audio) => Ok(NodePads::Audio(take_pad(audio, self.context)?)),
            };
        }
        self.artifact_input(id)
    }

    fn asset_input(
        &mut self,
        arguments: &[&str],
        asset: &crate::preflight::PreparedAsset,
        value_type: ValueType,
    ) -> NodePads {
        for argument in arguments {
            self.recipe.arg(*argument);
        }
        self.recipe.asset(asset.source_path());
        let index = self.take_input_index();
        match value_type {
            ValueType::Video => NodePads::Video(VideoPads {
                video: Pad(format!("{index}:v:0")),
                audio: Pad(format!("{index}:a:0")),
            }),
            ValueType::Audio => NodePads::Audio(Pad(format!("{index}:a:0"))),
        }
    }

    fn artifact_input(&mut self, id: NodeId) -> Result<NodePads> {
        self.artifact_input_with(id, &["-i"])
    }

    fn artifact_input_with(&mut self, id: NodeId, arguments: &[&str]) -> Result<NodePads> {
        if self.included.contains(&id) {
            return Err(invalid_plan(
                self.context,
                "input-scoped FFmpeg behavior requires a materialized artifact",
            ));
        }
        let value_type = self.node(id)?.value_type();
        for argument in arguments {
            self.recipe.arg(*argument);
        }
        self.recipe.artifact(id);
        Ok(self.input_pads(value_type))
    }

    fn lavfi_audio(&mut self, source: String) -> Pad {
        self.recipe.args(["-f", "lavfi", "-i"]).arg(source);
        let index = self.take_input_index();
        Pad(format!("{index}:a"))
    }

    fn lavfi_video(&mut self, source: String) -> Pad {
        self.recipe.args(["-f", "lavfi", "-i"]).arg(source);
        let index = self.take_input_index();
        Pad(format!("{index}:v"))
    }

    fn input_pads(&mut self, value_type: ValueType) -> NodePads {
        let index = self.take_input_index();
        match value_type {
            ValueType::Video => NodePads::Video(VideoPads {
                video: Pad(format!("{index}:v")),
                audio: Pad(format!("{index}:a")),
            }),
            ValueType::Audio => NodePads::Audio(Pad(format!("{index}:a"))),
        }
    }

    fn take_input_index(&mut self) -> usize {
        let index = self.next_input;
        self.next_input += 1;
        index
    }

    pub(super) fn output_video_pads(&mut self) -> VideoPads {
        VideoPads {
            video: self.output_video_pad(),
            audio: self.output_audio_pad(),
        }
    }

    pub(super) fn output_video_pad(&mut self) -> Pad {
        self.output_pad("v")
    }

    pub(super) fn output_audio_pad(&mut self) -> Pad {
        self.output_pad("a")
    }

    fn output_pad(&mut self, kind: &str) -> Pad {
        let id = self.next_pad;
        self.next_pad += 1;
        Pad(format!("g{id}{kind}"))
    }

    pub(super) fn clause(&mut self, clause: impl AsRef<str>) {
        self.filter.push_str(clause.as_ref());
        self.filter.push(';');
    }
}

fn store(pads: NodePads) -> StoredNodePads {
    match pads {
        NodePads::Video(video) => StoredNodePads::Video {
            video: StoredPad {
                pad: video.video,
                consumed: false,
            },
            audio: StoredPad {
                pad: video.audio,
                consumed: false,
            },
        },
        NodePads::Audio(audio) => StoredNodePads::Audio(StoredPad {
            pad: audio,
            consumed: false,
        }),
    }
}

fn take_pad(pad: &mut StoredPad, context: &RecipeContext<'_>) -> Result<Pad> {
    if pad.consumed {
        return Err(invalid_plan(
            context,
            "fused region requires one stream pad more than once",
        ));
    }
    pad.consumed = true;
    Ok(pad.pad.clone())
}

fn unused_pads(pads: &StoredNodePads) -> Vec<(Pad, ValueType)> {
    match pads {
        StoredNodePads::Video { video, audio } => [
            (!video.consumed).then(|| (video.pad.clone(), ValueType::Video)),
            (!audio.consumed).then(|| (audio.pad.clone(), ValueType::Audio)),
        ]
        .into_iter()
        .flatten()
        .collect(),
        StoredNodePads::Audio(audio) => (!audio.consumed)
            .then(|| (audio.pad.clone(), ValueType::Audio))
            .into_iter()
            .collect(),
    }
}

fn invalid_plan(context: &RecipeContext<'_>, message: &str) -> Diagnostic {
    Diagnostic::builtin(
        BuiltinDiagnostic::InvalidPlan,
        message,
        context.span().clone(),
    )
}
