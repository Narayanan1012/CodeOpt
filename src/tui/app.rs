//! Application state, navigation engine, and event handling for CodeOpt.

use std::fs;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
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
        data::{
            measure_execution_times, BenchmarkCategory, BenchmarkCorpus, BenchmarkItem,
            VectorProof,
        },
        theme::{
            COLOR_CYAN, COLOR_INDIGO, COLOR_PANEL_BG,
            COLOR_PINK, COLOR_SUCCESS, COLOR_TEXT_BODY, COLOR_TEXT_MUTED, COLOR_WARNING,
        },
        views::{
            render_analytics, render_benchmarks, render_home, render_optimizing,
            render_playground, render_program_modal,
        },
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
    AnalyticsDeck,
}

pub fn char_to_byte_idx(s: &str, char_idx: usize) -> usize {
    s.char_indices().nth(char_idx).map(|(i, _)| i).unwrap_or(s.len())
}

pub fn get_clipboard_text() -> Option<String> {
    if let Ok(output) = std::process::Command::new("wl-paste").output() {
        if output.status.success() {
            if let Ok(s) = String::from_utf8(output.stdout) {
                if !s.is_empty() {
                    return Some(s);
                }
            }
        }
    }
    if let Ok(output) = std::process::Command::new("xsel")
        .arg("--clipboard")
        .arg("--output")
        .output()
    {
        if output.status.success() {
            if let Ok(s) = String::from_utf8(output.stdout) {
                if !s.is_empty() {
                    return Some(s);
                }
            }
        }
    }
    None
}

