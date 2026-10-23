//! Interactive TAC Playground: Type custom instructions or load from path with live optimization.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Position, Rect},
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
    if app.playground_fullscreen {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),         // Header block
                Constraint::Min(5),            // Full-screen Editor Buffer
                Constraint::Length(1),         // Footer bar
            ])
            .split(area);

        render_playground_header(frame, app, chunks[0]);
        render_editor_buffer(frame, app, chunks[1], true);
        render_playground_footer(frame, app, chunks[2]);
    } else {
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
        render_playground_split_editor(frame, app, chunks[1]);
        render_playground_bottom(frame, app, chunks[2]);
        render_playground_footer(frame, app, chunks[3]);
    }

    if app.reset_confirm_modal_open {
        render_reset_confirm_modal(frame, area);
    } else if app.load_file_modal_open {
        render_load_path_modal(frame, app, area);
    }
}

fn render_playground_header(frame: &mut Frame, app: &App, area: Rect) {
    let title_text = if app.playground_fullscreen {
        " CodeOpt Interactive Playground (FULL-SCREEN EDITOR) "
    } else {
        " CodeOpt Interactive Playground (C & TAC) "
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLOR_BORDER_ACTIVE))
        .title(Line::from(vec![
            Span::styled(title_text, Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
            Span::styled(format!("— Source: {} ", app.playground_file_name), Style::default().fg(COLOR_TEXT_HEAD)),
        ]));

    let is_empty_buffer = app.playground_buffer.is_empty()
        || (app.playground_buffer.len() == 1 && app.playground_buffer[0].trim().is_empty());

    let mut header_spans = vec![
        Span::styled(" Live Editor & Real-Time Optimizer  ", Style::default().fg(COLOR_TEXT_MUTED)),
        Span::styled(" │ Mode: ", Style::default().fg(COLOR_TEXT_MUTED)),
        Span::styled(
            if app.playground_editing { "EDITING" } else { "NAV" },
            Style::default().fg(if app.playground_editing { COLOR_SUCCESS } else { COLOR_CYAN }).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" Ln:{}/{} Col:{}", app.playground_cursor_row + 1, app.playground_buffer.len(), app.playground_cursor_col + 1),
            Style::default().fg(COLOR_TEXT_MUTED),
        ),
    ];

    if app.playground_fullscreen {
        header_spans.push(Span::styled(" │ ", Style::default().fg(COLOR_TEXT_MUTED)));
        header_spans.push(Span::styled(" [FULLSCREEN ACTIVE] ", Style::default().fg(COLOR_PANEL_BG).bg(COLOR_CYAN).add_modifier(Modifier::BOLD)));
    }

    header_spans.push(Span::styled(" │ Status: ", Style::default().fg(COLOR_TEXT_MUTED)));
    if app.playground_parse_error.is_some() {
        header_spans.push(Span::styled("SYNTAX ERROR", Style::default().fg(COLOR_DANGER).add_modifier(Modifier::BOLD)));
    } else if is_empty_buffer {
        header_spans.push(Span::styled("AWAITING INPUT — TYPE C OR TAC CODE", Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)));
    } else if !app.playground_converted {
        header_spans.push(Span::styled("READY TO CONVERT — PRESS [C]", Style::default().fg(COLOR_WARNING).add_modifier(Modifier::BOLD)));
    } else if !app.playground_vm_executed {
        header_spans.push(Span::styled("CONVERTED & OPTIMIZED — PRESS [R] TO RUN VM", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)));
    } else {
        header_spans.push(Span::styled("OPTIMIZED & VERIFIED", Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)));
    }

    if let Some(msg) = &app.playground_status_message {
        header_spans.push(Span::styled(" │ ", Style::default().fg(COLOR_TEXT_MUTED)));
        header_spans.push(Span::styled(msg, Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)));
    }
    let line = Line::from(header_spans);

    let widget = Paragraph::new(line).block(block);
    frame.render_widget(widget, area);
}

