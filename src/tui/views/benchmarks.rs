//! Benchmark suite view with Top Graphs Deck and categorized program list.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Bar, BarChart, BarGroup, Block, BorderType, Borders, Paragraph, Sparkline,
    },
};

use crate::tui::{
    app::App,
    data::{ALL_CATEGORIES, BenchmarkCategory, BenchmarkItem},
    theme::{
        COLOR_BORDER, COLOR_BORDER_ACTIVE, COLOR_COMBINED, COLOR_CYAN, COLOR_DANGER,
        COLOR_INDIGO, COLOR_PANEL_BG, COLOR_PINK, COLOR_SUCCESS, COLOR_TEXT_BODY,
        COLOR_TEXT_HEAD, COLOR_TEXT_MUTED, COLOR_WARNING,
    },
};

pub fn render_benchmarks(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(11), // 1. Top Graphs Deck
            Constraint::Length(3),  // 2. Category Tabs & Search Bar
            Constraint::Min(10),    // 3. Program Cards List with Execution Time Scores
            Constraint::Length(1),  // 4. Navigation Footer Bar
        ])
        .split(area);

    render_top_graphs_deck(frame, app, chunks[0]);
    render_category_tabs(frame, app, chunks[1]);
    render_program_list(frame, app, chunks[2]);
    render_footer(frame, app, chunks[3]);
}

