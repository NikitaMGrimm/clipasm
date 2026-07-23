use crate::model::ValueType;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Cardinality {
    One,
    Variadic { min: usize },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InputPort {
    pub name: &'static str,
    pub value_type: ValueType,
    pub cardinality: Cardinality,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProgramDescriptor {
    pub name: &'static str,
    pub version: u32,
    pub inputs: &'static [InputPort],
    pub output: ValueType,
    pub primary_argument: Option<&'static str>,
}

const VIDEO: ValueType = ValueType::Video;
const NO_INPUTS: &[InputPort] = &[];
const ONE_VIDEO: &[InputPort] = &[InputPort {
    name: "video",
    value_type: VIDEO,
    cardinality: Cardinality::One,
}];
const VIDEOS: &[InputPort] = &[InputPort {
    name: "videos",
    value_type: VIDEO,
    cardinality: Cardinality::Variadic { min: 1 },
}];

pub const IMAGE: ProgramDescriptor = ProgramDescriptor {
    name: "image",
    version: 1,
    inputs: NO_INPUTS,
    output: VIDEO,
    primary_argument: Some("path"),
};
pub const CLIP: ProgramDescriptor = ProgramDescriptor {
    name: "clip",
    version: 1,
    inputs: ONE_VIDEO,
    output: VIDEO,
    primary_argument: Some("video"),
};
pub const CONCAT: ProgramDescriptor = ProgramDescriptor {
    name: "concat",
    version: 1,
    inputs: VIDEOS,
    output: VIDEO,
    primary_argument: Some("videos"),
};
pub const REPEAT: ProgramDescriptor = ProgramDescriptor {
    name: "repeat",
    version: 1,
    inputs: ONE_VIDEO,
    output: VIDEO,
    primary_argument: Some("count"),
};

#[derive(Clone, Copy, Debug, Default)]
pub struct ProgramRegistry;

impl ProgramRegistry {
    #[must_use]
    pub fn get(self, name: &str) -> Option<&'static ProgramDescriptor> {
        match name {
            "image" => Some(&IMAGE),
            "clip" => Some(&CLIP),
            "concat" => Some(&CONCAT),
            "repeat" => Some(&REPEAT),
            _ => None,
        }
    }
}
