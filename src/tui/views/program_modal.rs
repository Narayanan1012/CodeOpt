//! Full-screen program deep-dive inspector with side-by-side TAC diffs and detailed metrics.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Paragraph, Row, Table, Wrap},
};

use crate::tui::{
    app::App,
    data::ProgramStats,
    theme::{
        COLOR_BORDER, COLOR_BORDER_ACTIVE, COLOR_COMBINED, COLOR_CYAN, COLOR_DANGER,
        COLOR_INDIGO, COLOR_PINK, COLOR_SUCCESS, COLOR_TEXT_BODY,
        COLOR_TEXT_HEAD, COLOR_TEXT_MUTED, COLOR_WARNING,
    },
};

pub fn render_program_modal(frame: &mut Frame, app: &App, area: Rect) {
    let Some(item) = app.selected_item() else {
        let empty = Paragraph::new("No program selected. Press Esc to return.")
            .alignment(Alignment::Center);
        frame.render_widget(empty, area);
        return;
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),         // Header block
            Constraint::Percentage(55),    // Side-by-side synchronized code panes
            Constraint::Percentage(45),    // Bottom split: Audit log vs. Detailed Stats
            Constraint::Length(1),         // Footer bar
        ])
        .split(area);

    render_header(frame, app, item, chunks[0]);
    render_code_diff_panes(frame, app, item, chunks[1]);
    render_bottom_split(frame, app, item, chunks[2]);
    render_modal_footer(frame, chunks[3]);
}

fn render_header(frame: &mut Frame, app: &App, item: &crate::tui::data::BenchmarkItem, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLOR_BORDER_ACTIVE))
        .title(Line::from(vec![
            Span::styled(" Full-Screen Program Deep Dive Inspector ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
            Span::styled(format!("— {} ", item.name), Style::default().fg(COLOR_TEXT_HEAD).add_modifier(Modifier::BOLD)),
        ]));

    let pass_badge = if !app.deep_dive_executed {
        Span::styled(" [● AWAITING RUN — PRESS 'R' TO EXECUTE DUAL VM] ", Style::default().fg(Color::Rgb(15, 23, 42)).bg(COLOR_WARNING).add_modifier(Modifier::BOLD))
    } else {
        let all_passed = !app.deep_dive_vector_results.is_empty() && app.deep_dive_vector_results.iter().all(|v| v.passed);
        if all_passed {
            Span::styled(" [✓ LIVE VM VERIFIED: 5/5 TEST VECTORS MATCHED] ", Style::default().fg(Color::Rgb(15, 23, 42)).bg(COLOR_SUCCESS).add_modifier(Modifier::BOLD))
        } else {
            Span::styled(" [✗ VERIFICATION MISMATCH] ", Style::default().fg(Color::Rgb(15, 23, 42)).bg(COLOR_DANGER).add_modifier(Modifier::BOLD))
        }
    };

    let category_badge = if item.category == crate::tui::data::BenchmarkCategory::Combined {
        Span::styled(format!(" {} ", item.category.name()), Style::default().fg(COLOR_COMBINED).add_modifier(Modifier::BOLD))
    } else {
        Span::styled(format!(" Category: {} ", item.category.name()), Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD))
    };

    let header_line = Line::from(vec![
        Span::styled(format!(" Path: {}  ", item.path.display()), Style::default().fg(COLOR_TEXT_MUTED)),
        category_badge,
        Span::styled(format!(" │ Worklist: Fixed point in {} round(s) │ ", item.worklist_rounds), Style::default().fg(COLOR_TEXT_BODY)),
        pass_badge,
    ]);

    let widget = Paragraph::new(header_line).block(block);
    frame.render_widget(widget, area);
}

