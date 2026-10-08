//! Corpus management, category classification, and benchmark data structures.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::{
    cfg::build,
    logging::Transformation,
    parser::parse_program,
    passes::{OptimizationKind, OptimizationReport, optimize_with_worklist},
    tui::data::metrics::ProgramStats,
    vm::execute,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BenchmarkCategory {
    All,
    ConstantFolding,
    ConstantPropagation,
    AlgebraicSimplification,
    LocalCSE,
    DeadCodeElimination,
    Combined,
    ControlFlow,
}

impl BenchmarkCategory {
    pub fn name(&self) -> &'static str {
        match self {
            Self::All => "All",
            Self::ConstantFolding => "Constant Folding",
            Self::ConstantPropagation => "Constant Propagation",
            Self::AlgebraicSimplification => "Algebraic Simp",
            Self::LocalCSE => "Local CSE",
            Self::DeadCodeElimination => "Dead Code Elim",
            Self::Combined => "★ Combined ★",
            Self::ControlFlow => "Control Flow / Base",
        }
    }

    pub fn short_code(&self) -> &'static str {
        match self {
            Self::All => "ALL",
            Self::ConstantFolding => "CF",
            Self::ConstantPropagation => "CP",
            Self::AlgebraicSimplification => "AS",
            Self::LocalCSE => "CSE",
            Self::DeadCodeElimination => "DCE",
            Self::Combined => "COMB",
            Self::ControlFlow => "BASE",
        }
    }
}

pub const ALL_CATEGORIES: [BenchmarkCategory; 8] = [
    BenchmarkCategory::All,
    BenchmarkCategory::ConstantFolding,
    BenchmarkCategory::ConstantPropagation,
    BenchmarkCategory::AlgebraicSimplification,
    BenchmarkCategory::LocalCSE,
    BenchmarkCategory::DeadCodeElimination,
    BenchmarkCategory::Combined,
    BenchmarkCategory::ControlFlow,
];

#[derive(Debug, Clone)]
pub struct VectorProof {
    pub vector_id: usize,
    pub inputs: Vec<i32>,
    pub original_output: Vec<i32>,
    pub optimized_output: Vec<i32>,
    pub passed: bool,
}

#[derive(Debug, Clone)]
pub struct BenchmarkItem {
    pub id: usize,
    pub name: String,
    pub path: PathBuf,
    pub category: BenchmarkCategory,
    pub source: String,
    pub original_tac: String,
    pub optimized_tac: String,
    pub original_stats: ProgramStats,
    pub optimized_stats: ProgramStats,
    pub time_original_us: u128,
    pub time_optimized_us: u128,
    pub speedup_pct: f64,
    pub verification_passed: bool,
    pub transformations: Vec<Transformation>,
    pub worklist_rounds: usize,
    pub original_outputs: Vec<i32>,
    pub optimized_outputs: Vec<i32>,
    pub test_vectors: Vec<VectorProof>,
}

#[derive(Debug, Clone, Default)]
pub struct BenchmarkCorpus {
    pub items: Vec<BenchmarkItem>,
}

impl BenchmarkCorpus {
    pub fn load_from_dir<P: AsRef<Path>>(dir: P) -> Self {
        let mut items = Vec::new();
        let Ok(entries) = fs::read_dir(dir) else {
            return Self { items };
        };

        let mut paths: Vec<_> = entries
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .filter(|path| path.extension().is_some_and(|ext| ext == "tac"))
            .collect();
        paths.sort();

        for (idx, path) in paths.into_iter().enumerate() {
            let Ok(source) = fs::read_to_string(&path) else {
                continue;
            };
            let name = path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| format!("benchmark_{:02}.tac", idx + 1));

            if let Some(item) = Self::compile_item(idx + 1, name, path, &source) {
                items.push(item);
            }
        }

