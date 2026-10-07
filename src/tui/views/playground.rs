//! Interactive TAC Playground: Type custom instructions or load from path with live optimization.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
};

use crate::tui::{
    app::App,
    theme::{
        COLOR_BORDER, COLOR_BORDER_ACTIVE, COLOR_CYAN, COLOR_DANGER, COLOR_INDIGO,
        COLOR_PANEL_BG, COLOR_PINK, COLOR_SUCCESS, COLOR_TEXT_BODY, COLOR_TEXT_HEAD,
        COLOR_TEXT_MUTED, COLOR_WARNING,
    },
};

pub fn render_playground(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),         // Header block
            Constraint::Percentage(60),    // Split: Input Buffer vs. Live Optimized Output
            Constraint::Percentage(40),    // Split: Live Rewrites vs. VM Execution Console
            Constraint::Length(1),         // Footer bar
        ])
        .split(area);

    render_playground_header(frame, app, chunks[0]);
    render_playground_editor(frame, app, chunks[1]);
    render_playground_bottom(frame, app, chunks[2]);
    render_playground_footer(frame, chunks[3]);

    if app.load_file_modal_open {
        render_load_path_modal(frame, app, area);
    }
}

fn render_playground_header(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLOR_BORDER_ACTIVE))
        .title(Line::from(vec![
            Span::styled(" CodeOpt Interactive TAC Playground ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
            Span::styled(format!("— Source: {} ", app.playground_file_name), Style::default().fg(COLOR_TEXT_HEAD)),
        ]));

    let line = Line::from(vec![
        Span::styled(" Live Editor & Real-Time Optimizer  ", Style::default().fg(COLOR_TEXT_MUTED)),
        Span::styled(" │ Mode: ", Style::default().fg(COLOR_TEXT_MUTED)),
        Span::styled(if app.playground_editing { "EDITING [Type In-Place]" } else { "NAVIGATION [Press 'i' to edit]" }, Style::default().fg(if app.playground_editing { COLOR_SUCCESS } else { COLOR_CYAN }).add_modifier(Modifier::BOLD)),
        Span::styled(" │ Inputs: ", Style::default().fg(COLOR_TEXT_MUTED)),
        Span::styled(format!("{:?} ", app.playground_inputs), Style::default().fg(COLOR_PINK)),
        Span::styled(" │ Status: ", Style::default().fg(COLOR_TEXT_MUTED)),
        if app.playground_parse_error.is_some() {
            Span::styled("SYNTAX ERROR", Style::default().fg(COLOR_DANGER).add_modifier(Modifier::BOLD))
        } else {
            Span::styled("OPTIMIZATION CONVERGED", Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD))
        },
    ]);

    let widget = Paragraph::new(line).block(block);
    frame.render_widget(widget, area);
}

