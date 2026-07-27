use std::fmt::Write as _;

use clipasm::diagnostic::{Diagnostic, Result};
use clipasm::reference::{
    BodyInitialValueRole, BodyOutputs, BuiltinCategory, BuiltinProgram, Cardinality,
    TimelineBehavior,
};
use clipasm::source::SourceSpan;

pub(super) fn print(name: Option<&str>) -> Result<()> {
    if let Some(name) = name {
        let program = clipasm::reference::builtin_program(name).ok_or_else(|| {
            let displayed_name = safe_display_text(name);
            Diagnostic::new(
                "E_UNKNOWN_BUILTIN_PROGRAM",
                format!("unknown built-in program `{displayed_name}`"),
                SourceSpan::file_start("<command-line>"),
            )
            .note("run `clipasm programs` to list all built-in programs")
        })?;
        print!("{}", detail(&program));
    } else {
        print!("{}", list());
    }
    Ok(())
}

fn safe_display_text(value: &str) -> String {
    let mut displayed = String::with_capacity(value.len());
    for character in value.chars() {
        if character.is_control() {
            write!(displayed, "\\u{{{:04X}}}", character as u32)
                .expect("writing to a String cannot fail");
        } else {
            displayed.push(character);
        }
    }
    displayed
}

fn list() -> String {
    let programs = clipasm::reference::builtin_programs();
    let mut output = String::from(
        "Built-in programs\n\
         These are built into ClipAsm; project and imported programs are not inspected.\n",
    );
    for category in BuiltinCategory::ALL {
        writeln!(output, "\n{}", category.label()).expect("writing to a String cannot fail");
        for program in programs
            .iter()
            .filter(|program| program.category() == category)
        {
            writeln!(output, "  {} — {}", program.name(), program.summary())
                .expect("writing to a String cannot fail");
        }
    }
    output.push_str("\nDetails: clipasm programs NAME\n");
    output
}

