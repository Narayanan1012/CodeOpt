//! Application state, navigation engine, and event handling for CodeOpt.

use std::fs;
use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

use crate::{
    cfg::build,
    logging::Transformation,
    parser::parse_program,
    passes::optimize_with_worklist,
    tui::{
        data::{BenchmarkCategory, BenchmarkCorpus, BenchmarkItem},
        theme::{
            COLOR_CYAN, COLOR_INDIGO, COLOR_PANEL_BG,
            COLOR_PINK, COLOR_SUCCESS, COLOR_TEXT_BODY, COLOR_TEXT_MUTED,
        },
        views::{render_benchmarks, render_home, render_optimizing, render_playground, render_program_modal},
    },
    verification::verify,
    vm::execute,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurrentScreen {
    Home,
    Optimizing,
    BenchmarkSuite,
    Playground,
    ProgramDeepDive,
}

pub struct App {
    pub screen: CurrentScreen,
    pub selected_home_card: usize, // 0: Playground, 1: Benchmark Suite
    pub tick_count: usize,
    pub corpus: BenchmarkCorpus,
    pub active_category: BenchmarkCategory,
    pub selected_benchmark_idx: usize,
    pub search_query: String,
    pub search_active: bool,
    pub help_modal_open: bool,
    pub should_quit: bool,

    // Step 2 & 3: Optimizing Loading Screen
    pub optimizing_progress: usize,
    pub show_optimizing_screen: bool,

    // Step 3: Program Deep Dive Scroll States
    pub code_scroll_offset: usize,
    pub audit_scroll_offset: usize,

    // Step 3: Interactive Playground State
    pub playground_file_name: String,
    pub playground_buffer: Vec<String>,
    pub playground_cursor_row: usize,
    pub playground_editing: bool,
    pub playground_inputs: Vec<i32>,
    pub playground_opt_lines: Vec<String>,
    pub playground_transforms: Vec<Transformation>,
    pub playground_parse_error: Option<String>,
    pub playground_orig_out: Vec<i32>,
    pub playground_opt_out: Vec<i32>,
    pub playground_equiv_pass: bool,

    pub load_file_modal_open: bool,
    pub load_file_path_input: String,
}

impl App {
    pub fn new(corpus: BenchmarkCorpus) -> Self {
        let default_playground = vec![
            "read a".to_string(),
            "read b".to_string(),
            "t1 = a + 0".to_string(),
            "t2 = 10 * 20".to_string(),
            "t3 = a + b".to_string(),
            "t4 = b + a".to_string(),
            "print t4".to_string(),
        ];

        let mut app = Self {
            screen: CurrentScreen::Home,
            selected_home_card: 0,
            tick_count: 0,
            corpus,
            active_category: BenchmarkCategory::All,
            selected_benchmark_idx: 0,
            search_query: String::new(),
            search_active: false,
            help_modal_open: false,
            should_quit: false,

            optimizing_progress: 0,
            show_optimizing_screen: true,

            code_scroll_offset: 0,
            audit_scroll_offset: 0,

            playground_file_name: "scratchpad.tac".to_string(),
            playground_buffer: default_playground,
            playground_cursor_row: 2,
            playground_editing: false,
            playground_inputs: vec![4, 9],
            playground_opt_lines: Vec::new(),
            playground_transforms: Vec::new(),
            playground_parse_error: None,
            playground_orig_out: Vec::new(),
            playground_opt_out: Vec::new(),
            playground_equiv_pass: true,

            load_file_modal_open: false,
            load_file_path_input: "benchmarks/algebraic_01.tac".to_string(),
        };

        app.recompile_playground();
        app
    }

    pub fn tick(&mut self) {
        self.tick_count = self.tick_count.wrapping_add(1);

        if self.screen == CurrentScreen::Optimizing {
            self.optimizing_progress = self.optimizing_progress.saturating_add(12);
            if self.optimizing_progress >= 100 {
                self.screen = CurrentScreen::BenchmarkSuite;
            }
        }
    }

    pub fn recompile_playground(&mut self) {
        let source = self.playground_buffer.join("\n");
        let parsed = match parse_program(&source) {
            Ok(p) => p,
            Err(e) => {
                self.playground_parse_error = Some(e.to_string());
                return;
            }
        };

        let mut optimized = parsed.clone();
        let cfg = match build(&mut optimized) {
            Ok(c) => c,
            Err(e) => {
                self.playground_parse_error = Some(e.to_string());
                return;
            }
        };

        let report = optimize_with_worklist(&mut optimized, &cfg);
        self.playground_parse_error = None;
        self.playground_opt_lines = optimized.to_string().lines().map(String::from).collect();
        self.playground_transforms = report.transformations;

        // Run differential VM
        let orig_res = execute(&parsed, &self.playground_inputs);
        let opt_res = execute(&optimized, &self.playground_inputs);

        if let (Ok(o1), Ok(o2)) = (orig_res, opt_res) {
            let matches = o1.output == o2.output;
            self.playground_orig_out = o1.output;
            self.playground_opt_out = o2.output;
            self.playground_equiv_pass = matches;
        } else if let Ok(v) = verify(&parsed, &self.playground_inputs) {
            let matches = v.matches();
            self.playground_orig_out = v.original_output;
            self.playground_opt_out = v.optimized_output;
            self.playground_equiv_pass = matches;
        }
    }

    pub fn filtered_items(&self) -> Vec<&BenchmarkItem> {
        let items = self.corpus.filter_by_category(self.active_category);
        if self.search_query.is_empty() {
            items
        } else {
            let query = self.search_query.to_lowercase();
            items
                .into_iter()
                .filter(|item| {
                    item.name.to_lowercase().contains(&query)
                        || item.category.name().to_lowercase().contains(&query)
                        || item.category.short_code().to_lowercase().contains(&query)
                })
                .collect()
        }
    }

    pub fn selected_item(&self) -> Option<&BenchmarkItem> {
        let items = self.filtered_items();
        items.get(self.selected_benchmark_idx).copied()
    }

    pub fn handle_key(&mut self, key: KeyCode) {
        // Global modal toggle
        if self.help_modal_open {
            match key {
                KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') | KeyCode::Enter => {
                    self.help_modal_open = false;
                }
                _ => {}
            }
            return;
        }

        // Search mode handling in benchmark suite
        if self.screen == CurrentScreen::BenchmarkSuite && self.search_active {
            match key {
                KeyCode::Esc => {
                    self.search_active = false;
                    self.search_query.clear();
                    self.selected_benchmark_idx = 0;
                }
                KeyCode::Enter => {
                    self.search_active = false;
                }
                KeyCode::Backspace => {
                    self.search_query.pop();
                    self.selected_benchmark_idx = 0;
                }
                KeyCode::Char(c) => {
                    self.search_query.push(c);
                    self.selected_benchmark_idx = 0;
                }
                _ => {}
            }
            return;
        }

        // Load file path modal in playground
        if self.screen == CurrentScreen::Playground && self.load_file_modal_open {
            match key {
                KeyCode::Esc => {
                    self.load_file_modal_open = false;
                }
                KeyCode::Backspace => {
                    self.load_file_path_input.pop();
                }
                KeyCode::Enter => {
                    let path = self.load_file_path_input.trim();
                    if let Ok(content) = fs::read_to_string(path) {
                        self.playground_file_name = path.to_string();
                        self.playground_buffer = content.lines().map(String::from).collect();
                        self.playground_cursor_row = 0;
                        self.recompile_playground();
                    }
                    self.load_file_modal_open = false;
                }
                KeyCode::Char(c) => {
                    self.load_file_path_input.push(c);
                }
                _ => {}
            }
            return;
        }

        // In-place text editing in playground
        if self.screen == CurrentScreen::Playground && self.playground_editing {
            match key {
                KeyCode::Esc => {
                    self.playground_editing = false;
                }
                KeyCode::Enter => {
                    let insert_pos = (self.playground_cursor_row + 1).min(self.playground_buffer.len());
                    self.playground_buffer.insert(insert_pos, String::new());
                    self.playground_cursor_row = insert_pos;
                    self.recompile_playground();
                }
                KeyCode::Backspace => {
                    if self.playground_cursor_row < self.playground_buffer.len() {
                        if self.playground_buffer[self.playground_cursor_row].is_empty() {
                            if self.playground_buffer.len() > 1 {
                                self.playground_buffer.remove(self.playground_cursor_row);
                                self.playground_cursor_row = self.playground_cursor_row.saturating_sub(1);
                            }
                        } else {
                            self.playground_buffer[self.playground_cursor_row].pop();
                        }
                        self.recompile_playground();
                    }
                }
                KeyCode::Up => {
                    self.playground_cursor_row = self.playground_cursor_row.saturating_sub(1);
                }
                KeyCode::Down => {
                    if self.playground_cursor_row + 1 < self.playground_buffer.len() {
                        self.playground_cursor_row += 1;
                    }
                }
                KeyCode::Char(c) => {
                    if self.playground_cursor_row < self.playground_buffer.len() {
                        self.playground_buffer[self.playground_cursor_row].push(c);
                        self.recompile_playground();
                    }
                }
                _ => {}
            }
            return;
        }

        // Global keys when not typing
        match key {
            KeyCode::Char('?') => {
                self.help_modal_open = true;
                return;
            }
            KeyCode::Char('q') => {
                self.should_quit = true;
                return;
            }
            _ => {}
        }

        match self.screen {
            CurrentScreen::Home => match key {
                KeyCode::Esc => {
                    self.should_quit = true;
                }
                KeyCode::Left | KeyCode::Char('h') | KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                    self.selected_home_card = 1 - self.selected_home_card;
                }
                KeyCode::Char('1') | KeyCode::Char('p') | KeyCode::Char('t') => {
                    self.screen = CurrentScreen::Playground;
                }
                KeyCode::Char('2') | KeyCode::Char('b') => {
                    self.trigger_benchmark_mode();
                }
                KeyCode::Enter => {
                    if self.selected_home_card == 0 {
                        self.screen = CurrentScreen::Playground;
                    } else {
                        self.trigger_benchmark_mode();
                    }
                }
                _ => {}
            },
            CurrentScreen::Optimizing => match key {
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char(' ') => {
                    self.screen = CurrentScreen::BenchmarkSuite;
                }
                KeyCode::Char('o') | KeyCode::Char('O') => {
                    self.show_optimizing_screen = !self.show_optimizing_screen;
                }
                _ => {}
            },
            CurrentScreen::BenchmarkSuite => {
                let items_count = self.filtered_items().len();
                match key {
                    KeyCode::Esc => {
                        self.screen = CurrentScreen::Home;
                    }
                    KeyCode::Char('/') => {
                        self.search_active = true;
                    }
                    KeyCode::Char('1') => self.switch_category(BenchmarkCategory::All),
                    KeyCode::Char('2') => self.switch_category(BenchmarkCategory::ConstantFolding),
                    KeyCode::Char('3') => self.switch_category(BenchmarkCategory::ConstantPropagation),
                    KeyCode::Char('4') => self.switch_category(BenchmarkCategory::AlgebraicSimplification),
                    KeyCode::Char('5') => self.switch_category(BenchmarkCategory::LocalCSE),
                    KeyCode::Char('6') => self.switch_category(BenchmarkCategory::Combined),
                    KeyCode::Left | KeyCode::Char('h') => {
                        self.cycle_category(false);
                    }
                    KeyCode::Right | KeyCode::Char('l') => {
                        self.cycle_category(true);
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if items_count > 0 {
                            self.selected_benchmark_idx = (self.selected_benchmark_idx + 1).min(items_count - 1);
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.selected_benchmark_idx = self.selected_benchmark_idx.saturating_sub(1);
                    }
                    KeyCode::PageDown => {
                        if items_count > 0 {
                            self.selected_benchmark_idx = (self.selected_benchmark_idx + 5).min(items_count - 1);
                        }
                    }
                    KeyCode::PageUp => {
                        self.selected_benchmark_idx = self.selected_benchmark_idx.saturating_sub(5);
                    }
                    KeyCode::Home => {
                        self.selected_benchmark_idx = 0;
                    }
                    KeyCode::End => {
                        if items_count > 0 {
                            self.selected_benchmark_idx = items_count - 1;
                        }
                    }
                    KeyCode::Enter => {
                        if items_count > 0 {
                            self.code_scroll_offset = 0;
                            self.audit_scroll_offset = 0;
                            self.screen = CurrentScreen::ProgramDeepDive;
                        }
                    }
                    _ => {}
                }
            }
            CurrentScreen::Playground => match key {
                KeyCode::Esc => {
                    self.screen = CurrentScreen::Home;
                }
                KeyCode::Char('i') => {
                    self.playground_editing = true;
                }
                KeyCode::Char('l') | KeyCode::Char('L') => {
                    self.load_file_modal_open = true;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.playground_cursor_row = self.playground_cursor_row.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if self.playground_cursor_row + 1 < self.playground_buffer.len() {
                        self.playground_cursor_row += 1;
                    }
                }
                KeyCode::Char('c') | KeyCode::Char('r') => {
                    self.playground_buffer = vec![
                        "read a".to_string(),
                        "read b".to_string(),
                        "t1 = a + 0".to_string(),
                        "t2 = 10 * 20".to_string(),
                        "t3 = a + b".to_string(),
                        "t4 = b + a".to_string(),
                        "print t4".to_string(),
                    ];
                    self.playground_cursor_row = 0;
                    self.recompile_playground();
                }
                _ => {}
            },
            CurrentScreen::ProgramDeepDive => match key {
                KeyCode::Esc | KeyCode::Backspace | KeyCode::Char('h') | KeyCode::Left => {
                    self.screen = CurrentScreen::BenchmarkSuite;
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.code_scroll_offset = self.code_scroll_offset.saturating_add(1);
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.code_scroll_offset = self.code_scroll_offset.saturating_sub(1);
                }
                KeyCode::Tab => {
                    self.audit_scroll_offset = self.audit_scroll_offset.saturating_add(1);
                }
                _ => {}
            },
        }
    }

    fn trigger_benchmark_mode(&mut self) {
        if self.show_optimizing_screen {
            self.screen = CurrentScreen::Optimizing;
            self.optimizing_progress = 0;
        } else {
            self.screen = CurrentScreen::BenchmarkSuite;
        }
    }

    fn switch_category(&mut self, cat: BenchmarkCategory) {
        self.active_category = cat;
        self.selected_benchmark_idx = 0;
    }

    fn cycle_category(&mut self, forward: bool) {
        let all = crate::tui::data::ALL_CATEGORIES;
        if let Some(pos) = all.iter().position(|c| *c == self.active_category) {
            let next_pos = if forward {
                (pos + 1) % all.len()
            } else {
                (pos + all.len() - 1) % all.len()
            };
            self.switch_category(all[next_pos]);
        }
    }

    pub fn render(&self, frame: &mut Frame) {
        let area = frame.area();

        match self.screen {
            CurrentScreen::Home => render_home(frame, self, area),
            CurrentScreen::Optimizing => render_optimizing(frame, self, area),
            CurrentScreen::BenchmarkSuite => render_benchmarks(frame, self, area),
            CurrentScreen::Playground => render_playground(frame, self, area),
            CurrentScreen::ProgramDeepDive => render_program_modal(frame, self, area),
        }

        if self.help_modal_open {
            self.render_help_modal(frame, area);
        }
    }

    fn render_help_modal(&self, frame: &mut Frame, area: Rect) {
        let popup_width = 70.min(area.width.saturating_sub(4));
        let popup_height = 21.min(area.height.saturating_sub(2));

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
            .border_style(Style::default().fg(COLOR_CYAN))
            .style(Style::default().bg(COLOR_PANEL_BG))
            .title(Line::from(vec![
                Span::styled(" CodeOpt Keyboard Navigation & Quick Reference ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
            ]));

        let content = vec![
            Line::from(vec![
                Span::styled(" [1] or [P]   ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
                Span::styled("Launch Interactive Playground (type or load TAC)", Style::default().fg(COLOR_TEXT_BODY)),
            ]),
            Line::from(vec![
                Span::styled(" [2] or [B]   ", Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
                Span::styled("Launch Benchmark Suite (with optional Optimizing screen)", Style::default().fg(COLOR_TEXT_BODY)),
            ]),
            Line::from(vec![
                Span::styled(" [O]          ", Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
                Span::styled("Toggle Optimizing loading screen ON/OFF", Style::default().fg(COLOR_TEXT_BODY)),
            ]),
            Line::from(vec![
                Span::styled(" [Enter]      ", Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
                Span::styled("View More: Expand program to full-screen diff inspector", Style::default().fg(COLOR_TEXT_BODY)),
            ]),
            Line::from(vec![
                Span::styled(" [Esc]        ", Style::default().fg(COLOR_INDIGO).add_modifier(Modifier::BOLD)),
                Span::styled("Collapse full-screen / Return to previous view / Home", Style::default().fg(COLOR_TEXT_BODY)),
            ]),
            Line::from(vec![
                Span::styled(" [i]          ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
                Span::styled("Enter in-place buffer edit mode in Playground", Style::default().fg(COLOR_TEXT_BODY)),
            ]),
            Line::from(vec![
                Span::styled(" [L]          ", Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
                Span::styled("Load program file path in Playground", Style::default().fg(COLOR_TEXT_BODY)),
            ]),
            Line::from(vec![
                Span::styled(" [/]          ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
                Span::styled("Search and filter datasets in Benchmark Suite", Style::default().fg(COLOR_TEXT_BODY)),
            ]),
            Line::from(vec![
                Span::styled(" [?]          ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
                Span::styled("Toggle this Help reference modal", Style::default().fg(COLOR_TEXT_BODY)),
            ]),
            Line::from(vec![
                Span::styled(" [Q]          ", Style::default().fg(COLOR_TEXT_MUTED).add_modifier(Modifier::BOLD)),
                Span::styled("Exit Application", Style::default().fg(COLOR_TEXT_MUTED)),
            ]),
            Line::from(""),
            Line::from(Span::styled("All benchmark and playground metrics are computed dynamically from real VM execution.", Style::default().fg(COLOR_SUCCESS))),
        ];

        let widget = Paragraph::new(content).block(block);
        frame.render_widget(widget, popup_area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use crate::tui::data::BenchmarkCategory;

    #[test]
    fn test_home_navigation_and_mode_switching() {
        let mut app = App::new(BenchmarkCorpus::default());
        assert_eq!(app.screen, CurrentScreen::Home);
        assert_eq!(app.selected_home_card, 0);

        // Toggle card with Tab
        app.handle_key(KeyCode::Tab);
        assert_eq!(app.selected_home_card, 1);

        // Toggle back with Left
        app.handle_key(KeyCode::Left);
        assert_eq!(app.selected_home_card, 0);

        // Launch Playground with key '1'
        app.handle_key(KeyCode::Char('1'));
        assert_eq!(app.screen, CurrentScreen::Playground);

        // Return Home with Esc
        app.handle_key(KeyCode::Esc);
        assert_eq!(app.screen, CurrentScreen::Home);

        // Launch Benchmark Suite with key '2' (with optimizing screen enabled)
        app.handle_key(KeyCode::Char('2'));
        assert_eq!(app.screen, CurrentScreen::Optimizing);

        // Skip optimizing screen with Space
        app.handle_key(KeyCode::Char(' '));
        assert_eq!(app.screen, CurrentScreen::BenchmarkSuite);

        // Return Home with Esc
        app.handle_key(KeyCode::Esc);
        assert_eq!(app.screen, CurrentScreen::Home);
    }

    #[test]
    fn test_optimizing_screen_toggle() {
        let mut app = App::new(BenchmarkCorpus::default());
        assert!(app.show_optimizing_screen);

        // Go to optimizing screen
        app.handle_key(KeyCode::Char('2'));
        assert_eq!(app.screen, CurrentScreen::Optimizing);

        // Toggle optimizing screen OFF with 'o'
        app.handle_key(KeyCode::Char('o'));
        assert!(!app.show_optimizing_screen);

        // Skip to benchmark
        app.handle_key(KeyCode::Esc);
        assert_eq!(app.screen, CurrentScreen::BenchmarkSuite);

        // Return Home
        app.handle_key(KeyCode::Esc);
        assert_eq!(app.screen, CurrentScreen::Home);

        // Launch Benchmark Suite again: since toggle is OFF, it jumps directly to BenchmarkSuite!
        app.handle_key(KeyCode::Char('2'));
        assert_eq!(app.screen, CurrentScreen::BenchmarkSuite);
    }

    #[test]
    fn test_help_modal_toggle() {
        let mut app = App::new(BenchmarkCorpus::default());
        assert!(!app.help_modal_open);

        app.handle_key(KeyCode::Char('?'));
        assert!(app.help_modal_open);

        app.handle_key(KeyCode::Esc);
        assert!(!app.help_modal_open);
    }

    #[test]
    fn test_corpus_categorization() {
        let corpus = BenchmarkCorpus::load_from_dir("benchmarks");
        assert!(!corpus.items.is_empty(), "Corpus should load benchmark files");

        // Verify that algebraic_01 is marked as Combined (triggers CF, CP, and AS)
        let algebraic = corpus.items.iter().find(|i| i.name.contains("algebraic"));
        assert!(algebraic.is_some());
        assert_eq!(algebraic.unwrap().category, BenchmarkCategory::Combined);

        // Verify single-pass categorization:
        let single_as_src = "read x\nt1 = x + 0";
        let single_item = BenchmarkCorpus::compile_item(
            99,
            "test_single_as.tac".to_string(),
            PathBuf::from("test.tac"),
            single_as_src,
            &[5],
        );
        assert!(single_item.is_some());
        assert_eq!(
            single_item.unwrap().category,
            BenchmarkCategory::AlgebraicSimplification
        );
    }

    #[test]
    fn test_program_modal_and_scrolling() {
        let corpus = BenchmarkCorpus::load_from_dir("benchmarks");
        let mut app = App::new(corpus);

        // Turn off optimizing screen for instant jump
        app.show_optimizing_screen = false;
        app.handle_key(KeyCode::Char('2'));
        assert_eq!(app.screen, CurrentScreen::BenchmarkSuite);

        // Press Enter to expand into full-screen deep dive
        app.handle_key(KeyCode::Enter);
        assert_eq!(app.screen, CurrentScreen::ProgramDeepDive);
        assert_eq!(app.code_scroll_offset, 0);

        // Scroll code with 'j'
        app.handle_key(KeyCode::Char('j'));
        assert_eq!(app.code_scroll_offset, 1);

        // Scroll code with 'k'
        app.handle_key(KeyCode::Char('k'));
        assert_eq!(app.code_scroll_offset, 0);

        // Press Esc to collapse back to BenchmarkSuite
        app.handle_key(KeyCode::Esc);
        assert_eq!(app.screen, CurrentScreen::BenchmarkSuite);
    }

    #[test]
    fn test_playground_live_editing_and_recompile() {
        let mut app = App::new(BenchmarkCorpus::default());
        app.screen = CurrentScreen::Playground;

        // Verify initial state compiled cleanly
        assert!(app.playground_parse_error.is_none());
        assert!(!app.playground_opt_lines.is_empty());
        assert!(app.playground_equiv_pass);

        // Enter edit mode
        app.handle_key(KeyCode::Char('i'));
        assert!(app.playground_editing);

        // Type a line that causes a parse error: "invalid line"
        app.playground_buffer.push("invalid instruction line".to_string());
        app.recompile_playground();
        assert!(app.playground_parse_error.is_some());

        // Fix line
        app.playground_buffer.pop();
        app.recompile_playground();
        assert!(app.playground_parse_error.is_none());
    }
}
