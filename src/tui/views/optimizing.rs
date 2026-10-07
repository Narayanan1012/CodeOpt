//! Optimizing loading screen with animated compiler pipeline visualization.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

use crate::tui::{
    app::App,
    theme::{
        COLOR_CYAN, COLOR_PANEL_BG, COLOR_PINK,
        COLOR_SUCCESS, COLOR_TEXT_BODY, COLOR_TEXT_HEAD, COLOR_TEXT_MUTED,
    },
};

pub fn render_optimizing(frame: &mut Frame, app: &App, area: Rect) {
    let popup_width = 72.min(area.width.saturating_sub(4));
    let popup_height = 18.min(area.height.saturating_sub(2));

    let popup_area = Rect {
        x: (area.width.saturating_sub(popup_width)) / 2,
        y: (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width,
        height: popup_height,
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(COLOR_CYAN))
        .style(Style::default().bg(COLOR_PANEL_BG))
        .title(Line::from(vec![
            Span::styled(" ⚡ CodeOpt Pipeline: Optimizing Benchmark Corpus ⚡ ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
        ]));

    let progress = app.optimizing_progress.min(100);
    let bar_width: usize = 46;
    let filled = ((progress as f64 / 100.0) * bar_width as f64).round() as usize;
    let progress_bar = format!("{}{}", "█".repeat(filled), "░".repeat(bar_width.saturating_sub(filled)));

    let spinner_frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let spinner = spinner_frames[(app.tick_count) % spinner_frames.len()];

    let current_stage = if progress < 25 {
        "Phase 1/4: Parsing 32-bit TAC IR Quadruples..."
    } else if progress < 50 {
        "Phase 2/4: Partitioning Basic Blocks & Building CFG..."
    } else if progress < 75 {
        "Phase 3/4: Driving Demand-Driven Worklist (CF, CP, AS, CSE)..."
    } else {
        "Phase 4/4: Dual VM Execution & Differential Verification..."
    };

    let toggle_status = if app.show_optimizing_screen {
        "[ON]"
    } else {
        "[OFF]"
    };

    let content = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(format!(" {spinner} "), Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
            Span::styled(current_stage, Style::default().fg(COLOR_TEXT_HEAD).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::raw("   ["),
            Span::styled(&progress_bar, Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
            Span::styled(format!("] {:>3}%", progress), Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("   • Corpus Size: ", Style::default().fg(COLOR_TEXT_MUTED)),
            Span::styled(format!("{} benchmark programs", app.corpus.items.len()), Style::default().fg(COLOR_TEXT_BODY)),
        ]),
        Line::from(vec![
            Span::styled("   • Target Goals: ", Style::default().fg(COLOR_TEXT_MUTED)),
            Span::styled("20–50% instrs, 30–60% arith, 100% equivalence", Style::default().fg(COLOR_TEXT_BODY)),
        ]),
        Line::from(vec![
            Span::styled("   • VM Engine:   ", Style::default().fg(COLOR_TEXT_MUTED)),
            Span::styled("High-precision microsecond execution timing", Style::default().fg(COLOR_TEXT_BODY)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("   [Space / Enter / Esc] ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
            Span::styled("Skip Loading  │  ", Style::default().fg(COLOR_TEXT_BODY)),
            Span::styled(" [O] ", Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
            Span::styled(format!("Auto-animation: {toggle_status}"), Style::default().fg(COLOR_TEXT_MUTED)),
        ]),
    ];

    let widget = Paragraph::new(content).block(block);
    frame.render_widget(widget, popup_area);
}
