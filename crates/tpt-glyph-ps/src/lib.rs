// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-ps
//
// PostScript interpreter: tokenizer, parser, operand/execution/graphics-state
// stacks, and a knowledge-graph-driven operator dispatch table. Operator
// effects are integrated with the tpt-glyph-core rendering pipeline (immutable
// `GraphicsState` + `RenderTree`).

pub mod error;
pub mod interpreter;
pub mod lexer;
pub mod limits;
pub mod parser;
pub mod stacks;

pub use error::{PsError, Result};
pub use interpreter::{build_dispatch, Interpreter, OpEntry};
pub use lexer::tokenize;
pub use limits::ResourceLimits;
pub use parser::{parse, Program, PsObject};
pub use stacks::{Dictionary, ExecStack, OperandStack};