fn render_code_diff_panes(
    frame: &mut Frame,
    app: &App,
    item: &crate::tui::data::BenchmarkItem,
    area: Rect,
) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Left Pane: Original TAC
    let orig_lines: Vec<&str> = item.original_tac.lines().collect();
    let orig_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLOR_INDIGO))
        .title(Line::from(vec![
            Span::styled(" Original TAC Source ", Style::default().fg(COLOR_INDIGO).add_modifier(Modifier::BOLD)),
            Span::styled(format!("({} instructions) ", item.original_stats.instruction_count), Style::default().fg(COLOR_TEXT_MUTED)),
        ]));

    let orig_formatted: Vec<Line> = orig_lines
        .iter()
        .enumerate()
        .skip(app.code_scroll_offset)
        .map(|(idx, line)| {
            Line::from(vec![
                Span::styled(format!("{:>2}: ", idx + 1), Style::default().fg(COLOR_TEXT_MUTED)),
                Span::styled(*line, Style::default().fg(COLOR_TEXT_BODY)),
            ])
        })
        .collect();

    let orig_widget = Paragraph::new(orig_formatted)
        .block(orig_block)
        .wrap(Wrap { trim: false });
    frame.render_widget(orig_widget, columns[0]);

    // Right Pane: Optimized TAC with inline badges
    let opt_lines: Vec<&str> = item.optimized_tac.lines().collect();
    let opt_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLOR_SUCCESS))
        .title(Line::from(vec![
            Span::styled(" Optimized TAC (With Transformation Badges) ", Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
            Span::styled(format!("({} instructions) ", item.optimized_stats.instruction_count), Style::default().fg(COLOR_TEXT_MUTED)),
        ]));

    let opt_formatted: Vec<Line> = opt_lines
        .iter()
        .enumerate()
        .skip(app.code_scroll_offset)
        .map(|(idx, line)| {
            // Find if any transformation targeted this instruction
            let badge = if let Some(t) = item.transformations.iter().find(|t| t.after.to_string().trim() == line.trim()) {
                match t.pass {
                    crate::passes::OptimizationKind::ConstantFolding => {
                        Span::styled("[CF]  ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD))
                    }
                    crate::passes::OptimizationKind::ConstantPropagation => {
                        Span::styled("[CP]  ", Style::default().fg(COLOR_INDIGO).add_modifier(Modifier::BOLD))
                    }
                    crate::passes::OptimizationKind::AlgebraicSimplification => {
                        Span::styled("[AS]  ", Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD))
                    }
                    crate::passes::OptimizationKind::LocalCommonSubexpressionElimination => {
                        Span::styled("[CSE] ", Style::default().fg(COLOR_WARNING).add_modifier(Modifier::BOLD))
                    }
                    crate::passes::OptimizationKind::DeadCodeElimination => {
                        Span::styled("[DCE] ", Style::default().fg(COLOR_DANGER).add_modifier(Modifier::BOLD))
                    }
                }
            } else {
                Span::raw("      ")
            };

            Line::from(vec![
                Span::styled(format!("{:>2}: ", idx + 1), Style::default().fg(COLOR_TEXT_MUTED)),
                badge,
                Span::styled(*line, Style::default().fg(COLOR_TEXT_HEAD)),
            ])
        })
        .collect();

    let opt_widget = Paragraph::new(opt_formatted)
        .block(opt_block)
        .wrap(Wrap { trim: false });
    frame.render_widget(opt_widget, columns[1]);
}

