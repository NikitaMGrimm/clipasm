use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::diagnostic::{Diagnostic, Result};
use crate::source::{SourceSpan, Spanned};

#[derive(Clone, Debug)]
pub(super) struct VideoInputBinding {
    pub(super) path: PathBuf,
    pub(super) span: SourceSpan,
}

#[derive(Clone, Debug)]
pub(super) struct ParameterBinding {
    pub(super) value: String,
    pub(super) span: SourceSpan,
}

/// External values supplied when compiling a root source program.
///
/// Video inputs and scalar parameters are matched by name against the root
/// program's declared interface. Relative file paths resolve from the
/// [`SourceSpan`] supplied with each binding, allowing callers such as the CLI
/// to use their own working directory without rewriting authored YAML.
#[derive(Clone, Debug, Default)]
pub struct EntrypointBindings {
    pub(super) video_inputs: BTreeMap<String, VideoInputBinding>,
    pub(super) parameters: BTreeMap<String, ParameterBinding>,
    pub(super) output: Option<Spanned<PathBuf>>,
}

impl EntrypointBindings {
    /// Construct an empty set of root-program bindings.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            video_inputs: BTreeMap::new(),
            parameters: BTreeMap::new(),
            output: None,
        }
    }

    /// Bind one declared root `Video` input to a video-file path.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic when the same input name was already supplied.
    pub fn bind_video_input(
        &mut self,
        name: impl Into<String>,
        path: impl Into<PathBuf>,
        span: SourceSpan,
    ) -> Result<()> {
        let name = name.into();
        if let Some(previous) = self.video_inputs.get(&name) {
            return Err(duplicate_binding("input", &name, span, &previous.span));
        }
        self.video_inputs.insert(
            name,
            VideoInputBinding {
                path: path.into(),
                span,
            },
        );
        Ok(())
    }

    /// Bind one declared root scalar parameter from its authored text form.
    ///
    /// The value is converted according to the root program's declared
    /// parameter type during compilation.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic when the same parameter name was already supplied.
    pub fn bind_parameter(
        &mut self,
        name: impl Into<String>,
        value: impl Into<String>,
        span: SourceSpan,
    ) -> Result<()> {
        let name = name.into();
        if let Some(previous) = self.parameters.get(&name) {
            return Err(duplicate_binding("parameter", &name, span, &previous.span));
        }
        self.parameters.insert(
            name,
            ParameterBinding {
                value: value.into(),
                span,
            },
        );
        Ok(())
    }

    /// Override the root program's publication path for this compilation.
    pub fn set_output(&mut self, path: impl Into<PathBuf>, span: SourceSpan) {
        self.output = Some(Spanned::new(path.into(), span));
    }
}

fn duplicate_binding(
    role: &str,
    name: &str,
    span: SourceSpan,
    previous: &SourceSpan,
) -> Diagnostic {
    Diagnostic::new(
        "E_DUPLICATE_ARGUMENT",
        format!("root {role} `{name}` was supplied more than once"),
        span,
    )
    .note(format!(
        "the previous binding was supplied at {}:{}:{}",
        previous.file().display(),
        previous.line,
        previous.column
    ))
}
