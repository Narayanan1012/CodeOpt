//! Theme definitions, color palettes, and styling constants for CodeOpt TUI.

use ratatui::style::{Color, Modifier, Style};

// Primary palette
pub const COLOR_BG: Color = Color::Rgb(15, 23, 42);          // Slate 900
pub const COLOR_PANEL_BG: Color = Color::Rgb(30, 41, 59);    // Slate 800
pub const COLOR_BORDER: Color = Color::Rgb(71, 85, 105);      // Slate 600
pub const COLOR_BORDER_ACTIVE: Color = Color::Rgb(56, 189, 248); // Sky 400

// Brand & Accents
pub const COLOR_CYAN: Color = Color::Rgb(56, 189, 248);      // Sky 400
pub const COLOR_INDIGO: Color = Color::Rgb(129, 140, 248);   // Indigo 400
pub const COLOR_PURPLE: Color = Color::Rgb(192, 132, 252);   // Purple 400
pub const COLOR_PINK: Color = Color::Rgb(244, 114, 182);     // Pink 400

// Text shades
pub const COLOR_TEXT_HEAD: Color = Color::Rgb(255, 255, 255);
pub const COLOR_TEXT_BODY: Color = Color::Rgb(226, 232, 240); // Slate 200
pub const COLOR_TEXT_MUTED: Color = Color::Rgb(148, 163, 184);// Slate 400

// Semantic status colors
pub const COLOR_SUCCESS: Color = Color::Rgb(52, 211, 153);   // Emerald 400 (PASS)
pub const COLOR_WARNING: Color = Color::Rgb(251, 191, 36);   // Amber 400 (Rewrite / Cap)
pub const COLOR_DANGER: Color = Color::Rgb(248, 113, 113);    // Red 400 (FAIL)
pub const COLOR_COMBINED: Color = Color::Rgb(236, 72, 153);  // Pink 500 (Multi-Pass)

// Preset Styles
pub fn style_header() -> Style {
    Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)
}

pub fn style_title() -> Style {
    Style::default().fg(COLOR_TEXT_HEAD).add_modifier(Modifier::BOLD)
}

pub fn style_body() -> Style {
    Style::default().fg(COLOR_TEXT_BODY)
}

pub fn style_muted() -> Style {
    Style::default().fg(COLOR_TEXT_MUTED)
}

pub fn style_border_active() -> Style {
    Style::default().fg(COLOR_BORDER_ACTIVE).add_modifier(Modifier::BOLD)
}

pub fn style_border_inactive() -> Style {
    Style::default().fg(COLOR_BORDER)
}

pub fn style_success() -> Style {
    Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)
}

pub fn style_warning() -> Style {
    Style::default().fg(COLOR_WARNING).add_modifier(Modifier::BOLD)
}

pub fn style_danger() -> Style {
    Style::default().fg(COLOR_DANGER).add_modifier(Modifier::BOLD)
}

pub fn style_combined() -> Style {
    Style::default().fg(COLOR_COMBINED).add_modifier(Modifier::BOLD)
}
