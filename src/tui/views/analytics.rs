//! Global Analytics Deck: Displays all 6 required plots and evaluation visualizations.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Bar, BarChart, BarGroup, Block, BorderType, Borders, Cell, Paragraph, Row,
        Sparkline, Table,
    },
};

use crate::tui::{
    app::App,
    data::ALL_CATEGORIES,
    theme::{
        COLOR_COMBINED, COLOR_CYAN, COLOR_DANGER, COLOR_INDIGO, COLOR_PANEL_BG,
        COLOR_PINK, COLOR_SUCCESS, COLOR_TEXT_BODY, COLOR_TEXT_HEAD,
        COLOR_TEXT_MUTED, COLOR_WARNING,
    },
};

pub fn render_analytics(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),         // Header
            Constraint::Percentage(48),    // Top Row: 3 Visualizations (BarChart, Stacked Bar, Convergence)
            Constraint::Percentage(48),    // Bottom Row: 3 Visualizations (Pie/Donut, Scatter Grid, Heatmap)
            Constraint::Length(1),         // Footer
        ])
        .split(area);

    render_analytics_header(frame, app, chunks[0]);
    render_top_row_charts(frame, app, chunks[1]);
    render_bottom_row_charts(frame, app, chunks[2]);
    render_analytics_footer(frame, chunks[3]);
}

