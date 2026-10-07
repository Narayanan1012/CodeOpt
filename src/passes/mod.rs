//! Optimizer pass configuration and shared pass types.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OptimizationKind {
    ConstantFolding,
    ConstantPropagation,
    AlgebraicSimplification,
    LocalCommonSubexpressionElimination,
}

pub const REVIEW_ONE_PASSES: [OptimizationKind; 4] = [
    OptimizationKind::ConstantFolding,
    OptimizationKind::ConstantPropagation,
    OptimizationKind::AlgebraicSimplification,
    OptimizationKind::LocalCommonSubexpressionElimination,
];
