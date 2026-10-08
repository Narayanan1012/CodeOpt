//! Optimizer pass configuration and shared pass types.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::cfg::ControlFlowGraph;
use crate::ir::{Instruction, OpCode, Operand, Program};
use crate::logging::Transformation;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OptimizationKind {
    ConstantFolding,
    ConstantPropagation,
    AlgebraicSimplification,
    LocalCommonSubexpressionElimination,
    DeadCodeElimination,
}

pub const REVIEW_ONE_PASSES: [OptimizationKind; 5] = [
    OptimizationKind::ConstantFolding,
    OptimizationKind::ConstantPropagation,
    OptimizationKind::AlgebraicSimplification,
    OptimizationKind::LocalCommonSubexpressionElimination,
    OptimizationKind::DeadCodeElimination,
];

const WORKLIST_ROUND_LIMIT: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorklistEvent {
    pub definition_id: usize,
    pub consumer_id: usize,
}

#[derive(Debug, Clone)]
pub struct OptimizationReport {
    pub transformations: Vec<Transformation>,
    pub worklist_events: Vec<WorklistEvent>,
    pub rounds: usize,
    pub reached_round_limit: bool,
}

/// Optimizes until the local worklist and local CSE both reach a fixed point.
/// A rewrite queues only direct consumers of its changed destination.
pub fn optimize_with_worklist(program: &mut Program, cfg: &ControlFlowGraph) -> OptimizationReport {
    let def_use = DefUseChains::build(program, cfg);
    let instruction_offsets = program
        .instructions
        .iter()
        .enumerate()
        .map(|(offset, instruction)| (instruction.id, offset))
        .collect::<HashMap<_, _>>();
    let mut queue = WorkQueue::seed(
        program
            .instructions
            .iter()
            .map(|instruction| instruction.id),
    );
    let mut transformations = Vec::new();
    let mut worklist_events = Vec::new();

    for round in 1..=WORKLIST_ROUND_LIMIT {
        drain_basic_worklist(
            program,
            cfg,
            &instruction_offsets,
            &def_use,
            &mut queue,
            &mut transformations,
            &mut worklist_events,
        );

        let cse_transformations = optimize_local_cse(program, cfg);
        if cse_transformations.is_empty() {
            let dce = eliminate_dead_code(program);
            transformations.extend(dce);
            return OptimizationReport {
                transformations,
                worklist_events,
                rounds: round,
                reached_round_limit: false,
            };
        }

        for transformation in cse_transformations {
            queue_consumers(
                transformation.instruction_id,
                &def_use,
                &mut queue,
                &mut worklist_events,
            );
            transformations.push(transformation);
        }
    }

    let dce = eliminate_dead_code(program);
    transformations.extend(dce);
    OptimizationReport {
        transformations,
        worklist_events,
        rounds: WORKLIST_ROUND_LIMIT,
        reached_round_limit: true,
    }
}

/// Runs the first three Review 1 rules locally within each basic block.
/// Local CSE is added in the next plan step.
pub fn optimize_basic_rules(program: &mut Program, cfg: &ControlFlowGraph) -> Vec<Transformation> {
    let instruction_offsets = program
        .instructions
        .iter()
        .enumerate()
        .map(|(offset, instruction)| (instruction.id, offset))
        .collect::<HashMap<_, _>>();
    let mut transformations = Vec::new();

    for block in &cfg.blocks {
        let mut values = HashMap::new();
        for instruction_id in &block.instruction_ids {
            let offset = instruction_offsets[instruction_id];
            propagate_operands(
                &mut program.instructions[offset],
                &values,
                &mut transformations,
            );
            simplify_instruction(&mut program.instructions[offset], &mut transformations);
            update_known_values(&program.instructions[offset], &mut values);
        }
    }

    transformations
}

fn drain_basic_worklist(
    program: &mut Program,
    cfg: &ControlFlowGraph,
    instruction_offsets: &HashMap<usize, usize>,
    def_use: &DefUseChains,
    queue: &mut WorkQueue,
    transformations: &mut Vec<Transformation>,
    worklist_events: &mut Vec<WorklistEvent>,
) {
    while let Some(instruction_id) = queue.pop() {
        let values = known_values_before(program, cfg, instruction_id);
        let offset = instruction_offsets[&instruction_id];
        let transformation_count = transformations.len();
        propagate_operands(&mut program.instructions[offset], &values, transformations);
        simplify_instruction(&mut program.instructions[offset], transformations);

        if transformations.len() > transformation_count {
            queue_consumers(instruction_id, def_use, queue, worklist_events);
        }
    }
}

