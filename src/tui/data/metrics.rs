//! Metrics calculation for TAC programs.

use crate::ir::{OpCode, Operand, Program};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgramStats {
    pub instruction_count: usize,
    pub arithmetic_count: usize,
    pub temp_count: usize,
    pub code_size_bytes: usize,
}

impl ProgramStats {
    pub fn compute(program: &Program) -> Self {
        let instruction_count = program.instructions.len();
        let mut arithmetic_count = 0;
        let mut temps = HashSet::new();

        for instruction in &program.instructions {
            if matches!(
                instruction.op,
                OpCode::Add | OpCode::Sub | OpCode::Mul | OpCode::Div | OpCode::Mod
            ) {
                arithmetic_count += 1;
            }

            for operand in [&instruction.arg1, &instruction.arg2, &instruction.dest] {
                if let Operand::Temp(id) = operand {
                    temps.insert(*id);
                }
            }
        }

        let code_size_bytes = program.to_string().len();

        Self {
            instruction_count,
            arithmetic_count,
            temp_count: temps.len(),
            code_size_bytes,
        }
    }

    pub fn reduction_pct(before: usize, after: usize) -> f64 {
        if before == 0 {
            0.0
        } else {
            (before.saturating_sub(after) as f64 / before as f64) * 100.0
        }
    }
}
