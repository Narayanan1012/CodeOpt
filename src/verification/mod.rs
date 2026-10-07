//! Differential execution for original and optimized TAC programs.

use crate::{
    cfg::build,
    ir::Program,
    passes::optimize_with_worklist,
    vm::{ExecutionError, execute},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationResult {
    pub original_output: Vec<i32>,
    pub optimized_output: Vec<i32>,
}

impl VerificationResult {
    pub fn matches(&self) -> bool {
        self.original_output == self.optimized_output
    }
}

/// Runs both representations with the same inputs and compares printed output.
pub fn verify(program: &Program, inputs: &[i32]) -> Result<VerificationResult, String> {
    let original_output = execute(program, inputs)
        .map_err(|error| format_execution_error("original", error))?
        .output;

    let mut optimized = program.clone();
    let cfg =
        build(&mut optimized).map_err(|error| format!("cannot build optimized CFG: {error}"))?;
    optimize_with_worklist(&mut optimized, &cfg);
    let optimized_output = execute(&optimized, inputs)
        .map_err(|error| format_execution_error("optimized", error))?
        .output;

    Ok(VerificationResult {
        original_output,
        optimized_output,
    })
}

fn format_execution_error(version: &str, error: ExecutionError) -> String {
    format!("{version} execution failed: {error}")
}