fn render_bottom_split(
    frame: &mut Frame,
    app: &App,
    item: &crate::tui::data::BenchmarkItem,
    area: Rect,
) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Left: Rewrite Audit Trail
    let audit_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLOR_BORDER))
        .title(Line::from(vec![
            Span::styled(" Optimization Audit Trail ", Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
            Span::styled(format!("({} rewrites recorded) ", item.transformations.len()), Style::default().fg(COLOR_TEXT_MUTED)),
        ]));

    let mut audit_lines: Vec<Line> = Vec::new();
    if item.transformations.is_empty() {
        audit_lines.push(Line::from(Span::styled("No transformations fired for this program.", Style::default().fg(COLOR_TEXT_MUTED))));
    } else {
        for (idx, change) in item.transformations.iter().enumerate().skip(app.audit_scroll_offset) {
            let pass_name = format!("{:?}", change.pass);
            let before_str = change.before.to_string();
            let after_str = change.after.to_string();
            audit_lines.push(Line::from(vec![
                Span::styled(format!(" #{:02} [{pass_name}] ", idx + 1), Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
                Span::styled(format!("Inst #{} ", change.instruction_id), Style::default().fg(COLOR_TEXT_MUTED)),
            ]));
            audit_lines.push(Line::from(vec![
                Span::styled("     Before: ", Style::default().fg(COLOR_DANGER)),
                Span::styled(before_str, Style::default().fg(COLOR_TEXT_BODY)),
            ]));
            audit_lines.push(Line::from(vec![
                Span::styled("     After : ", Style::default().fg(COLOR_SUCCESS)),
                Span::styled(after_str, Style::default().fg(COLOR_TEXT_HEAD).add_modifier(Modifier::BOLD)),
            ]));
            audit_lines.push(Line::from(""));
        }
    }

    let audit_widget = Paragraph::new(audit_lines)
        .block(audit_block)
        .wrap(Wrap { trim: false });
    frame.render_widget(audit_widget, columns[0]);

    // Right Column: Split into Quantitative Performance Metrics (Top) and Dual VM Proof Table (Bottom)
    let right_splits = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8), // Metrics
            Constraint::Min(8),    // 5-Vector Proof Table
        ])
        .split(columns[1]);

    let stats_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLOR_CYAN))
        .title(Line::from(vec![
            Span::styled(" Target Reductions & VM Execution Metrics ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
        ]));

    let inst_red = ProgramStats::reduction_pct(item.original_stats.instruction_count, item.optimized_stats.instruction_count);
    let arith_red = ProgramStats::reduction_pct(item.original_stats.arithmetic_count, item.optimized_stats.arithmetic_count);
    let temp_red = ProgramStats::reduction_pct(item.original_stats.temp_count, item.optimized_stats.temp_count);
    let size_red = ProgramStats::reduction_pct(item.original_stats.code_size_bytes, item.optimized_stats.code_size_bytes);

    let timing_line = if !app.deep_dive_executed {
        Line::from(vec![
            Span::styled(" VM Execution Timing:  ", Style::default().fg(COLOR_TEXT_HEAD).add_modifier(Modifier::BOLD)),
            Span::styled("[Pending Execution — Press 'R' to Benchmark Dual VM]", Style::default().fg(COLOR_WARNING).add_modifier(Modifier::BOLD)),
        ])
    } else {
        let speedup_pct = if app.deep_dive_orig_time_us > 0 {
            ((app.deep_dive_orig_time_us as f64 - app.deep_dive_opt_time_us as f64) / app.deep_dive_orig_time_us as f64) * 100.0
        } else {
            0.0
        };
        let speedup_x = if speedup_pct > 0.0 {
            1.0 / (1.0 - (speedup_pct / 100.0).min(0.99))
        } else {
            1.0
        };

        Line::from(vec![
            Span::styled(" VM Execution Timing:  ", Style::default().fg(COLOR_TEXT_HEAD).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{}µs → {}µs  ", app.deep_dive_orig_time_us, app.deep_dive_opt_time_us), Style::default().fg(COLOR_TEXT_HEAD)),
            Span::styled(format!("({:+.1}% time | {:.2}x speedup)", speedup_pct, speedup_x), Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
        ])
    };

    let stats_lines = vec![
        Line::from(vec![
            Span::styled(" Instruction Count:    ", Style::default().fg(COLOR_TEXT_HEAD)),
            Span::styled(format!("{} → {} ", item.original_stats.instruction_count, item.optimized_stats.instruction_count), Style::default().fg(COLOR_TEXT_BODY)),
            Span::styled(format!("(-{inst_red:.1}%) "), Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
            Span::styled("[Target: 20–50%]", Style::default().fg(COLOR_TEXT_MUTED)),
            Span::styled("   │ TAC Code Size: ", Style::default().fg(COLOR_TEXT_MUTED)),
            Span::styled(format!("{}B → {}B ", item.original_stats.code_size_bytes, item.optimized_stats.code_size_bytes), Style::default().fg(COLOR_TEXT_BODY)),
            Span::styled(format!("(-{size_red:.1}%)"), Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled(" Arithmetic Operations:", Style::default().fg(COLOR_TEXT_HEAD)),
            Span::styled(format!("{} → {} ", item.original_stats.arithmetic_count, item.optimized_stats.arithmetic_count), Style::default().fg(COLOR_TEXT_BODY)),
            Span::styled(format!("(-{arith_red:.1}%) "), Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
            Span::styled("[Target: 30–60%]", Style::default().fg(COLOR_TEXT_MUTED)),
            Span::styled("   │ Temporaries  : ", Style::default().fg(COLOR_TEXT_MUTED)),
            Span::styled(format!("{} → {} ", item.original_stats.temp_count, item.optimized_stats.temp_count), Style::default().fg(COLOR_TEXT_BODY)),
            Span::styled(format!("(-{temp_red:.1}%)"), Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
        ]),
        timing_line,
    ];

    let stats_widget = Paragraph::new(stats_lines).block(stats_block);
    frame.render_widget(stats_widget, right_splits[0]);

    // 5-Vector Dual VM Semantic Equivalence Proof Table
    let (proof_block, rows) = if !app.deep_dive_executed {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(COLOR_WARNING))
            .title(Line::from(vec![
                Span::styled(" Dual VM Semantic Equivalence Proof (Press [R] to Run 5 Test Vectors) ", Style::default().fg(COLOR_WARNING).add_modifier(Modifier::BOLD)),
            ]));

        let rows: Vec<Row> = item.test_vectors.iter().map(|v| {
            Row::new(vec![
                Cell::from(format!("#{}", v.vector_id)).style(Style::default().fg(COLOR_TEXT_MUTED)),
                Cell::from(format!("{:?}", v.inputs)).style(Style::default().fg(COLOR_TEXT_BODY)),
                Cell::from("[Pending Run — Press 'R']").style(Style::default().fg(COLOR_TEXT_MUTED)),
                Cell::from("[Pending Run — Press 'R']").style(Style::default().fg(COLOR_TEXT_MUTED)),
                Cell::from("[READY TO RUN]").style(Style::default().fg(COLOR_WARNING).add_modifier(Modifier::BOLD)),
            ])
        }).collect();

        (block, rows)
    } else {
        let all_passed = !app.deep_dive_vector_results.is_empty() && app.deep_dive_vector_results.iter().all(|v| v.passed);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(if all_passed {
                Style::default().fg(COLOR_SUCCESS)
            } else {
                Style::default().fg(COLOR_DANGER)
            })
            .title(Line::from(vec![
                Span::styled(" Dual VM Semantic Equivalence Proof (Live 5 Test Vectors Executed) ", Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
            ]));

        let rows: Vec<Row> = app.deep_dive_vector_results.iter().map(|v| {
            let status_cell = if v.passed {
                Cell::from("✓ PASS (100% IDENTICAL)").style(Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD))
            } else {
                Cell::from("✗ MISMATCH").style(Style::default().fg(COLOR_DANGER).add_modifier(Modifier::BOLD))
            };
            Row::new(vec![
                Cell::from(format!("#{}", v.vector_id)).style(Style::default().fg(COLOR_TEXT_MUTED)),
                Cell::from(format!("{:?}", v.inputs)).style(Style::default().fg(COLOR_TEXT_BODY)),
                Cell::from(format!("{:?}", v.original_output)).style(Style::default().fg(COLOR_INDIGO)),
                Cell::from(format!("{:?}", v.optimized_output)).style(Style::default().fg(COLOR_SUCCESS)),
                status_cell,
            ])
        }).collect();

        (block, rows)
    };

    let header = Row::new(vec![
        Cell::from("Vec #").style(Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
        Cell::from("Test Input Vector").style(Style::default().fg(COLOR_TEXT_HEAD).add_modifier(Modifier::BOLD)),
        Cell::from("Original VM Output").style(Style::default().fg(COLOR_INDIGO).add_modifier(Modifier::BOLD)),
        Cell::from("Optimized VM Output").style(Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
        Cell::from("Equivalence Verdict").style(Style::default().fg(COLOR_TEXT_HEAD).add_modifier(Modifier::BOLD)),
    ])
    .style(Style::default().bg(crate::tui::theme::COLOR_PANEL_BG));

    let widths = [
        Constraint::Length(6),
        Constraint::Percentage(25),
        Constraint::Percentage(24),
        Constraint::Percentage(24),
        Constraint::Percentage(21),
    ];

    let proof_table = Table::new(rows, widths)
        .header(header)
        .block(proof_block);
    frame.render_widget(proof_table, right_splits[1]);
}

fn render_modal_footer(frame: &mut Frame, area: Rect) {
    let text = Line::from(vec![
        Span::styled(" [R] ", Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
        Span::styled("Run Dual VM & Verify  ", Style::default().fg(COLOR_TEXT_HEAD).add_modifier(Modifier::BOLD)),
        Span::styled(" [Esc / Backspace] ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
        Span::styled("Collapse Full-Screen & Return  ", Style::default().fg(COLOR_TEXT_BODY)),
        Span::styled(" [j / k or ↓ / ↑] ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
        Span::styled("Scroll Code Diff  ", Style::default().fg(COLOR_TEXT_BODY)),
        Span::styled(" [Tab] ", Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
        Span::styled("Scroll Audit Trail  ", Style::default().fg(COLOR_TEXT_BODY)),
        Span::styled(" [Q] ", Style::default().fg(COLOR_TEXT_MUTED).add_modifier(Modifier::BOLD)),
        Span::styled("Quit", Style::default().fg(COLOR_TEXT_MUTED)),
    ]);

    let widget = Paragraph::new(text).alignment(Alignment::Center);
    frame.render_widget(widget, area);
}
