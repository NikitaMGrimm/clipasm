mod time;
mod video;

pub use time::{FrameCount, FrameRange, SourceTime, SourceTimeRange};
pub use video::{FrameRate, ImageFit, VideoDomain, VideoSpec};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ValueId(pub u32);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct NodeId(pub u32);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueType {
    Video,
    #[cfg(test)]
    Test,
}

impl std::fmt::Display for ValueType {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Video => formatter.write_str("Video"),
            #[cfg(test)]
            Self::Test => formatter.write_str("Test"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct ValueRef {
    id: ValueId,
    value_type: ValueType,
}

impl ValueRef {
    #[must_use]
    pub fn new(id: ValueId, value_type: ValueType) -> Self {
        Self { id, value_type }
    }

    #[must_use]
    pub fn id(self) -> ValueId {
        self.id
    }

    #[must_use]
    pub fn value_type(self) -> ValueType {
        self.value_type
    }
}

pub type ValueStack = Vec<ValueRef>;
