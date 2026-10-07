//! Basic-block and control-flow-graph model.

use std::collections::{BTreeSet, HashMap};
use std::fmt;

use crate::ir::{OpCode, Operand, Program};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BasicBlock {
    pub id: usize,
    pub label: Option<String>,
    pub instruction_ids: Vec<usize>,
    pub predecessors: Vec<usize>,
    pub successors: Vec<usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ControlFlowGraph {
    pub blocks: Vec<BasicBlock>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CfgError {
    pub instruction_id: usize,
    pub message: String,
}

impl fmt::Display for CfgError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "instruction {}: {}",
            self.instruction_id, self.message
        )
    }
}

impl std::error::Error for CfgError {}

/// Partitions a program into basic blocks and creates its intraprocedural CFG.
///
/// Leaders are the first instruction, label targets, and instructions directly
/// following conditional or unconditional jumps. The function assigns each
/// instruction its containing block ID.
pub fn build(program: &mut Program) -> Result<ControlFlowGraph, CfgError> {
    if program.instructions.is_empty() {
        return Ok(ControlFlowGraph::default());
    }

    let label_offsets = collect_labels(program)?;
    validate_jump_targets(program, &label_offsets)?;

    let mut leaders = BTreeSet::from([0]);
    for (offset, instruction) in program.instructions.iter().enumerate() {
        if instruction.op == OpCode::Label {
            leaders.insert(offset);
        }
        if matches!(instruction.op, OpCode::Goto | OpCode::IfFalse)
            && offset + 1 < program.instructions.len()
        {
            leaders.insert(offset + 1);
        }
    }

    let leader_offsets = leaders.into_iter().collect::<Vec<_>>();
    let mut offset_to_block = vec![0; program.instructions.len()];
    let mut blocks = Vec::with_capacity(leader_offsets.len());

    for (block_id, start) in leader_offsets.iter().copied().enumerate() {
        let end = leader_offsets
            .get(block_id + 1)
            .copied()
            .unwrap_or(program.instructions.len());
        let instruction_ids = (start..end)
            .map(|offset| {
                offset_to_block[offset] = block_id;
                program.instructions[offset].block_id = Some(block_id);
                program.instructions[offset].id
            })
            .collect();
        let label = match &program.instructions[start].dest {
            Operand::Label(label) if program.instructions[start].op == OpCode::Label => {
                Some(label.clone())
            }
            _ => None,
        };

        blocks.push(BasicBlock {
            id: block_id,
            label,
            instruction_ids,
            predecessors: Vec::new(),
            successors: Vec::new(),
        });
    }

    let label_blocks = label_offsets
        .into_iter()
        .map(|(label, offset)| (label, offset_to_block[offset]))
        .collect::<HashMap<_, _>>();

    for block_id in 0..blocks.len() {
        let last_instruction_id = *blocks[block_id]
            .instruction_ids
            .last()
            .expect("every CFG block contains an instruction");
        let last_offset = program
            .instructions
            .iter()
            .position(|instruction| instruction.id == last_instruction_id)
            .expect("basic blocks only contain program instruction IDs");
        let instruction = &program.instructions[last_offset];
        let fallthrough = (block_id + 1 < blocks.len()).then_some(block_id + 1);

        let successors = match instruction.op {
            OpCode::Goto => vec![jump_block(instruction, &label_blocks)?],
            OpCode::IfFalse => {
                let mut targets = vec![jump_block(instruction, &label_blocks)?];
                if let Some(next) = fallthrough {
                    targets.push(next);
                }
                targets
            }
            _ => fallthrough.into_iter().collect(),
        };

        for successor in successors {
            if !blocks[block_id].successors.contains(&successor) {
                blocks[block_id].successors.push(successor);
                blocks[successor].predecessors.push(block_id);
            }
        }
    }

    Ok(ControlFlowGraph { blocks })
}

fn collect_labels(program: &Program) -> Result<HashMap<String, usize>, CfgError> {
    let mut labels = HashMap::new();
    for (offset, instruction) in program.instructions.iter().enumerate() {
        if instruction.op != OpCode::Label {
            continue;
        }
        let Operand::Label(label) = &instruction.dest else {
            return Err(cfg_error(
                instruction.id,
                "label instruction has no label destination",
            ));
        };
        if labels.insert(label.clone(), offset).is_some() {
            return Err(cfg_error(
                instruction.id,
                format!("label `{label}` is defined more than once"),
            ));
        }
    }
    Ok(labels)
}

fn validate_jump_targets(
    program: &Program,
    labels: &HashMap<String, usize>,
) -> Result<(), CfgError> {
    for instruction in &program.instructions {
        if !matches!(instruction.op, OpCode::Goto | OpCode::IfFalse) {
            continue;
        }
        let Operand::Label(label) = &instruction.dest else {
            return Err(cfg_error(
                instruction.id,
                "jump instruction has no label destination",
            ));
        };
        if !labels.contains_key(label) {
            return Err(cfg_error(
                instruction.id,
                format!("jump target `{label}` is not defined"),
            ));
        }
    }
    Ok(())
}

fn jump_block(
    instruction: &crate::ir::Instruction,
    label_blocks: &HashMap<String, usize>,
) -> Result<usize, CfgError> {
    let Operand::Label(label) = &instruction.dest else {
        return Err(cfg_error(
            instruction.id,
            "jump instruction has no label destination",
        ));
    };
    label_blocks.get(label).copied().ok_or_else(|| {
        cfg_error(
            instruction.id,
            format!("jump target `{label}` is not defined"),
        )
    })
}

fn cfg_error(instruction_id: usize, message: impl Into<String>) -> CfgError {
    CfgError {
        instruction_id,
        message: message.into(),
    }
}
