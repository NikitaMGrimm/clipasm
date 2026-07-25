//! Native `ClipAsm` source language.

mod lexer;
mod loader;
mod lower;
mod parser;
mod sugar;
mod syntax;

pub use loader::parse_file;
pub use lower::parse_str;
