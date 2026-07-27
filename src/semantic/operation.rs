use std::num::NonZeroU64;
use std::path::PathBuf;

use serde::Serialize;

use crate::external::ExternalInvocation;
use crate::model::{
    ExactNumber, FrameCount, ImageFit, NativeRange, TimelineRangeExpression, ValueRef, ValueType,
};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub(crate) struct SymbolId(u32);

impl SymbolId {
    #[must_use]
    pub(crate) const fn new(value: u32) -> Self {
        Self(value)
    }

    #[must_use]
    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Debug)]
pub(crate) enum SemanticNodeKind {
    ImageVideo {
        path: PathBuf,
        frames: FrameCount,
        fit: ImageFit,
    },
    DeferredImageVideo {
        path: PathBuf,
        extent: crate::model::TimelineExpression,
        fit: ImageFit,
    },
    VideoSource {
        path: PathBuf,
        fit: ImageFit,
    },
    AudioSource {
        path: PathBuf,
    },
    Reference {
        symbol: SymbolId,
        value_type: ValueType,
    },
    Repeat {
        input: ValueRef,
        count: NonZeroU64,
    },
    ZoomIn {
        input: ValueRef,
        by: ExactNumber,
    },
    FlashCut {
        before: ValueRef,
        after: ValueRef,
        frames: FrameCount,
    },
    Crossfade {
        before: ValueRef,
        after: ValueRef,
        frames: FrameCount,
    },
    Concat {
        inputs: Vec<ValueRef>,
    },
    Slice {
        input: ValueRef,
        range: NativeRange,
    },
    DeferredSlice {
        input: ValueRef,
        range: TimelineRangeExpression,
    },
    ReplaceRange {
        base: ValueRef,
        replacement: ValueRef,
        range: NativeRange,
    },
    DeferredReplaceRange {
        base: ValueRef,
        replacement: ValueRef,
        range: TimelineRangeExpression,
    },
    ExtractAudio {
        video: ValueRef,
    },
    SetAudio {
        audio: ValueRef,
        video: ValueRef,
    },
    AudioOnBlack {
        audio: ValueRef,
    },
    ExternalVideo {
        invocation: ExternalInvocation,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SemanticDependency {
    Value(ValueRef),
    Symbol(SymbolId),
}

impl SemanticNodeKind {
    #[must_use]
    pub(crate) fn value_type(&self) -> ValueType {
        match self {
            Self::AudioSource { .. } | Self::ExtractAudio { .. } => ValueType::Audio,
            Self::Reference { value_type, .. } => *value_type,
            Self::Repeat { input, .. }
            | Self::Slice { input, .. }
            | Self::DeferredSlice { input, .. } => input.value_type(),
            Self::Concat { inputs } => inputs
                .first()
                .expect("semantic concat inputs are nonempty")
                .value_type(),
            Self::ReplaceRange { base, .. } | Self::DeferredReplaceRange { base, .. } => {
                base.value_type()
            }
            Self::ImageVideo { .. }
            | Self::DeferredImageVideo { .. }
            | Self::VideoSource { .. }
            | Self::ZoomIn { .. }
            | Self::FlashCut { .. }
            | Self::Crossfade { .. }
            | Self::SetAudio { .. }
            | Self::AudioOnBlack { .. }
            | Self::ExternalVideo { .. } => ValueType::Video,
        }
    }

    #[must_use]
    pub(crate) fn dependency_at(&self, index: usize) -> Option<SemanticDependency> {
        let value = |value| Some(SemanticDependency::Value(value));
        match self {
            Self::Reference { symbol, .. } if index == 0 => {
                Some(SemanticDependency::Symbol(*symbol))
            }
            Self::Repeat { input, .. }
            | Self::ZoomIn { input, .. }
            | Self::Slice { input, .. }
            | Self::ExtractAudio { video: input }
            | Self::AudioOnBlack { audio: input }
                if index == 0 =>
            {
                value(*input)
            }
            Self::DeferredSlice { input, range } => {
                if index == 0 {
                    return value(*input);
                }
                range
                    .start
                    .terms()
                    .iter()
                    .chain(range.end.terms())
                    .nth(index - 1)
                    .map(|term| SemanticDependency::Value(term.value))
            }
            Self::DeferredImageVideo { extent, .. } => extent
                .terms()
                .get(index)
                .map(|term| SemanticDependency::Value(term.value)),
            Self::Concat { inputs } => inputs.get(index).copied().map(SemanticDependency::Value),
            Self::FlashCut { before, after, .. } | Self::Crossfade { before, after, .. } => {
                [*before, *after]
                    .get(index)
                    .copied()
                    .map(SemanticDependency::Value)
            }
            Self::ReplaceRange {
                base, replacement, ..
            } => [*base, *replacement]
                .get(index)
                .copied()
                .map(SemanticDependency::Value),
            Self::DeferredReplaceRange {
                base,
                replacement,
                range,
            } => {
                if let Some(value) = [*base, *replacement].get(index).copied() {
                    return Some(SemanticDependency::Value(value));
                }
                range
                    .start
                    .terms()
                    .iter()
                    .chain(range.end.terms())
                    .nth(index - 2)
                    .map(|term| SemanticDependency::Value(term.value))
            }
            Self::SetAudio { audio, video } => [*audio, *video]
                .get(index)
                .copied()
                .map(SemanticDependency::Value),
            Self::ExternalVideo { invocation } => invocation
                .inputs
                .values()
                .nth(index)
                .copied()
                .map(SemanticDependency::Value),
            Self::ImageVideo { .. }
            | Self::VideoSource { .. }
            | Self::AudioSource { .. }
            | Self::Reference { .. }
            | Self::Repeat { .. }
            | Self::ZoomIn { .. }
            | Self::Slice { .. }
            | Self::ExtractAudio { .. }
            | Self::AudioOnBlack { .. } => None,
        }
    }

    pub(crate) fn visit_dependencies(&self, mut visitor: impl FnMut(SemanticDependency)) {
        let mut index = 0;
        while let Some(dependency) = self.dependency_at(index) {
            visitor(dependency);
            index += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FrameRange, ValueId};

    fn value(id: u32, value_type: ValueType) -> ValueRef {
        ValueRef::new(ValueId::new(id), value_type)
    }

    #[test]
    fn operation_structure_owns_result_type() {
        let reference = SemanticNodeKind::Reference {
            symbol: SymbolId::new(7),
            value_type: ValueType::Audio,
        };
        assert_eq!(reference.value_type(), ValueType::Audio);

        let repeat = SemanticNodeKind::Repeat {
            input: value(0, ValueType::Audio),
            count: NonZeroU64::new(2).expect("nonzero"),
        };
        assert_eq!(repeat.value_type(), ValueType::Audio);

        let slice = SemanticNodeKind::Slice {
            input: value(1, ValueType::Video),
            range: NativeRange::Frames(FrameRange::new(0, 1).expect("range")),
        };
        assert_eq!(slice.value_type(), ValueType::Video);
    }

    #[test]
    fn dependencies_have_one_canonical_order() {
        let audio = value(0, ValueType::Audio);
        let video = value(1, ValueType::Video);
        let operation = SemanticNodeKind::SetAudio { audio, video };
        let mut dependencies = Vec::new();
        operation.visit_dependencies(|dependency| dependencies.push(dependency));

        assert_eq!(
            dependencies,
            [
                SemanticDependency::Value(audio),
                SemanticDependency::Value(video),
            ]
        );
    }
}
