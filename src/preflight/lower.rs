use std::collections::HashMap;

use crate::compiler::CompiledProgram;
use crate::diagnostic::{Diagnostic, Result};
use crate::model::{FrameCount, FrameRange, NodeId, ValueId, ValueRef, VideoDomain, VideoSpec};
use crate::semantic::{SemanticNodeKind, SourceOrigin};
use crate::source::SourceSpan;

use super::assets::{prepare_image_asset, prepare_video_asset};
use super::identity::node_fingerprint;
use super::tools::ToolIdentity;
use super::{PreparedNode, PreparedNodeKind};

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
                self.add_node(
                    PreparedNodeKind::ImageVideo {
                        asset,
                        frames: *frames,
                        fit: *fit,
                    },
                    compiled_node.domain().expect("Video node domain").clone(),
                    compiled_node.semantic_version(),
                    compiled_node.origin().clone(),
                )?
            }
            SemanticNodeKind::VideoSource { path, fit } => {
                let (asset, frames) = prepare_video_asset(
                    path,
                    self.compiled.video(),
                    compiled_node.origin(),
                    self.ffmpeg,
                    self.ffprobe,
                )?;
                self.add_node(
                    PreparedNodeKind::VideoSource {
                        asset,
                        frames,
                        fit: *fit,
                    },
                    project_domain(self.compiled.video(), frames),
                    compiled_node.semantic_version(),
                    compiled_node.origin().clone(),
                )?
            }
            SemanticNodeKind::Reference { name } => {
                let target = self.compiled.named_values()[name];
                self.prepared_dependency(target, compiled_node.origin())?
            }
            SemanticNodeKind::Repeat { input, count } => {
                let input = self.prepared_dependency(*input, compiled_node.origin())?;
                let frames = self.nodes[input.get() as usize]
                    .domain
                    .frames
                    .checked_mul(count.get(), &compiled_node.origin().span)?;
                self.add_node(
                    PreparedNodeKind::Repeat {
                        input,
                        count: *count,
                        frames,
                    },
                    project_domain(self.compiled.video(), frames),
                    compiled_node.semantic_version(),
                    compiled_node.origin().clone(),
                )?
            }
            SemanticNodeKind::Zoom { input, percent } => {
                let input = self.prepared_dependency(*input, compiled_node.origin())?;
                let domain = self.nodes[input.get() as usize].domain.clone();
                self.add_node(
                    PreparedNodeKind::Zoom {
                        input,
                        percent: *percent,
                    },
                    domain,
                    compiled_node.semantic_version(),
                    compiled_node.origin().clone(),
                )?
            }
            SemanticNodeKind::Wobble { input, pixels } => {
                let input = self.prepared_dependency(*input, compiled_node.origin())?;
                let domain = self.nodes[input.get() as usize].domain.clone();
                self.add_node(
                    PreparedNodeKind::Wobble {
                        input,
                        pixels: *pixels,
                    },
                    domain,
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
                let after_frames = self.nodes[after.get() as usize].domain.frames;
                validate_flash_frames(*frames, after_frames, &compiled_node.origin().span)?;
                let total = self.nodes[before.get() as usize]
                    .domain
                    .frames
                    .checked_add(after_frames, &compiled_node.origin().span)?;
                self.add_node(
                    PreparedNodeKind::FlashJoin {
                        before,
                        after,
                        frames: *frames,
                    },
                    project_domain(self.compiled.video(), total),
                    compiled_node.semantic_version(),
                    compiled_node.origin().clone(),
                )?
            }
            SemanticNodeKind::Concat { inputs } => {
                let inputs = inputs
                    .iter()
                    .map(|input| self.prepared_dependency(*input, compiled_node.origin()))
                    .collect::<Result<Vec<_>>>()?;
                let domain = self.concat_domain(&inputs, &compiled_node.origin().span)?;
                self.add_node(
                    PreparedNodeKind::Concat { inputs },
                    domain,
                    compiled_node.semantic_version(),
                    compiled_node.origin().clone(),
                )?
            }
            SemanticNodeKind::Slice { input, range } => {
                let input = self.prepared_dependency(*input, compiled_node.origin())?;
                let input_domain = &self.nodes[input.get() as usize].domain;
                validate_prepared_range(*range, input_domain, &compiled_node.origin().span)?;
                self.add_node(
                    PreparedNodeKind::Slice {
                        input,
                        range: *range,
                    },
                    project_domain(self.compiled.video(), range.frames()),
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
                let base_domain = self.nodes[base_node.get() as usize].domain.clone();
                validate_prepared_range(*range, &base_domain, &compiled_node.origin().span)?;
                let mut pieces = Vec::new();
                if range.start() > 0 {
                    pieces.push(self.add_node(
                        PreparedNodeKind::Slice {
                            input: base_node,
                            range:
                                FrameRange::new(0, range.start()).expect("nonempty during prefix"),
                        },
                        VideoDomain {
                            frames: FrameCount(range.start()),
                            width: base_domain.width,
                            height: base_domain.height,
                            frame_rate: base_domain.frame_rate,
                        },
                        compiled_node.semantic_version(),
                        compiled_node.origin().clone_with_construct("range prefix"),
                    )?);
                }
                pieces.push(replacement_node);
                if range.end() < base_domain.frames.0 {
                    pieces.push(
                        self.add_node(
                            PreparedNodeKind::Slice {
                                input: base_node,
                                range: FrameRange::new(range.end(), base_domain.frames.0)
                                    .expect("nonempty during suffix"),
                            },
                            VideoDomain {
                                frames: FrameCount(base_domain.frames.0 - range.end()),
                                width: base_domain.width,
                                height: base_domain.height,
                                frame_rate: base_domain.frame_rate,
                            },
                            compiled_node.semantic_version(),
                            compiled_node.origin().clone_with_construct("range suffix"),
                        )?,
                    );
                }
                if pieces.len() == 1 {
                    pieces[0]
                } else {
                    let domain = self.concat_domain(&pieces, &compiled_node.origin().span)?;
                    self.add_node(
                        PreparedNodeKind::Concat { inputs: pieces },
                        domain,
                        compiled_node.semantic_version(),
                        compiled_node.origin().clone(),
                    )?
                }
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

    fn concat_domain(&self, inputs: &[NodeId], span: &SourceSpan) -> Result<VideoDomain> {
        let mut frames = FrameCount(0);
        for input in inputs {
            frames = frames.checked_add(self.nodes[input.get() as usize].domain.frames, span)?;
        }
        Ok(project_domain(self.compiled.video(), frames))
    }

    fn add_node(
        &mut self,
        kind: PreparedNodeKind,
        domain: VideoDomain,
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
        let fingerprint = node_fingerprint(&kind, &domain, semantic_version, &self.nodes)?;
        self.nodes.push(PreparedNode {
            id,
            kind,
            domain,
            origin,
            fingerprint,
        });
        Ok(id)
    }
}

fn project_domain(video: &VideoSpec, frames: FrameCount) -> VideoDomain {
    VideoDomain {
        frames,
        width: video.width,
        height: video.height,
        frame_rate: video.fps,
    }
}

fn validate_prepared_range(
    range: FrameRange,
    input: &VideoDomain,
    span: &SourceSpan,
) -> Result<()> {
    if range.end() > input.frames.0 {
        return Err(Diagnostic::new(
            "E_INVALID_TIME_RANGE",
            format!(
                "frame range {}..{} is outside the base Video domain of {} frames",
                range.start(),
                range.end(),
                input.frames.0
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
