//! Execution boundary for differential TAC verification.

use std::collections::HashMap;
use std::fmt;

use crate::ir::{OpCode, Operand, Program};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExecutionResult {
    pub output: Vec<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionError {
    pub instruction_id: Option<usize>,
    pub message: String,
}

impl fmt::Display for ExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.instruction_id {
            Some(id) => write!(formatter, "instruction {id}: {}", self.message),
            None => write!(formatter, "{}", self.message),
        }
    }
}

impl std::error::Error for ExecutionError {}

/// Executes a TAC program with a defensive step cap.
///
/// Arithmetic follows signed two's-complement 32-bit wrapping behaviour.
/// Division and remainder by zero return an execution error rather than
/// crashing the process.
pub fn execute(program: &Program, inputs: &[i32]) -> Result<ExecutionResult, ExecutionError> {
    let limit = program.instructions.len().saturating_mul(1_000).max(1_000);
    execute_with_step_limit(program, inputs, limit)
}

pub fn execute_with_step_limit(
    program: &Program,
    inputs: &[i32],
    step_limit: usize,
) -> Result<ExecutionResult, ExecutionError> {
    let labels = collect_labels(program)?;
    let mut memory = HashMap::new();
    let mut input_index = 0;
    let mut output = Vec::new();
    let mut program_counter = 0;
    let mut steps = 0;

    while program_counter < program.instructions.len() {
        if steps >= step_limit {
            return Err(error(
                None,
                format!("step limit of {step_limit} reached; possible infinite loop"),
            ));
        }
        steps += 1;

        let instruction = &program.instructions[program_counter];
        match instruction.op {
            OpCode::Add | OpCode::Sub | OpCode::Mul | OpCode::Div | OpCode::Mod => {
                let left = value(&instruction.arg1, &memory, instruction.id)?;
                let right = value(&instruction.arg2, &memory, instruction.id)?;
                let result = match instruction.op {
                    OpCode::Add => left.wrapping_add(right),
                    OpCode::Sub => left.wrapping_sub(right),
                    OpCode::Mul => left.wrapping_mul(right),
                    OpCode::Div => {
                        if right == 0 {
                            return Err(error(Some(instruction.id), "division by zero"));
                        }
                        left.wrapping_div(right)
                    }
                    OpCode::Mod => {
                        if right == 0 {
                            return Err(error(Some(instruction.id), "remainder by zero"));
                        }
                        left.wrapping_rem(right)
                    }
                    _ => unreachable!("only binary operations reach this branch"),
                };
                write_destination(&instruction.dest, result, &mut memory, instruction.id)?;
                program_counter += 1;
            }
            OpCode::Assign => {
                let result = value(&instruction.arg1, &memory, instruction.id)?;
                write_destination(&instruction.dest, result, &mut memory, instruction.id)?;
                program_counter += 1;
            }
            OpCode::IfFalse => {
                let condition = value(&instruction.arg1, &memory, instruction.id)?;
                if condition == 0 {
                    program_counter = jump_target(&instruction.dest, &labels, instruction.id)?;
                } else {
                    program_counter += 1;
                }
            }
            OpCode::Goto => {
                program_counter = jump_target(&instruction.dest, &labels, instruction.id)?;
            }
            OpCode::Label | OpCode::Nop => program_counter += 1,
            OpCode::Read => {
                let input = inputs.get(input_index).copied().ok_or_else(|| {
                    error(
                        Some(instruction.id),
                        "program requested more input values than were provided",
                    )
                })?;
                input_index += 1;
                write_destination(&instruction.dest, input, &mut memory, instruction.id)?;
                program_counter += 1;
            }
            OpCode::Print => {
                output.push(value(&instruction.arg1, &memory, instruction.id)?);
                program_counter += 1;
            }
        }
    }

    Ok(ExecutionResult { output })
}

fn collect_labels(program: &Program) -> Result<HashMap<String, usize>, ExecutionError> {
    let mut labels = HashMap::new();
    for (offset, instruction) in program.instructions.iter().enumerate() {
        if instruction.op != OpCode::Label {
            continue;
        }
        let Operand::Label(label) = &instruction.dest else {
            return Err(error(
                Some(instruction.id),
                "label instruction has no label name",
            ));
        };
        if labels.insert(label.clone(), offset).is_some() {
            return Err(error(
                Some(instruction.id),
                format!("label `{label}` is defined more than once"),
            ));
        }
    }
    Ok(labels)
}

fn value(
    operand: &Operand,
    memory: &HashMap<String, i32>,
    instruction_id: usize,
) -> Result<i32, ExecutionError> {
    match operand {
        Operand::Const(value) => Ok(*value),
        Operand::Var(name) => memory.get(name).copied().ok_or_else(|| {
            error(
                Some(instruction_id),
                format!("variable `{name}` is undefined"),
            )
        }),
        Operand::Temp(index) => memory.get(&format!("t{index}")).copied().ok_or_else(|| {
            error(
                Some(instruction_id),
                format!("temporary `t{index}` is undefined"),
            )
        }),
        Operand::Label(label) => Err(error(
            Some(instruction_id),
            format!("label `{label}` cannot be used as a value"),
        )),
        Operand::None => Err(error(
            Some(instruction_id),
            "missing value operand for this instruction",
        )),
    }
}

fn write_destination(
    operand: &Operand,
    value: i32,
    memory: &mut HashMap<String, i32>,
    instruction_id: usize,
) -> Result<(), ExecutionError> {
    match operand {
        Operand::Var(name) => {
            memory.insert(name.clone(), value);
            Ok(())
        }
        Operand::Temp(index) => {
            memory.insert(format!("t{index}"), value);
            Ok(())
        }
        _ => Err(error(
            Some(instruction_id),
            "instruction destination must be a variable or temporary",
        )),
    }
}

fn jump_target(
    operand: &Operand,
    labels: &HashMap<String, usize>,
    instruction_id: usize,
) -> Result<usize, ExecutionError> {
    let Operand::Label(label) = operand else {
        return Err(error(
            Some(instruction_id),
            "jump instruction has no label destination",
        ));
    };
    labels.get(label).copied().ok_or_else(|| {
        error(
            Some(instruction_id),
            format!("jump target `{label}` is not defined"),
        )
    })
}

fn error(instruction_id: Option<usize>, message: impl Into<String>) -> ExecutionError {
    ExecutionError {
        instruction_id,
        message: message.into(),
    }
}
