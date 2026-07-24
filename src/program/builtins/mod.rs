mod body;
mod direct;

use super::ProgramDefinition;

pub(crate) static BUILTIN_PROGRAMS: &[ProgramDefinition] = &[
    direct::IMAGE,
    direct::VIDEO_SOURCE,
    direct::CONCAT,
    direct::REPEAT,
    direct::TRIM,
    direct::ZOOM,
    direct::WOBBLE,
    direct::FLASH,
    body::JOIN,
    body::GLUE,
    body::DURING,
];