        Self { items }
    }

    pub fn compile_item(
        id: usize,
        name: String,
        path: PathBuf,
        source: &str,
    ) -> Option<BenchmarkItem> {
        let original = parse_program(source).ok()?;
        let original_stats = ProgramStats::compute(&original);

        let mut optimized = original.clone();
        let cfg = build(&mut optimized).ok()?;
        let OptimizationReport {
            transformations,
            rounds,
            ..
        } = optimize_with_worklist(&mut optimized, &cfg);
        let optimized_stats = ProgramStats::compute(&optimized);

        // Run 5 distinct differential test vectors through the TAC VM for undeniable semantic proof
        let test_input_sets: Vec<Vec<i32>> = if name.contains("control") || name.contains("ctrl") {
            vec![
                vec![0, 1, 2, 3, 4],
                vec![1, 2, 3, 4, 5],
                vec![2, 0, 5, 1, 3],
                vec![3, 4, 1, 2, 0],
                vec![4, 2, 0, 3, 1],
            ]
        } else {
            vec![
                vec![4, 9, 3, 7, 5],
                vec![0, 1, 2, 3, 4],
                vec![10, 5, 20, 1, 8],
                vec![100, 25, 50, 200, 15],
                vec![7, 14, 42, 6, 2],
            ]
        };

        let mut test_vectors = Vec::with_capacity(5);
        let mut all_passed = true;

        for (v_idx, inp) in test_input_sets.into_iter().enumerate() {
            let orig_res = execute(&original, &inp);
            let opt_res = execute(&optimized, &inp);

            let (orig_out, opt_out, matched) = match (orig_res, opt_res) {
                (Ok(o1), Ok(o2)) => {
                    let m = o1.output == o2.output;
                    (o1.output, o2.output, m)
                }
                _ => (Vec::new(), Vec::new(), false),
            };

            if !matched {
                all_passed = false;
            }

            test_vectors.push(VectorProof {
                vector_id: v_idx + 1,
                inputs: inp,
                original_output: orig_out,
                optimized_output: opt_out,
                passed: matched,
            });
        }

        let primary_inputs = test_vectors.first().map(|v| v.inputs.clone()).unwrap_or_default();
        let original_outputs = test_vectors.first().map(|v| v.original_output.clone()).unwrap_or_default();
        let optimized_outputs = test_vectors.first().map(|v| v.optimized_output.clone()).unwrap_or_default();

        // Measure VM execution times across 100 runs for stable microsecond timing
        let (time_original_us, time_optimized_us) = measure_execution_times(&original, &optimized, &primary_inputs);
        let speedup_pct = if time_original_us > 0 {
            if time_original_us >= time_optimized_us {
                ((time_original_us - time_optimized_us) as f64 / time_original_us as f64) * 100.0
            } else {
                -(((time_optimized_us - time_original_us) as f64 / time_original_us as f64) * 100.0)
            }
        } else {
            0.0
        };

        // Categorization logic:
        let mut passes_fired = HashSet::new();
        for trans in &transformations {
            passes_fired.insert(trans.pass);
        }

        let name_lower = name.to_lowercase();
        let category = if name_lower.starts_with("combined") || name_lower.contains("comb") {
            BenchmarkCategory::Combined
        } else if name_lower.starts_with("const") {
            BenchmarkCategory::ConstantFolding
        } else if name_lower.starts_with("prop") {
            BenchmarkCategory::ConstantPropagation
        } else if name_lower.starts_with("alg") {
            BenchmarkCategory::AlgebraicSimplification
        } else if name_lower.starts_with("cse") {
            BenchmarkCategory::LocalCSE
        } else if name_lower.starts_with("dce") || name_lower.starts_with("dead") {
            BenchmarkCategory::DeadCodeElimination
        } else if name_lower.starts_with("ctrl") || name_lower.starts_with("control") {
            BenchmarkCategory::ControlFlow
        } else if passes_fired.len() >= 2 {
            BenchmarkCategory::Combined
        } else if passes_fired.contains(&OptimizationKind::ConstantFolding) {
            BenchmarkCategory::ConstantFolding
        } else if passes_fired.contains(&OptimizationKind::ConstantPropagation) {
            BenchmarkCategory::ConstantPropagation
        } else if passes_fired.contains(&OptimizationKind::AlgebraicSimplification) {
            BenchmarkCategory::AlgebraicSimplification
        } else if passes_fired.contains(&OptimizationKind::LocalCommonSubexpressionElimination) {
            BenchmarkCategory::LocalCSE
        } else if passes_fired.contains(&OptimizationKind::DeadCodeElimination) {
            BenchmarkCategory::DeadCodeElimination
        } else {
            BenchmarkCategory::ControlFlow
        };

        Some(BenchmarkItem {
            id,
            name,
            path,
            category,
            source: source.to_string(),
            original_tac: original.to_string(),
            optimized_tac: optimized.to_string(),
            original_stats,
            optimized_stats,
            time_original_us,
            time_optimized_us,
            speedup_pct,
            verification_passed: all_passed,
            transformations,
            worklist_rounds: rounds,
            original_outputs,
            optimized_outputs,
            test_vectors,
        })
    }

    pub fn filter_by_category(&self, cat: BenchmarkCategory) -> Vec<&BenchmarkItem> {
        if cat == BenchmarkCategory::All {
            self.items.iter().collect()
        } else {
            self.items.iter().filter(|item| item.category == cat).collect()
        }
    }
}

fn measure_execution_times(
    original: &crate::ir::Program,
    optimized: &crate::ir::Program,
    inputs: &[i32],
) -> (u128, u128) {
    const WARMUP_RUNS: usize = 20;
    const MEASURE_RUNS: usize = 100;

    for _ in 0..WARMUP_RUNS {
        let _ = execute(original, inputs);
        let _ = execute(optimized, inputs);
    }

    let start_orig = Instant::now();
    for _ in 0..MEASURE_RUNS {
        let _ = execute(original, inputs);
    }
    let dur_orig = start_orig.elapsed().as_micros();

    let start_opt = Instant::now();
    for _ in 0..MEASURE_RUNS {
        let _ = execute(optimized, inputs);
    }
    let dur_opt = start_opt.elapsed().as_micros();

    (dur_orig / MEASURE_RUNS as u128, dur_opt / MEASURE_RUNS as u128)
}
