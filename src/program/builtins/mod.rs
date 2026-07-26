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
        effects::zoom_in(),
        transitions::flash_cut(),
        transitions::crossfade(),
        body::join(),
        body::during(),
    ]
}
