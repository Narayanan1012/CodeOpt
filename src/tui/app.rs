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
    pub reset_confirm_modal_open: bool,
    pub playground_fullscreen: bool,
    pub playground_scroll_row: usize,
    pub playground_scroll_col: usize,
}

impl App {
    pub fn new(corpus: BenchmarkCorpus) -> Self {
        let app = Self {
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
            playground_file_name: "scratchpad".to_string(),
            playground_buffer: vec![String::new()],
            playground_cursor_row: 0,
            playground_cursor_col: 0,
            playground_editing: false,
            playground_inputs: vec![4, 9],
            playground_opt_lines: Vec::new(),
            playground_transforms: Vec::new(),
            playground_parse_error: None,
            playground_orig_out: Vec::new(),
            playground_opt_out: Vec::new(),
            playground_equiv_pass: true,
            playground_converted: false,
            playground_vm_executed: false,
            playground_status_message: Some(
                "ENTER C OR TAC CODE — PRESS 'i' TO EDIT, 'C' TO CONVERT".to_string(),
            ),
            load_file_modal_open: false,
            load_file_path_input: "samples/demo_math.c".to_string(),
            reset_confirm_modal_open: false,
            playground_fullscreen: false,
            playground_scroll_row: 0,
            playground_scroll_col: 0,
        };
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
        let mut source = self.playground_buffer.join("\n");
        if source.trim().is_empty() {
            self.playground_parse_error = Some("Input buffer is empty. Please enter C or TAC code first.".to_string());
            self.playground_opt_lines.clear();
            self.playground_transforms.clear();
            self.playground_converted = false;
            self.playground_vm_executed = false;
            return;
        }

        let was_c_code = crate::c_frontend::looks_like_c_code(&source);
        if was_c_code {
            match crate::c_frontend::lower_c_to_tac(&source) {
                Ok(tac) => {
                    self.playground_buffer = tac.lines().map(String::from).collect();
                    self.playground_status_message =
                        Some("LOWERED C SOURCE TO TAC (NATIVE ENGINE)".to_string());
                    source = tac;
                }
                Err(e) => {
                    // Fall back to dormant GCC backup compiler
                    match crate::c_frontend::lower_c_source_via_gcc_backup(&source) {
                        Ok(tac) => {
                            self.playground_buffer = tac.lines().map(String::from).collect();
                            self.playground_status_message =
                                Some("LOWERED C SOURCE VIA GCC BACKUP COMPILER".to_string());
                            source = tac;
                        }
                        Err(gcc_err) => {
                            self.playground_parse_error = Some(format!(
                                "C Frontend Error:\n• Native Lowerer: {e}\n• GCC Backup Lowerer: {gcc_err}\n\nNote: CodeOpt TAC optimizes scalar integer computations, wrapping arithmetic, and control flow. Recursive functions, dynamic arrays, and heap pointer dereferences exceed the flat scalar TAC model."
                            ));
                            self.playground_opt_lines.clear();
                            self.playground_transforms.clear();
                            self.playground_converted = false;
                            self.playground_vm_executed = false;
                            return;
                        }
                    }
                }
            }
        } else {
            // Check if user pasted raw GIMPLE / GCC dump with artifacts
            if source.contains(">;_[") || source.contains("MEM_") || source.contains("MEM[") || source.contains("<bb ") {
                source = crate::c_frontend::sanitize_gimple_tac(&source);
                self.playground_buffer = source.lines().map(String::from).collect();
                self.playground_status_message =
                    Some("SANITIZED & NORMALIZED GCC DUMP TAC".to_string());
            } else {
                self.playground_status_message =
                    Some("OPTIMIZED TAC INSTRUCTIONS".to_string());
            }
        }
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
                        let is_c = path.ends_with(".c") || crate::c_frontend::looks_like_c_code(&content);
                        if is_c {
                            match crate::c_frontend::lower_c_to_tac(&content) {
                                Ok(tac) => {
                                    self.playground_buffer = tac.lines().map(String::from).collect();
                                    self.playground_status_message =
                                        Some("LOWERED C SOURCE TO TAC (NATIVE ENGINE)".to_string());
                                }
                                Err(err) => {
                                    self.playground_buffer = content.lines().map(String::from).collect();
                                    self.playground_status_message =
                                        Some(format!("C LOWERING ERROR: {err}"));
                                }
                            }
                        } else {
                            self.playground_buffer = content.lines().map(String::from).collect();
                        }
                        self.playground_cursor_row = 0;
                        self.playground_cursor_col = 0;
                        self.recompile_playground();
                        if is_c {
                            self.playground_status_message =
                                Some("LOWERED C SOURCE TO TAC (NATIVE ENGINE)".to_string());
                        }
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

        // Reset confirmation modal in playground
        if self.screen == CurrentScreen::Playground && self.reset_confirm_modal_open {
            match key {
                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                    self.playground_buffer = vec![String::new()];
                    self.playground_cursor_row = 0;
                    self.playground_cursor_col = 0;
                    self.playground_opt_lines.clear();
                    self.playground_transforms.clear();
                    self.playground_parse_error = None;
                    self.playground_orig_out.clear();
                    self.playground_opt_out.clear();
                    self.playground_converted = false;
                    self.playground_vm_executed = false;
                    self.playground_status_message =
                        Some("BUFFER CLEARED — ENTER C OR TAC CODE".to_string());
                    self.reset_confirm_modal_open = false;
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    self.reset_confirm_modal_open = false;
                    self.playground_status_message = Some("RESET CANCELLED".to_string());
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
                    KeyCode::Char('f') | KeyCode::Char('F') => {
                        self.playground_fullscreen = !self.playground_fullscreen;
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
                KeyCode::F(11) => {
                    self.playground_fullscreen = !self.playground_fullscreen;
                }
                KeyCode::PageUp => {
                    self.playground_cursor_row = self.playground_cursor_row.saturating_sub(10);
                    if self.playground_cursor_row < self.playground_buffer.len() {
                        self.playground_cursor_col = self.playground_cursor_col.min(self.playground_buffer[self.playground_cursor_row].chars().count());
                    }
                }
                KeyCode::PageDown => {
                    self.playground_cursor_row = (self.playground_cursor_row + 10).min(self.playground_buffer.len().saturating_sub(1));
                    if self.playground_cursor_row < self.playground_buffer.len() {
                        self.playground_cursor_col = self.playground_cursor_col.min(self.playground_buffer[self.playground_cursor_row].chars().count());
                    }
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
                        if self.playground_fullscreen {
                            self.playground_fullscreen = false;
                            self.playground_status_message = Some("EXITED FULLSCREEN — SPLIT VIEW ACTIVE".to_string());
                        } else {
                            self.screen = CurrentScreen::Home;
                        }
                    }
                    KeyCode::Char('f') | KeyCode::Char('F') | KeyCode::F(11) => {
                        self.playground_fullscreen = !self.playground_fullscreen;
                        if self.playground_fullscreen {
                            self.playground_status_message = Some("FULLSCREEN EDITOR ACTIVE — PRESS 'f' OR 'Esc' FOR SPLIT VIEW".to_string());
                        } else {
                            self.playground_status_message = Some("SPLIT VIEW ACTIVE — PRESS 'f' FOR FULLSCREEN EDITOR".to_string());
                        }
                    }
                    KeyCode::PageUp => {
                        self.playground_cursor_row = self.playground_cursor_row.saturating_sub(10);
                        if self.playground_cursor_row < self.playground_buffer.len() {
                            self.playground_cursor_col = self.playground_cursor_col.min(self.playground_buffer[self.playground_cursor_row].chars().count());
                        }
                    }
                    KeyCode::PageDown => {
                        self.playground_cursor_row = (self.playground_cursor_row + 10).min(self.playground_buffer.len().saturating_sub(1));
                        if self.playground_cursor_row < self.playground_buffer.len() {
                            self.playground_cursor_col = self.playground_cursor_col.min(self.playground_buffer[self.playground_cursor_row].chars().count());
                        }
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
                        self.reset_confirm_modal_open = true;
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
    use crate::tui::data::metrics::ProgramStats;

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
        let mut total_orig_instr = 0;
        let mut total_opt_instr = 0;
        let mut total_orig_arith = 0;
        let mut total_opt_arith = 0;
        let mut total_orig_temps = 0;
        let mut total_opt_temps = 0;
        let mut total_orig_size = 0;
        let mut total_opt_size = 0;
        let mut total_orig_time = 0;
        let mut total_opt_time = 0;
        let mut total_vectors = 0;
        let mut passed_vectors = 0;

        for item in &corpus.items {
            total_orig_instr += item.original_stats.instruction_count;
            total_opt_instr += item.optimized_stats.instruction_count;
            total_orig_arith += item.original_stats.arithmetic_count;
            total_opt_arith += item.optimized_stats.arithmetic_count;
            total_orig_temps += item.original_stats.temp_count;
            total_opt_temps += item.optimized_stats.temp_count;
            total_orig_size += item.original_stats.code_size_bytes;
            total_opt_size += item.optimized_stats.code_size_bytes;
            total_orig_time += item.time_original_us;
            total_opt_time += item.time_optimized_us;

            assert_eq!(item.test_vectors.len(), 5, "Item {} must have 5 test vectors", item.name);
            assert!(item.verification_passed, "Item {} must pass verification across all 5 vectors", item.name);
            for v in &item.test_vectors {
                total_vectors += 1;
                if v.passed {
                    passed_vectors += 1;
                }
                assert!(v.passed, "Vector {} in {} must match", v.vector_id, item.name);
                assert_eq!(v.original_output, v.optimized_output);
            }
        }
        assert_eq!(corpus.items.len(), 50);
        assert_eq!(total_vectors, 250);
        assert_eq!(passed_vectors, 250);

        let instr_red = ProgramStats::reduction_pct(total_orig_instr, total_opt_instr);
        let arith_red = ProgramStats::reduction_pct(total_orig_arith, total_opt_arith);
        let temps_red = ProgramStats::reduction_pct(total_orig_temps, total_opt_temps);
        let size_red = ProgramStats::reduction_pct(total_orig_size, total_opt_size);

        // Target: 20-50% instruction count reduction
        assert!((20.0..=50.0).contains(&instr_red), "Instr reduction was {:.1}%", instr_red);
        // Target: 30-60% arithmetic reduction (or better)
        assert!(arith_red >= 30.0, "Arith reduction was {:.1}%", arith_red);
        // Target: 20-40% temporary reduction (or better)
        assert!(temps_red >= 20.0, "Temps reduction was {:.1}%", temps_red);
        // Target: 15-40% code size reduction
        assert!((15.0..=45.0).contains(&size_red), "Size reduction was {:.1}%", size_red);
        // Measurable execution time speedup
        assert!(total_orig_time > 0 && total_opt_time > 0);
    }

    #[test]
    fn test_playground_c_code_conversion() {
        let mut app = App::new(BenchmarkCorpus::default());
        app.screen = CurrentScreen::Playground;

        // User pastes C code into playground buffer
        app.playground_buffer = vec![
            "#include <stdio.h>".to_string(),
            "int main() {".to_string(),
            "    int x;".to_string(),
            "    scanf(\"%d\", &x);".to_string(),
            "    int y = x * 8;".to_string(),
            "    printf(\"%d\\n\", y);".to_string(),
            "    return 0;".to_string(),
            "}".to_string(),
        ];
        app.playground_converted = false;

        // User presses 'C' to convert and optimize
        app.convert_playground();

        assert!(app.playground_converted);
        assert!(app.playground_parse_error.is_none());
        assert_eq!(
            app.playground_status_message.as_deref(),
            Some("LOWERED C SOURCE TO TAC (NATIVE ENGINE)")
        );
        // Verify buffer now has lowered TAC
        assert!(app.playground_buffer.iter().any(|l| l.contains("read x")));
        assert!(app.playground_buffer.iter().any(|l| l.contains("y = x * 8")));

        // Verify optimization ran on the lowered code
        assert!(app.playground_opt_lines.iter().any(|l| l.contains("y = x << 3")));

        // Run dual VM with test inputs [4] -> 4 * 8 = 32
        app.playground_inputs = vec![4];
        app.run_playground_vm();
        assert!(app.playground_vm_executed);
        assert!(app.playground_equiv_pass);
        assert_eq!(app.playground_orig_out, vec![32]);
        assert_eq!(app.playground_opt_out, vec![32]);
    }

    #[test]
    fn test_playground_load_c_file() {
        let mut app = App::new(BenchmarkCorpus::default());
        app.screen = CurrentScreen::Playground;
        app.load_file_modal_open = true;
        app.load_file_path_input = "samples/demo_math.c".to_string();

        app.handle_key(KeyCode::Enter);
        assert!(!app.load_file_modal_open);
        assert_eq!(
            app.playground_status_message.as_deref(),
            Some("LOWERED C SOURCE TO TAC (NATIVE ENGINE)")
        );
        assert!(app.playground_converted);
        assert!(app.playground_vm_executed);
    }

    #[test]
    fn test_playground_reset_confirmation_confirm() {
        let mut app = App::new(BenchmarkCorpus::default());
        app.screen = CurrentScreen::Playground;
        app.playground_buffer = vec!["read a".to_string(), "print a".to_string()];
        app.convert_playground();
        assert!(app.playground_converted);

        // User presses 'x' to reset
        app.handle_key(KeyCode::Char('x'));
        assert!(app.reset_confirm_modal_open);
        // Code is not yet wiped
        assert_eq!(app.playground_buffer.len(), 2);

        // User confirms with 'y'
        app.handle_key(KeyCode::Char('y'));
        assert!(!app.reset_confirm_modal_open);
        assert_eq!(app.playground_buffer, vec![String::new()]);
        assert_eq!(
            app.playground_status_message.as_deref(),
            Some("BUFFER CLEARED — ENTER C OR TAC CODE")
        );
        assert!(!app.playground_converted);
    }

    #[test]
    fn test_playground_reset_confirmation_cancel() {
        let mut app = App::new(BenchmarkCorpus::default());
        app.screen = CurrentScreen::Playground;
        app.playground_buffer = vec!["read a".to_string(), "print a".to_string()];
        app.convert_playground();

        // User presses 'x' to reset
        app.handle_key(KeyCode::Char('x'));
        assert!(app.reset_confirm_modal_open);

        // User cancels with Esc
        app.handle_key(KeyCode::Esc);
        assert!(!app.reset_confirm_modal_open);
        // Code remains intact
        assert_eq!(app.playground_buffer.len(), 2);
        assert_eq!(
            app.playground_status_message.as_deref(),
            Some("RESET CANCELLED")
        );
    }

    #[test]
    fn test_playground_c_pointer_program_fallback() {
        let mut app = App::new(BenchmarkCorpus::default());
        app.screen = CurrentScreen::Playground;
        app.playground_buffer = vec![
            "#include <stdio.h>".to_string(),
            "void swap(int *a, int *b) {".to_string(),
            "    int t = *a;".to_string(),
            "    *a = *b;".to_string(),
            "    *b = t;".to_string(),
            "}".to_string(),
            "int main() {".to_string(),
            "    int x, y;".to_string(),
            "    scanf(\"%d %d\", &x, &y);".to_string(),
            "    swap(&x, &y);".to_string(),
            "    int res = x * 8 + y;".to_string(),
            "    printf(\"%d\\n\", res);".to_string(),
            "    return 0;".to_string(),
            "}".to_string(),
        ];
        app.convert_playground();

        assert!(app.playground_converted);
        assert!(app.playground_parse_error.is_none());
        assert_eq!(
            app.playground_status_message.as_deref(),
            Some("LOWERED C SOURCE VIA GCC BACKUP COMPILER")
        );
        assert!(app.playground_opt_lines.iter().any(|l| l.contains("<< 3") || l.contains("* 8")));

        app.playground_inputs = vec![3, 5];
        app.run_playground_vm();
        assert!(app.playground_vm_executed);
        assert!(app.playground_equiv_pass);
        assert_eq!(app.playground_orig_out, vec![43]);
        assert_eq!(app.playground_opt_out, vec![43]);
    }

    #[test]
    fn test_playground_gimple_dump_conversion_and_optimization() {
        let mut app = App::new(BenchmarkCorpus::default());
        app.screen = CurrentScreen::Playground;
        app.playground_buffer = vec![
            "read n".to_string(),
            "t1 = n".to_string(),
            "t2 = t1".to_string(),
            "t3 = t2 * 4".to_string(),
            "goto bb_3>;_[89_00%]".to_string(),
            "goto bb_23>;_[11_00%]".to_string(),
            "t4 = a_56".to_string(),
            "t5 = t6".to_string(),
            "read t5".to_string(),
            "i_87 = i_100 + 1".to_string(),
            "t7 = n".to_string(),
            "t8 = t6 + 4".to_string(),
            "goto bb_4>;_[89_00%]".to_string(),
            "goto bb_5>;_[11_00%]".to_string(),
            "goto bb_6>;_[66_29%]".to_string(),
            "goto bb_23>;_[33_71%]".to_string(),
            "low_58 = 0".to_string(),
            "t9 = t7 + -1".to_string(),
            "high_60 = t9".to_string(),
            "goto bb_16>;_[100_00%]".to_string(),
            "t10 = top_46".to_string(),
            "t11 = t10 * 4".to_string(),
            "t12 = low_58 + t11".to_string(),
            "l_69 = t12".to_string(),
            "top_70 = top_46 + -1".to_string(),
            "t13 = high_60 + t11".to_string(),
            "h_71 = t13".to_string(),
            "t14 = h_71".to_string(),
            "t15 = t14 * 4".to_string(),
            "t16 = a_56 + t15".to_string(),
            "i_73 = l_69 + -1".to_string(),
            "goto bb_8>;_[89_00%]".to_string(),
            "goto bb_22>;_[11_00%]".to_string(),
            "pivot_72 = t16".to_string(),
            "t17 = l_69".to_string(),
            "t18 = t17 * 4".to_string(),
            "t19 = a_56".to_string(),
            "t20 = t18 + t19".to_string(),
            "t21 = h_71".to_string(),
            "t22 = l_69".to_string(),
            "t23 = t21 - t22".to_string(),
            "t24 = t23".to_string(),
            "t25 = t17 + t24".to_string(),
            "t26 = t25 * 4".to_string(),
            "t27 = t19 + t26".to_string(),
            "t28 = t29".to_string(),
            "if (_21 < = pivot_72)".to_string(),
            "goto bb_10>;_[50_00%]".to_string(),
            "goto bb_11>;_[50_00%]".to_string(),
            "i_81 = i_101 + 1".to_string(),
            "t31 = i_81".to_string(),
            "t32 = t31 * 4".to_string(),
            "t33 = a_56 + t32".to_string(),
            "temp_82 = t33".to_string(),
            "t33 = t30".to_string(),
            "MEM_(int *)_121 = temp_82".to_string(),
            "t34 = t29 + 4".to_string(),
            "t35 = t36".to_string(),
            "goto bb_9>;_[89_00%]".to_string(),
            "goto bb_12>;_[11_00%]".to_string(),
            "t37 = i_41".to_string(),
            "t38 = t37 + 1".to_string(),
            "t39 = t38 * 4".to_string(),
            "t40 = a_56 + t39".to_string(),
            "temp_74 = t40".to_string(),
            "t41 = t16".to_string(),
            "t40 = t41".to_string(),
            "t16 = temp_74".to_string(),
            "goto bb_13>;_[56_18%]".to_string(),
            "goto bb_14>;_[43_82%]".to_string(),
            "t13 = i_41".to_string(),
            "t42 = i_93 + 2".to_string(),
            "goto bb_15>;_[5_50%]".to_string(),
            "goto bb_18>;_[94_50%]".to_string(),
            "top_78 = top_45 + 1".to_string(),
            "t43 = top_78".to_string(),
            "t44 = t43 * 4".to_string(),
            "t45 = low_58 + t44".to_string(),
            "t45 = t42".to_string(),
            "t46 = high_60 + t44".to_string(),
            "t46 = h_71".to_string(),
            "goto bb_17>;_[100_00%]".to_string(),
            "if (top_46 > = 0)".to_string(),
            "goto bb_7>;_[94_50%]".to_string(),
            "goto bb_19>;_[5_50%]".to_string(),
            "t47 = n".to_string(),
            "goto bb_20>;_[89_00%]".to_string(),
            "goto bb_21>;_[11_00%]".to_string(),
            "print t48".to_string(),
            "t49 = n".to_string(),
            "t50 = t51 + 1".to_string(),
            "i_114 = t50".to_string(),
            "goto bb_20>;_[89_00%]".to_string(),
            "goto bb_21>;_[11_00%]".to_string(),
            "t52 = l_69".to_string(),
            "t53 = t52 * 4".to_string(),
            "t54 = a_56 + t53".to_string(),
            "temp_47 = t54".to_string(),
            "t55 = t16".to_string(),
            "t54 = t55".to_string(),
            "t16 = temp_47".to_string(),
            "goto bb_14>;_[100_00%]".to_string(),
        ];

        app.convert_playground();

        // Must succeed without syntax error
        assert!(app.playground_converted);
        assert!(app.playground_parse_error.is_none());
        assert_eq!(
            app.playground_status_message.as_deref(),
            Some("SANITIZED & NORMALIZED GCC DUMP TAC")
        );

        // Optimizations must have fired
        assert!(!app.playground_transforms.is_empty());
        assert!(app.playground_opt_lines.iter().any(|l| l.contains("<< 2")));
    }

    #[test]
    fn test_analytics_deck_rendering_and_metrics() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        use crate::tui::views::analytics::render_analytics;

        let corpus = BenchmarkCorpus::load_from_dir("benchmarks");
        assert_eq!(corpus.items.len(), 50);

        let mut app = App::new(corpus);
        app.screen = CurrentScreen::AnalyticsDeck;

        // Verify keypress 'g' navigates to analytics
        app.screen = CurrentScreen::BenchmarkSuite;
        app.handle_key(KeyCode::Char('g'));
        assert_eq!(app.screen, CurrentScreen::AnalyticsDeck);

        // Test rendering across multiple terminal sizes
        for (w, h) in [(140, 45), (100, 32), (160, 50)] {
            let backend = TestBackend::new(w, h);
            let mut terminal = Terminal::new(backend).unwrap();

            terminal.draw(|f| {
                render_analytics(f, &app, f.area());
            }).unwrap();

            let buffer = terminal.backend().buffer();
            let content: String = buffer.content().iter().map(|c| c.symbol()).collect();

            // Verify Header & Navigation
            assert!(content.contains("CodeOpt Comprehensive Analytics"));
            assert!(content.contains("Corpus Total: 50"));

            // Verify all 6 Plots exist
            assert!(content.contains("1. Instrs"));
            assert!(content.contains("2. Stacked"));
            assert!(content.contains("3. Convergence"));
            assert!(content.contains("4. Pass Distribution (Pie)"));
            assert!(content.contains("5. Scatter"));
            assert!(content.contains("6. Heatmap"));

            // Verify Section 5 Aggregate Metrics Summary Table & Per-Optimizer Panel
            assert!(content.contains("Section 5: Aggregate Benchmark Metrics"));
            assert!(content.contains("Instruction Count"));
            assert!(content.contains("Arithmetic Operations"));
            assert!(content.contains("Temporary Variables"));
            assert!(content.contains("Code Size"));
            assert!(content.contains("Execution Time"));
            assert!(content.contains("Semantic Correctness"));
            assert!(content.contains("Per-Optimizer Effectiveness"));
            assert!(content.contains("PASS"));
        }
    }

    #[test]
    fn test_playground_fullscreen_and_scrolling() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        use crate::tui::views::playground::render_playground;

        let corpus = BenchmarkCorpus::load_from_dir("benchmarks");
        let mut app = App::new(corpus);

        // Navigate to Playground
        app.screen = CurrentScreen::Playground;
        assert!(!app.playground_fullscreen);

        // 1. Test toggling fullscreen with 'f'
        app.handle_key(KeyCode::Char('f'));
        assert!(app.playground_fullscreen);
        assert!(app.playground_status_message.as_ref().unwrap().contains("FULLSCREEN EDITOR ACTIVE"));

        // 2. Test toggling fullscreen back to split view with 'f'
        app.handle_key(KeyCode::Char('f'));
        assert!(!app.playground_fullscreen);
        assert!(app.playground_status_message.as_ref().unwrap().contains("SPLIT VIEW ACTIVE"));

        // 3. Test toggling fullscreen with 'f', then exiting fullscreen via 'Esc'
        app.handle_key(KeyCode::Char('f'));
        assert!(app.playground_fullscreen);
        app.handle_key(KeyCode::Esc);
        assert!(!app.playground_fullscreen);
        assert_eq!(app.screen, CurrentScreen::Playground);

        // 4. Pressing Esc when already in split view exits to Home
        app.handle_key(KeyCode::Esc);
        assert_eq!(app.screen, CurrentScreen::Home);

        // Return to Playground
        app.screen = CurrentScreen::Playground;

        // 5. Populate a large 60-line program to test vertical scrolling
        app.playground_buffer = (1..=60)
            .map(|i| format!("int v{} = {} * 2;", i, i))
            .collect();
        app.playground_cursor_row = 0;
        app.playground_cursor_col = 0;

        // Navigate down past viewport using PageDown
        app.handle_key(KeyCode::PageDown);
        assert_eq!(app.playground_cursor_row, 10);
        app.handle_key(KeyCode::PageDown);
        assert_eq!(app.playground_cursor_row, 20);

        // Navigate down using Down key
        for _ in 0..5 {
            app.handle_key(KeyCode::Down);
        }
        assert_eq!(app.playground_cursor_row, 25);

        // Navigate up using PageUp
        app.handle_key(KeyCode::PageUp);
        assert_eq!(app.playground_cursor_row, 15);

        // 6. Test rendering split view with scrolled buffer (terminal height 25)
        let backend = TestBackend::new(120, 25);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| {
            render_playground(f, &app, f.area());
        }).unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = buffer.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("Input Buffer (C or TAC)"));
        assert!(content.contains("60 lines"));

        // 7. Toggle Fullscreen mode with large buffer and render
        app.handle_key(KeyCode::Char('f'));
        assert!(app.playground_fullscreen);

        let backend = TestBackend::new(120, 35);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| {
            render_playground(f, &app, f.area());
        }).unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = buffer.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("FULL-SCREEN EDITOR"));
        assert!(content.contains("60 lines"));
        assert!(content.contains("FULLSCREEN ACTIVE"));

        // 8. Test edit mode and cursor blinker toggle
        app.handle_key(KeyCode::Char('i'));
        assert!(app.playground_editing);

        // Verify tick increments and alternates blinker
        let initial_tick = app.tick_count;
        let initial_blinker = (initial_tick / 3) % 2 == 0;
        for _ in 0..3 {
            app.tick();
        }
        assert_eq!((app.tick_count / 3) % 2 == 0, !initial_blinker);

        // 9. Convert & run VM inside fullscreen mode
        app.convert_playground();
        assert!(app.playground_converted);
    }
}