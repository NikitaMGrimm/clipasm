//! `RhythmCut`'s compiler and renderer.
//!
//! Internal program-extension machinery is intentionally not public.
//!
//! ```compile_fail
//! use rhythmcut::program::ProgramDefinition;
//! ```

pub mod cli;
pub mod compiler;
pub mod diagnostic;
pub mod model;
pub mod preflight;
pub(crate) mod program;
pub mod render;
pub mod syntax;