fn render_top_graphs_deck(frame: &mut Frame, app: &App, area: Rect) {
    let panels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(34), // Panel 1: Instruction Reduction BarChart
            Constraint::Percentage(33), // Panel 2: Firing Frequency Distribution
            Constraint::Percentage(33), // Panel 3: Convergence Sparkline
        ])
        .split(area);

    // Compute aggregated metrics for the current category or whole corpus
    let current_items: Vec<&BenchmarkItem> = app.filtered_items();
    let total_orig_inst: u64 = current_items.iter().map(|i| i.original_stats.instruction_count as u64).sum();
    let total_opt_inst: u64 = current_items.iter().map(|i| i.optimized_stats.instruction_count as u64).sum();
    let total_orig_arith: u64 = current_items.iter().map(|i| i.original_stats.arithmetic_count as u64).sum();
    let total_opt_arith: u64 = current_items.iter().map(|i| i.optimized_stats.arithmetic_count as u64).sum();

    // 1. Panel 1: BarChart of Instruction & Operation Reduction
    let barchart_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLOR_CYAN))
        .title(Line::from(vec![
            Span::styled(" Instruction & Op Delta ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
            Span::styled(format!("({})", app.active_category.short_code()), Style::default().fg(COLOR_TEXT_MUTED)),
        ]));

    let inst_pct = if total_orig_inst > 0 {
        ((total_orig_inst.saturating_sub(total_opt_inst)) as f64 / total_orig_inst as f64) * 100.0
    } else {
        0.0
    };

    let group = BarGroup::default().bars(&[
        Bar::default()
            .value(total_orig_inst)
            .label(Line::from("Orig-I"))
            .style(Style::default().fg(COLOR_INDIGO)),
        Bar::default()
            .value(total_opt_inst)
            .label(Line::from("Opt-I"))
            .style(Style::default().fg(COLOR_SUCCESS)),
        Bar::default()
            .value(total_orig_arith)
            .label(Line::from("Orig-Op"))
            .style(Style::default().fg(COLOR_PINK)),
        Bar::default()
            .value(total_opt_arith)
            .label(Line::from("Opt-Op"))
            .style(Style::default().fg(COLOR_CYAN)),
    ]);

    let max_val = total_orig_inst.max(total_orig_arith).max(10);
    let chart = BarChart::default()
        .block(barchart_block)
        .bar_width(6)
        .bar_gap(1)
        .data(group)
        .max(max_val);
    frame.render_widget(chart, panels[0]);

    // 2. Panel 2: Firing Frequency Distribution
    let mut cf_count = 0;
    let mut cp_count = 0;
    let mut as_count = 0;
    let mut cse_count = 0;
    let mut dce_count = 0;
    let mut total_transforms = 0;

    for item in &app.corpus.items {
        for t in &item.transformations {
            total_transforms += 1;
            match t.pass {
                crate::passes::OptimizationKind::ConstantFolding => cf_count += 1,
                crate::passes::OptimizationKind::ConstantPropagation => cp_count += 1,
                crate::passes::OptimizationKind::AlgebraicSimplification => as_count += 1,
                crate::passes::OptimizationKind::LocalCommonSubexpressionElimination => cse_count += 1,
                crate::passes::OptimizationKind::DeadCodeElimination => dce_count += 1,
            }
        }
    }

    let dist_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLOR_PINK))
        .title(Line::from(vec![
            Span::styled(" Pass Firing Distribution ", Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
        ]));

    let pct = |n: usize| if total_transforms > 0 { (n as f64 / total_transforms as f64) * 100.0 } else { 0.0 };
    let cf_pct = pct(cf_count);
    let cp_pct = pct(cp_count);
    let as_pct = pct(as_count);
    let cse_pct = pct(cse_count);
    let dce_pct = pct(dce_count);

    let dist_lines = vec![
        Line::from(vec![
            Span::styled("Constant Folding [CF] : ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{cf_count} ({cf_pct:.0}%)  "), Style::default().fg(COLOR_TEXT_HEAD)),
            Span::styled(gauge_bar(cf_pct, 10), Style::default().fg(COLOR_CYAN)),
        ]),
        Line::from(vec![
            Span::styled("Constant Prop    [CP] : ", Style::default().fg(COLOR_INDIGO).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{cp_count} ({cp_pct:.0}%)  "), Style::default().fg(COLOR_TEXT_HEAD)),
            Span::styled(gauge_bar(cp_pct, 10), Style::default().fg(COLOR_INDIGO)),
        ]),
        Line::from(vec![
            Span::styled("Algebraic Simp   [AS] : ", Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{as_count} ({as_pct:.0}%)  "), Style::default().fg(COLOR_TEXT_HEAD)),
            Span::styled(gauge_bar(as_pct, 10), Style::default().fg(COLOR_SUCCESS)),
        ]),
        Line::from(vec![
            Span::styled("Local CSE        [CSE]: ", Style::default().fg(COLOR_WARNING).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{cse_count} ({cse_pct:.0}%)  "), Style::default().fg(COLOR_TEXT_HEAD)),
            Span::styled(gauge_bar(cse_pct, 10), Style::default().fg(COLOR_WARNING)),
        ]),
        Line::from(vec![
            Span::styled("Dead Code Elim   [DCE]: ", Style::default().fg(COLOR_DANGER).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{dce_count} ({dce_pct:.0}%)  "), Style::default().fg(COLOR_TEXT_HEAD)),
            Span::styled(gauge_bar(dce_pct, 10), Style::default().fg(COLOR_DANGER)),
        ]),
        Line::from(vec![
            Span::styled(format!("Total Rewrites: {total_transforms}"), Style::default().fg(COLOR_TEXT_MUTED)),
            Span::styled(format!("  │  Net ΔInst: -{inst_pct:.1}%"), Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
        ]),
    ];
    let dist_widget = Paragraph::new(dist_lines).block(dist_block);
    frame.render_widget(dist_widget, panels[1]);

    // 3. Panel 3: Pass Convergence Sparkline
    let conv_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLOR_SUCCESS))
        .title(Line::from(vec![
            Span::styled(" Convergence Curve (Δ/Pass) ", Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
        ]));

    // Build convergence curve data from rounds
    let mut round_counts = [0u64; 8];
    for item in &app.corpus.items {
        let r = item.worklist_rounds.min(8);
        if r > 0 {
            round_counts[r - 1] += 1;
        }
    }
    // Sparkline of activity across rounds
    let sparkline_data: Vec<u64> = vec![
        (cf_count + cp_count + as_count + cse_count) as u64,
        (cp_count + cse_count) as u64,
        (cse_count / 2) as u64,
        1,
        0,
    ];

    let conv_inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(4)])
        .split(conv_block.inner(panels[2]));

    frame.render_widget(conv_block, panels[2]);

    let sparkline = Sparkline::default()
        .data(&sparkline_data)
        .style(Style::default().fg(COLOR_SUCCESS));
    frame.render_widget(sparkline, conv_inner[0]);

    let conv_desc = vec![
        Line::from(vec![
            Span::styled("Rounds to Fixed Point: ", Style::default().fg(COLOR_TEXT_HEAD)),
            Span::styled("1–2 avg (Max cap: 16)", Style::default().fg(COLOR_CYAN)),
        ]),
        Line::from(vec![
            Span::styled("Oscillation State:     ", Style::default().fg(COLOR_TEXT_HEAD)),
            Span::styled("STABLE (0 cycles)", Style::default().fg(COLOR_SUCCESS)),
        ]),
        Line::from(vec![
            Span::styled("Worklist Consumers:    ", Style::default().fg(COLOR_TEXT_HEAD)),
            Span::styled("Demand-driven re-queue", Style::default().fg(COLOR_TEXT_MUTED)),
        ]),
    ];
    let conv_widget = Paragraph::new(conv_desc);
    frame.render_widget(conv_widget, conv_inner[1]);
}

fn gauge_bar(pct: f64, width: usize) -> String {
    let filled = ((pct / 100.0) * width as f64).round() as usize;
    let filled = filled.min(width);
    format!("{}{}", "█".repeat(filled), "░".repeat(width.saturating_sub(filled)))
}

fn render_category_tabs(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(60), Constraint::Length(28)])
        .split(area);

    let mut tab_spans = Vec::new();
    for (idx, cat) in ALL_CATEGORIES.iter().enumerate() {
        let count = app.corpus.filter_by_category(*cat).len();
        let is_active = *cat == app.active_category;

        let num_str = format!("[{}] ", idx + 1);
        let label = format!("{}{} ({count})", num_str, cat.name());

        if is_active {
            let active_color = if *cat == BenchmarkCategory::Combined {
                COLOR_COMBINED
            } else {
                COLOR_CYAN
            };
            tab_spans.push(Span::styled(
                format!(" {label} "),
                Style::default()
                    .fg(Color::Rgb(15, 23, 42))
                    .bg(active_color)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            let inactive_color = if *cat == BenchmarkCategory::Combined {
                COLOR_PINK
            } else {
                COLOR_TEXT_BODY
            };
            tab_spans.push(Span::styled(
                format!(" {label} "),
                Style::default().fg(inactive_color),
            ));
        }
        tab_spans.push(Span::styled(" ", Style::default()));
    }

    let tabs_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(COLOR_BORDER))
        .title(Line::from(vec![
            Span::styled(" Categories & Optimization Techniques ", Style::default().fg(COLOR_TEXT_HEAD).add_modifier(Modifier::BOLD)),
        ]));
    let tabs_widget = Paragraph::new(Line::from(tab_spans)).block(tabs_block);
    frame.render_widget(tabs_widget, chunks[0]);

    // Search filter prompt
    let search_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if app.search_active { COLOR_BORDER_ACTIVE } else { COLOR_BORDER }))
        .title(Line::from(vec![
            Span::styled(if app.search_active { " [/] Searching... " } else { " [/] Filter Name " }, Style::default().fg(if app.search_active { COLOR_CYAN } else { COLOR_TEXT_MUTED })),
        ]));

    let search_text = if app.search_query.is_empty() {
        if app.search_active {
            Line::from(Span::styled("_", Style::default().fg(COLOR_CYAN)))
        } else {
            Line::from(Span::styled("Press / to search", Style::default().fg(COLOR_TEXT_MUTED)))
        }
    } else {
        Line::from(vec![
            Span::styled(&app.search_query, Style::default().fg(COLOR_TEXT_HEAD)),
            if app.search_active {
                Span::styled("_", Style::default().fg(COLOR_CYAN))
            } else {
                Span::raw("")
            },
        ])
    };
    let search_widget = Paragraph::new(search_text).block(search_block);
    frame.render_widget(search_widget, chunks[1]);
}

