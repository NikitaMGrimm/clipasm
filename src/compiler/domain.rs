use crate::compiler::evaluate::Evaluation;
use crate::diagnostic::{Diagnostic, Result};
use crate::model::{FrameCount, ValueRef, ValueType, VideoDomain, VideoSpec};
use crate::semantic::SemanticNodeKind;

#[derive(Clone)]
enum DomainKnowledge {
    NotVideo,
    Deferred,
    Known(VideoDomain),
}

#[allow(clippy::too_many_lines)]
pub(super) fn infer_domains(
    evaluation: &Evaluation,
    video: &VideoSpec,
    order: &[ValueRef],
) -> Result<Vec<Option<VideoDomain>>> {
    let mut knowledge = vec![DomainKnowledge::NotVideo; evaluation.nodes.len()];
    for value in order {
        let index = value.id().get() as usize;
        if value.value_type() != ValueType::Video {
            continue;
        }
        let node = &evaluation.nodes[index];
        knowledge[index] = match node.kind() {
            SemanticNodeKind::ImageVideo { frames, .. } => {
                DomainKnowledge::Known(project_domain(video, *frames))
            }
            SemanticNodeKind::VideoSource { .. } | SemanticNodeKind::AudioOnBlack { .. } => {
                DomainKnowledge::Deferred
            }
            SemanticNodeKind::ExternalVideo { invocation } => {
                let preserved = invocation.inputs[&invocation.preserve_input];
                knowledge[preserved.id().get() as usize].clone()
            }
            SemanticNodeKind::AudioSource { .. }
            | SemanticNodeKind::AudioRepeat { .. }
            | SemanticNodeKind::AudioConcat { .. }
            | SemanticNodeKind::AudioSlice { .. }
            | SemanticNodeKind::ExtractAudio { .. } => DomainKnowledge::NotVideo,
            SemanticNodeKind::Reference { symbol, .. } => {
                let target = evaluation.symbols[symbol.index()]
                    .value
                    .expect("references were resolved before domain inference");
                knowledge[target.id().get() as usize].clone()
            }
            SemanticNodeKind::Repeat { input, count } => {
                match &knowledge[input.id().get() as usize] {
                    DomainKnowledge::Known(domain) => DomainKnowledge::Known(project_domain(
                        video,
                        domain
                            .frames()
                            .checked_mul(count.get(), &node.origin().span)?,
                    )),
                    DomainKnowledge::Deferred => DomainKnowledge::Deferred,
                    DomainKnowledge::NotVideo => DomainKnowledge::NotVideo,
                }
            }
            SemanticNodeKind::Zoom { input, .. }
            | SemanticNodeKind::Wobble { input, .. }
            | SemanticNodeKind::SetAudio { video: input, .. } => {
                knowledge[input.id().get() as usize].clone()
            }
            SemanticNodeKind::FlashJoin {
                before,
                after,
                frames,
            } => infer_flash_domain(
                &knowledge[before.id().get() as usize],
                &knowledge[after.id().get() as usize],
                *frames,
                video,
                &node.origin().span,
            )?,
            SemanticNodeKind::Concat { inputs } => {
                infer_concat_domain(inputs, &knowledge, video, &node.origin().span)?
            }
            SemanticNodeKind::Slice { input, range } => {
                if let DomainKnowledge::Known(input_domain) = &knowledge[input.id().get() as usize]
                {
                    validate_range(*range, input_domain.frames(), &node.origin().span)?;
                }
                DomainKnowledge::Known(project_domain(video, range.frames()))
            }
            SemanticNodeKind::ReplaceRange {
                base,
                replacement,
                range,
            } => {
                let base_domain = &knowledge[base.id().get() as usize];
                if let DomainKnowledge::Known(base_domain) = base_domain {
                    validate_range(*range, base_domain.frames(), &node.origin().span)?;
                }
                let replacement_domain = &knowledge[replacement.id().get() as usize];
                match (base_domain, replacement_domain) {
                    (
                        DomainKnowledge::Known(base_domain),
                        DomainKnowledge::Known(replacement_domain),
                    ) => DomainKnowledge::Known(project_domain(
                        video,
                        FrameCount(base_domain.frames().0 - range.frames().0)
                            .checked_add(replacement_domain.frames(), &node.origin().span)?,
                    )),
                    (DomainKnowledge::NotVideo, _) | (_, DomainKnowledge::NotVideo) => {
                        unreachable!("replace-range inputs are typed Video")
                    }
                    _ => DomainKnowledge::Deferred,
                }
            }
        };
    }

    Ok(knowledge
        .into_iter()
        .map(|knowledge| match knowledge {
            DomainKnowledge::Known(domain) => Some(domain),
            DomainKnowledge::NotVideo | DomainKnowledge::Deferred => None,
        })
        .collect())
}

fn project_domain(video: &VideoSpec, frames: FrameCount) -> VideoDomain {
    VideoDomain::new(frames, *video)
}

fn validate_range(
    range: crate::model::FrameRange,
    input: FrameCount,
    span: &crate::source::SourceSpan,
) -> Result<()> {
    if range.end() > input.0 {
        return Err(Diagnostic::new(
            "E_INVALID_TIME_RANGE",
            format!(
                "frame range {}..{} is outside the base Video domain of {} frames",
                range.start(),
                range.end(),
                input.0
            ),
            span.clone(),
        ));
    }
    Ok(())
}

fn validate_flash_frames(
    frames: FrameCount,
    after: FrameCount,
    span: &crate::source::SourceSpan,
) -> Result<()> {
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

fn infer_flash_domain(
    before: &DomainKnowledge,
    after: &DomainKnowledge,
    frames: FrameCount,
    video: &VideoSpec,
    span: &crate::source::SourceSpan,
) -> Result<DomainKnowledge> {
    if let DomainKnowledge::Known(after) = after {
        validate_flash_frames(frames, after.frames(), span)?;
    }
    Ok(match (before, after) {
        (DomainKnowledge::Known(before), DomainKnowledge::Known(after)) => DomainKnowledge::Known(
            project_domain(video, before.frames().checked_add(after.frames(), span)?),
        ),
        (DomainKnowledge::NotVideo, _) | (_, DomainKnowledge::NotVideo) => {
            unreachable!("flash inputs are typed Video")
        }
        _ => DomainKnowledge::Deferred,
    })
}

fn infer_concat_domain(
    inputs: &[ValueRef],
    knowledge: &[DomainKnowledge],
    video: &VideoSpec,
    span: &crate::source::SourceSpan,
) -> Result<DomainKnowledge> {
    if inputs.iter().any(|input| {
        matches!(
            knowledge[input.id().get() as usize],
            DomainKnowledge::Deferred
        )
    }) {
        return Ok(DomainKnowledge::Deferred);
    }
    let mut total = FrameCount(0);
    for input in inputs {
        match &knowledge[input.id().get() as usize] {
            DomainKnowledge::Known(domain) => {
                total = total.checked_add(domain.frames(), span)?;
            }
            DomainKnowledge::Deferred => unreachable!("deferred concat handled before summing"),
            DomainKnowledge::NotVideo => unreachable!("concat inputs are typed Video"),
        }
    }
    Ok(DomainKnowledge::Known(project_domain(video, total)))
}