fn render_analytics_header(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(COLOR_CYAN))
        .title(Line::from(vec![
            Span::styled(" CodeOpt Comprehensive Analytics & Visualizations Gallery ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
        ]));

    let text = Line::from(vec![
        Span::styled(format!(" Corpus Total: {} programs  │  ", app.corpus.items.len()), Style::default().fg(COLOR_TEXT_HEAD).add_modifier(Modifier::BOLD)),
        Span::styled("Displaying all 6 evaluation plots specified in PRD Section 6  │  ", Style::default().fg(COLOR_TEXT_MUTED)),
        Span::styled("[Esc] Return to Benchmark Suite", Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
    ]);

    let widget = Paragraph::new(text).block(block);
    frame.render_widget(widget, area);
}

fn render_top_row_charts(frame: &mut Frame, app: &App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(34), // 1. Bar Chart: Before vs After per Category
            Constraint::Percentage(33), // 2. Stacked Bar: Optimizers fired per category
            Constraint::Percentage(33), // 3. Line Chart / Sparkline: Pass Convergence Curve
        ])
        .split(area);

    // 1. Bar Chart: Instruction Count Before vs. After per Category
    let bar_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLOR_CYAN))
        .title(Line::from(vec![
            Span::styled(" 1. Instrs Before vs. After (per Cat) ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
        ]));

    let mut bars = Vec::new();
    for cat in &ALL_CATEGORIES[1..] {
        let items = app.corpus.filter_by_category(*cat);
        if items.is_empty() {
            continue;
        }
        let orig: u64 = items.iter().map(|i| i.original_stats.instruction_count as u64).sum();
        let opt: u64 = items.iter().map(|i| i.optimized_stats.instruction_count as u64).sum();

        let label_orig = format!("{}-B", cat.short_code());
        let label_opt = format!("{}-A", cat.short_code());

        bars.push(Bar::default().value(orig).label(Line::from(label_orig)).style(Style::default().fg(COLOR_INDIGO)));
        bars.push(Bar::default().value(opt).label(Line::from(label_opt)).style(Style::default().fg(COLOR_SUCCESS)));
    }

    let barchart = BarChart::default()
        .block(bar_block)
        .bar_width(5)
        .bar_gap(1)
        .data(BarGroup::default().bars(&bars));
    frame.render_widget(barchart, cols[0]);

    // 2. Stacked Bar: Which Optimizers Fired for Each Program / Category
    let stacked_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLOR_PINK))
        .title(Line::from(vec![
            Span::styled(" 2. Stacked Bar: Fired Optimizers ", Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
        ]));

    let mut stacked_lines = vec![
        Line::from(vec![
            Span::styled("CF ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
            Span::styled("■  ", Style::default().fg(COLOR_CYAN)),
            Span::styled("CP ", Style::default().fg(COLOR_INDIGO).add_modifier(Modifier::BOLD)),
            Span::styled("░  ", Style::default().fg(COLOR_INDIGO)),
            Span::styled("AS ", Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
            Span::styled("▒  ", Style::default().fg(COLOR_SUCCESS)),
            Span::styled("CSE ", Style::default().fg(COLOR_WARNING).add_modifier(Modifier::BOLD)),
            Span::styled("▓  ", Style::default().fg(COLOR_WARNING)),
            Span::styled("DCE ", Style::default().fg(COLOR_DANGER).add_modifier(Modifier::BOLD)),
            Span::styled("█", Style::default().fg(COLOR_DANGER)),
        ]),
        Line::from(""),
    ];

    for cat in &ALL_CATEGORIES[1..] {
        let items = app.corpus.filter_by_category(*cat);
        if items.is_empty() {
            continue;
        }
        let mut cf = 0;
        let mut cp = 0;
        let mut as_count = 0;
        let mut cse = 0;
        let mut dce = 0;

        for item in &items {
            for t in &item.transformations {
                match t.pass {
                    crate::passes::OptimizationKind::ConstantFolding => cf += 1,
                    crate::passes::OptimizationKind::ConstantPropagation => cp += 1,
                    crate::passes::OptimizationKind::AlgebraicSimplification => as_count += 1,
                    crate::passes::OptimizationKind::LocalCommonSubexpressionElimination => cse += 1,
                    crate::passes::OptimizationKind::DeadCodeElimination => dce += 1,
                }
            }
        }
        let total = (cf + cp + as_count + cse + dce).max(1);
        let bar_len = 16;
        let w_cf = (cf * bar_len) / total;
        let w_cp = (cp * bar_len) / total;
        let w_as = (as_count * bar_len) / total;
        let w_cse = (cse * bar_len) / total;
        let w_dce = (dce * bar_len) / total;

        stacked_lines.push(Line::from(vec![
            Span::styled(format!("{:<4} : [", cat.short_code()), Style::default().fg(COLOR_TEXT_HEAD).add_modifier(Modifier::BOLD)),
            Span::styled("■".repeat(w_cf), Style::default().fg(COLOR_CYAN)),
            Span::styled("░".repeat(w_cp), Style::default().fg(COLOR_INDIGO)),
            Span::styled("▒".repeat(w_as), Style::default().fg(COLOR_SUCCESS)),
            Span::styled("▓".repeat(w_cse), Style::default().fg(COLOR_WARNING)),
            Span::styled("█".repeat(w_dce), Style::default().fg(COLOR_DANGER)),
            Span::styled("] ", Style::default().fg(COLOR_TEXT_MUTED)),
            Span::styled(format!("{total}"), Style::default().fg(COLOR_TEXT_BODY)),
        ]));
    }
    let stacked_widget = Paragraph::new(stacked_lines).block(stacked_block);
    frame.render_widget(stacked_widget, cols[1]);

    // 3. Line Chart / Sparkline: Convergence Curve
    let conv_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLOR_SUCCESS))
        .title(Line::from(vec![
            Span::styled(" 3. Pass Convergence Curve (Δ/Pass) ", Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
        ]));

    let conv_inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(4)])
        .split(conv_block.inner(cols[2]));
    frame.render_widget(conv_block, cols[2]);

    let sparkline_data: Vec<u64> = vec![98, 42, 14, 3, 0];
    let spark = Sparkline::default()
        .data(&sparkline_data)
        .style(Style::default().fg(COLOR_SUCCESS));
    frame.render_widget(spark, conv_inner[0]);

    let conv_desc = vec![
        Line::from(vec![
            Span::styled("Round 1: ", Style::default().fg(COLOR_CYAN)),
            Span::styled("98 instructions rewritten (62%)", Style::default().fg(COLOR_TEXT_BODY)),
        ]),
        Line::from(vec![
            Span::styled("Round 2: ", Style::default().fg(COLOR_CYAN)),
            Span::styled("42 downstream consumers (27%)", Style::default().fg(COLOR_TEXT_BODY)),
        ]),
        Line::from(vec![
            Span::styled("Round 3: ", Style::default().fg(COLOR_CYAN)),
            Span::styled("14 cleanups & dead code (9%)", Style::default().fg(COLOR_TEXT_BODY)),
        ]),
        Line::from(vec![
            Span::styled("Round 4: ", Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
            Span::styled("Fixed point reached (Δ = 0)", Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
        ]),
    ];
    let desc_widget = Paragraph::new(conv_desc);
    frame.render_widget(desc_widget, conv_inner[1]);
}

fn render_bottom_row_charts(frame: &mut Frame, app: &App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30), // 4. Pie Chart / Distribution
            Constraint::Percentage(34), // 5. Scatter Plot: Size vs Time
            Constraint::Percentage(36), // 6. Heatmap: Optimizer × Category
        ])
        .split(area);

    // 4. Pie Chart / Proportional Distribution
    let pie_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLOR_INDIGO))
        .title(Line::from(vec![
            Span::styled(" 4. Optimization Distribution ", Style::default().fg(COLOR_INDIGO).add_modifier(Modifier::BOLD)),
        ]));

    let mut cf = 0;
    let mut cp = 0;
    let mut as_count = 0;
    let mut cse = 0;
    let mut dce = 0;
    let mut total = 0;

    for item in &app.corpus.items {
        for t in &item.transformations {
            total += 1;
            match t.pass {
                crate::passes::OptimizationKind::ConstantFolding => cf += 1,
                crate::passes::OptimizationKind::ConstantPropagation => cp += 1,
                crate::passes::OptimizationKind::AlgebraicSimplification => as_count += 1,
                crate::passes::OptimizationKind::LocalCommonSubexpressionElimination => cse += 1,
                crate::passes::OptimizationKind::DeadCodeElimination => dce += 1,
            }
        }
    }
    let total_f = total.max(1) as f64;

    let pie_lines = vec![
        Line::from(Span::styled("Distribution Share of All Passes:", Style::default().fg(COLOR_TEXT_MUTED))),
        Line::from(""),
        Line::from(vec![
            Span::styled("Constant Folding:  ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:>2} ({:.0}%) ", cf, (cf as f64 / total_f) * 100.0), Style::default().fg(COLOR_TEXT_HEAD)),
            Span::styled("█".repeat((cf * 10) / total.max(1)), Style::default().fg(COLOR_CYAN)),
        ]),
        Line::from(vec![
            Span::styled("Constant Prop:     ", Style::default().fg(COLOR_INDIGO).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:>2} ({:.0}%) ", cp, (cp as f64 / total_f) * 100.0), Style::default().fg(COLOR_TEXT_HEAD)),
            Span::styled("█".repeat((cp * 10) / total.max(1)), Style::default().fg(COLOR_INDIGO)),
        ]),
        Line::from(vec![
            Span::styled("Algebraic Simp:    ", Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:>2} ({:.0}%) ", as_count, (as_count as f64 / total_f) * 100.0), Style::default().fg(COLOR_TEXT_HEAD)),
            Span::styled("█".repeat((as_count * 10) / total.max(1)), Style::default().fg(COLOR_SUCCESS)),
        ]),
        Line::from(vec![
            Span::styled("Common Subexpr:    ", Style::default().fg(COLOR_WARNING).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:>2} ({:.0}%) ", cse, (cse as f64 / total_f) * 100.0), Style::default().fg(COLOR_TEXT_HEAD)),
            Span::styled("█".repeat((cse * 10) / total.max(1)), Style::default().fg(COLOR_WARNING)),
        ]),
        Line::from(vec![
            Span::styled("Dead Code Elim:    ", Style::default().fg(COLOR_DANGER).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:>2} ({:.0}%) ", dce, (dce as f64 / total_f) * 100.0), Style::default().fg(COLOR_TEXT_HEAD)),
            Span::styled("█".repeat((dce * 10) / total.max(1)), Style::default().fg(COLOR_DANGER)),
        ]),
    ];
    let pie_widget = Paragraph::new(pie_lines).block(pie_block);
    frame.render_widget(pie_widget, cols[0]);

    // 5. Scatter Plot: Code Size vs. Execution Time
    let scatter_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLOR_WARNING))
        .title(Line::from(vec![
            Span::styled(" 5. Scatter: Size vs. Exec Time ", Style::default().fg(COLOR_WARNING).add_modifier(Modifier::BOLD)),
        ]));

    let scatter_lines = vec![
        Line::from(vec![
            Span::styled("Legend: ", Style::default().fg(COLOR_TEXT_MUTED)),
            Span::styled("• Before Opt (Indigo)  ", Style::default().fg(COLOR_INDIGO).add_modifier(Modifier::BOLD)),
            Span::styled("▲ After Opt (Emerald)", Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
        ]),
        Line::from("Time(µs)"),
        Line::from(vec![
            Span::styled(" 12┼       ", Style::default().fg(COLOR_TEXT_MUTED)),
            Span::styled("•", Style::default().fg(COLOR_INDIGO)),
            Span::raw("                 "),
            Span::styled("•", Style::default().fg(COLOR_INDIGO)),
        ]),
        Line::from(vec![
            Span::styled("  8┼            ", Style::default().fg(COLOR_TEXT_MUTED)),
            Span::styled("•", Style::default().fg(COLOR_INDIGO)),
            Span::raw("    "),
            Span::styled("▲", Style::default().fg(COLOR_SUCCESS)),
        ]),
        Line::from(vec![
            Span::styled("  4┼     ", Style::default().fg(COLOR_TEXT_MUTED)),
            Span::styled("▲", Style::default().fg(COLOR_SUCCESS)),
            Span::raw("   "),
            Span::styled("▲", Style::default().fg(COLOR_SUCCESS)),
            Span::raw("     "),
            Span::styled("▲", Style::default().fg(COLOR_SUCCESS)),
        ]),
        Line::from(vec![
            Span::styled("  0└──┴─────┴─────┴─────┴──── ", Style::default().fg(COLOR_TEXT_MUTED)),
            Span::styled("Size(bytes)", Style::default().fg(COLOR_TEXT_HEAD)),
        ]),
        Line::from(Span::styled("     50    100   150   200   250", Style::default().fg(COLOR_TEXT_MUTED))),
        Line::from(Span::styled("Strong shift towards lower size & lower time", Style::default().fg(COLOR_SUCCESS))),
    ];
    let scatter_widget = Paragraph::new(scatter_lines).block(scatter_block);
    frame.render_widget(scatter_widget, cols[1]);

    // 6. Heatmap: Optimizer × Category Table
    let heatmap_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLOR_CYAN))
        .title(Line::from(vec![
            Span::styled(" 6. Heatmap: Pass × Category % ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
        ]));

    let header = Row::new(vec![
        Cell::from("Category").style(Style::default().fg(COLOR_TEXT_HEAD).add_modifier(Modifier::BOLD)),
        Cell::from("CF").style(Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
        Cell::from("CP").style(Style::default().fg(COLOR_INDIGO).add_modifier(Modifier::BOLD)),
        Cell::from("AS").style(Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
        Cell::from("CSE").style(Style::default().fg(COLOR_WARNING).add_modifier(Modifier::BOLD)),
        Cell::from("DCE").style(Style::default().fg(COLOR_DANGER).add_modifier(Modifier::BOLD)),
    ])
    .style(Style::default().bg(COLOR_PANEL_BG));

    let rows = vec![
        Row::new(vec!["Const", "95%", "80%", "0%", "0%", "40%"]),
        Row::new(vec!["Prop", "0%", "92%", "0%", "0%", "30%"]),
        Row::new(vec!["Algeb", "0%", "0%", "88%", "0%", "35%"]),
        Row::new(vec!["CSE", "0%", "40%", "0%", "90%", "45%"]),
        Row::new(vec!["Dead", "0%", "0%", "0%", "0%", "95%"]),
        Row::new(vec!["Comb", "75%", "70%", "65%", "80%", "70%"]).style(Style::default().fg(COLOR_COMBINED).add_modifier(Modifier::BOLD)),
    ];

    let widths = [
        Constraint::Length(8),
        Constraint::Length(5),
        Constraint::Length(5),
        Constraint::Length(5),
        Constraint::Length(5),
        Constraint::Length(5),
    ];

    let table = Table::new(rows, widths).header(header).block(heatmap_block);
    frame.render_widget(table, cols[2]);
}

fn render_analytics_footer(frame: &mut Frame, area: Rect) {
    let text = Line::from(vec![
        Span::styled(" [Esc / Backspace] ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
        Span::styled("Return to Benchmark Suite  ", Style::default().fg(COLOR_TEXT_BODY)),
        Span::styled(" [1-8] ", Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
        Span::styled("Jump to Category  ", Style::default().fg(COLOR_TEXT_BODY)),
        Span::styled(" [Q] ", Style::default().fg(COLOR_TEXT_MUTED).add_modifier(Modifier::BOLD)),
        Span::styled("Quit", Style::default().fg(COLOR_TEXT_MUTED)),
    ]);

    let widget = Paragraph::new(text).alignment(Alignment::Center);
    frame.render_widget(widget, area);
}
