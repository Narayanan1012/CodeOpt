//! Audit records for optimization rewrites.

use crate::ir::Instruction;
use crate::passes::OptimizationKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transformation {
    pub pass: OptimizationKind,
    pub instruction_id: usize,
    pub before: Instruction,
    pub after: Instruction,
}