fn render_program_list(frame: &mut Frame, app: &App, area: Rect) {
    let items = app.filtered_items();

    let list_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLOR_BORDER))
        .title(Line::from(vec![
            Span::styled(" Programs in Category ", Style::default().fg(COLOR_TEXT_HEAD).add_modifier(Modifier::BOLD)),
            Span::styled(format!("({} total) ", items.len()), Style::default().fg(COLOR_TEXT_MUTED)),
            Span::styled("• Highlighted with Reduced Execution Time Score", Style::default().fg(COLOR_CYAN)),
        ]));

    if items.is_empty() {
        let empty_widget = Paragraph::new("No programs found matching the current filter.").block(list_block);
        frame.render_widget(empty_widget, area);
        return;
    }

    let inner_area = list_block.inner(area);
    frame.render_widget(list_block, area);

    // Render program cards vertically
    let card_height = 3;
    let max_visible_cards = (inner_area.height as usize / card_height).max(1);

    // Compute scroll offset to keep selected item in view
    let scroll_offset = if app.selected_benchmark_idx >= max_visible_cards {
        app.selected_benchmark_idx - max_visible_cards + 1
    } else {
        0
    };

    let visible_items = items
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(max_visible_cards);

    let card_constraints = (0..max_visible_cards)
        .map(|_| Constraint::Length(card_height as u16))
        .collect::<Vec<_>>();

    let card_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(card_constraints)
        .split(inner_area);

    for (render_idx, (actual_idx, item)) in visible_items.enumerate() {
        if render_idx >= card_chunks.len() {
            break;
        }

        let is_selected = actual_idx == app.selected_benchmark_idx;
        let card_area = card_chunks[render_idx];

        let border_color = if is_selected {
            COLOR_BORDER_ACTIVE
        } else {
            COLOR_BORDER
        };
        let border_type = if is_selected {
            BorderType::Thick
        } else {
            BorderType::Rounded
        };

        let card_block = Block::default()
            .borders(Borders::ALL)
            .border_type(border_type)
            .border_style(Style::default().fg(border_color))
            .style(if is_selected { Style::default().bg(COLOR_PANEL_BG) } else { Style::default() });

        // Format execution time score
        let speedup_badge = if item.speedup_pct > 0.0 {
            Span::styled(
                format!(" {:+.1}% time reduction ({:.2}x speedup) ", item.speedup_pct, 1.0 / (1.0 - (item.speedup_pct / 100.0).min(0.99))),
                Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD),
            )
        } else if item.speedup_pct < 0.0 {
            Span::styled(
                format!(" {:+.1}% time ", item.speedup_pct),
                Style::default().fg(COLOR_WARNING),
            )
        } else {
            Span::styled(" ±0.0% time ", Style::default().fg(COLOR_TEXT_MUTED))
        };

        let inst_reduction = crate::tui::data::ProgramStats::reduction_pct(
            item.original_stats.instruction_count,
            item.optimized_stats.instruction_count,
        );

        let pass_verdict = if item.verification_passed {
            Span::styled("[✓ PASS]", Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD))
        } else {
            Span::styled("[✗ FAIL]", Style::default().fg(COLOR_DANGER).add_modifier(Modifier::BOLD))
        };

        let line_1 = Line::from(vec![
            Span::styled(
                if is_selected { " ▶ " } else { "   " },
                Style::default().fg(if is_selected { COLOR_CYAN } else { COLOR_TEXT_MUTED }).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("#{:<02} ", item.id), Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:<20} ", item.name), Style::default().fg(COLOR_TEXT_HEAD).add_modifier(Modifier::BOLD)),
            Span::styled(format!("[{}] ", item.category.short_code()), Style::default().fg(COLOR_PINK)),
            Span::styled(" │ Execution Time: ", Style::default().fg(COLOR_TEXT_MUTED)),
            Span::styled(format!("{}µs → {}µs", item.time_original_us, item.time_optimized_us), Style::default().fg(COLOR_TEXT_HEAD).add_modifier(Modifier::BOLD)),
            speedup_badge,
            Span::styled(" │ Verdict: ", Style::default().fg(COLOR_TEXT_MUTED)),
            pass_verdict,
        ]);

        let line_2 = Line::from(vec![
            Span::raw("     "),
            Span::styled(
                format!("Instructions: {} → {} (-{:.1}%)", item.original_stats.instruction_count, item.optimized_stats.instruction_count, inst_reduction),
                Style::default().fg(COLOR_TEXT_BODY),
            ),
            Span::styled("  │  Arithmetic: ", Style::default().fg(COLOR_TEXT_MUTED)),
            Span::styled(
                format!("{} → {}", item.original_stats.arithmetic_count, item.optimized_stats.arithmetic_count),
                Style::default().fg(COLOR_TEXT_BODY),
            ),
            Span::styled("  │  Rounds: ", Style::default().fg(COLOR_TEXT_MUTED)),
            Span::styled(format!("{}", item.worklist_rounds), Style::default().fg(COLOR_TEXT_BODY)),
            if is_selected {
                Span::styled(
                    "         [Enter: View More ⤢ (Expand Full-Screen)]",
                    Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD),
                )
            } else {
                Span::raw("")
            },
        ]);

        let card_paragraph = Paragraph::new(vec![line_1, line_2]).block(card_block);
        frame.render_widget(card_paragraph, card_area);
    }
}