fn render_playground_split_editor(frame: &mut Frame, app: &App, area: Rect) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    render_editor_buffer(frame, app, columns[0], false);
    render_playground_opt_column(frame, app, columns[1]);
}

fn render_editor_buffer(frame: &mut Frame, app: &App, area: Rect, is_fullscreen: bool) {
    let is_empty_buffer = app.playground_buffer.is_empty()
        || (app.playground_buffer.len() == 1 && app.playground_buffer[0].is_empty());

    let title_prefix = if is_fullscreen {
        " Full-Screen Editor (C / TAC) [f / Esc to Split View] "
    } else {
        " Input Buffer (C or TAC) [f for Full-Screen] "
    };

    let buffer_block = Block::default()
        .borders(Borders::ALL)
        .border_type(if app.playground_editing {
            BorderType::Thick
        } else {
            BorderType::Rounded
        })
        .border_style(Style::default().fg(if app.playground_editing {
            COLOR_BORDER_ACTIVE
        } else {
            COLOR_BORDER
        }))
        .title(Line::from(vec![
            Span::styled(title_prefix, Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
            Span::styled(format!("({} lines) ", app.playground_buffer.len()), Style::default().fg(COLOR_TEXT_MUTED)),
            Span::styled(
                if app.playground_editing { "● REC" } else { "○ NAV" },
                Style::default().fg(if app.playground_editing { COLOR_DANGER } else { COLOR_CYAN }).add_modifier(Modifier::BOLD),
            ),
        ]));

    let inner_area = buffer_block.inner(area);
    frame.render_widget(buffer_block, area);

    if inner_area.height == 0 || inner_area.width == 0 {
        return;
    }

    // Quick Start Guide when empty and not editing
    if is_empty_buffer && !app.playground_editing {
        let guide_lines = vec![
            Line::from(vec![
                Span::styled("▶ ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
                Span::styled(" 1: ", Style::default().fg(COLOR_TEXT_MUTED)),
                Span::styled("/* Enter your C code or TAC instructions here */", Style::default().fg(COLOR_TEXT_MUTED)),
            ]),
            Line::from(""),
            Line::from(Span::styled("  ┌────────────────────────────────────────────────────────┐", Style::default().fg(COLOR_BORDER))),
            Line::from(Span::styled("  │  Quick Start Guide:                                    │", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD))),
            Line::from(Span::styled("  │                                                        │", Style::default().fg(COLOR_BORDER))),
            Line::from(vec![
                Span::styled("  │  • Press ", Style::default().fg(COLOR_BORDER)),
                Span::styled("[i]", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
                Span::styled(" to enter edit mode and start typing.        │", Style::default().fg(COLOR_TEXT_BODY)),
            ]),
            Line::from(vec![
                Span::styled("  │  • Press ", Style::default().fg(COLOR_BORDER)),
                Span::styled("[f]", Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
                Span::styled(" to toggle full-screen editing mode.         │", Style::default().fg(COLOR_TEXT_BODY)),
            ]),
            Line::from(vec![
                Span::styled("  │  • Press ", Style::default().fg(COLOR_BORDER)),
                Span::styled("[Ctrl+V]", Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
                Span::styled(" or ", Style::default().fg(COLOR_TEXT_BODY)),
                Span::styled("[p]", Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
                Span::styled(" to paste C or TAC code.         │", Style::default().fg(COLOR_TEXT_BODY)),
            ]),
            Line::from(vec![
                Span::styled("  │  • Press ", Style::default().fg(COLOR_BORDER)),
                Span::styled("[L]", Style::default().fg(COLOR_WARNING).add_modifier(Modifier::BOLD)),
                Span::styled(" to load a sample .c or .tac file.            │", Style::default().fg(COLOR_TEXT_BODY)),
            ]),
            Line::from(vec![
                Span::styled("  │  • Press ", Style::default().fg(COLOR_BORDER)),
                Span::styled("[C]", Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
                Span::styled(" to convert C code to TAC & optimize it.    │", Style::default().fg(COLOR_TEXT_BODY)),
            ]),
            Line::from(vec![
                Span::styled("  │  • Press ", Style::default().fg(COLOR_BORDER)),
                Span::styled("[R]", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
                Span::styled(" to run both in dual VM & verify output.    │", Style::default().fg(COLOR_TEXT_BODY)),
            ]),
            Line::from(Span::styled("  └────────────────────────────────────────────────────────┘", Style::default().fg(COLOR_BORDER))),
        ];
        let guide_widget = Paragraph::new(guide_lines);
        frame.render_widget(guide_widget, inner_area);
        return;
    }

    let visible_rows = inner_area.height as usize;
    let total_lines = app.playground_buffer.len();
    let cursor_row = app.playground_cursor_row.min(total_lines.saturating_sub(1));

    // Calculate vertical scroll offset to guarantee cursor is always visible
    let scroll_row = if cursor_row < app.playground_scroll_row {
        cursor_row
    } else if cursor_row >= app.playground_scroll_row + visible_rows {
        cursor_row.saturating_sub(visible_rows.saturating_sub(1))
    } else {
        app.playground_scroll_row
    };
    let scroll_row = scroll_row.min(total_lines.saturating_sub(visible_rows).max(0));

    // Dynamic line prefix formatting
    let digits = total_lines.to_string().len().max(2);
    let prefix_width = 2 + digits + 2; // "▶ " + width + ": "
    let visible_cols = (inner_area.width as usize).saturating_sub(prefix_width);
    let visible_cols = if visible_cols == 0 { 1 } else { visible_cols };

    // Calculate horizontal scroll offset
    let cursor_col = app.playground_cursor_col;
    let scroll_col = if cursor_col < app.playground_scroll_col {
        cursor_col
    } else if cursor_col >= app.playground_scroll_col + visible_cols {
        cursor_col.saturating_sub(visible_cols.saturating_sub(1))
    } else {
        app.playground_scroll_col
    };

    // Smooth software cursor blinking at ~300ms intervals (10 Hz cadence / 3)
    let is_blinker_on = (app.tick_count / 3) % 2 == 0;

    let end_row = (scroll_row + visible_rows).min(total_lines);
    let mut rendered_lines = Vec::with_capacity(end_row.saturating_sub(scroll_row));

    for idx in scroll_row..end_row {
        let line = &app.playground_buffer[idx];
        let is_cursor_line = idx == cursor_row;

        let cursor_prefix = if is_cursor_line {
            Span::styled("▶ ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD))
        } else {
            Span::raw("  ")
        };

        let line_num = Span::styled(
            format!("{:>width$}: ", idx + 1, width = digits),
            Style::default().fg(COLOR_TEXT_MUTED),
        );

        if is_cursor_line {
            let char_count = line.chars().count();
            let col = cursor_col.min(char_count);

            let visible_chars: Vec<char> = line.chars().skip(scroll_col).take(visible_cols).collect();
            let rel_col = if col >= scroll_col { col - scroll_col } else { 0 };

            let mut spans = vec![cursor_prefix, line_num];

            if line.is_empty() && idx == 0 && app.playground_editing {
                let cursor_display = " ";
                let cursor_style = if is_blinker_on {
                    Style::default().fg(COLOR_PANEL_BG).bg(COLOR_CYAN).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(COLOR_CYAN).add_modifier(Modifier::UNDERLINED | Modifier::BOLD)
                };
                spans.push(Span::styled(cursor_display, cursor_style));
                spans.push(Span::styled(" // Type C code (e.g. int main()...) or TAC instructions here...", Style::default().fg(COLOR_TEXT_MUTED)));
            } else {
                let before: String = visible_chars.iter().take(rel_col).collect();
                let cursor_char_opt = visible_chars.get(rel_col);
                let cursor_display = match cursor_char_opt {
                    Some(c) => c.to_string(),
                    None => " ".to_string(),
                };
                let after: String = visible_chars.iter().skip(rel_col + 1).collect();

                let cursor_style = if app.playground_editing {
                    if is_blinker_on {
                        Style::default().fg(COLOR_PANEL_BG).bg(COLOR_CYAN).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(COLOR_CYAN).add_modifier(Modifier::UNDERLINED | Modifier::BOLD)
                    }
                } else {
                    if is_blinker_on {
                        Style::default().fg(COLOR_PANEL_BG).bg(COLOR_PINK).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(COLOR_PINK).add_modifier(Modifier::UNDERLINED | Modifier::BOLD)
                    }
                };

                spans.push(Span::styled(before, Style::default().fg(COLOR_TEXT_HEAD)));
                spans.push(Span::styled(cursor_display, cursor_style));
                spans.push(Span::styled(after, Style::default().fg(COLOR_TEXT_HEAD)));
            }

            rendered_lines.push(Line::from(spans));
        } else {
            let visible_text: String = line.chars().skip(scroll_col).take(visible_cols).collect();
            let spans = vec![
                cursor_prefix,
                line_num,
                Span::styled(visible_text, Style::default().fg(COLOR_TEXT_BODY)),
            ];
            rendered_lines.push(Line::from(spans));
        }
    }

    let widget = Paragraph::new(rendered_lines);
    frame.render_widget(widget, inner_area);

    // Hardware terminal cursor positioning
    if cursor_row >= scroll_row && cursor_row < scroll_row + visible_rows {
        let rel_row = (cursor_row - scroll_row) as u16;
        let col = cursor_col.min(app.playground_buffer[cursor_row].chars().count());
        if col >= scroll_col && col <= scroll_col + visible_cols {
            let rel_col = (col - scroll_col) as u16;
            let screen_x = inner_area.x + prefix_width as u16 + rel_col;
            let screen_y = inner_area.y + rel_row;
            if screen_x < inner_area.x + inner_area.width && screen_y < inner_area.y + inner_area.height {
                frame.set_cursor_position(Position::new(screen_x, screen_y));
            }
        }
    }
}

fn render_playground_opt_column(frame: &mut Frame, app: &App, area: Rect) {
    let is_empty_buffer = app.playground_buffer.is_empty()
        || (app.playground_buffer.len() == 1 && app.playground_buffer[0].is_empty());

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
            Line::from(Span::styled("Supported inputs:", Style::default().fg(COLOR_TEXT_MUTED))),
            Line::from(Span::styled("  • C code: int a = (b * 8) + 1; scanf(\"%d\", &x); printf(\"%d\", a);", Style::default().fg(COLOR_TEXT_BODY))),
            Line::from(Span::styled("  • TAC code: read a, t1 = a * 8, print t1", Style::default().fg(COLOR_TEXT_BODY))),
        ];
        Paragraph::new(err_lines).block(opt_block)
    } else if !app.playground_converted {
        let pending_lines = if is_empty_buffer {
            vec![
                Line::from(""),
                Line::from(Span::styled(" [Awaiting C or TAC Code] ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD))),
                Line::from(""),
                Line::from(Span::styled(" 1. Type or paste your C source code or TAC instructions on the left.", Style::default().fg(COLOR_TEXT_HEAD))),
                Line::from(Span::styled(" 2. Press [C] to convert C code into TAC and run optimization passes.", Style::default().fg(COLOR_TEXT_BODY))),
                Line::from(Span::styled(" 3. Optimized TAC and pass transformations will appear here live!", Style::default().fg(COLOR_SUCCESS))),
                Line::from(Span::styled(" 4. Press [R] to execute both codes in the VM and verify outputs.", Style::default().fg(COLOR_TEXT_BODY))),
            ]
        } else {
            vec![
                Line::from(""),
                Line::from(Span::styled(" [Code Ready — Press [C] to Convert & Optimize] ", Style::default().fg(COLOR_WARNING).add_modifier(Modifier::BOLD))),
                Line::from(""),
                Line::from(Span::styled(" Press [C] to lower C code into TAC and run the optimization pipeline.", Style::default().fg(COLOR_TEXT_BODY))),
            ]
        };
        Paragraph::new(pending_lines).block(opt_block)
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

    frame.render_widget(opt_widget, area);
}

fn render_playground_bottom(frame: &mut Frame, app: &App, area: Rect) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let is_empty_buffer = app.playground_buffer.is_empty()
        || (app.playground_buffer.len() == 1 && app.playground_buffer[0].is_empty());

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
    if is_empty_buffer {
        rewrite_lines.push(Line::from(Span::styled("Awaiting code — enter C or TAC code above, then press [C].", Style::default().fg(COLOR_TEXT_MUTED))));
    } else if !app.playground_converted {
        rewrite_lines.push(Line::from(Span::styled("Press [C] to convert code and generate transformation trail.", Style::default().fg(COLOR_WARNING))));
    } else if app.playground_transforms.is_empty() {
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

    let vm_lines = if is_empty_buffer {
        vec![
            Line::from(vec![
                Span::styled(" Test Inputs:       ", Style::default().fg(COLOR_TEXT_HEAD)),
                Span::styled(format!("{:?}", app.playground_inputs), Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled(" Original Output:   ", Style::default().fg(COLOR_TEXT_HEAD)),
                Span::styled("[Awaiting Code]", Style::default().fg(COLOR_TEXT_MUTED)),
            ]),
            Line::from(vec![
                Span::styled(" Optimized Output:  ", Style::default().fg(COLOR_TEXT_HEAD)),
                Span::styled("[Awaiting Code]", Style::default().fg(COLOR_TEXT_MUTED)),
            ]),
            Line::from(vec![
                Span::styled(" Equivalence Check: ", Style::default().fg(COLOR_TEXT_HEAD)),
                Span::styled("[AWAITING CODE — ENTER C OR TAC CODE ABOVE]", Style::default().fg(COLOR_TEXT_MUTED)),
            ]),
            Line::from(""),
            Line::from(Span::styled("Press [i] Edit  [f] Fullscreen  [C] Convert  [R] Run VM  [L] Load  [X] Reset", Style::default().fg(COLOR_TEXT_MUTED))),
        ]
    } else if !app.playground_vm_executed {
        vec![
            Line::from(vec![
                Span::styled(" Test Inputs:       ", Style::default().fg(COLOR_TEXT_HEAD)),
                Span::styled(format!("{:?}", app.playground_inputs), Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled(" Original Output:   ", Style::default().fg(COLOR_TEXT_HEAD)),
                Span::styled("[Pending Run — Press 'R']", Style::default().fg(COLOR_TEXT_MUTED)),
            ]),
            Line::from(vec![
                Span::styled(" Optimized Output:  ", Style::default().fg(COLOR_TEXT_HEAD)),
                Span::styled("[Pending Run — Press 'R']", Style::default().fg(COLOR_TEXT_MUTED)),
            ]),
            Line::from(vec![
                Span::styled(" Equivalence Check: ", Style::default().fg(COLOR_TEXT_HEAD)),
                Span::styled("[READY — PRESS 'R' TO RUN DUAL VM & VERIFY]", Style::default().fg(COLOR_WARNING).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(""),
            Line::from(Span::styled("Press [i] Edit  [f] Fullscreen  [C] Convert  [R] Run VM  [L] Load  [X] Reset", Style::default().fg(COLOR_TEXT_MUTED))),
        ]
    } else {
        vec![
            Line::from(vec![
                Span::styled(" Test Inputs:       ", Style::default().fg(COLOR_TEXT_HEAD)),
                Span::styled(format!("{:?}", app.playground_inputs), Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled(" Original Output:   ", Style::default().fg(COLOR_TEXT_HEAD)),
                Span::styled(format!("{:?}", app.playground_orig_out), Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled(" Optimized Output:  ", Style::default().fg(COLOR_TEXT_HEAD)),
                Span::styled(format!("{:?}", app.playground_opt_out), Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled(" Equivalence Check: ", Style::default().fg(COLOR_TEXT_HEAD)),
                if app.playground_equiv_pass {
                    Span::styled("100% IDENTICAL OUTPUTS (VERIFIED)", Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD))
                } else {
                    Span::styled("OUTPUT MISMATCH DETECTED", Style::default().fg(COLOR_DANGER).add_modifier(Modifier::BOLD))
                },
            ]),
            Line::from(""),
            Line::from(Span::styled("Press [i] Edit  [f] Fullscreen  [C] Convert  [R] Run VM  [L] Load  [X] Reset", Style::default().fg(COLOR_TEXT_MUTED))),
        ]
    };

    let vm_widget = Paragraph::new(vm_lines).block(vm_block);
    frame.render_widget(vm_widget, columns[1]);
}

fn render_playground_footer(frame: &mut Frame, app: &App, area: Rect) {
    let text = if app.playground_fullscreen {
        if app.playground_editing {
            Line::from(vec![
                Span::styled(" [Esc] ", Style::default().fg(COLOR_INDIGO).add_modifier(Modifier::BOLD)),
                Span::styled("Normal Mode (Press 'f' to Exit Fullscreen)  ", Style::default().fg(COLOR_TEXT_BODY)),
                Span::styled(" [Ctrl+V / p] ", Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
                Span::styled("Paste  ", Style::default().fg(COLOR_TEXT_BODY)),
                Span::styled(" [Ctrl+K] ", Style::default().fg(COLOR_WARNING).add_modifier(Modifier::BOLD)),
                Span::styled("Cut Line  ", Style::default().fg(COLOR_TEXT_BODY)),
                Span::styled(" [F11] ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
                Span::styled("Split View  ", Style::default().fg(COLOR_TEXT_BODY)),
                Span::styled(" [Q] ", Style::default().fg(COLOR_TEXT_MUTED).add_modifier(Modifier::BOLD)),
                Span::styled("Quit", Style::default().fg(COLOR_TEXT_MUTED)),
            ])
        } else {
            Line::from(vec![
                Span::styled(" [i] ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
                Span::styled("Edit Buffer  ", Style::default().fg(COLOR_TEXT_BODY)),
                Span::styled(" [f / Esc] ", Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
                Span::styled("Exit Fullscreen (Split View)  ", Style::default().fg(COLOR_TEXT_HEAD)),
                Span::styled(" [C] ", Style::default().fg(COLOR_WARNING).add_modifier(Modifier::BOLD)),
                Span::styled("Convert & Optimize  ", Style::default().fg(COLOR_TEXT_BODY)),
                Span::styled(" [R] ", Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
                Span::styled("Run Dual VM  ", Style::default().fg(COLOR_TEXT_BODY)),
                Span::styled(" [L] ", Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
                Span::styled("Load File  ", Style::default().fg(COLOR_TEXT_BODY)),
                Span::styled(" [X] ", Style::default().fg(COLOR_TEXT_MUTED).add_modifier(Modifier::BOLD)),
                Span::styled("Reset Code  ", Style::default().fg(COLOR_TEXT_BODY)),
                Span::styled(" [Q] ", Style::default().fg(COLOR_TEXT_MUTED).add_modifier(Modifier::BOLD)),
                Span::styled("Quit", Style::default().fg(COLOR_TEXT_MUTED)),
            ])
        }
    } else {
        if app.playground_editing {
            Line::from(vec![
                Span::styled(" [Esc] ", Style::default().fg(COLOR_INDIGO).add_modifier(Modifier::BOLD)),
                Span::styled("Normal Mode (Press 'f' for Fullscreen)  ", Style::default().fg(COLOR_TEXT_BODY)),
                Span::styled(" [Ctrl+V / p] ", Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
                Span::styled("Paste  ", Style::default().fg(COLOR_TEXT_BODY)),
                Span::styled(" [Ctrl+K] ", Style::default().fg(COLOR_WARNING).add_modifier(Modifier::BOLD)),
                Span::styled("Cut Line  ", Style::default().fg(COLOR_TEXT_BODY)),
                Span::styled(" [F11] ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
                Span::styled("Fullscreen  ", Style::default().fg(COLOR_TEXT_BODY)),
                Span::styled(" [Q] ", Style::default().fg(COLOR_TEXT_MUTED).add_modifier(Modifier::BOLD)),
                Span::styled("Quit", Style::default().fg(COLOR_TEXT_MUTED)),
            ])
        } else {
            Line::from(vec![
                Span::styled(" [i] ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
                Span::styled("Edit Buffer  ", Style::default().fg(COLOR_TEXT_BODY)),
                Span::styled(" [f] ", Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
                Span::styled("Fullscreen Editor  ", Style::default().fg(COLOR_TEXT_HEAD)),
                Span::styled(" [C] ", Style::default().fg(COLOR_WARNING).add_modifier(Modifier::BOLD)),
                Span::styled("Convert & Optimize  ", Style::default().fg(COLOR_TEXT_BODY)),
                Span::styled(" [R] ", Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
                Span::styled("Run Dual VM & Verify  ", Style::default().fg(COLOR_TEXT_BODY)),
                Span::styled(" [Esc] ", Style::default().fg(COLOR_INDIGO).add_modifier(Modifier::BOLD)),
                Span::styled("Home  ", Style::default().fg(COLOR_TEXT_BODY)),
                Span::styled(" [L] ", Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
                Span::styled("Load File  ", Style::default().fg(COLOR_TEXT_BODY)),
                Span::styled(" [X] ", Style::default().fg(COLOR_TEXT_MUTED).add_modifier(Modifier::BOLD)),
                Span::styled("Reset Code  ", Style::default().fg(COLOR_TEXT_BODY)),
                Span::styled(" [Q] ", Style::default().fg(COLOR_TEXT_MUTED).add_modifier(Modifier::BOLD)),
                Span::styled("Quit", Style::default().fg(COLOR_TEXT_MUTED)),
            ])
        }
    };

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

fn render_reset_confirm_modal(frame: &mut Frame, area: Rect) {
    let popup_width = 58.min(area.width.saturating_sub(4));
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
        .border_style(Style::default().fg(COLOR_WARNING))
        .style(Style::default().bg(COLOR_PANEL_BG))
        .title(Line::from(vec![
            Span::styled(" ⚠ Confirm Buffer Reset ", Style::default().fg(COLOR_WARNING).add_modifier(Modifier::BOLD)),
        ]));

    let content = vec![
        Line::from(""),
        Line::from(Span::styled(" Are you sure you want to reset the editor buffer?", Style::default().fg(COLOR_TEXT_HEAD).add_modifier(Modifier::BOLD))),
        Line::from(Span::styled(" All current code, optimizations, and VM states will be cleared.", Style::default().fg(COLOR_TEXT_MUTED))),
        Line::from(""),
        Line::from(vec![
            Span::styled(" [Y / Enter] ", Style::default().fg(COLOR_DANGER).add_modifier(Modifier::BOLD)),
            Span::styled("Yes, Reset Buffer   ", Style::default().fg(COLOR_TEXT_BODY)),
            Span::styled(" [N / Esc] ", Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
            Span::styled("Cancel & Keep Code", Style::default().fg(COLOR_TEXT_BODY)),
        ]),
    ];

    let widget = Paragraph::new(content).block(block);
    frame.render_widget(widget, popup_area);
}