fn known_values_before(
    program: &Program,
    cfg: &ControlFlowGraph,
    target_instruction_id: usize,
) -> HashMap<String, Operand> {
    let mut values = HashMap::new();
    let Some(block) = cfg
        .blocks
        .iter()
        .find(|block| block.instruction_ids.contains(&target_instruction_id))
    else {
        return values;
    };

    for instruction_id in &block.instruction_ids {
        if *instruction_id == target_instruction_id {
            break;
        }
        if let Some(instruction) = program
            .instructions
            .iter()
            .find(|instruction| instruction.id == *instruction_id)
        {
            update_known_values(instruction, &mut values);
        }
    }

    values
}

fn queue_consumers(
    definition_id: usize,
    def_use: &DefUseChains,
    queue: &mut WorkQueue,
    worklist_events: &mut Vec<WorklistEvent>,
) {
    for consumer_id in def_use.consumers(definition_id) {
        if queue.push(*consumer_id) {
            worklist_events.push(WorklistEvent {
                definition_id,
                consumer_id: *consumer_id,
            });
        }
    }
}

/// Eliminates repeated binary expressions within each basic block using local
/// value numbering. `a + b` and `b + a` receive the same expression signature.
pub fn optimize_local_cse(program: &mut Program, cfg: &ControlFlowGraph) -> Vec<Transformation> {
    let instruction_offsets = program
        .instructions
        .iter()
        .enumerate()
        .map(|(offset, instruction)| (instruction.id, offset))
        .collect::<HashMap<_, _>>();
    let mut transformations = Vec::new();

    for block in &cfg.blocks {
        let mut state = ValueNumberState::default();
        for instruction_id in &block.instruction_ids {
            let offset = instruction_offsets[instruction_id];
            state.process(&mut program.instructions[offset], &mut transformations);
        }
    }

    transformations
}

fn propagate_operands(
    instruction: &mut Instruction,
    values: &HashMap<String, Operand>,
    transformations: &mut Vec<Transformation>,
) {
    let before = instruction.clone();
    let mut changed = false;

    if instruction_uses_arg1(instruction.op) {
        let replacement = resolve_copy(&instruction.arg1, values);
        changed |= replacement != instruction.arg1;
        instruction.arg1 = replacement;
    }
    if instruction_uses_arg2(instruction.op) {
        let replacement = resolve_copy(&instruction.arg2, values);
        changed |= replacement != instruction.arg2;
        instruction.arg2 = replacement;
    }

    if changed {
        transformations.push(Transformation {
            pass: OptimizationKind::ConstantPropagation,
            instruction_id: instruction.id,
            before,
            after: instruction.clone(),
        });
    }
}

fn simplify_instruction(instruction: &mut Instruction, transformations: &mut Vec<Transformation>) {
    if !is_binary(instruction.op) {
        return;
    }

    if let (Operand::Const(left), Operand::Const(right)) = (&instruction.arg1, &instruction.arg2)
        && let Some(result) = evaluate(instruction.op, *left, *right) {
            let before = instruction.clone();
            replace_with_assignment(instruction, Operand::Const(result));
            transformations.push(Transformation {
                pass: OptimizationKind::ConstantFolding,
                instruction_id: instruction.id,
                before,
                after: instruction.clone(),
            });
            return;
        }

    if let Some(replacement) = algebraic_replacement(instruction) {
        let before = instruction.clone();
        replace_with_assignment(instruction, replacement);
        transformations.push(Transformation {
            pass: OptimizationKind::AlgebraicSimplification,
            instruction_id: instruction.id,
            before,
            after: instruction.clone(),
        });
    }
}

fn update_known_values(instruction: &Instruction, values: &mut HashMap<String, Operand>) {
    let Some(destination) = operand_key(&instruction.dest) else {
        return;
    };

    // A reassignment ends direct copies of the old destination value.
    values.retain(|key, value| {
        key != &destination && operand_key(value).as_deref() != Some(destination.as_str())
    });

    if instruction.op == OpCode::Assign && is_propagatable(&instruction.arg1) {
        values.insert(destination, instruction.arg1.clone());
    }
}

fn resolve_copy(operand: &Operand, values: &HashMap<String, Operand>) -> Operand {
    let mut resolved = operand.clone();
    for _ in 0..=values.len() {
        let Some(key) = operand_key(&resolved) else {
            break;
        };
        let Some(replacement) = values.get(&key) else {
            break;
        };
        if replacement == &resolved {
            break;
        }
        resolved = replacement.clone();
    }
    resolved
}

fn algebraic_replacement(instruction: &Instruction) -> Option<Operand> {
    let left = &instruction.arg1;
    let right = &instruction.arg2;

    match instruction.op {
        OpCode::Add if is_constant(left, 0) => Some(right.clone()),
        OpCode::Add if is_constant(right, 0) => Some(left.clone()),
        OpCode::Sub if is_constant(right, 0) => Some(left.clone()),
        OpCode::Sub if left == right => Some(Operand::Const(0)),
        OpCode::Mul if is_constant(left, 0) || is_constant(right, 0) => Some(Operand::Const(0)),
        OpCode::Mul if is_constant(left, 1) => Some(right.clone()),
        OpCode::Mul if is_constant(right, 1) => Some(left.clone()),
        OpCode::Div if is_constant(right, 1) => Some(left.clone()),
        OpCode::Mod if is_constant(right, 1) => Some(Operand::Const(0)),
        _ => None,
    }
}

