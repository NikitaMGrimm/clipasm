//! Restricted YAML frontend for authored `ClipAsm` source programs.

mod builtins;
mod language;
mod loader;
mod lower;
mod raw;

pub use loader::parse_file;
pub use lower::parse_str;

#[cfg(test)]
pub(crate) use language::Language;
#[cfg(test)]
pub(crate) use lower::parse_str_with_language;
