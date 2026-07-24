mod body;
mod direct;

use super::ProgramDefinition;

pub(crate) fn builtin_programs() -> Vec<ProgramDefinition> {
    vec![
        direct::image(),
        direct::video_source(),
        direct::audio_source(),
        direct::extract_audio(),
        direct::set_audio(),
        direct::concat(),
        direct::repeat(),
        direct::trim(),
        direct::drop_value(),
        direct::zoom(),
        direct::wobble(),
        direct::flash(),
        body::join(),
        body::glue(),
        body::during(),
    ]
}
