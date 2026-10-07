//! Review 1 terminal dashboard for inspecting one optimization run.

use std::io::{self, stdout};
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::{
    cfg::{BasicBlock, build},
    ir::{OpCode, Program},
    logging::Transformation,
    parser::parse_program,
    passes::{OptimizationReport, WorklistEvent, optimize_with_worklist},
    verification::verify,
};

const BLUE: Color = Color::Rgb(52, 129, 235);
const LIGHT_BLUE: Color = Color::Rgb(125, 211, 252);
const INK: Color = Color::Rgb(211, 226, 246);
const MUTED: Color = Color::Rgb(141, 169, 205);
const SUCCESS: Color = Color::Rgb(103, 232, 161);

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

    Ok(DemoData {
        original_tac: original.to_string(),
        optimized_tac: optimized.to_string(),
        transformations,
        worklist_events,
        blocks: cfg.blocks,
        rounds,
        reached_round_limit,
        original_instruction_count: original.instructions.len(),
        optimized_instruction_count: optimized.instructions.len(),
        original_arithmetic_count: arithmetic_count(&original),
        optimized_arithmetic_count: arithmetic_count(&optimized),
        verification_passed: verification.matches(),
        original_output: verification.original_output,
        optimized_output: verification.optimized_output,
    })
}

pub fn run_tui(demo: DemoData) -> io::Result<()> {
    enable_raw_mode()?;
    let mut output = stdout();
    execute!(output, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(output);
    let mut terminal = Terminal::new(backend)?;
    let result = event_loop(&mut terminal, &demo);
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    demo: &DemoData,
) -> io::Result<()> {
    let mut log_offset = 0;
    loop {
        terminal.draw(|frame| draw_dashboard(frame, demo, log_offset))?;
        if !event::poll(Duration::from_millis(250))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
            KeyCode::Char('j') | KeyCode::Down => log_offset = log_offset.saturating_add(1),
            KeyCode::Char('k') | KeyCode::Up => log_offset = log_offset.saturating_sub(1),
            KeyCode::Home => log_offset = 0,
            _ => {}
        }
    }
}

fn draw_dashboard(frame: &mut ratatui::Frame, demo: &DemoData, log_offset: usize) {
    let area = frame.area();
    let page = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(12),
            Constraint::Length(2),
        ])
        .split(area);

    let header = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(BLUE))
        .title(
            Line::from(" CodeOpt ")
                .style(Style::default().fg(LIGHT_BLUE).add_modifier(Modifier::BOLD)),
        );
    let header_text = Paragraph::new("Review 1 • Rule-based TAC optimizer • Demo workspace")
        .style(Style::default().fg(INK))
        .block(header);
    frame.render_widget(header_text, page[0]);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(44), Constraint::Percentage(56)])
        .split(page[1]);
    let code_panes = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(columns[0]);
    render_code_pane(
        frame,
        code_panes[0],
        "Original TAC",
        &demo.original_tac,
        MUTED,
    );
    render_code_pane(
        frame,
        code_panes[1],
        "Optimized TAC",
        &demo.optimized_tac,
        SUCCESS,
    );

    let inspector = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(58),
            Constraint::Percentage(22),
            Constraint::Percentage(20),
        ])
        .split(columns[1]);
    render_audit_log(frame, inspector[0], demo, log_offset);
    render_metrics(frame, inspector[1], demo);
    render_cfg(frame, inspector[2], demo);

    let footer = Paragraph::new(" q / Esc quit   j / ↓ audit down   k / ↑ audit up   Home top ")
        .style(Style::default().fg(MUTED))
        .block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(BLUE)),
        );
    frame.render_widget(footer, page[2]);
}

fn render_code_pane(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    title: &str,
    code: &str,
    title_color: Color,
) {
    let pane = Paragraph::new(code)
        .style(Style::default().fg(INK))
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(BLUE))
                .title(Line::from(format!(" {title} ")).style(Style::default().fg(title_color))),
        );
    frame.render_widget(pane, area);
}

fn render_audit_log(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    demo: &DemoData,
    log_offset: usize,
) {
    let mut entries = demo
        .transformations
        .iter()
        .map(|change| {
            format!(
                "{:?}  #{}\n  {}  →  {}",
                change.pass, change.instruction_id, change.before, change.after
            )
        })
        .collect::<Vec<_>>();
    entries.extend(demo.worklist_events.iter().map(|event| {
        format!(
            "Worklist\n  definition #{} queued consumer #{}",
            event.definition_id, event.consumer_id
        )
    }));
    if entries.is_empty() {
        entries.push("No transformations fired for this input.".to_owned());
    }
    let visible = entries
        .iter()
        .skip(log_offset.min(entries.len().saturating_sub(1)))
        .cloned()
        .collect::<Vec<_>>()
        .join("\n\n");
    let log = Paragraph::new(visible)
        .style(Style::default().fg(INK))
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(BLUE))
                .title(Line::from(" Optimization audit ").style(Style::default().fg(LIGHT_BLUE))),
        );
    frame.render_widget(log, area);
}

fn render_metrics(frame: &mut ratatui::Frame, area: ratatui::layout::Rect, demo: &DemoData) {
    let arithmetic_reduction = reduction(
        demo.original_arithmetic_count,
        demo.optimized_arithmetic_count,
    );
    let instruction_reduction = reduction(
        demo.original_instruction_count,
        demo.optimized_instruction_count,
    );
    let fixed_point = if demo.reached_round_limit {
        format!("safety cap at {} rounds", demo.rounds)
    } else {
        format!("fixed point in {} round(s)", demo.rounds)
    };
    let verification = if demo.verification_passed {
        format!("PASS  output {:?}", demo.optimized_output)
    } else {
        format!(
            "FAIL  original {:?}; optimized {:?}",
            demo.original_output, demo.optimized_output
        )
    };
    let text = format!(
        "Instructions  {} → {}  ({instruction_reduction:.0}% reduction)\nArithmetic    {} → {}  ({arithmetic_reduction:.0}% reduction)\nVerification  {verification}\nWorklist      {fixed_point}",
        demo.original_instruction_count,
        demo.optimized_instruction_count,
        demo.original_arithmetic_count,
        demo.optimized_arithmetic_count,
    );
    let metrics = Paragraph::new(text).style(Style::default().fg(INK)).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BLUE))
            .title(Line::from(" Live metrics ").style(Style::default().fg(LIGHT_BLUE))),
    );
    frame.render_widget(metrics, area);
}

fn render_cfg(frame: &mut ratatui::Frame, area: ratatui::layout::Rect, demo: &DemoData) {
    let text = demo
        .blocks
        .iter()
        .map(|block| {
            format!(
                "B{}  instructions {:?}  → {:?}",
                block.id, block.instruction_ids, block.successors
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let cfg = Paragraph::new(text)
        .style(Style::default().fg(INK))
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(BLUE))
                .title(Line::from(" Basic blocks / CFG ").style(Style::default().fg(LIGHT_BLUE))),
        );
    frame.render_widget(cfg, area);
}

fn arithmetic_count(program: &Program) -> usize {
    program
        .instructions
        .iter()
        .filter(|instruction| {
            matches!(
                instruction.op,
                OpCode::Add | OpCode::Sub | OpCode::Mul | OpCode::Div | OpCode::Mod
            )
        })
        .count()
}

fn reduction(before: usize, after: usize) -> f64 {
    if before == 0 {
        return 0.0;
    }
    (before.saturating_sub(after) as f64 / before as f64) * 100.0
}
