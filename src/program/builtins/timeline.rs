use std::num::NonZeroU64;

use crate::diagnostic::{Diagnostic, Result};
use crate::program::{Cardinality, ParameterType, ProgramDefinition, ProgramOutputs, ResolvedCall};
use crate::semantic::GraphBuilder;

use super::support::{direct, generic_descriptor, one_output, parameter};

pub(super) fn concat() -> ProgramDefinition {
    direct(
        generic_descriptor(
            "concat",
            2,
            "values",
            Cardinality::Variadic { min: 1 },
            vec![],
            true,
        ),
        lower_concat,
    )
}

pub(super) fn repeat() -> ProgramDefinition {
    direct(
        generic_descriptor(
            "repeat",
            3,
            "value",
            Cardinality::One,
            vec![parameter("count", ParameterType::Integer, true)],
            true,
        ),
        lower_repeat,
    )
}

pub(super) fn trim() -> ProgramDefinition {
    direct(
        generic_descriptor(
            "trim",
            2,
            "value",
            Cardinality::One,
            vec![parameter("range", ParameterType::TimeRange, true)],
            true,
        ),
        lower_trim,
    )
}

pub(super) fn drop_value() -> ProgramDefinition {
    direct(
        generic_descriptor("drop", 1, "value", Cardinality::One, vec![], false),
        lower_drop,
    )
}

fn lower_concat(call: &ResolvedCall, builder: &mut GraphBuilder<'_>) -> Result<ProgramOutputs> {
    one_output(builder.concat(call.variadic_input("values")?.to_vec()))
}

fn lower_repeat(call: &ResolvedCall, builder: &mut GraphBuilder<'_>) -> Result<ProgramOutputs> {
    let value = call.one_input("value")?;
    let (count, span) = call.integer_parameter("count")?;
    let count = u64::try_from(count)
        .ok()
        .and_then(NonZeroU64::new)
        .ok_or_else(|| {
            Diagnostic::new(
                "E_INVALID_REPEAT_COUNT",
                "`repeat.count` must be an integer greater than or equal to one",
                span.clone(),
            )
        })?;
    one_output(builder.repeat(value, count))
}

fn lower_trim(call: &ResolvedCall, builder: &mut GraphBuilder<'_>) -> Result<ProgramOutputs> {
    let value = call.one_input("value")?;
    let (range, span) = call.time_range_parameter("range")?;
    one_output(builder.at_span(span.clone()).trim(value, range))
}

#[allow(clippy::unnecessary_wraps)]
fn lower_drop(_call: &ResolvedCall, _builder: &mut GraphBuilder<'_>) -> Result<ProgramOutputs> {
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::semantic::SemanticNodeKind;

    fn compile_repeat(count: u64) -> crate::compiler::CompiledProgram {
        let workflow = crate::language::parse_str(
            Path::new("repeat.clipasm"),
            &format!(
                "clipasm 1\nconfig {{ video {{ fps = 10 }} }}\nimage(\"card.png\", 1s)\nrepeat({count})\n"
            ),
        )
        .expect("workflow");
        crate::compiler::compile(&workflow).expect("compile")
    }

    #[test]
    fn repeat_one_aliases_while_two_emits_one_compact_node() {
        let once = compile_repeat(1);
        assert_eq!(once.value_count(), 1);

        let twice = compile_repeat(2);
        assert_eq!(twice.value_count(), 2);
        assert!(matches!(
            twice.nodes()[1].kind(),
            SemanticNodeKind::Repeat { count, .. } if count.get() == 2
        ));
    }

    #[test]
    fn a_million_repeats_have_bounded_graph_and_json_size() {
        let compiled = compile_repeat(1_000_000);
        let json = compiled.canonical_json().expect("compiled JSON");

        assert_eq!(compiled.value_count(), 2);
        assert!(json.len() < 10_000, "compact plan was {} bytes", json.len());
    }

    #[test]
    fn timeline_versions_cover_native_duration_semantics() {
        assert_eq!(concat().descriptor.semantic_version, 2);
        assert_eq!(repeat().descriptor.semantic_version, 3);
        assert_eq!(trim().descriptor.semantic_version, 2);
    }
}
