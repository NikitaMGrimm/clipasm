#![deny(rustdoc::broken_intra_doc_links)]

//! Embed the `ClipAsm` typed video compiler and renderer.
//!
//! `ClipAsm` separates authoring from execution. Parse a
//! [`source::SourcePackage`] through a frontend,
//! compile it into a pure [`compiler::CompiledProgram`], resolve media and
//! tools into a [`preflight::PreparedPlan`], then execute that plan with
//! [`render::render`].
//!
//! Compilation performs no media or external-tool I/O. [`preflight::preflight`]
//! is the first phase allowed to inspect assets, `FFmpeg`, or `FFprobe`.
//!
//! # Example
//!
//! ```no_run
//! use std::path::Path;
//!
//! let program = clipasm::frontend::yaml::parse_file(Path::new("program.yaml"))?;
//! let compiled = clipasm::compiler::compile(&program)?;
//! let prepared = clipasm::preflight::preflight(&compiled)?;
//! let report = clipasm::render::render(&prepared)?;
//! println!("{}", report.output.display());
//! # Ok::<(), clipasm::diagnostic::Diagnostic>(())
//! ```
//!
//! The canonical language, YAML frontend, programs, and stack behavior are
//! documented in the project guide rather than duplicated in this Rust API
//! reference.

pub mod compiler;
pub mod diagnostic;
pub(crate) mod external;
pub(crate) mod format;
pub mod frontend;
pub(crate) mod language;
pub mod model;
pub mod preflight;
pub(crate) mod program;
pub mod render;
pub(crate) mod semantic;
pub mod source;
