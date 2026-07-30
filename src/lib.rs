#![deny(rustdoc::broken_intra_doc_links)]

//! Embed the `ClipAsm` typed Video and Audio compiler and renderer.
//!
//! `ClipAsm` separates authoring from execution. Parse a native `.clipasm`
//! [`source::SourcePackage`] through [`language`], then compile it into a pure
//! [`compiler::CompiledProgram`]. The dependency-light base library also exposes
//! virtual-asset browser preparation and recipe serialization.
//!
//! Compilation performs no media or external-tool I/O. The `native` feature is
//! the first layer allowed to inspect assets, `FFmpeg`, or `FFprobe`.
//!
//! # Cargo features
//!
//! - No features builds the language, compiler, model, reference catalog, and
//!   pure browser preparation/render-recipe adapters.
//! - `native` adds filesystem/tool preflight and native rendering.
//! - The default `cli` feature includes `native` and the `clipasm` executable.
//!
//! # Example
//!
//! ```
//! use std::path::Path;
//!
//! let program = clipasm::language::parse_str(
//!     Path::new("program.clipasm"),
//!     "clipasm 1\nimage(\"missing.png\", 1s)\n",
//! )?;
//! let compiled = clipasm::compiler::compile(&program)?;
//! assert_eq!(compiled.value_count(), 1);
//! # Ok::<(), clipasm::diagnostic::Diagnostic>(())
//! ```
//!
//! Native execution is available when the `native` feature is enabled:
//!
//! ```no_run
//! # #[cfg(feature = "native")]
//! # fn run() -> Result<(), clipasm::diagnostic::Diagnostic> {
//! use std::path::Path;
//!
//! let program = clipasm::language::parse_file(Path::new("program.clipasm"))?;
//! let compiled = clipasm::compiler::compile(&program)?;
//! let prepared = clipasm::preflight::preflight(&compiled)?;
//! let report = clipasm::render::render(&prepared)?;
//! println!("{}", report.output().display());
//! # Ok(())
//! # }
//! ```
//!
//! The native language, programs, and stack behavior are
//! documented in the project guide rather than duplicated in this Rust API
//! reference.

pub(crate) mod catalog;
pub mod compiler;
pub(crate) mod contracts;
pub mod diagnostic;
pub(crate) mod external;
pub(crate) mod format;
pub(crate) mod identity;
pub mod language;
#[cfg(feature = "native")]
pub(crate) mod media_tool;
pub mod model;
pub mod preflight;
#[cfg(feature = "native")]
mod process;
pub(crate) mod program;
pub mod reference;
pub mod render;
pub(crate) mod semantic;
pub mod source;
