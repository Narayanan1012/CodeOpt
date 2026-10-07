//! CodeOpt interactive terminal user interface.

pub mod app;
pub mod data;
pub mod theme;
pub mod views;

use std::io::{self, stdout};
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
};

pub use app::{App, CurrentScreen};
pub use data::{BenchmarkCategory, BenchmarkCorpus, BenchmarkItem, ProgramStats};

use crate::{
    cfg::{BasicBlock, build},
    logging::Transformation,
    parser::parse_program,
    passes::{OptimizationReport, WorklistEvent, optimize_with_worklist},
    verification::verify,
};

/// Backward-compatible single-run inspection payload for Review 1 demo.
pub struct DemoData {
    pub original_tac: String,
    pub optimized_tac: String,
    pub transformations: Vec<Transformation>,
    pub worklist_events: Vec<WorklistEvent>,
    pub blocks: Vec<BasicBlock>,
    pub rounds: usize,
    pub reached_round_limit: bool,
    pub original_instruction_count: usize,
    pub optimized_instruction_count: usize,
    pub original_arithmetic_count: usize,
    pub optimized_arithmetic_count: usize,
    pub verification_passed: bool,
    pub original_output: Vec<i32>,
    pub optimized_output: Vec<i32>,
}

pub fn build_demo(source: &str, inputs: &[i32]) -> Result<DemoData, String> {
    let original = parse_program(source).map_err(|error| error.to_string())?;
    let verification = verify(&original, inputs)?;
    let mut optimized = original.clone();
    let cfg = build(&mut optimized).map_err(|error| error.to_string())?;
    let OptimizationReport {
        transformations,
        worklist_events,
        rounds,
        reached_round_limit,
    } = optimize_with_worklist(&mut optimized, &cfg);

    let original_stats = ProgramStats::compute(&original);
    let optimized_stats = ProgramStats::compute(&optimized);

    Ok(DemoData {
        original_tac: original.to_string(),
        optimized_tac: optimized.to_string(),
        transformations,
        worklist_events,
        blocks: cfg.blocks,
        rounds,
        reached_round_limit,
        original_instruction_count: original_stats.instruction_count,
        optimized_instruction_count: optimized_stats.instruction_count,
        original_arithmetic_count: original_stats.arithmetic_count,
        optimized_arithmetic_count: optimized_stats.arithmetic_count,
        verification_passed: verification.matches(),
        original_output: verification.original_output,
        optimized_output: verification.optimized_output,
    })
}

/// Runs the full interactive workbench with the given corpus.
pub fn run_workbench(corpus: BenchmarkCorpus) -> io::Result<()> {
    enable_raw_mode()?;
    let mut output = stdout();
    execute!(output, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(output);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(corpus);
    let result = event_loop(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

/// Backward compatible run_tui: automatically loads corpus from benchmarks/ and launches workbench.
pub fn run_tui(_demo: DemoData) -> io::Result<()> {
    let corpus = BenchmarkCorpus::load_from_dir("benchmarks");
    run_workbench(corpus)
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
) -> io::Result<()> {
    loop {
        terminal.draw(|frame| app.render(frame))?;

        if !event::poll(Duration::from_millis(150))? {
            app.tick();
            continue;
        }

        let Event::Key(key) = event::read()? else {
            continue;
        };

        if key.kind != KeyEventKind::Press {
            continue;
        }

        app.handle_key(key.code);

        if app.should_quit {
            return Ok(());
        }
    }
}
