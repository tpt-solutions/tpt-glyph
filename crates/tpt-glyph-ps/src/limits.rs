// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-ps / limits
//
// Resource limits for untrusted PostScript execution (Phase 10). These bounds
// make the interpreter fail-closed: a malicious or pathological program can tie
// up memory, stack depth, or output size, but cannot run unbounded. The limits
// are conservative defaults; callers may tighten or loosen them per document.

/// Bounding envelope for a single interpreter run.
#[derive(Debug, Clone, Copy)]
pub struct ResourceLimits {
    /// Maximum number of objects on the operand stack. `0` = unbounded.
    pub max_operand_stack: usize,
    /// Maximum depth of the execution stack (procedure/loop nesting). `0` = unbounded.
    pub max_exec_depth: usize,
    /// Maximum number of draw commands emitted into the render tree. `0` = unbounded.
    pub max_draw_commands: usize,
    /// Maximum total objects executed (instruction budget). `0` = unbounded.
    pub max_steps: u64,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_operand_stack: 1_000,
            max_exec_depth: 1_000,
            max_draw_commands: 1_000_000,
            max_steps: 50_000_000,
        }
    }
}

impl ResourceLimits {
    /// Fail-closed limits suitable for fully untrusted input.
    pub fn strict() -> Self {
        Self {
            max_operand_stack: 256,
            max_exec_depth: 256,
            max_draw_commands: 100_000,
            max_steps: 5_000_000,
        }
    }

    /// Unbounded limits (trust the input; used by tests/benchmarks).
    pub fn unbounded() -> Self {
        Self {
            max_operand_stack: 0,
            max_exec_depth: 0,
            max_draw_commands: 0,
            max_steps: 0,
        }
    }
}