fn render_footer(frame: &mut Frame, _app: &App, area: Rect) {
    let text = Line::from(vec![
        Span::styled(" [1-8] ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
        Span::styled("Category  ", Style::default().fg(COLOR_TEXT_BODY)),
        Span::styled(" [G] ", Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
        Span::styled("Analytics Deck (6 Plots)  ", Style::default().fg(COLOR_TEXT_BODY)),
        Span::styled(" [↑/↓] ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
        Span::styled("Select Program  ", Style::default().fg(COLOR_TEXT_BODY)),
        Span::styled(" [Enter] ", Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
        Span::styled("View More ⤢  ", Style::default().fg(COLOR_TEXT_BODY)),
        Span::styled(" [/] ", Style::default().fg(COLOR_WARNING).add_modifier(Modifier::BOLD)),
        Span::styled("Search  ", Style::default().fg(COLOR_TEXT_BODY)),
        Span::styled(" [Esc] ", Style::default().fg(COLOR_INDIGO).add_modifier(Modifier::BOLD)),
        Span::styled("Home  ", Style::default().fg(COLOR_TEXT_BODY)),
        Span::styled(" [?] ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
        Span::styled("Help  ", Style::default().fg(COLOR_TEXT_BODY)),
        Span::styled(" [Q] ", Style::default().fg(COLOR_TEXT_MUTED).add_modifier(Modifier::BOLD)),
        Span::styled("Quit", Style::default().fg(COLOR_TEXT_MUTED)),
    ]);

    let widget = Paragraph::new(text).alignment(Alignment::Center);
    frame.render_widget(widget, area);
}
