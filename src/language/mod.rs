//! Native `ClipAsm` source language.

mod lexer;
mod loader;
mod lower;
mod parser;
mod sugar;
mod syntax;

pub use loader::parse_file;
pub use lower::parse_str;
#[cfg(test)]
pub(crate) use lower::parse_str_with_registry;
