use std::num::NonZeroU64;
use std::path::PathBuf;

use serde::Serialize;

use crate::external::ExternalInvocation;
use crate::model::{FrameCount, FrameRange, ImageFit, SampleRange, ValueRef, ValueType};

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

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub(crate) enum SemanticNodeKind {
    ImageVideo {
        path: PathBuf,
        frames: FrameCount,
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
    AudioRepeat {
        input: ValueRef,
        count: NonZeroU64,
    },
    Zoom {
        input: ValueRef,
        percent: u32,
    },
    Wobble {
        input: ValueRef,
        pixels: u32,
    },
    FlashJoin {
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
    AudioConcat {
        inputs: Vec<ValueRef>,
    },
    Slice {
        input: ValueRef,
        range: FrameRange,
    },
    AudioSlice {
        input: ValueRef,
        range: SampleRange,
    },
    ReplaceRange {
        base: ValueRef,
        replacement: ValueRef,
        range: FrameRange,
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
    pub(crate) const fn value_type(&self) -> ValueType {
        match self {
            Self::AudioSource { .. }
            | Self::AudioRepeat { .. }
            | Self::AudioConcat { .. }
            | Self::AudioSlice { .. }
            | Self::ExtractAudio { .. } => ValueType::Audio,
            Self::Reference { value_type, .. } => *value_type,
            Self::ImageVideo { .. }
            | Self::VideoSource { .. }
            | Self::Repeat { .. }
            | Self::Zoom { .. }
            | Self::Wobble { .. }
            | Self::FlashJoin { .. }
            | Self::Crossfade { .. }
            | Self::Concat { .. }
            | Self::Slice { .. }
            | Self::ReplaceRange { .. }
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
            | Self::AudioRepeat { input, .. }
            | Self::AudioSlice { input, .. }
            | Self::Zoom { input, .. }
            | Self::Wobble { input, .. }
            | Self::Slice { input, .. }
            | Self::ExtractAudio { video: input }
            | Self::AudioOnBlack { audio: input }
                if index == 0 =>
            {
                value(*input)
            }
            Self::Concat { inputs } | Self::AudioConcat { inputs } => {
                inputs.get(index).copied().map(SemanticDependency::Value)
            }
            Self::FlashJoin { before, after, .. } | Self::Crossfade { before, after, .. } => {
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
            | Self::AudioRepeat { .. }
            | Self::AudioSlice { .. }
            | Self::Zoom { .. }
            | Self::Wobble { .. }
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
    use crate::model::ValueId;

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

        let zoom = SemanticNodeKind::Zoom {
            input: value(0, ValueType::Video),
            percent: 8,
        };
        assert_eq!(zoom.value_type(), ValueType::Video);
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
