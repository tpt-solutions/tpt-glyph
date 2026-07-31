// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-ps / stacks
//
// Operand stack, graphics-state (gsave/grestore) stack, and execution stack.
// The interpreter is otherwise stateless apart from the dictionary; the
// immutable `GraphicsState` lives on its own explicit stack so callers can
// snapshot it for concurrent rendering.

use crate::error::{PsError, Result};
use crate::parser::PsObject;
use std::collections::HashMap;

/// The operand stack. PostScript pushes operands here and operators consume
/// from the top. We store `PsObject` values.
#[derive(Debug, Clone, Default)]
pub struct OperandStack {
    items: Vec<PsObject>,
}

impl OperandStack {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, v: PsObject) {
        self.items.push(v);
    }

    pub fn pop(&mut self) -> Result<PsObject> {
        self.items.pop().ok_or(PsError::OperandStackUnderflow)
    }

    /// Pop the top two elements as a pair (a, b) where b is the top.
    pub fn pop2(&mut self) -> Result<(PsObject, PsObject)> {
        let b = self.pop()?;
        let a = self.pop()?;
        Ok((a, b))
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn top(&self) -> Result<&PsObject> {
        self.items.last().ok_or(PsError::OperandStackUnderflow)
    }

    pub fn as_slice(&self) -> &[PsObject] {
        &self.items
    }
}

/// A user/dictionary name space. `def` installs names; lookups resolve to the
/// stored object. Simple single-dictionary model sufficient for our path.
#[derive(Debug, Clone, Default)]
pub struct Dictionary {
    entries: HashMap<String, PsObject>,
}

impl Dictionary {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn def(&mut self, name: &str, value: PsObject) {
        self.entries.insert(name.to_string(), value);
    }

    pub fn lookup(&self, name: &str) -> Option<&PsObject> {
        self.entries.get(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }
}

/// Execution stack: holds objects pending execution (used to implement
/// procedure execution and loops). Objects are appended to the front of the
/// remaining program; we model this as a stack of programs.
#[derive(Debug, Clone, Default)]
pub struct ExecStack {
    frames: Vec<Vec<PsObject>>,
}

impl ExecStack {
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a continuation program (e.g. a procedure body) to be executed next.
    pub fn push_program(&mut self, prog: Vec<PsObject>) {
        if !prog.is_empty() {
            self.frames.push(prog);
        }
    }

    /// Pop the next object to execute, drawing from the top frame. Objects
    /// within a frame execute left-to-right: we pop from the front (index 0).
    /// Returns `None` when the stack is empty.
    pub fn pop_next(&mut self) -> Option<PsObject> {
        while let Some(frame) = self.frames.last_mut() {
            if frame.is_empty() {
                self.frames.pop();
                continue;
            }
            return Some(frame.remove(0));
        }
        None
    }

    /// Number of pending execution frames (a proxy for procedure/loop nesting
    /// depth, used by resource-limit enforcement in Phase 10).
    pub fn depth(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.iter().all(|f| f.is_empty()) && self.frames.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operand_stack_order() {
        let mut s = OperandStack::new();
        s.push(PsObject::Int(1));
        s.push(PsObject::Int(2));
        let (a, b) = s.pop2().unwrap();
        assert_eq!(a, PsObject::Int(1));
        assert_eq!(b, PsObject::Int(2));
    }

    #[test]
    fn dictionary_def_lookup() {
        let mut d = Dictionary::new();
        d.def("x", PsObject::Int(7));
        assert_eq!(d.lookup("x"), Some(&PsObject::Int(7)));
    }

    #[test]
    fn exec_stack_runs_procedures() {
        let mut e = ExecStack::new();
        // Push program so objects execute right-to-left pop order: push [3,2,1]
        // so pop_next returns 1,2,3 (LIFO within a frame).
        e.push_program(vec![PsObject::Int(1), PsObject::Int(2), PsObject::Int(3)]);
        assert_eq!(e.pop_next(), Some(PsObject::Int(1)));
        assert_eq!(e.pop_next(), Some(PsObject::Int(2)));
        assert_eq!(e.pop_next(), Some(PsObject::Int(3)));
        assert_eq!(e.pop_next(), None);
    }
}
