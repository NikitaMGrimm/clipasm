//! Invariant-protected identifiers, frame quantities, and video properties.
//!
//! These values appear in compiled and prepared-plan inspection APIs. IDs are
//! engine-assigned and opaque; domains and ranges use exact project frames.

mod audio;
mod time;
mod video;

pub use audio::{AudioDomain, AudioSpec};
pub use time::{FrameCount, FrameRange};
pub(crate) use time::{SourceTime, SourceTimeRange};
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// The closed set of semantic value types understood by `ClipAsm`.
pub enum ValueType {
    /// A finite audiovisual video value.
    Video,
    /// A finite standalone audio value.
    Audio,
    #[cfg(test)]
    /// Synthetic value type used to verify internal type checks.
    Test,
}

impl std::fmt::Display for ValueType {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Video => formatter.write_str("Video"),
            Self::Audio => formatter.write_str("Audio"),
            #[cfg(test)]
            Self::Test => formatter.write_str("Test"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
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
    pub fn value_type(self) -> ValueType {
        self.value_type
    }
}
