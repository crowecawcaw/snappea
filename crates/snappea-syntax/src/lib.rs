//! Front end for the P modeling language: lexer, parser and AST.
//! No I/O: callers load sources into a [`SourceMap`].

pub mod ast;
pub mod lexer;
mod parser;
mod span;
mod suggest;
pub mod token;

pub use parser::{parse, ParseError, MAX_NESTING};
pub use span::{FileId, LineCol, Location, SourceFile, SourceMap, Span};
pub use suggest::{closest, edit_distance};
