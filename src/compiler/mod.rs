//! Pure source-program compilation and semantic graph inspection.
//!
//! Compilation binds typed program calls, evaluates scoped stack frames, resolves
//! references, infers source-independent video domains, and computes semantic
//! identity. It never reads media files or invokes external tools.

mod bind;
mod evaluate;
pub(crate) mod fingerprint;
mod resolve;
mod stack;
pub(crate) mod traversal;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::diagnostic::{Diagnostic, Result, SourceSpan, Spanned};
use crate::model::{ValueRef, VideoDomain, VideoSpec};
use crate::program::ProgramRegistry;
use crate::semantic::CompiledNode;
use crate::syntax::SourceProgram;

pub use crate::semantic::SourceOrigin;

const COMPILED_FORMAT_VERSION: u32 = 6;

#[derive(Clone, Debug, Serialize)]
/// A pure compiled program whose media-dependent facts may remain deferred.
///
/// Use [`result_domain`](Self::result_domain) to inspect a domain known from
/// authored data, or pass the program to [`crate::preflight::preflight`] to
/// resolve assets and exact renderer primitives.
pub struct CompiledProgram {
    format_version: u32,
    program_version: u64,
    engine_version: String,
    structure_hash: String,
    video: VideoSpec,
    nodes: Vec<CompiledNode>,
    result: ValueRef,
    named_values: BTreeMap<String, ValueRef>,
    explain: Vec<ExplainEntry>,
    output: Option<Spanned<PathBuf>>,
    #[serde(skip)]
    source_path: PathBuf,
}

impl CompiledProgram {
    /// Return the number of semantic values in the compiled graph.
    #[must_use]
    pub fn value_count(&self) -> usize {
        self.nodes.len()
    }

    /// Serialize the pure compiled structure as stable, pretty JSON.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic if serialization fails.
    pub fn canonical_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(|error| {
            Diagnostic::new(
                "E_PLAN_SERIALIZATION",
                format!("could not serialize compiled program: {error}"),
                SourceSpan::file_start("<compiled-program>"),
            )
        })
    }

    #[must_use]
    /// Return the stable hash of compiled language and graph semantics.
    ///
    /// Source locations, comments, project location, and the Cargo package
    /// version do not contribute to this identity.
    pub fn structure_hash(&self) -> &str {
        &self.structure_hash
    }

    #[must_use]
    /// Return the project-wide dimensions and canonical frame rate.
    pub fn video(&self) -> &VideoSpec {
        &self.video
    }

    #[must_use]
    /// Return the result Video domain when it is knowable without reading media.
    ///
    /// Video-file source durations remain deferred until preflight.
    ///
    /// ```
    /// use std::path::Path;
    ///
    /// let program = clipasm::syntax::parse_str(
    ///     Path::new("program.yaml"),
    ///     "- program:\n    version: 1\n\n- image: {path: missing.png, duration: 1s}\n",
    /// )?;
    /// let compiled = clipasm::compiler::compile(&program)?;
    ///
    /// assert_eq!(compiled.result_domain().expect("authored domain").frames.0, 30);
    /// # Ok::<(), clipasm::diagnostic::Diagnostic>(())
    /// ```
    pub fn result_domain(&self) -> Option<&VideoDomain> {
        self.nodes[self.result.id().get() as usize].domain()
    }

    #[must_use]
    /// Return source-oriented entries for user-visible program constructs.
    pub fn explain(&self) -> &[ExplainEntry] {
        &self.explain
    }

    pub(crate) fn nodes(&self) -> &[CompiledNode] {
        &self.nodes
    }

    pub(crate) const fn result(&self) -> ValueRef {
        self.result
    }

    pub(crate) fn named_values(&self) -> &BTreeMap<String, ValueRef> {
        &self.named_values
    }

    pub(crate) fn output(&self) -> Option<&Spanned<PathBuf>> {
        self.output.as_ref()
    }

    pub(crate) fn source_path(&self) -> &Path {
        &self.source_path
    }
}

#[derive(Clone, Debug, Serialize)]
/// A user-visible source construct and the semantic value it produced.
///
/// Explain entries preserve authoring constructs even when their lowering
/// becomes one or more semantic operations.
pub struct ExplainEntry {
    construct: String,
    output: ValueRef,
    id: Option<String>,
    span: SourceSpan,
}

impl ExplainEntry {
    #[must_use]
    /// Return the registered program name or reference/declaration label.
    pub fn construct(&self) -> &str {
        &self.construct
    }

    #[must_use]
    /// Return the semantic value produced by this construct.
    pub const fn output(&self) -> ValueRef {
        self.output
    }

    #[must_use]
    /// Return the optional user-authored name attached with `id` or declared by a named clip.
    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    #[must_use]
    /// Return the source location of this authored construct.
    pub const fn span(&self) -> &SourceSpan {
        &self.span
    }
}

/// Parse and purely compile a source-program file without reading media assets or
/// invoking external tools.
///
/// # Errors
///
/// Returns a source-located syntax or compilation diagnostic.
pub fn compile_file(path: &Path) -> Result<CompiledProgram> {
    let program = crate::syntax::parse_file(path)?;
    compile(&program)
}

/// Purely compile an already parsed source program.
///
/// Compilation can validate a video source even when the asset is unavailable:
///
/// ```
/// use std::path::Path;
///
/// let program = clipasm::syntax::parse_str(
///     Path::new("program.yaml"),
///     "- program:\n    version: 1\n\n- video: unavailable.mp4\n",
/// )?;
/// let compiled = clipasm::compiler::compile(&program)?;
///
/// assert!(compiled.result_domain().is_none());
/// # Ok::<(), clipasm::diagnostic::Diagnostic>(())
/// ```
///
/// # Errors
///
/// Returns a diagnostic for invalid programs, stack behavior, references,
/// types, cycles, or frame domains.
pub fn compile(program: &SourceProgram) -> Result<CompiledProgram> {
    compile_with_registry(program, ProgramRegistry::default())
}

pub(crate) fn compile_with_registry(
    program: &SourceProgram,
    registry: ProgramRegistry,
) -> Result<CompiledProgram> {
    let video = resolve_video_spec(program)?;
    let evaluation = evaluate::evaluate(program, &video, registry)?;
    resolve::finalize(program, video, evaluation, COMPILED_FORMAT_VERSION)
}

fn resolve_video_spec(program: &SourceProgram) -> Result<VideoSpec> {
    let mut spec = VideoSpec::default();
    if let Some(width) = &program.video().width {
        spec.width = width.value;
    }
    if let Some(height) = &program.video().height {
        spec.height = height.value;
    }
    if let Some(fps) = &program.video().fps {
        spec.fps = crate::model::FrameRate::parse(&fps.value, &fps.span)?;
    }
    Ok(spec)
}