fn render_playground_editor(frame: &mut Frame, app: &App, area: Rect) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Left Pane: In-place Input Buffer
    let buffer_block = Block::default()
        .borders(Borders::ALL)
        .border_type(if app.playground_editing { BorderType::Thick } else { BorderType::Rounded })
        .border_style(Style::default().fg(if app.playground_editing { COLOR_BORDER_ACTIVE } else { COLOR_BORDER }))
        .title(Line::from(vec![
            Span::styled(" Input TAC Buffer ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
            Span::styled(format!("({} lines) ", app.playground_buffer.len()), Style::default().fg(COLOR_TEXT_MUTED)),
            Span::styled(if app.playground_editing { "● REC" } else { "" }, Style::default().fg(COLOR_DANGER)),
        ]));

    let buffer_lines: Vec<Line> = app
        .playground_buffer
        .iter()
        .enumerate()
        .map(|(idx, line)| {
            let is_cursor_line = idx == app.playground_cursor_row;
            let cursor_prefix = if is_cursor_line {
                Span::styled("▶ ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD))
            } else {
                Span::raw("  ")
            };

            Line::from(vec![
                cursor_prefix,
                Span::styled(format!("{:>2}: ", idx + 1), Style::default().fg(COLOR_TEXT_MUTED)),
                Span::styled(line, Style::default().fg(if is_cursor_line { COLOR_TEXT_HEAD } else { COLOR_TEXT_BODY })),
                if is_cursor_line && app.playground_editing {
                    Span::styled("█", Style::default().fg(COLOR_CYAN))
                } else {
                    Span::raw("")
                },
            ])
        })
        .collect();

    let buffer_widget = Paragraph::new(buffer_lines)
        .block(buffer_block)
        .wrap(Wrap { trim: false });
    frame.render_widget(buffer_widget, columns[0]);

    // Right Pane: Live Optimized TAC
    let opt_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLOR_SUCCESS))
        .title(Line::from(vec![
            Span::styled(" Live Optimized TAC ", Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
            Span::styled(format!("({} lines) ", app.playground_opt_lines.len()), Style::default().fg(COLOR_TEXT_MUTED)),
        ]));

    let opt_widget = if let Some(error) = &app.playground_parse_error {
        let err_lines = vec![
            Line::from(""),
            Line::from(Span::styled("Parse / Validation Error:", Style::default().fg(COLOR_DANGER).add_modifier(Modifier::BOLD))),
            Line::from(Span::styled(error, Style::default().fg(COLOR_WARNING))),
            Line::from(""),
            Line::from(Span::styled("Expected syntax examples:", Style::default().fg(COLOR_TEXT_MUTED))),
            Line::from(Span::styled("  read x", Style::default().fg(COLOR_TEXT_BODY))),
            Line::from(Span::styled("  t1 = x + 0", Style::default().fg(COLOR_TEXT_BODY))),
            Line::from(Span::styled("  t2 = 10 * 20", Style::default().fg(COLOR_TEXT_BODY))),
            Line::from(Span::styled("  print t1", Style::default().fg(COLOR_TEXT_BODY))),
        ];
        Paragraph::new(err_lines).block(opt_block)
    } else {
        let opt_lines: Vec<Line> = app
            .playground_opt_lines
            .iter()
            .enumerate()
            .map(|(idx, line)| {
                Line::from(vec![
                    Span::styled(format!("{:>2}: ", idx + 1), Style::default().fg(COLOR_TEXT_MUTED)),
                    Span::styled(line, Style::default().fg(COLOR_TEXT_HEAD)),
                ])
            })
            .collect();
        Paragraph::new(opt_lines).block(opt_block).wrap(Wrap { trim: false })
    };

    frame.render_widget(opt_widget, columns[1]);
}

fn render_playground_bottom(frame: &mut Frame, app: &App, area: Rect) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Left: Live Rewrites
    let rewrites_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLOR_BORDER))
        .title(Line::from(vec![
            Span::styled(" Transformation Trail ", Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
            Span::styled(format!("({} passes fired) ", app.playground_transforms.len()), Style::default().fg(COLOR_TEXT_MUTED)),
        ]));

    let mut rewrite_lines = Vec::new();
    if app.playground_transforms.is_empty() {
        rewrite_lines.push(Line::from(Span::styled("No transformations needed for current code.", Style::default().fg(COLOR_TEXT_MUTED))));
    } else {
        for (idx, t) in app.playground_transforms.iter().enumerate() {
            rewrite_lines.push(Line::from(vec![
                Span::styled(format!(" #{:02} [{:?}] ", idx + 1, t.pass), Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
                Span::styled(format!("'{}' ==> '{}'", t.before, t.after), Style::default().fg(COLOR_TEXT_BODY)),
            ]));
        }
    }

    let rewrite_widget = Paragraph::new(rewrite_lines).block(rewrites_block).wrap(Wrap { trim: false });
    frame.render_widget(rewrite_widget, columns[0]);

    // Right: VM Execution Console
    let vm_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLOR_CYAN))
        .title(Line::from(vec![
            Span::styled(" Dual VM Execution Console ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
        ]));

    let vm_lines = vec![
        Line::from(vec![
            Span::styled(" Test Inputs:       ", Style::default().fg(COLOR_TEXT_HEAD)),
            Span::styled(format!("{:?}", app.playground_inputs), Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled(" Original Output:   ", Style::default().fg(COLOR_TEXT_HEAD)),
            Span::styled(format!("{:?}", app.playground_orig_out), Style::default().fg(COLOR_TEXT_BODY)),
        ]),
        Line::from(vec![
            Span::styled(" Optimized Output:  ", Style::default().fg(COLOR_TEXT_HEAD)),
            Span::styled(format!("{:?}", app.playground_opt_out), Style::default().fg(COLOR_TEXT_BODY)),
        ]),
        Line::from(vec![
            Span::styled(" Equivalence Check: ", Style::default().fg(COLOR_TEXT_HEAD)),
            if app.playground_equiv_pass {
                Span::styled("[✓ PASS: 100% OUTPUT MATCH]", Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD))
            } else {
                Span::styled("[✗ FAIL: OUTPUT DIFFERENCE]", Style::default().fg(COLOR_DANGER).add_modifier(Modifier::BOLD))
            },
        ]),
        Line::from(""),
        Line::from(Span::styled("Press [i] to Edit Buffer   [L] Load File Path   [Ctrl+L] Clear", Style::default().fg(COLOR_TEXT_MUTED))),
    ];

    let vm_widget = Paragraph::new(vm_lines).block(vm_block);
    frame.render_widget(vm_widget, columns[1]);
}