#[expect(
    clippy::too_many_lines,
    reason = "terminal reference output keeps one stable top-to-bottom section order"
)]
fn detail(program: &BuiltinProgram) -> String {
    let mut output = String::new();
    writeln!(output, "Built-in program: {}", program.name())
        .expect("writing to a String cannot fail");
    writeln!(output, "{}", program.summary()).expect("writing to a String cannot fail");
    writeln!(
        output,
        "\nCall shape (reference notation; not declaration syntax):\n  {}",
        program.call_shape()
    )
    .expect("writing to a String cannot fail");

    output.push_str("\nInputs:\n");
    if program.inputs().is_empty() {
        output.push_str("  none\n");
    } else {
        for input in program.inputs() {
            match input.cardinality() {
                Cardinality::Variadic { minimum } => {
                    writeln!(
                        output,
                        "  {}: {} (variadic; minimum {minimum})",
                        input.name(),
                        input.value_type()
                    )
                    .expect("writing to a String cannot fail");
                }
                _ => {
                    writeln!(output, "  {}: {}", input.name(), input.value_type())
                        .expect("writing to a String cannot fail");
                }
            }
        }
    }

    output.push_str("\nParameters:\n");
    if program.parameters().is_empty() {
        output.push_str("  none\n");
    } else {
        for parameter in program.parameters() {
            write!(
                output,
                "  {}: {}",
                parameter.name(),
                parameter.parameter_type()
            )
            .expect("writing to a String cannot fail");
            if parameter.is_required() {
                output.push_str(" (required)");
            } else if let Some(default) = parameter.default() {
                write!(output, " (optional; default: {default})")
                    .expect("writing to a String cannot fail");
            } else if let Some(behavior) = parameter.omission_behavior() {
                write!(output, " (optional; when omitted: {behavior})")
                    .expect("writing to a String cannot fail");
            } else {
                output.push_str(" (optional; no fixed default)");
            }
            output.push('\n');
        }
    }

    output.push_str("\nOutputs:\n");
    if program.outputs().is_empty() {
        output.push_str("  none\n");
    } else {
        for value_type in program.outputs() {
            writeln!(output, "  {value_type}").expect("writing to a String cannot fail");
        }
    }

    output.push_str("\nGeneric type:\n  ");
    if program.is_generic() {
        output.push_str(
            "one homogeneous Video or Audio type; use <Video> or <Audio> when ambiguous\n",
        );
    } else {
        output.push_str("not generic\n");
    }
    writeln!(output, "\nStack access:\n  {}", program.stack_access())
        .expect("writing to a String cannot fail");

    output.push_str("\nBody:\n");
    if let Some(body) = program.body() {
        output.push_str("  accepted\n  initial stack:\n");
        for initial in body.initial_values() {
            match initial.role() {
                BodyInitialValueRole::Input { input } => {
                    writeln!(
                        output,
                        "    {} from bound input `{input}`",
                        initial.value_type()
                    )
                    .expect("writing to a String cannot fail");
                }
                BodyInitialValueRole::SelectedRange { input, parameter } => {
                    writeln!(
                        output,
                        "    {} selected from `{input}` by `{parameter}`",
                        initial.value_type()
                    )
                    .expect("writing to a String cannot fail");
                }
                _ => unreachable!("all current body initial value roles are rendered"),
            }
        }
        output.push_str("  required body outputs:\n");
        match body.outputs() {
            BodyOutputs::Exactly(outputs) => {
                writeln!(
                    output,
                    "    exactly {}",
                    outputs
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
                .expect("writing to a String cannot fail");
            }
            BodyOutputs::Variadic {
                value_type,
                minimum,
            } => {
                writeln!(
                    output,
                    "    at least {minimum} homogeneous {value_type} value(s)"
                )
                .expect("writing to a String cannot fail");
            }
            _ => unreachable!("all current body output contracts are rendered"),
        }
    } else {
        output.push_str("  not accepted\n");
    }

    writeln!(output, "\nTimeline:\n  {}", timeline_description(program))
        .expect("writing to a String cannot fail");
    if !program.behavior_notes().is_empty() {
        output.push_str("\nBehavior:\n");
        for note in program.behavior_notes() {
            writeln!(output, "  - {note}").expect("writing to a String cannot fail");
        }
    }
    if !program.constraints().is_empty() {
        output.push_str("\nConstraints:\n");
        for constraint in program.constraints() {
            writeln!(output, "  - {constraint}").expect("writing to a String cannot fail");
        }
    }
    if !program.diagnostics().is_empty() {
        writeln!(
            output,
            "\nImportant diagnostics:\n  {}",
            program.diagnostics().join(", ")
        )
        .expect("writing to a String cannot fail");
    }
    output.push_str("\nExample:\n");
    for line in program.example().lines() {
        writeln!(output, "  {line}").expect("writing to a String cannot fail");
    }
    writeln!(output, "\nFull guide:\n  {}", program.documentation_url())
        .expect("writing to a String cannot fail");
    if !program.related_programs().is_empty() {
        writeln!(
            output,
            "\nRelated built-in programs:\n  {}",
            program.related_programs().join(", ")
        )
        .expect("writing to a String cannot fail");
    }
    output
}

fn timeline_description(program: &BuiltinProgram) -> String {
    match program.timeline_behavior() {
        TimelineBehavior::Fresh if program.outputs().is_empty() => {
            "removes its input without producing a timeline".to_owned()
        }
        TimelineBehavior::Fresh => "creates a fresh timeline".to_owned(),
        TimelineBehavior::Identity { input } => {
            format!("preserves the layout of `{input}`")
        }
        TimelineBehavior::Repeat { input } => {
            format!("repeats the layout of `{input}`; repeat(1) is an identity")
        }
        TimelineBehavior::Concat { input } => {
            format!("concatenates the layouts bound to `{input}`")
        }
        TimelineBehavior::BodyConcat { inputs } => {
            format!(
                "concatenates the body result initialized from {}",
                inputs
                    .iter()
                    .map(|input| format!("`{input}`"))
                    .collect::<Vec<_>>()
                    .join(" and ")
            )
        }
        TimelineBehavior::Crop { input } => {
            format!("crops and rebases the selected layout from `{input}`")
        }
        TimelineBehavior::Replace { base } => {
            format!("splices the body result into the selected range of `{base}`")
        }
        TimelineBehavior::FlashCut { before, after } => {
            format!("creates sequential regions from `{before}` and `{after}`")
        }
        TimelineBehavior::Crossfade { before, after } => {
            format!("creates overlapping regions from `{before}` and `{after}`")
        }
        _ => unreachable!("all current timeline behaviors are rendered"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_contains_every_built_in_name() {
        let output = list();
        for program in clipasm::reference::builtin_programs() {
            assert!(
                output.contains(&format!("  {} — ", program.name())),
                "missing {}",
                program.name()
            );
        }
    }
}
