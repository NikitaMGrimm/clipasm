mod effects;
mod external;
mod host;
mod media;
mod timeline;
mod transitions;

use std::collections::HashMap;

use crate::compiler::CompiledProgram;
use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::model::{
    AudioDomain, FrameCount, NodeId, TimelineRate, ValueId, ValueRef, VideoDomain, VideoSpec,
};
use crate::semantic::{SemanticNodeKind, SourceOrigin};

use super::identity::node_fingerprint;
use super::plan::PreparedMedia;
use super::{PreparedAudioKind, PreparedNode, PreparedVideoKind};

pub(super) struct PreflightLowerer<'a> {
    pub(super) compiled: &'a CompiledProgram,
    pub(super) host: &'a mut dyn host::PreparationHost,
    pub(super) nodes: Vec<PreparedNode>,
    pub(super) lowered: HashMap<ValueId, NodeId>,
}

#[cfg(feature = "native")]
pub(super) use host::NativePreparationHost;
pub(super) use host::PreparationHost;

impl PreflightLowerer<'_> {
    pub(super) fn lower(&mut self, value: ValueRef) -> Result<NodeId> {
        if let Some(node) = self.lowered.get(&value.id()) {
            return Ok(*node);
        }
        let compiled_node = &self.compiled.nodes()[value.id().get() as usize];
        let result = match compiled_node.kind() {
            SemanticNodeKind::ImageVideo { path, frames, fit } => {
                media::image(self, compiled_node, path, *frames, *fit)?
            }
            SemanticNodeKind::DeferredImageVideo { path, extent, fit } => {
                media::deferred_image(self, compiled_node, path, extent, *fit)?
            }
            SemanticNodeKind::VideoSource { path, fit } => {
                media::video_source(self, compiled_node, path, *fit)?
            }
            SemanticNodeKind::AudioSource { path } => {
                media::audio_source(self, compiled_node, path)?
            }
            SemanticNodeKind::Reference { symbol, .. } => {
                let target = self.compiled.symbol_value(*symbol).ok_or_else(|| {
                    Diagnostic::builtin(
                        BuiltinDiagnostic::MissingReference,
                        format!("reference names unknown symbol {}", symbol.index()),
                        compiled_node.origin().span.clone(),
                    )
                })?;
                self.prepared_dependency(target, compiled_node.origin())?
            }
            SemanticNodeKind::Repeat { input, count } => {
                timeline::repeat(self, compiled_node, *input, *count)?
            }
            SemanticNodeKind::ZoomIn { input, by } => {
                effects::zoom_in(self, compiled_node, *input, by.clone())?
            }
            SemanticNodeKind::FlashCut {
                before,
                after,
                frames,
            } => transitions::flash_cut(self, compiled_node, *before, *after, *frames)?,
            SemanticNodeKind::Crossfade {
                before,
                after,
                duration,
            } => transitions::crossfade(self, compiled_node, *before, *after, *duration)?,
            SemanticNodeKind::Concat { inputs } => timeline::concat(self, compiled_node, inputs)?,
            SemanticNodeKind::Slice { input, range } => {
                timeline::slice(self, compiled_node, *input, *range)?
            }
            SemanticNodeKind::DeferredSlice { input, range } => {
                timeline::deferred_slice(self, compiled_node, *input, range)?
            }
            SemanticNodeKind::ReplaceRange {
                base,
                replacement,
                range,
            } => timeline::replace_range(self, compiled_node, *base, *replacement, *range)?,
            SemanticNodeKind::DeferredReplaceRange {
                base,
                replacement,
                range,
            } => timeline::deferred_replace_range(self, compiled_node, *base, *replacement, range)?,
            SemanticNodeKind::ExtractAudio { video } => {
                media::extract_audio(self, compiled_node, *video)?
            }
            SemanticNodeKind::SetAudio { audio, video } => {
                media::set_audio(self, compiled_node, *audio, *video)?
            }
            SemanticNodeKind::AudioOnBlack { audio } => {
                media::audio_on_black(self, compiled_node, *audio)?
            }
            SemanticNodeKind::ExternalVideo { invocation } => {
                external::video(self, compiled_node, invocation)?
            }
        };
        self.lowered.insert(value.id(), result);
        Ok(result)
    }

    pub(super) fn prepared_dependency(
        &self,
        value: ValueRef,
        origin: &SourceOrigin,
    ) -> Result<NodeId> {
        self.lowered.get(&value.id()).copied().ok_or_else(|| {
            Diagnostic::builtin(
                BuiltinDiagnostic::InvalidGraph,
                format!(
                    "semantic dependency {} was not prepared before its consumer",
                    value.id().get()
                ),
                origin.span.clone(),
            )
        })
    }

    pub(super) fn video_domain(
        &self,
        node: NodeId,
        origin: &SourceOrigin,
    ) -> Result<(&VideoDomain, bool)> {
        match self.nodes[node.get() as usize].prepared_media() {
            PreparedMedia::Video {
                domain, has_audio, ..
            } => Ok((domain, *has_audio)),
            PreparedMedia::Audio { .. } => Err(Diagnostic::builtin(
                BuiltinDiagnostic::InvalidGraph,
                format!(
                    "prepared dependency {} is Audio, but Video is required",
                    node.get()
                ),
                origin.span.clone(),
            )),
        }
    }

    pub(super) fn audio_domain(&self, node: NodeId, origin: &SourceOrigin) -> Result<&AudioDomain> {
        match self.nodes[node.get() as usize].prepared_media() {
            PreparedMedia::Audio { domain, .. } => Ok(domain),
            PreparedMedia::Video { .. } => Err(Diagnostic::builtin(
                BuiltinDiagnostic::InvalidGraph,
                format!(
                    "prepared dependency {} is Video, but Audio is required",
                    node.get()
                ),
                origin.span.clone(),
            )),
        }
    }

    pub(super) fn concat_domain(
        &self,
        inputs: &[NodeId],
        origin: &SourceOrigin,
    ) -> Result<VideoDomain> {
        let mut frames = FrameCount(0);
        for input in inputs {
            frames =
                frames.checked_add(self.video_domain(*input, origin)?.0.frames(), &origin.span)?;
        }
        Ok(project_domain(self.compiled.video(), frames))
    }

    pub(super) fn add_video_node(
        &mut self,
        kind: PreparedVideoKind,
        domain: VideoDomain,
        has_audio: bool,
        semantic_version: u32,
        origin: SourceOrigin,
    ) -> Result<NodeId> {
        let project_audio = *self.compiled.audio();
        let samples = TimelineRate::new(domain.video_spec(), project_audio)
            .samples_for_frames(domain.frames(), &origin.span)?;
        self.add_node(
            PreparedMedia::Video {
                kind,
                domain,
                working_audio: AudioDomain::new(samples, project_audio),
                has_audio,
            },
            semantic_version,
            origin,
        )
    }

    pub(super) fn add_audio_node(
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
            Diagnostic::builtin(
                BuiltinDiagnostic::GraphTooLarge,
                "prepared graph contains too many primitive nodes",
                origin.span.clone(),
            )
        })?);
        let fingerprint = node_fingerprint(&media, semantic_version, &self.nodes)?;
        self.nodes
            .push(PreparedNode::new(id, media, origin, fingerprint));
        Ok(id)
    }
}

pub(super) fn project_domain(video: &VideoSpec, frames: FrameCount) -> VideoDomain {
    VideoDomain::new(frames, *video)
}
