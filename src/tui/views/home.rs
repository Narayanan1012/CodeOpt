//! Hero landing view with Codex-style ASCII art, animated telemetry, and mode launchers.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

use crate::tui::{
    app::App,
    theme::{
        COLOR_BORDER_ACTIVE, COLOR_BORDER, COLOR_CYAN, COLOR_INDIGO,
        COLOR_PINK, COLOR_SUCCESS, COLOR_TEXT_BODY, COLOR_TEXT_HEAD,
        COLOR_TEXT_MUTED, COLOR_WARNING,
    },
};

pub fn render_home(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),  // Hero ASCII Art banner
            Constraint::Length(2),  // Subtitle & Status Pulse
            Constraint::Min(12),    // Dual Launcher Cards
            Constraint::Length(3),  // Engine Telemetry Bar
            Constraint::Length(1),  // Navigation Footer
        ])
        .split(area);

    render_ascii_hero(frame, app, chunks[0]);
    render_subtitle_pulse(frame, app, chunks[1]);
    render_launcher_cards(frame, app, chunks[2]);
    render_telemetry(frame, app, chunks[3]);
    render_footer(frame, chunks[4]);
}

fn render_ascii_hero(frame: &mut Frame, _app: &App, area: Rect) {
    let banner = vec![
        Line::from(vec![
            Span::styled("  ██████╗ ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
            Span::styled("██████╗ ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
            Span::styled("██████╗ ", Style::default().fg(COLOR_INDIGO).add_modifier(Modifier::BOLD)),
            Span::styled("███████╗", Style::default().fg(COLOR_INDIGO).add_modifier(Modifier::BOLD)),
            Span::styled(" ██████╗ ", Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
            Span::styled("██████╗ ", Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
            Span::styled("████████╗", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled(" ██╔════╝", Style::default().fg(COLOR_CYAN)),
            Span::styled("██╔═══██╗", Style::default().fg(COLOR_CYAN)),
            Span::styled("██╔══██╗", Style::default().fg(COLOR_INDIGO)),
            Span::styled("██╔════╝", Style::default().fg(COLOR_INDIGO)),
            Span::styled("██╔═══██╗", Style::default().fg(COLOR_PINK)),
            Span::styled("██╔══██╗", Style::default().fg(COLOR_PINK)),
            Span::styled("╚══██╔══╝", Style::default().fg(COLOR_CYAN)),
        ]),
        Line::from(vec![
            Span::styled(" ██║     ", Style::default().fg(COLOR_CYAN)),
            Span::styled("██║   ██║", Style::default().fg(COLOR_CYAN)),
            Span::styled("██║  ██║", Style::default().fg(COLOR_INDIGO)),
            Span::styled("█████╗  ", Style::default().fg(COLOR_INDIGO)),
            Span::styled("██║   ██║", Style::default().fg(COLOR_PINK)),
            Span::styled("██████╔╝", Style::default().fg(COLOR_PINK)),
            Span::styled("   ██║   ", Style::default().fg(COLOR_CYAN)),
        ]),
        Line::from(vec![
            Span::styled(" ██║     ", Style::default().fg(COLOR_CYAN)),
            Span::styled("██║   ██║", Style::default().fg(COLOR_CYAN)),
            Span::styled("██║  ██║", Style::default().fg(COLOR_INDIGO)),
            Span::styled("██╔══╝  ", Style::default().fg(COLOR_INDIGO)),
            Span::styled("██║   ██║", Style::default().fg(COLOR_PINK)),
            Span::styled("██╔═══╝ ", Style::default().fg(COLOR_PINK)),
            Span::styled("   ██║   ", Style::default().fg(COLOR_CYAN)),
        ]),
        Line::from(vec![
            Span::styled(" ╚██████╗", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
            Span::styled("╚██████╔╝", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
            Span::styled("██████╔╝", Style::default().fg(COLOR_INDIGO).add_modifier(Modifier::BOLD)),
            Span::styled("███████╗", Style::default().fg(COLOR_INDIGO).add_modifier(Modifier::BOLD)),
            Span::styled("╚██████╔╝", Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
            Span::styled("██║     ", Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
            Span::styled("   ██║   ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("  ╚═════╝ ", Style::default().fg(COLOR_TEXT_MUTED)),
            Span::styled("╚═════╝ ", Style::default().fg(COLOR_TEXT_MUTED)),
            Span::styled("╚═════╝ ", Style::default().fg(COLOR_TEXT_MUTED)),
            Span::styled("╚══════╝", Style::default().fg(COLOR_TEXT_MUTED)),
            Span::styled(" ╚═════╝ ", Style::default().fg(COLOR_TEXT_MUTED)),
            Span::styled("╚═╝     ", Style::default().fg(COLOR_TEXT_MUTED)),
            Span::styled("   ╚═╝   ", Style::default().fg(COLOR_TEXT_MUTED)),
        ]),
    ];

    let widget = Paragraph::new(banner)
        .alignment(Alignment::Center)
        .block(Block::default());
    frame.render_widget(widget, area);
}

fn render_subtitle_pulse(frame: &mut Frame, app: &App, area: Rect) {
    let pulse_chars = ["●", "◉", "○", "◉"];
    let pulse_char = pulse_chars[(app.tick_count / 2) % pulse_chars.len()];
    let pulse_color = if (app.tick_count / 2).is_multiple_of(2) {
        COLOR_SUCCESS
    } else {
        COLOR_CYAN
    };

    let text = Line::from(vec![
        Span::styled(format!(" {pulse_char} "), Style::default().fg(pulse_color).add_modifier(Modifier::BOLD)),
        Span::styled("AUTOMATED TAC OPTIMIZER & INTERACTIVE WORKBENCH", Style::default().fg(COLOR_TEXT_HEAD).add_modifier(Modifier::BOLD)),
        Span::styled("  •  ", Style::default().fg(COLOR_TEXT_MUTED)),
        Span::styled("Pure Rust 2024", Style::default().fg(COLOR_CYAN)),
        Span::styled("  •  ", Style::default().fg(COLOR_TEXT_MUTED)),
        Span::styled("Def-Use Worklist Engine", Style::default().fg(COLOR_INDIGO)),
        Span::styled("  •  ", Style::default().fg(COLOR_TEXT_MUTED)),
        Span::styled("VM Verifier Active", Style::default().fg(COLOR_SUCCESS)),
    ]);

    let widget = Paragraph::new(text).alignment(Alignment::Center);
    frame.render_widget(widget, area);
}

fn render_launcher_cards(frame: &mut Frame, app: &App, area: Rect) {
    let cards = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let is_playground_active = app.selected_home_card == 0;
    let is_benchmark_active = app.selected_home_card == 1;

    // Card 1: Interactive Playground
    let border_color_1 = if is_playground_active {
        COLOR_BORDER_ACTIVE
    } else {
        COLOR_BORDER
    };
    let border_type_1 = if is_playground_active {
        BorderType::Thick
    } else {
        BorderType::Rounded
    };

    let block_1 = Block::default()
        .borders(Borders::ALL)
        .border_type(border_type_1)
        .border_style(Style::default().fg(border_color_1))
        .title(Line::from(vec![
            Span::styled(
                if is_playground_active { " ▶ [1] Interactive Playground " } else { "   [1] Interactive Playground " },
                Style::default().fg(if is_playground_active { COLOR_CYAN } else { COLOR_TEXT_HEAD }).add_modifier(Modifier::BOLD),
            ),
        ]));

    let content_1 = vec![
        Line::from(Span::styled("Optimize, edit, and step-inspect arbitrary TAC code.", Style::default().fg(COLOR_TEXT_BODY))),
        Line::from(""),
        Line::from(vec![
            Span::styled(" • [T] Type Custom Code: ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
            Span::styled("Interactive buffer editor", Style::default().fg(COLOR_TEXT_MUTED)),
        ]),
        Line::from(vec![
            Span::styled(" • [L] Load External File: ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
            Span::styled("Ingest any .tac file by path", Style::default().fg(COLOR_TEXT_MUTED)),
        ]),
        Line::from(vec![
            Span::styled(" • Live Differential VM: ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
            Span::styled("Test vectors & output verification", Style::default().fg(COLOR_TEXT_MUTED)),
        ]),
        Line::from(vec![
            Span::styled(" • Audit Trail Inspector: ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
            Span::styled("View pass-by-pass rewrites", Style::default().fg(COLOR_TEXT_MUTED)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            if is_playground_active { ">> Press [Enter] or [1] to Launch Playground <<" } else { "   Select with [←] or press [1]   " },
            Style::default().fg(if is_playground_active { COLOR_SUCCESS } else { COLOR_TEXT_MUTED }).add_modifier(Modifier::BOLD),
        )),
    ];
    let widget_1 = Paragraph::new(content_1).block(block_1);
    frame.render_widget(widget_1, cards[0]);

    // Card 2: Benchmark Suite & Datasets
    let border_color_2 = if is_benchmark_active {
        COLOR_BORDER_ACTIVE
    } else {
        COLOR_BORDER
    };
    let border_type_2 = if is_benchmark_active {
        BorderType::Thick
    } else {
        BorderType::Rounded
    };

    let block_2 = Block::default()
        .borders(Borders::ALL)
        .border_type(border_type_2)
        .border_style(Style::default().fg(border_color_2))
        .title(Line::from(vec![
            Span::styled(
                if is_benchmark_active { " ▶ [2] Benchmark Suite & Datasets " } else { "   [2] Benchmark Suite & Datasets " },
                Style::default().fg(if is_benchmark_active { COLOR_PINK } else { COLOR_TEXT_HEAD }).add_modifier(Modifier::BOLD),
            ),
        ]));

    let loaded_count = app.corpus.items.len();
    let content_2 = vec![
        Line::from(Span::styled(format!("Run and analyze corpus ({loaded_count} programs loaded) with metrics."), Style::default().fg(COLOR_TEXT_BODY))),
        Line::from(""),
        Line::from(vec![
            Span::styled(" • [Top Graphs Deck]: ", Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
            Span::styled("Instruction delta, distribution & sparkline", Style::default().fg(COLOR_TEXT_MUTED)),
        ]),
        Line::from(vec![
            Span::styled(" • [Categorized Dropdowns]: ", Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
            Span::styled("CF, CP, AS, CSE, and ★ Combined ★", Style::default().fg(COLOR_TEXT_MUTED)),
        ]),
        Line::from(vec![
            Span::styled(" • [Execution Time Score]: ", Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
            Span::styled("Displays reduced runtime & % speedup", Style::default().fg(COLOR_TEXT_MUTED)),
        ]),
        Line::from(vec![
            Span::styled(" • [Full-Screen View More]: ", Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
            Span::styled("Expands code diffs & complete stats", Style::default().fg(COLOR_TEXT_MUTED)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            if is_benchmark_active { ">> Press [Enter] or [2] to Launch Benchmark Suite <<" } else { "   Select with [→] or press [2]   " },
            Style::default().fg(if is_benchmark_active { COLOR_SUCCESS } else { COLOR_TEXT_MUTED }).add_modifier(Modifier::BOLD),
        )),
    ];
    let widget_2 = Paragraph::new(content_2).block(block_2);
    frame.render_widget(widget_2, cards[1]);
}

fn render_telemetry(frame: &mut Frame, _app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(COLOR_BORDER))
        .title(Line::from(vec![
            Span::styled(" Engine Telemetry & System Status ", Style::default().fg(COLOR_TEXT_MUTED)),
        ]));

    let line = Line::from(vec![
        Span::styled(" Passes: ", Style::default().fg(COLOR_TEXT_MUTED)),
        Span::styled("[CF: Ready] ", Style::default().fg(COLOR_SUCCESS)),
        Span::styled("[CP: Ready] ", Style::default().fg(COLOR_SUCCESS)),
        Span::styled("[AS: Ready] ", Style::default().fg(COLOR_SUCCESS)),
        Span::styled("[CSE: Ready] ", Style::default().fg(COLOR_SUCCESS)),
        Span::styled("[SR: Staged] ", Style::default().fg(COLOR_WARNING)),
        Span::styled("[DCE: Staged] ", Style::default().fg(COLOR_WARNING)),
        Span::styled(" │ VM: ", Style::default().fg(COLOR_TEXT_MUTED)),
        Span::styled("32-bit Signed Wrapping ", Style::default().fg(COLOR_CYAN)),
        Span::styled(" │ Verification: ", Style::default().fg(COLOR_TEXT_MUTED)),
        Span::styled("Differential Equivalence", Style::default().fg(COLOR_SUCCESS)),
    ]);

    let widget = Paragraph::new(line).block(block);
    frame.render_widget(widget, area);
}

fn render_footer(frame: &mut Frame, area: Rect) {
    let text = Line::from(vec![
        Span::styled(" [1/2] ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
        Span::styled("Select Mode  ", Style::default().fg(COLOR_TEXT_BODY)),
        Span::styled(" [Tab / ← / →] ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
        Span::styled("Switch Card  ", Style::default().fg(COLOR_TEXT_BODY)),
        Span::styled(" [Enter] ", Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
        Span::styled("Launch  ", Style::default().fg(COLOR_TEXT_BODY)),
        Span::styled(" [?] ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
        Span::styled("Help Modal  ", Style::default().fg(COLOR_TEXT_BODY)),
        Span::styled(" [Q / Esc] ", Style::default().fg(COLOR_TEXT_MUTED).add_modifier(Modifier::BOLD)),
        Span::styled("Quit", Style::default().fg(COLOR_TEXT_MUTED)),
    ]);

    let widget = Paragraph::new(text).alignment(Alignment::Center);
    frame.render_widget(widget, area);
}
