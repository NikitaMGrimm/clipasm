//! Restricted YAML parsing and canonical source-program normalization.
//!
//! Parsing validates YAML shape, source-program structure, registered surface
//! forms, references, and source spans without opening media files. The
//! normalized representation remains opaque; embedding applications pass
//! [`SourceProgram`] directly to [`crate::compiler::compile`].

mod ast;
mod normalize;
mod raw;

pub use ast::SourceProgram;
pub use normalize::{parse_file, parse_str};

pub(crate) use ast::{
    Argument, InputExpression, Invocation, Item, ItemKind, OutputBindings, ParameterArgument,
    ProgramBody, Reference,
};
#[cfg(test)]
pub(crate) use normalize::parse_str_with_language;
