//! Invariant-protected identifiers, time quantities, and media properties.
//!
//! These values appear in compiled and prepared-plan inspection APIs. IDs are
//! engine-assigned and opaque; Video and Audio domains retain exact native frames and samples.

mod audio;
mod number;
mod time;
mod timeline;
mod timeline_expression;
mod video;

pub use audio::{AudioDomain, AudioSpec};
pub use number::Number;
pub(crate) use number::Number as ExactNumber;
pub use time::{FrameCount, FrameRange, SampleRange};
pub(crate) use time::{
    NativeRange, SourceTime, SourceTimeRange, exact_seconds_to_frames, exact_seconds_to_samples,
};
pub(crate) use timeline::{FrameSampleStep, TimelineRate};
pub(crate) use timeline_expression::{TimelineExpression, TimelineRangeExpression};
pub use video::{FrameRate, ImageFit, VideoDomain, VideoSpec};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
/// An engine-owned semantic value identifier.
///
/// ```compile_fail
/// use clipasm::model::ValueId;
///
/// let fabricated = ValueId(42);
/// ```
pub struct ValueId(u32);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
/// An engine-assigned identifier in a prepared plan's node space.
pub struct NodeId(u32);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct TimelineViewId(u32);

impl ValueId {
    pub(crate) const fn new(value: u32) -> Self {
        Self(value)
    }

    #[must_use]
    /// Return the stable numeric index within its compiled program.
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl NodeId {
    pub(crate) const fn new(value: u32) -> Self {
        Self(value)
    }

    #[must_use]
    /// Return the stable numeric index within its prepared plan.
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl TimelineViewId {
    pub(crate) const fn new(value: u32) -> Self {
        Self(value)
    }

    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// The closed set of semantic value types understood by `ClipAsm`.
pub enum ValueType {
    /// A finite audiovisual video value.
    Video,
    /// A finite standalone audio value.
    Audio,
}

impl ValueType {
    #[must_use]
    pub(crate) fn from_source_name(name: &str) -> Option<Self> {
        match name {
            "Video" => Some(Self::Video),
            "Audio" => Some(Self::Audio),
            _ => None,
        }
    }

    #[must_use]
    pub(crate) const fn native_unit_name(self) -> &'static str {
        match self {
            Self::Video => "frames",
            Self::Audio => "samples",
        }
    }
}

impl std::fmt::Display for ValueType {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Video => formatter.write_str("Video"),
            Self::Audio => formatter.write_str("Audio"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
/// An immutable typed reference to a compiled semantic value.
///
/// Multiple stack occurrences can contain the same reference without copying
/// or consuming the underlying value.
pub struct ValueRef {
    id: ValueId,
    value_type: ValueType,
}

impl ValueRef {
    #[must_use]
    pub(crate) const fn new(id: ValueId, value_type: ValueType) -> Self {
        Self { id, value_type }
    }

    #[must_use]
    /// Return the engine-assigned semantic value identifier.
    pub fn id(self) -> ValueId {
        self.id
    }

    #[must_use]
    /// Return the value's compiler-checked type.
    pub const fn value_type(self) -> ValueType {
        self.value_type
    }
}