fn render_playground_footer(frame: &mut Frame, area: Rect) {
    let text = Line::from(vec![
        Span::styled(" [i] ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
        Span::styled("Edit Buffer  ", Style::default().fg(COLOR_TEXT_BODY)),
        Span::styled(" [Esc] ", Style::default().fg(COLOR_INDIGO).add_modifier(Modifier::BOLD)),
        Span::styled("Exit Edit / Home  ", Style::default().fg(COLOR_TEXT_BODY)),
        Span::styled(" [L] ", Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
        Span::styled("Load File Path  ", Style::default().fg(COLOR_TEXT_BODY)),
        Span::styled(" [↑/↓] ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
        Span::styled("Move Cursor  ", Style::default().fg(COLOR_TEXT_BODY)),
        Span::styled(" [Ctrl+L] ", Style::default().fg(COLOR_WARNING).add_modifier(Modifier::BOLD)),
        Span::styled("Reset Code  ", Style::default().fg(COLOR_TEXT_BODY)),
        Span::styled(" [Q] ", Style::default().fg(COLOR_TEXT_MUTED).add_modifier(Modifier::BOLD)),
        Span::styled("Quit", Style::default().fg(COLOR_TEXT_MUTED)),
    ]);

    let widget = Paragraph::new(text).alignment(Alignment::Center);
    frame.render_widget(widget, area);
}

fn render_load_path_modal(frame: &mut Frame, app: &App, area: Rect) {
    let popup_width = 64.min(area.width.saturating_sub(4));
    let popup_height = 8.min(area.height.saturating_sub(2));

    let popup_area = Rect {
        x: (area.width.saturating_sub(popup_width)) / 2,
        y: (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width,
        height: popup_height,
    };

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(COLOR_PINK))
        .style(Style::default().bg(COLOR_PANEL_BG))
        .title(Line::from(vec![
            Span::styled(" Load TAC Program from File Path ", Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
        ]));

    let content = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(" File Path: ", Style::default().fg(COLOR_TEXT_HEAD).add_modifier(Modifier::BOLD)),
            Span::styled(&app.load_file_path_input, Style::default().fg(COLOR_CYAN)),
            Span::styled("█", Style::default().fg(COLOR_PINK)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(" [Enter] ", Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
            Span::styled("Load & Recompile   ", Style::default().fg(COLOR_TEXT_BODY)),
            Span::styled(" [Esc] ", Style::default().fg(COLOR_TEXT_MUTED).add_modifier(Modifier::BOLD)),
            Span::styled("Cancel", Style::default().fg(COLOR_TEXT_MUTED)),
        ]),
    ];

    let widget = Paragraph::new(content).block(block);
    frame.render_widget(widget, popup_area);
}
