mod audio;
mod body;
mod effects;
mod sources;
mod support;
mod timeline;
mod transitions;

use super::ProgramDefinition;

pub(crate) fn builtin_programs() -> Vec<ProgramDefinition> {
    vec![
        sources::image(),
        sources::video(),
        sources::audio(),
        audio::extract_audio(),
        audio::set_audio(),
        timeline::concat(),
        timeline::repeat(),
        timeline::trim(),
        timeline::drop_value(),
        effects::zoom(),
        effects::wobble(),
        transitions::flash(),
        transitions::crossfade(),
        body::join(),
        body::glue(),
        body::during(),
    ]
}
