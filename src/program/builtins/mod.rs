mod body;
mod direct;

use super::ProgramDefinition;

pub(crate) static BUILTIN_PROGRAMS: &[ProgramDefinition] = &[
    direct::IMAGE,
    direct::VIDEO_SOURCE,
    direct::CONCAT,
    direct::REPEAT,
    body::THEN,
    body::JOIN,
    body::TIMELINE,
    body::DURING,
];