fn evaluate(operation: OpCode, left: i32, right: i32) -> Option<i32> {
    match operation {
        OpCode::Add => Some(left.wrapping_add(right)),
        OpCode::Sub => Some(left.wrapping_sub(right)),
        OpCode::Mul => Some(left.wrapping_mul(right)),
        OpCode::Div if right != 0 => Some(left.wrapping_div(right)),
        OpCode::Mod if right != 0 => Some(left.wrapping_rem(right)),
        _ => None,
    }
}

fn replace_with_assignment(instruction: &mut Instruction, value: Operand) {
    instruction.op = OpCode::Assign;
    instruction.arg1 = value;
    instruction.arg2 = Operand::None;
}

fn is_binary(operation: OpCode) -> bool {
    matches!(
        operation,
        OpCode::Add | OpCode::Sub | OpCode::Mul | OpCode::Div | OpCode::Mod
    )
}

fn instruction_uses_arg1(operation: OpCode) -> bool {
    matches!(
        operation,
        OpCode::Add
            | OpCode::Sub
            | OpCode::Mul
            | OpCode::Div
            | OpCode::Mod
            | OpCode::Assign
            | OpCode::IfFalse
            | OpCode::Print
    )
}

fn instruction_uses_arg2(operation: OpCode) -> bool {
    is_binary(operation)
}

fn is_propagatable(operand: &Operand) -> bool {
    matches!(
        operand,
        Operand::Const(_) | Operand::Var(_) | Operand::Temp(_)
    )
}

fn is_constant(operand: &Operand, expected: i32) -> bool {
    matches!(operand, Operand::Const(value) if *value == expected)
}

fn operand_key(operand: &Operand) -> Option<String> {
    match operand {
        Operand::Var(name) => Some(name.clone()),
        Operand::Temp(index) => Some(format!("t{index}")),
        _ => None,
    }
}

#[derive(Default)]
struct DefUseChains {
    consumers_by_definition: HashMap<usize, Vec<usize>>,
}

impl DefUseChains {
    fn build(program: &Program, cfg: &ControlFlowGraph) -> Self {
        let mut consumers_by_definition = HashMap::new();
        let instruction_by_id = program
            .instructions
            .iter()
            .map(|instruction| (instruction.id, instruction))
            .collect::<HashMap<_, _>>();

        for block in &cfg.blocks {
            let mut last_definition = HashMap::new();
            for instruction_id in &block.instruction_ids {
                let instruction = instruction_by_id[instruction_id];
                for operand in used_operands(instruction) {
                    if let Some(key) = operand_key(operand)
                        && let Some(definition_id) = last_definition.get(&key) {
                            consumers_by_definition
                                .entry(*definition_id)
                                .or_insert_with(Vec::new)
                                .push(*instruction_id);
                        }
                }
                if defines_destination(instruction)
                    && let Some(key) = operand_key(&instruction.dest) {
                        last_definition.insert(key, *instruction_id);
                    }
            }
        }

        Self {
            consumers_by_definition,
        }
    }

    fn consumers(&self, definition_id: usize) -> &[usize] {
        self.consumers_by_definition
            .get(&definition_id)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }
}

#[derive(Default)]
struct WorkQueue {
    pending: VecDeque<usize>,
    queued: HashSet<usize>,
}

impl WorkQueue {
    fn seed(ids: impl IntoIterator<Item = usize>) -> Self {
        let mut queue = Self::default();
        for id in ids {
            queue.push(id);
        }
        queue
    }

    fn push(&mut self, instruction_id: usize) -> bool {
        if self.queued.insert(instruction_id) {
            self.pending.push_back(instruction_id);
            return true;
        }
        false
    }

    fn pop(&mut self) -> Option<usize> {
        let instruction_id = self.pending.pop_front()?;
        self.queued.remove(&instruction_id);
        Some(instruction_id)
    }
}

fn used_operands(instruction: &Instruction) -> Vec<&Operand> {
    let mut operands = Vec::new();
    if instruction_uses_arg1(instruction.op) {
        operands.push(&instruction.arg1);
    }
    if instruction_uses_arg2(instruction.op) {
        operands.push(&instruction.arg2);
    }
    operands
}

