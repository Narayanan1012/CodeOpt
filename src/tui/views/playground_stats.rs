//! Full-screen Playground Program Statistics View (Accessible via 'S' key after execution).

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
};

use crate::parser::parse_program;
use crate::passes::OptimizationKind;
use crate::tui::{
    app::App,
    data::ProgramStats,
    theme::{
        COLOR_BORDER, COLOR_BORDER_ACTIVE, COLOR_CYAN, COLOR_DANGER,
        COLOR_INDIGO, COLOR_PINK, COLOR_SUCCESS, COLOR_TEXT_BODY,
        COLOR_TEXT_HEAD, COLOR_TEXT_MUTED, COLOR_WARNING,
    },
};

pub fn render_playground_stats(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),         // Header block
            Constraint::Length(10),        // Top split: Quantitative Metrics vs. Per-Optimizer Breakdown
            Constraint::Min(8),            // Bottom split: Terminal I/O Console vs. Audit Trail
            Constraint::Length(1),         // Footer bar
        ])
        .split(area);

    render_stats_header(frame, app, chunks[0]);
    render_metrics_and_optimizer_split(frame, app, chunks[1]);
    render_io_and_audit_split(frame, app, chunks[2]);
    render_stats_footer(frame, chunks[3]);
}

fn render_stats_header(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLOR_BORDER_ACTIVE))
        .title(Line::from(vec![
            Span::styled(" CodeOpt Interactive Statistics & Performance Deck ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
            Span::styled(format!("— Source: {} ", app.playground_file_name), Style::default().fg(COLOR_TEXT_HEAD).add_modifier(Modifier::BOLD)),
        ]));

    let equiv_badge = if let Some(err) = &app.playground_vm_error {
        Span::styled(format!(" [⚠ VM ERROR: {err}] "), Style::default().fg(Color::Rgb(15, 23, 42)).bg(COLOR_DANGER).add_modifier(Modifier::BOLD))
    } else if !app.playground_vm_executed {
        Span::styled(" [• AWAITING RUN — PRESS 'R' TO RUN DUAL VM] ", Style::default().fg(Color::Rgb(15, 23, 42)).bg(COLOR_WARNING).add_modifier(Modifier::BOLD))
    } else if app.playground_equiv_pass {
        Span::styled(" [✓ 100% IDENTICAL OUTPUTS (SEMANTICALLY VERIFIED)] ", Style::default().fg(Color::Rgb(15, 23, 42)).bg(COLOR_SUCCESS).add_modifier(Modifier::BOLD))
    } else {
        Span::styled(" [✗ OUTPUT MISMATCH DETECTED] ", Style::default().fg(Color::Rgb(15, 23, 42)).bg(COLOR_DANGER).add_modifier(Modifier::BOLD))
    };

    let header_line = Line::from(vec![
        Span::styled(" Runtime Optimization Deck  ", Style::default().fg(COLOR_TEXT_MUTED)),
        Span::styled(format!("│ Transformations: {} fired ", app.playground_transforms.len()), Style::default().fg(COLOR_TEXT_BODY)),
        Span::styled(format!("│ Buffer: {} lines ", app.playground_buffer.len()), Style::default().fg(COLOR_TEXT_MUTED)),
        Span::styled("│ ", Style::default().fg(COLOR_TEXT_MUTED)),
        equiv_badge,
    ]);

    let widget = Paragraph::new(header_line).block(block);
    frame.render_widget(widget, area);
}

