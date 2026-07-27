use std::fmt::Write as _;

use clipasm::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use clipasm::reference::DiagnosticReference;
use clipasm::source::SourceSpan;

const DIAGNOSTIC_INDEX_URL: &str = "https://nikitamgrimm.github.io/clipasm/reference/diagnostics/";
const MAX_DISPLAYED_CODE_CHARACTERS: usize = 120;

pub(super) fn print(code: &str) -> Result<()> {
    let reference = clipasm::reference::diagnostic(code).ok_or_else(|| unknown_code(code))?;
    print!("{}", detail(*reference));
    Ok(())
}

fn detail(reference: DiagnosticReference) -> String {
    let mut output = String::new();
    writeln!(output, "{}: {}", reference.code(), reference.title())
        .expect("writing to a String cannot fail");
    writeln!(output, "\nCategory: {}", reference.category())
        .expect("writing to a String cannot fail");
    writeln!(output, "\n{}", reference.summary()).expect("writing to a String cannot fail");

    write_list(&mut output, "Common causes:", reference.common_causes());
    write_list(&mut output, "Try:", reference.recommended_actions());

    writeln!(
        output,
        "\nRetry:\n  {}",
        reference.retry_guidance().explanation()
    )
    .expect("writing to a String cannot fail");
    writeln!(output, "\nReference:\n  {}", reference.documentation_url())
        .expect("writing to a String cannot fail");
    output
}

fn write_list(output: &mut String, heading: &str, entries: &[&str]) {
    if entries.is_empty() {
        return;
    }
    write!(output, "\n{heading}\n").expect("writing to a String cannot fail");
    for entry in entries {
        writeln!(output, "  - {entry}").expect("writing to a String cannot fail");
    }
}

fn unknown_code(code: &str) -> Diagnostic {
    let displayed_code = safe_display_code(code);
    Diagnostic::builtin(
        BuiltinDiagnostic::UnknownDiagnosticCode,
        format!("unknown ClipAsm diagnostic code `{displayed_code}`"),
        SourceSpan::file_start("<command-line>"),
    )
    .note(
        "check the code's spelling and run `clipasm explain E_UNKNOWN_DIAGNOSTIC_CODE` for details",
    )
    .note(format!("diagnostic index: {DIAGNOSTIC_INDEX_URL}"))
}

fn safe_display_code(code: &str) -> String {
    let mut displayed = String::with_capacity(code.len().min(MAX_DISPLAYED_CODE_CHARACTERS));
    let mut characters = code.chars();
    for character in characters.by_ref().take(MAX_DISPLAYED_CODE_CHARACTERS) {
        if character.is_ascii_graphic() || character == ' ' {
            displayed.push(character);
        } else {
            write!(displayed, "\\u{{{:X}}}", character as u32)
                .expect("writing to a String cannot fail");
        }
    }
    if characters.next().is_some() {
        displayed.push_str("...");
    }
    displayed
}