pub fn set_clipboard_text(text: &str) {
    if let Ok(mut child) = std::process::Command::new("wl-copy")
        .stdin(std::process::Stdio::piped())
        .spawn()
    {
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            let _ = stdin.write_all(text.as_bytes());
        }
        let _ = child.wait();
        return;
    }
    if let Ok(mut child) = std::process::Command::new("xsel")
        .arg("--clipboard")
        .arg("--input")
        .stdin(std::process::Stdio::piped())
        .spawn()
    {
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            let _ = stdin.write_all(text.as_bytes());
        }
        let _ = child.wait();
    }
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

    // Step 3: Program Deep Dive Scroll & Live VM Execution States
    pub code_scroll_offset: usize,
    pub audit_scroll_offset: usize,
    pub deep_dive_executed: bool,
    pub deep_dive_orig_time_us: u128,
    pub deep_dive_opt_time_us: u128,
    pub deep_dive_vector_results: Vec<VectorProof>,

    // Step 3: Interactive Playground State
    pub playground_file_name: String,
    pub playground_buffer: Vec<String>,
    pub playground_cursor_row: usize,
    pub playground_cursor_col: usize,
    pub playground_editing: bool,
    pub playground_inputs: Vec<i32>,
    pub playground_opt_lines: Vec<String>,
    pub playground_transforms: Vec<Transformation>,
    pub playground_parse_error: Option<String>,
    pub playground_orig_out: Vec<i32>,
    pub playground_opt_out: Vec<i32>,
    pub playground_equiv_pass: bool,
    pub playground_converted: bool,
    pub playground_vm_executed: bool,
    pub playground_status_message: Option<String>,
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
            deep_dive_executed: false,
            deep_dive_orig_time_us: 0,
            deep_dive_opt_time_us: 0,
            deep_dive_vector_results: Vec::new(),
            playground_file_name: "scratchpad.tac".to_string(),
            playground_buffer: default_playground,
            playground_cursor_row: 2,
            playground_cursor_col: 0,
            playground_editing: false,
            playground_inputs: vec![4, 9],
            playground_opt_lines: Vec::new(),
            playground_transforms: Vec::new(),
            playground_parse_error: None,
            playground_orig_out: Vec::new(),
            playground_opt_out: Vec::new(),
            playground_equiv_pass: true,
            playground_converted: true,
            playground_vm_executed: true,
            playground_status_message: None,
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

    pub fn run_deep_dive_vm(&mut self) {
        let Some(item) = self.selected_item() else { return; };
        let Ok(original) = parse_program(&item.source) else { return; };
        let mut optimized = original.clone();
        let Ok(cfg) = build(&mut optimized) else { return; };
        let _report = optimize_with_worklist(&mut optimized, &cfg);

        let test_input_sets = if item.name.contains("control") || item.name.contains("ctrl") {
            vec![
                vec![0, 1, 2, 3, 4],
                vec![1, 2, 3, 4, 5],
                vec![2, 0, 5, 1, 3],
                vec![3, 4, 1, 2, 0],
                vec![4, 2, 0, 3, 1],
            ]
        } else {
            vec![
                vec![4, 9, 3, 7, 5],
                vec![0, 1, 2, 3, 4],
                vec![10, 5, 20, 1, 8],
                vec![100, 25, 50, 200, 15],
                vec![7, 14, 42, 6, 2],
            ]
        };

        let mut results = Vec::with_capacity(5);
        for (idx, inp) in test_input_sets.into_iter().enumerate() {
            let orig_res = execute(&original, &inp);
            let opt_res = execute(&optimized, &inp);
            let (orig_out, opt_out, matched) = match (orig_res, opt_res) {
                (Ok(o1), Ok(o2)) => {
                    let m = o1.output == o2.output;
                    (o1.output, o2.output, m)
                }
                _ => (Vec::new(), Vec::new(), false),
            };
            results.push(VectorProof {
                vector_id: idx + 1,
                inputs: inp,
                original_output: orig_out,
                optimized_output: opt_out,
                passed: matched,
            });
        }

        let primary_inputs = results.first().map(|v| v.inputs.as_slice()).unwrap_or(&[]);
        let (t_orig, t_opt) = measure_execution_times(&original, &optimized, primary_inputs);

        self.deep_dive_orig_time_us = t_orig;
        self.deep_dive_opt_time_us = t_opt;
        self.deep_dive_vector_results = results;
        self.deep_dive_executed = true;
    }

    pub fn convert_playground(&mut self) {
        let source = self.playground_buffer.join("\n");
        let parsed = match parse_program(&source) {
            Ok(p) => p,
            Err(e) => {
                self.playground_parse_error = Some(e.to_string());
                self.playground_opt_lines.clear();
                self.playground_transforms.clear();
                self.playground_converted = false;
                self.playground_vm_executed = false;
                return;
            }
        };

        let mut optimized = parsed.clone();
        let cfg = match build(&mut optimized) {
            Ok(c) => c,
            Err(e) => {
                self.playground_parse_error = Some(e.to_string());
                self.playground_opt_lines.clear();
                self.playground_transforms.clear();
                self.playground_converted = false;
                self.playground_vm_executed = false;
                return;
            }
        };

        let report = optimize_with_worklist(&mut optimized, &cfg);
        self.playground_parse_error = None;
        self.playground_opt_lines = optimized.to_string().lines().map(String::from).collect();
        self.playground_transforms = report.transformations;
        self.playground_converted = true;
        self.playground_vm_executed = false;
    }

    pub fn run_playground_vm(&mut self) {
        if !self.playground_converted {
            self.convert_playground();
        }
        if self.playground_parse_error.is_some() {
            return;
        }
        let source = self.playground_buffer.join("\n");
        let Ok(parsed) = parse_program(&source) else { return; };
        let opt_source = self.playground_opt_lines.join("\n");
        let Ok(optimized) = parse_program(&opt_source) else { return; };

        let orig_res = execute(&parsed, &self.playground_inputs);
        let opt_res = execute(&optimized, &self.playground_inputs);

        if let (Ok(o1), Ok(o2)) = (orig_res, opt_res) {
            let matches = o1.output == o2.output;
            self.playground_orig_out = o1.output;
            self.playground_opt_out = o2.output;
            self.playground_equiv_pass = matches;
            self.playground_vm_executed = true;
        } else if let Ok(v) = verify(&parsed, &self.playground_inputs) {
            let matches = v.matches();
            self.playground_orig_out = v.original_output;
            self.playground_opt_out = v.optimized_output;
            self.playground_equiv_pass = matches;
            self.playground_vm_executed = true;
        }
    }

    pub fn recompile_playground(&mut self) {
        self.convert_playground();
        self.run_playground_vm();
    }

    pub fn copy_playground_to_clipboard(&mut self) {
        let text = self.playground_buffer.join("\n");
        set_clipboard_text(&text);
        self.playground_status_message = Some("COPIED TAC BUFFER TO CLIPBOARD".to_string());
    }

    pub fn handle_paste(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        if self.screen == CurrentScreen::Playground {
            if self.load_file_modal_open {
                self.load_file_path_input.push_str(text);
                return;
            }
            if self.playground_buffer.is_empty() {
                self.playground_buffer.push(String::new());
                self.playground_cursor_row = 0;
                self.playground_cursor_col = 0;
            }
            let row = self.playground_cursor_row.min(self.playground_buffer.len().saturating_sub(1));
            let line_chars = self.playground_buffer[row].chars().count();
            let col = self.playground_cursor_col.min(line_chars);

            let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
            let paste_lines: Vec<&str> = normalized.split('\n').collect();

            if paste_lines.len() == 1 {
                let single = paste_lines[0];
                let byte_idx = char_to_byte_idx(&self.playground_buffer[row], col);
                self.playground_buffer[row].insert_str(byte_idx, single);
                self.playground_cursor_col = col + single.chars().count();
            } else {
                let current_line = &self.playground_buffer[row];
                let byte_idx = char_to_byte_idx(current_line, col);
                let prefix = current_line[..byte_idx].to_string();
                let suffix = current_line[byte_idx..].to_string();

                self.playground_buffer[row] = format!("{}{}", prefix, paste_lines[0]);
                for (idx, line_str) in paste_lines[1..paste_lines.len() - 1].iter().enumerate() {
                    self.playground_buffer.insert(row + 1 + idx, line_str.to_string());
                }
                let last_idx = row + paste_lines.len() - 1;
                let last_content = paste_lines.last().unwrap();
                let last_str = format!("{}{}", last_content, suffix);
                self.playground_buffer.insert(last_idx, last_str);

                self.playground_cursor_row = last_idx;
                self.playground_cursor_col = last_content.chars().count();
            }
            self.playground_converted = false;
            self.playground_vm_executed = false;
            self.playground_status_message = Some("PASTED FROM CLIPBOARD".to_string());
        } else if self.screen == CurrentScreen::BenchmarkSuite && self.search_active {
            self.search_query.push_str(text);
            self.selected_benchmark_idx = 0;
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
        self.handle_key_with_modifiers(key, KeyModifiers::empty());
    }

    pub fn handle_key_event(&mut self, key: KeyEvent) {
        self.handle_key_with_modifiers(key.code, key.modifiers);
    }

    pub fn handle_key_with_modifiers(&mut self, key: KeyCode, modifiers: KeyModifiers) {
        self.playground_status_message = None;

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
            if modifiers.contains(KeyModifiers::CONTROL) && (key == KeyCode::Char('v') || key == KeyCode::Char('V')) {
                if let Some(text) = get_clipboard_text() {
                    self.handle_paste(&text);
                }
                return;
            }
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
            if modifiers.contains(KeyModifiers::CONTROL) && (key == KeyCode::Char('v') || key == KeyCode::Char('V')) {
                if let Some(text) = get_clipboard_text() {
                    self.handle_paste(&text);
                }
                return;
            }
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
                        self.playground_cursor_col = 0;
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
            if modifiers.contains(KeyModifiers::CONTROL) {
                match key {
                    KeyCode::Char('v') | KeyCode::Char('V') => {
                        if let Some(text) = get_clipboard_text() {
                            self.handle_paste(&text);
                        }
                        return;
                    }
                    KeyCode::Char('c') | KeyCode::Char('C') => {
                        self.copy_playground_to_clipboard();
                        return;
                    }
                    KeyCode::Char('a') | KeyCode::Char('A') => {
                        self.playground_cursor_col = 0;
                        return;
                    }
                    KeyCode::Char('e') | KeyCode::Char('E') => {
                        if let Some(line) = self.playground_buffer.get(self.playground_cursor_row) {
                            self.playground_cursor_col = line.chars().count();
                        }
                        return;
                    }
                    KeyCode::Char('k') | KeyCode::Char('K') => {
                        if self.playground_cursor_row < self.playground_buffer.len() {
                            let line = &mut self.playground_buffer[self.playground_cursor_row];
                            let char_count = line.chars().count();
                            let col = self.playground_cursor_col.min(char_count);
                            let byte_idx = char_to_byte_idx(line, col);
                            line.truncate(byte_idx);
                            self.playground_converted = false;
                            self.playground_vm_executed = false;
                        }
                        return;
                    }
                    KeyCode::Char('u') | KeyCode::Char('U') => {
                        if self.playground_cursor_row < self.playground_buffer.len() {
                            let line = &mut self.playground_buffer[self.playground_cursor_row];
                            let char_count = line.chars().count();
                            let col = self.playground_cursor_col.min(char_count);
                            let byte_idx = char_to_byte_idx(line, col);
                            let rem = line[byte_idx..].to_string();
                            *line = rem;
                            self.playground_cursor_col = 0;
                            self.playground_converted = false;
                            self.playground_vm_executed = false;
                        }
                        return;
                    }
                    _ => {}
                }
            }

            match key {
                KeyCode::Esc => {
                    self.playground_editing = false;
                }
                KeyCode::Left => {
                    if self.playground_cursor_col > 0 {
                        self.playground_cursor_col -= 1;
                    } else if self.playground_cursor_row > 0 {
                        self.playground_cursor_row -= 1;
                        self.playground_cursor_col = self.playground_buffer[self.playground_cursor_row].chars().count();
                    }
                }
                KeyCode::Right => {
                    if self.playground_cursor_row < self.playground_buffer.len() {
                        let line_chars = self.playground_buffer[self.playground_cursor_row].chars().count();
                        if self.playground_cursor_col < line_chars {
                            self.playground_cursor_col += 1;
                        } else if self.playground_cursor_row + 1 < self.playground_buffer.len() {
                            self.playground_cursor_row += 1;
                            self.playground_cursor_col = 0;
                        }
                    }
                }
                KeyCode::Up => {
                    if self.playground_cursor_row > 0 {
                        self.playground_cursor_row -= 1;
                        self.playground_cursor_col = self.playground_cursor_col.min(self.playground_buffer[self.playground_cursor_row].chars().count());
                    }
                }
                KeyCode::Down => {
                    if self.playground_cursor_row + 1 < self.playground_buffer.len() {
                        self.playground_cursor_row += 1;
                        self.playground_cursor_col = self.playground_cursor_col.min(self.playground_buffer[self.playground_cursor_row].chars().count());
                    }
                }
                KeyCode::Home => {
                    self.playground_cursor_col = 0;
                }
                KeyCode::End => {
                    if let Some(line) = self.playground_buffer.get(self.playground_cursor_row) {
                        self.playground_cursor_col = line.chars().count();
                    }
                }
                KeyCode::Enter => {
                    if self.playground_cursor_row < self.playground_buffer.len() {
                        let line = &mut self.playground_buffer[self.playground_cursor_row];
                        let col = self.playground_cursor_col.min(line.chars().count());
                        let byte_idx = char_to_byte_idx(line, col);
                        let remainder = line.split_off(byte_idx);
                        let insert_pos = self.playground_cursor_row + 1;
                        self.playground_buffer.insert(insert_pos, remainder);
                        self.playground_cursor_row = insert_pos;
                        self.playground_cursor_col = 0;
                        self.playground_converted = false;
                        self.playground_vm_executed = false;
                    }
                }
                KeyCode::Backspace => {
                    if self.playground_cursor_row < self.playground_buffer.len() {
                        let col = self.playground_cursor_col.min(self.playground_buffer[self.playground_cursor_row].chars().count());
                        if col > 0 {
                            let line = &mut self.playground_buffer[self.playground_cursor_row];
                            let byte_start = char_to_byte_idx(line, col - 1);
                            let byte_end = char_to_byte_idx(line, col);
                            line.drain(byte_start..byte_end);
                            self.playground_cursor_col = col - 1;
                            self.playground_converted = false;
                            self.playground_vm_executed = false;
                        } else if self.playground_cursor_row > 0 {
                            let current_line = self.playground_buffer.remove(self.playground_cursor_row);
                            self.playground_cursor_row -= 1;
                            let prev_chars = self.playground_buffer[self.playground_cursor_row].chars().count();
                            self.playground_buffer[self.playground_cursor_row].push_str(&current_line);
                            self.playground_cursor_col = prev_chars;
                            self.playground_converted = false;
                            self.playground_vm_executed = false;
                        } else if self.playground_cursor_row == 0 && self.playground_buffer.len() > 1 && self.playground_buffer[0].is_empty() {
                            self.playground_buffer.remove(0);
                            self.playground_cursor_col = 0;
                            self.playground_converted = false;
                            self.playground_vm_executed = false;
                        }
                    }
                }
                KeyCode::Delete => {
                    if self.playground_cursor_row < self.playground_buffer.len() {
                        let line_chars = self.playground_buffer[self.playground_cursor_row].chars().count();
                        let col = self.playground_cursor_col.min(line_chars);
                        if col < line_chars {
                            let line = &mut self.playground_buffer[self.playground_cursor_row];
                            let byte_start = char_to_byte_idx(line, col);
                            let byte_end = char_to_byte_idx(line, col + 1);
                            line.drain(byte_start..byte_end);
                            self.playground_converted = false;
                            self.playground_vm_executed = false;
                        } else if col == line_chars && self.playground_cursor_row + 1 < self.playground_buffer.len() {
                            let next_line = self.playground_buffer.remove(self.playground_cursor_row + 1);
                            self.playground_buffer[self.playground_cursor_row].push_str(&next_line);
                            self.playground_converted = false;
                            self.playground_vm_executed = false;
                        }
                    }
                }
                KeyCode::Char(c) => {
                    if self.playground_cursor_row < self.playground_buffer.len() {
                        let line = &mut self.playground_buffer[self.playground_cursor_row];
                        let col = self.playground_cursor_col.min(line.chars().count());
                        let byte_idx = char_to_byte_idx(line, col);
                        line.insert(byte_idx, c);
                        self.playground_cursor_col = col + 1;
                        self.playground_converted = false;
                        self.playground_vm_executed = false;
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
                    KeyCode::Char('6') => self.switch_category(BenchmarkCategory::DeadCodeElimination),
                    KeyCode::Char('7') => self.switch_category(BenchmarkCategory::StrengthReduction),
                    KeyCode::Char('8') => self.switch_category(BenchmarkCategory::Combined),
                    KeyCode::Char('9') => self.switch_category(BenchmarkCategory::ControlFlow),
                    KeyCode::Char('g') | KeyCode::Char('G') => {
                        self.screen = CurrentScreen::AnalyticsDeck;
                    }
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
                    KeyCode::Enter
                        if items_count > 0 => {
                            self.code_scroll_offset = 0;
                            self.audit_scroll_offset = 0;
                            self.deep_dive_executed = false;
                            self.deep_dive_orig_time_us = 0;
                            self.deep_dive_opt_time_us = 0;
                            self.deep_dive_vector_results.clear();
                            self.screen = CurrentScreen::ProgramDeepDive;
                        }
                    _ => {}
                }
            }
            CurrentScreen::Playground => {
                if modifiers.contains(KeyModifiers::CONTROL) {
                    match key {
                        KeyCode::Char('v') | KeyCode::Char('V') => {
                            if let Some(text) = get_clipboard_text() {
                                self.handle_paste(&text);
                            }
                            return;
                        }
                        KeyCode::Char('c') | KeyCode::Char('C') => {
                            self.copy_playground_to_clipboard();
                            return;
                        }
                        _ => {}
                    }
                }
                match key {
                    KeyCode::Esc => {
                        self.screen = CurrentScreen::Home;
                    }
                    KeyCode::Char('i') => {
                        self.playground_editing = true;
                        if self.playground_cursor_row < self.playground_buffer.len() {
                            self.playground_cursor_col = self.playground_cursor_col.min(self.playground_buffer[self.playground_cursor_row].chars().count());
                        }
                    }
                    KeyCode::Char('a') => {
                        self.playground_editing = true;
                        if self.playground_cursor_row < self.playground_buffer.len() {
                            let len = self.playground_buffer[self.playground_cursor_row].chars().count();
                            self.playground_cursor_col = (self.playground_cursor_col + 1).min(len);
                        }
                    }
                    KeyCode::Char('A') => {
                        self.playground_editing = true;
                        if self.playground_cursor_row < self.playground_buffer.len() {
                            self.playground_cursor_col = self.playground_buffer[self.playground_cursor_row].chars().count();
                        }
                    }
                    KeyCode::Char('I') => {
                        self.playground_editing = true;
                        self.playground_cursor_col = 0;
                    }
                    KeyCode::Char('o') => {
                        let insert_pos = (self.playground_cursor_row + 1).min(self.playground_buffer.len());
                        self.playground_buffer.insert(insert_pos, String::new());
                        self.playground_cursor_row = insert_pos;
                        self.playground_cursor_col = 0;
                        self.playground_editing = true;
                        self.playground_converted = false;
                        self.playground_vm_executed = false;
                    }
                    KeyCode::Char('O') => {
                        self.playground_buffer.insert(self.playground_cursor_row, String::new());
                        self.playground_cursor_col = 0;
                        self.playground_editing = true;
                        self.playground_converted = false;
                        self.playground_vm_executed = false;
                    }
                    KeyCode::Char('d') => {
                        if !self.playground_buffer.is_empty() {
                            self.playground_buffer.remove(self.playground_cursor_row);
                            if self.playground_buffer.is_empty() {
                                self.playground_buffer.push(String::new());
                            }
                            self.playground_cursor_row = self.playground_cursor_row.min(self.playground_buffer.len() - 1);
                            self.playground_cursor_col = self.playground_cursor_col.min(self.playground_buffer[self.playground_cursor_row].chars().count());
                            self.playground_converted = false;
                            self.playground_vm_executed = false;
                        }
                    }
                    KeyCode::Char('y') => {
                        self.copy_playground_to_clipboard();
                    }
                    KeyCode::Char('p') => {
                        if let Some(text) = get_clipboard_text() {
                            self.handle_paste(&text);
                        }
                    }
                    KeyCode::Char('l') | KeyCode::Char('L') => {
                        self.load_file_modal_open = true;
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.playground_cursor_row = self.playground_cursor_row.saturating_sub(1);
                        if self.playground_cursor_row < self.playground_buffer.len() {
                            self.playground_cursor_col = self.playground_cursor_col.min(self.playground_buffer[self.playground_cursor_row].chars().count());
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if self.playground_cursor_row + 1 < self.playground_buffer.len() {
                            self.playground_cursor_row += 1;
                            self.playground_cursor_col = self.playground_cursor_col.min(self.playground_buffer[self.playground_cursor_row].chars().count());
                        }
                    }
                    KeyCode::Left | KeyCode::Char('h') => {
                        self.playground_cursor_col = self.playground_cursor_col.saturating_sub(1);
                    }
                    KeyCode::Right => {
                        if self.playground_cursor_row < self.playground_buffer.len() {
                            let len = self.playground_buffer[self.playground_cursor_row].chars().count();
                            self.playground_cursor_col = (self.playground_cursor_col + 1).min(len);
                        }
                    }
                    KeyCode::Home => {
                        self.playground_cursor_col = 0;
                    }
                    KeyCode::End => {
                        if self.playground_cursor_row < self.playground_buffer.len() {
                            self.playground_cursor_col = self.playground_buffer[self.playground_cursor_row].chars().count();
                        }
                    }
                    KeyCode::Char('c') | KeyCode::Char('C') => {
                        self.convert_playground();
                    }
                    KeyCode::Char('r') | KeyCode::Char('R') => {
                        self.run_playground_vm();
                    }
                    KeyCode::Char('x') | KeyCode::Char('X') => {
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
                        self.playground_cursor_col = 0;
                        self.recompile_playground();
                    }
                    _ => {}
                }
            }
            CurrentScreen::ProgramDeepDive => match key {
                KeyCode::Esc | KeyCode::Backspace | KeyCode::Char('h') | KeyCode::Left => {
                    self.screen = CurrentScreen::BenchmarkSuite;
                }
                KeyCode::Char('r') | KeyCode::Char('R') => {
                    self.run_deep_dive_vm();
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
            CurrentScreen::AnalyticsDeck => match key {
                KeyCode::Esc | KeyCode::Backspace | KeyCode::Char('g') | KeyCode::Char('G') => {
                    self.screen = CurrentScreen::BenchmarkSuite;
                }
                KeyCode::Char('1') => {
                    self.switch_category(BenchmarkCategory::All);
                    self.screen = CurrentScreen::BenchmarkSuite;
                }
                KeyCode::Char('2') => {
                    self.switch_category(BenchmarkCategory::ConstantFolding);
                    self.screen = CurrentScreen::BenchmarkSuite;
                }
                KeyCode::Char('3') => {
                    self.switch_category(BenchmarkCategory::ConstantPropagation);
                    self.screen = CurrentScreen::BenchmarkSuite;
                }
                KeyCode::Char('4') => {
                    self.switch_category(BenchmarkCategory::AlgebraicSimplification);
                    self.screen = CurrentScreen::BenchmarkSuite;
                }
                KeyCode::Char('5') => {
                    self.switch_category(BenchmarkCategory::LocalCSE);
                    self.screen = CurrentScreen::BenchmarkSuite;
                }
                KeyCode::Char('6') => {
                    self.switch_category(BenchmarkCategory::DeadCodeElimination);
                    self.screen = CurrentScreen::BenchmarkSuite;
                }
                KeyCode::Char('7') => {
                    self.switch_category(BenchmarkCategory::StrengthReduction);
                    self.screen = CurrentScreen::BenchmarkSuite;
                }
                KeyCode::Char('8') => {
                    self.switch_category(BenchmarkCategory::Combined);
                    self.screen = CurrentScreen::BenchmarkSuite;
                }
                KeyCode::Char('9') => {
                    self.switch_category(BenchmarkCategory::ControlFlow);
                    self.screen = CurrentScreen::BenchmarkSuite;
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
            CurrentScreen::AnalyticsDeck => render_analytics(frame, self, area),
        }

        if self.help_modal_open {
            self.render_help_modal(frame, area);
        }
    }

    fn render_help_modal(&self, frame: &mut Frame, area: Rect) {
        let popup_width = 72.min(area.width.saturating_sub(4));
        let popup_height = 25.min(area.height.saturating_sub(2));
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
                Span::styled(" [Enter]      ", Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
                Span::styled("View More: Expand program to full-screen diff inspector", Style::default().fg(COLOR_TEXT_BODY)),
            ]),
            Line::from(vec![
                Span::styled(" [R]          ", Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
                Span::styled("Run Dual VM & Verify Equivalence (Deep Dive & Playground)", Style::default().fg(COLOR_TEXT_BODY)),
            ]),
            Line::from(vec![
                Span::styled(" [C]          ", Style::default().fg(COLOR_WARNING).add_modifier(Modifier::BOLD)),
                Span::styled("Convert TAC buffer: Run optimizations & generate audit trail", Style::default().fg(COLOR_TEXT_BODY)),
            ]),
            Line::from(vec![
                Span::styled(" [i]          ", Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)),
                Span::styled("Enter in-place buffer edit mode in Playground (Esc to exit)", Style::default().fg(COLOR_TEXT_BODY)),
            ]),
            Line::from(vec![
                Span::styled(" [X]          ", Style::default().fg(COLOR_WARNING).add_modifier(Modifier::BOLD)),
                Span::styled("Reset sample buffer in Playground", Style::default().fg(COLOR_TEXT_BODY)),
            ]),
            Line::from(vec![
                Span::styled(" [L]          ", Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
                Span::styled("Load program file path in Playground", Style::default().fg(COLOR_TEXT_BODY)),
            ]),
            Line::from(vec![
                Span::styled(" [Esc]        ", Style::default().fg(COLOR_INDIGO).add_modifier(Modifier::BOLD)),
                Span::styled("Collapse full-screen / Return to previous view / Home", Style::default().fg(COLOR_TEXT_BODY)),
            ]),
            Line::from(vec![
                Span::styled(" [G]          ", Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)),
                Span::styled("Open Global Analytics Deck (All 6 PRD Evaluation Plots)", Style::default().fg(COLOR_TEXT_BODY)),
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
    use crate::tui::data::BenchmarkCategory;

    #[test]
    fn test_home_navigation_and_mode_switching() {
        let mut app = App::new(BenchmarkCorpus::default());
        assert_eq!(app.screen, CurrentScreen::Home);
        assert_eq!(app.selected_home_card, 0);

        app.handle_key(KeyCode::Tab);
        assert_eq!(app.selected_home_card, 1);

        app.handle_key(KeyCode::Left);
        assert_eq!(app.selected_home_card, 0);

        app.handle_key(KeyCode::Char('1'));
        assert_eq!(app.screen, CurrentScreen::Playground);

        app.handle_key(KeyCode::Esc);
        assert_eq!(app.screen, CurrentScreen::Home);

        app.handle_key(KeyCode::Char('2'));
        assert_eq!(app.screen, CurrentScreen::Optimizing);

        app.handle_key(KeyCode::Char(' '));
        assert_eq!(app.screen, CurrentScreen::BenchmarkSuite);

        app.handle_key(KeyCode::Esc);
        assert_eq!(app.screen, CurrentScreen::Home);
    }

    #[test]
    fn test_optimizing_screen_toggle() {
        let mut app = App::new(BenchmarkCorpus::default());
        assert!(app.show_optimizing_screen);

        app.handle_key(KeyCode::Char('2'));
        assert_eq!(app.screen, CurrentScreen::Optimizing);

        app.handle_key(KeyCode::Char('o'));
        assert!(!app.show_optimizing_screen);

        app.handle_key(KeyCode::Esc);
        assert_eq!(app.screen, CurrentScreen::BenchmarkSuite);

        app.handle_key(KeyCode::Esc);
        assert_eq!(app.screen, CurrentScreen::Home);

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
    fn test_strength_reduction_pass() {
        let source = "read a\nt1 = a * 8\nt2 = a * 2\nt3 = a / 4\nprint t1\nprint t2\nprint t3";
        let mut prog = parse_program(source).unwrap();
        let cfg = build(&mut prog).unwrap();
        let report = optimize_with_worklist(&mut prog, &cfg);

        let opt_str = prog.to_string();
        assert!(opt_str.contains("t1 = a << 3"));
        assert!(opt_str.contains("t2 = a + a"));
        assert!(opt_str.contains("t3 = a >> 2"));
        assert!(report.transformations.iter().any(|t| t.pass == crate::passes::OptimizationKind::StrengthReduction));
    }

    #[test]
    fn test_5_vector_verification_proof() {
        let corpus = BenchmarkCorpus::load_from_dir("benchmarks");
        assert!(!corpus.items.is_empty());
        for item in &corpus.items {
            assert_eq!(item.test_vectors.len(), 5, "Item {} must have 5 test vectors", item.name);
            assert!(item.verification_passed, "Item {} must pass verification across all 5 vectors", item.name);
            for v in &item.test_vectors {
                assert!(v.passed, "Vector {} in {} must match", v.vector_id, item.name);
                assert_eq!(v.original_output, v.optimized_output);
            }
        }
    }
}