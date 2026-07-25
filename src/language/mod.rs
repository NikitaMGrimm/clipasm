//! Native `ClipAsm` source language.

#![allow(dead_code)] // File package loading and CLI integration follow this migration stage.

mod lexer;
mod lower;
mod parser;
mod sugar;
mod syntax;