fn defines_destination(instruction: &Instruction) -> bool {
    is_binary(instruction.op) || matches!(instruction.op, OpCode::Assign | OpCode::Read)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ExpressionKey {
    operation: OpCode,
    left: usize,
    right: usize,
}

#[derive(Default)]
struct ValueNumberState {
    next_value_number: usize,
    variable_values: HashMap<String, usize>,
    constant_values: HashMap<i32, usize>,
    expressions: HashMap<ExpressionKey, Operand>,
}

impl ValueNumberState {
    fn process(
        &mut self,
        instruction: &mut Instruction,
        transformations: &mut Vec<Transformation>,
    ) {
        match instruction.op {
            operation if is_binary(operation) => self.process_binary(instruction, transformations),
            OpCode::Assign => {
                let value_number = self.value_number(&instruction.arg1);
                self.bind_destination(&instruction.dest, value_number);
            }
            OpCode::Read => {
                let value_number = self.fresh_value_number();
                self.bind_destination(&instruction.dest, value_number);
            }
            _ => {}
        }
    }

    fn process_binary(
        &mut self,
        instruction: &mut Instruction,
        transformations: &mut Vec<Transformation>,
    ) {
        let left = self.value_number(&instruction.arg1);
        let right = self.value_number(&instruction.arg2);
        let (left, right) = if is_commutative(instruction.op) && left > right {
            (right, left)
        } else {
            (left, right)
        };
        let expression = ExpressionKey {
            operation: instruction.op,
            left,
            right,
        };

        self.remove_destination_representative(&instruction.dest);
        if let Some(representative) = self.expressions.get(&expression).cloned() {
            let before = instruction.clone();
            replace_with_assignment(instruction, representative.clone());
            transformations.push(Transformation {
                pass: OptimizationKind::LocalCommonSubexpressionElimination,
                instruction_id: instruction.id,
                before,
                after: instruction.clone(),
            });
            let value_number = self.value_number(&representative);
            self.bind_destination(&instruction.dest, value_number);
            return;
        }

        let value_number = self.fresh_value_number();
        self.bind_destination(&instruction.dest, value_number);
        self.expressions
            .insert(expression, instruction.dest.clone());
    }

    fn value_number(&mut self, operand: &Operand) -> usize {
        match operand {
            Operand::Const(value) => {
                if let Some(value_number) = self.constant_values.get(value) {
                    return *value_number;
                }
                let value_number = self.fresh_value_number();
                self.constant_values.insert(*value, value_number);
                value_number
            }
            Operand::Var(_) | Operand::Temp(_) => {
                let key = operand_key(operand).expect("variables and temporaries have keys");
                if let Some(value_number) = self.variable_values.get(&key) {
                    return *value_number;
                }
                let value_number = self.fresh_value_number();
                self.variable_values.insert(key, value_number);
                value_number
            }
            Operand::Label(_) | Operand::None => self.fresh_value_number(),
        }
    }

    fn bind_destination(&mut self, destination: &Operand, value_number: usize) {
        self.remove_destination_representative(destination);
        if let Some(key) = operand_key(destination) {
            self.variable_values.insert(key, value_number);
        }
    }

    fn remove_destination_representative(&mut self, destination: &Operand) {
        self.expressions
            .retain(|_, representative| representative != destination);
    }

    fn fresh_value_number(&mut self) -> usize {
        let value_number = self.next_value_number;
        self.next_value_number += 1;
        value_number
    }
}

fn is_commutative(operation: OpCode) -> bool {
    matches!(operation, OpCode::Add | OpCode::Mul)
}

/// Backward sweep: eliminates instructions whose destination temporary/variable is never read downstream.
pub fn eliminate_dead_code(program: &mut Program) -> Vec<Transformation> {
    let mut transformations = Vec::new();
    let mut changed = true;

    while changed {
        changed = false;
        let mut used = HashSet::new();

        for inst in &program.instructions {
            if inst.op == OpCode::Nop {
                continue;
            }
            for op in used_operands(inst) {
                if let Some(key) = operand_key(op) {
                    used.insert(key);
                }
            }
        }

        for inst in &mut program.instructions {
            if inst.op == OpCode::Nop
                || inst.op == OpCode::Print
                || inst.op == OpCode::Read
                || inst.op == OpCode::Goto
                || inst.op == OpCode::IfFalse
                || inst.op == OpCode::Label
            {
                continue;
            }

            if defines_destination(inst)
                && let Some(dest_key) = operand_key(&inst.dest)
                    && !used.contains(&dest_key) {
                        let before = inst.clone();
                        inst.op = OpCode::Nop;
                        inst.arg1 = Operand::None;
                        inst.arg2 = Operand::None;
                        inst.dest = Operand::None;
                        transformations.push(Transformation {
                            pass: OptimizationKind::DeadCodeElimination,
                            instruction_id: inst.id,
                            before,
                            after: inst.clone(),
                        });
                        changed = true;
                    }
        }

        if changed {
            program.instructions.retain(|inst| inst.op != OpCode::Nop);
        }
    }

    transformations
}