fn render_metrics_and_optimizer_split(frame: &mut Frame, app: &App, area: Rect) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);

    // Left: Quantitative Metrics Comparison Table
    let orig_source = app.playground_buffer.join("\n");
    let opt_source = app.playground_opt_lines.join("\n");
    let orig_stats = parse_program(&orig_source).map(|p| ProgramStats::compute(&p)).unwrap_or(ProgramStats {
        instruction_count: app.playground_buffer.len(),
        arithmetic_count: 0,
        temp_count: 0,
        code_size_bytes: orig_source.len(),
    });
    let opt_stats = parse_program(&opt_source).map(|p| ProgramStats::compute(&p)).unwrap_or(ProgramStats {
        instruction_count: app.playground_opt_lines.len(),
        arithmetic_count: 0,
        temp_count: 0,
        code_size_bytes: opt_source.len(),
    });

    let inst_red = ProgramStats::reduction_pct(orig_stats.instruction_count, opt_stats.instruction_count);
    let arith_red = ProgramStats::reduction_pct(orig_stats.arithmetic_count, opt_stats.arithmetic_count);
    let temp_red = ProgramStats::reduction_pct(orig_stats.temp_count, opt_stats.temp_count);
    let size_red = ProgramStats::reduction_pct(orig_stats.code_size_bytes, opt_stats.code_size_bytes);

    let speedup_pct = if app.playground_orig_steps > 0 {
        ((app.playground_orig_steps as f64 - app.playground_opt_steps as f64) / app.playground_orig_steps as f64) * 100.0
    } else {
        0.0
    };
    let speedup_x = if speedup_pct > 0.0 {
        1.0 / (1.0 - (speedup_pct / 100.0).min(0.99))
    } else {
        1.0
    };

    let metrics_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLOR_CYAN))
        .title(Line::from(vec![
            Span::styled(" Section 5 Quantitative Reduction Metrics ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
        ]));

    let metrics_lines = vec![
        Line::from(vec![
            Span::styled(" • Instruction Count:    ", Style::default().fg(COLOR_TEXT_HEAD).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{} -> {} ", orig_stats.instruction_count, opt_stats.instruction_count), Style::default().fg(COLOR_TEXT_BODY)),
            Span::styled(format!("(-{inst_red:.1}%) "), Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
            Span::styled(if inst_red >= 20.0 { "[PASS - Target 20-50%]" } else { "[In Range]" }, Style::default().fg(COLOR_TEXT_MUTED)),
        ]),
        Line::from(vec![
            Span::styled(" • Arithmetic Operations:", Style::default().fg(COLOR_TEXT_HEAD).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{} -> {} ", orig_stats.arithmetic_count, opt_stats.arithmetic_count), Style::default().fg(COLOR_TEXT_BODY)),
            Span::styled(format!("(-{arith_red:.1}%) "), Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
            Span::styled(if arith_red >= 30.0 { "[PASS - Target 30-60%]" } else { "[In Range]" }, Style::default().fg(COLOR_TEXT_MUTED)),
        ]),
        Line::from(vec![
            Span::styled(" • Temporary Variables:  ", Style::default().fg(COLOR_TEXT_HEAD).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{} -> {} ", orig_stats.temp_count, opt_stats.temp_count), Style::default().fg(COLOR_TEXT_BODY)),
            Span::styled(format!("(-{temp_red:.1}%) "), Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
            Span::styled(if temp_red >= 20.0 { "[PASS - Target 20-40%]" } else { "[In Range]" }, Style::default().fg(COLOR_TEXT_MUTED)),
        ]),
        Line::from(vec![
            Span::styled(" • TAC Code Size:        ", Style::default().fg(COLOR_TEXT_HEAD).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{}B -> {}B ", orig_stats.code_size_bytes, opt_stats.code_size_bytes), Style::default().fg(COLOR_TEXT_BODY)),
            Span::styled(format!("(-{size_red:.1}%) "), Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
            Span::styled(if size_red >= 15.0 { "[PASS - Target 15-40%]" } else { "[In Range]" }, Style::default().fg(COLOR_TEXT_MUTED)),
        ]),
        Line::from(vec![
            Span::styled(" • VM Execution Steps:   ", Style::default().fg(COLOR_TEXT_HEAD).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{} steps -> {} steps ", app.playground_orig_steps, app.playground_opt_steps), Style::default().fg(COLOR_TEXT_BODY)),
            Span::styled(format!("({:+.1}% steps | {:.2}x speedup)", speedup_pct, speedup_x), Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled(" • Interpreter Timing:   ", Style::default().fg(COLOR_TEXT_HEAD).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{}µs -> {}µs (Measured interpreter duration)", app.playground_orig_time_us, app.playground_opt_time_us), Style::default().fg(COLOR_TEXT_MUTED)),
        ]),
    ];

    let metrics_widget = Paragraph::new(metrics_lines).block(metrics_block);
    frame.render_widget(metrics_widget, columns[0]);

    // Right: Per-Optimizer Pass Breakdown
    let opt_breakdown_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLOR_PINK))
        .title(Line::from(vec![
            Span::styled(" Per-Optimizer Pass Contribution ", Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
        ]));

    let mut cf = 0;
    let mut cp = 0;
    let mut as_pass = 0;
    let mut cse = 0;
    let mut sr = 0;
    let mut dce = 0;

    for t in &app.playground_transforms {
        match t.pass {
            OptimizationKind::ConstantFolding => cf += 1,
            OptimizationKind::ConstantPropagation => cp += 1,
            OptimizationKind::AlgebraicSimplification => as_pass += 1,
            OptimizationKind::LocalCommonSubexpressionElimination => cse += 1,
            OptimizationKind::StrengthReduction => sr += 1,
            OptimizationKind::DeadCodeElimination => dce += 1,
        }
    }

    let total = app.playground_transforms.len().max(1) as f64;
    let breakdown_lines = vec![
        Line::from(vec![
            Span::styled(" [SR]  Strength Reduction    : ", Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:>2} rewrites ", sr), Style::default().fg(COLOR_TEXT_HEAD)),
            Span::styled(format!("({:>4.1}%)", (sr as f64 / total) * 100.0), Style::default().fg(COLOR_CYAN)),
        ]),
        Line::from(vec![
            Span::styled(" [CP]  Constant Propagation  : ", Style::default().fg(COLOR_INDIGO).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:>2} rewrites ", cp), Style::default().fg(COLOR_TEXT_HEAD)),
            Span::styled(format!("({:>4.1}%)", (cp as f64 / total) * 100.0), Style::default().fg(COLOR_CYAN)),
        ]),
        Line::from(vec![
            Span::styled(" [CF]  Constant Folding      : ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:>2} rewrites ", cf), Style::default().fg(COLOR_TEXT_HEAD)),
            Span::styled(format!("({:>4.1}%)", (cf as f64 / total) * 100.0), Style::default().fg(COLOR_CYAN)),
        ]),
        Line::from(vec![
            Span::styled(" [AS]  Algebraic Simplification: ", Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:>2} rewrites ", as_pass), Style::default().fg(COLOR_TEXT_HEAD)),
            Span::styled(format!("({:>4.1}%)", (as_pass as f64 / total) * 100.0), Style::default().fg(COLOR_CYAN)),
        ]),
        Line::from(vec![
            Span::styled(" [CSE] Common Subexpression  : ", Style::default().fg(COLOR_WARNING).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:>2} rewrites ", cse), Style::default().fg(COLOR_TEXT_HEAD)),
            Span::styled(format!("({:>4.1}%)", (cse as f64 / total) * 100.0), Style::default().fg(COLOR_CYAN)),
        ]),
        Line::from(vec![
            Span::styled(" [DCE] Dead Code Elimination : ", Style::default().fg(COLOR_DANGER).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:>2} rewrites ", dce), Style::default().fg(COLOR_TEXT_HEAD)),
            Span::styled(format!("({:>4.1}%)", (dce as f64 / total) * 100.0), Style::default().fg(COLOR_CYAN)),
        ]),
    ];

    let opt_widget = Paragraph::new(breakdown_lines).block(opt_breakdown_block);
    frame.render_widget(opt_widget, columns[1]);
}

fn render_io_and_audit_split(frame: &mut Frame, app: &App, area: Rect) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);

    // Left: Terminal I/O & Equivalence Verification
    let io_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLOR_SUCCESS))
        .title(Line::from(vec![
            Span::styled(" Terminal Stdin / Stdout Verification ", Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
        ]));

    let mut io_lines = vec![
        Line::from(vec![
            Span::styled(" User Stdin Inputs : ", Style::default().fg(COLOR_TEXT_HEAD).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:?}", app.playground_inputs), Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
        ]),
    ];

    if let Some(gcc_out) = &app.playground_gcc_out {
        io_lines.push(Line::from(vec![
            Span::styled(" Native GCC Stdout : ", Style::default().fg(COLOR_TEXT_HEAD).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:?}", gcc_out), Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
        ]));
    }

    io_lines.push(Line::from(vec![
        Span::styled(" Original TAC Output: ", Style::default().fg(COLOR_TEXT_HEAD).add_modifier(Modifier::BOLD)),
        Span::styled(format!("{:?}", app.playground_orig_out), Style::default().fg(COLOR_TEXT_BODY)),
    ]));
    io_lines.push(Line::from(vec![
        Span::styled(" Optimized TAC Out : ", Style::default().fg(COLOR_TEXT_HEAD).add_modifier(Modifier::BOLD)),
        Span::styled(format!("{:?}", app.playground_opt_out), Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
    ]));

    io_lines.push(Line::from(""));
    io_lines.push(Line::from(vec![
        Span::styled(" Verification Result: ", Style::default().fg(COLOR_TEXT_HEAD).add_modifier(Modifier::BOLD)),
        if app.playground_equiv_pass {
            Span::styled("PASS — 100% IDENTICAL OUTPUTS", Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD))
        } else {
            Span::styled("FAIL — OUTPUT MISMATCH", Style::default().fg(COLOR_DANGER).add_modifier(Modifier::BOLD))
        },
    ]));

    if let Some(err) = &app.playground_vm_error {
        io_lines.push(Line::from(vec![
            Span::styled(" Error Message      : ", Style::default().fg(COLOR_DANGER).add_modifier(Modifier::BOLD)),
            Span::styled(err, Style::default().fg(COLOR_WARNING)),
        ]));
    }

    let io_widget = Paragraph::new(io_lines).block(io_block);
    frame.render_widget(io_widget, columns[0]);

    // Right: Scrollable Transformation Audit Trail
    let audit_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLOR_BORDER))
        .title(Line::from(vec![
            Span::styled(" Transformation Trail ", Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
            Span::styled(format!("({} passes recorded) ", app.playground_transforms.len()), Style::default().fg(COLOR_TEXT_MUTED)),
        ]));

    let mut audit_lines: Vec<Line> = Vec::new();
    if app.playground_transforms.is_empty() {
        audit_lines.push(Line::from(Span::styled("No transformations needed for this code.", Style::default().fg(COLOR_TEXT_MUTED))));
    } else {
        for (idx, change) in app.playground_transforms.iter().enumerate().skip(app.playground_stats_scroll) {
            let pass_name = format!("{:?}", change.pass);
            audit_lines.push(Line::from(vec![
                Span::styled(format!(" #{:02} [{pass_name}] ", idx + 1), Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
                Span::styled(format!("Inst #{} ", change.instruction_id), Style::default().fg(COLOR_TEXT_MUTED)),
            ]));
            audit_lines.push(Line::from(vec![
                Span::styled("     Before: ", Style::default().fg(COLOR_DANGER)),
                Span::styled(change.before.to_string(), Style::default().fg(COLOR_TEXT_BODY)),
            ]));
            audit_lines.push(Line::from(vec![
                Span::styled("     After : ", Style::default().fg(COLOR_SUCCESS)),
                Span::styled(change.after.to_string(), Style::default().fg(COLOR_TEXT_HEAD).add_modifier(Modifier::BOLD)),
            ]));
            audit_lines.push(Line::from(""));
        }
    }

    let audit_widget = Paragraph::new(audit_lines).block(audit_block).wrap(Wrap { trim: false });
    frame.render_widget(audit_widget, columns[1]);
}

fn render_stats_footer(frame: &mut Frame, area: Rect) {
    let text = Line::from(vec![
        Span::styled(" [S / Esc] ", Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
        Span::styled("Return to Editor  │  ", Style::default().fg(COLOR_TEXT_HEAD)),
        Span::styled(" [j / Down] ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
        Span::styled("Scroll Audit Down  │  ", Style::default().fg(COLOR_TEXT_BODY)),
        Span::styled(" [k / Up] ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
        Span::styled("Scroll Audit Up  │  ", Style::default().fg(COLOR_TEXT_BODY)),
        Span::styled(" [Q] ", Style::default().fg(COLOR_TEXT_MUTED).add_modifier(Modifier::BOLD)),
        Span::styled("Quit", Style::default().fg(COLOR_TEXT_MUTED)),
    ]);

    let widget = Paragraph::new(text).alignment(Alignment::Center);
    frame.render_widget(widget, area);
}
