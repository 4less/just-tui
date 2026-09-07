//! One place for every colour the app uses.

use ratatui::style::{Color, Modifier, Style};

pub const BG: Color = Color::Reset;
pub const FG: Color = Color::Rgb(0xd0, 0xd4, 0xdc);
pub const DIM: Color = Color::Rgb(0x6b, 0x74, 0x86);
pub const BORDER: Color = Color::Rgb(0x3a, 0x41, 0x50);
pub const BORDER_FOCUS: Color = Color::Rgb(0x7a, 0xa2, 0xf7);
pub const ACCENT: Color = Color::Rgb(0x7a, 0xa2, 0xf7);
pub const RECIPE: Color = Color::Rgb(0x9e, 0xce, 0x6a);
pub const MODULE: Color = Color::Rgb(0xe0, 0xaf, 0x68);
pub const ALIAS: Color = Color::Rgb(0xbb, 0x9a, 0xf7);
pub const STRING: Color = Color::Rgb(0x9e, 0xce, 0x6a);
pub const COMMENT: Color = Color::Rgb(0x56, 0x5f, 0x89);
pub const KEYWORD: Color = Color::Rgb(0xbb, 0x9a, 0xf7);
pub const INTERP: Color = Color::Rgb(0xf7, 0x76, 0x8e);
pub const VARIABLE: Color = Color::Rgb(0x7d, 0xcf, 0xff);
pub const NUMBER: Color = Color::Rgb(0xff, 0x9e, 0x64);
pub const ATTRIBUTE: Color = Color::Rgb(0xe0, 0xaf, 0x68);
pub const SELECTION_BG: Color = Color::Rgb(0x2c, 0x33, 0x45);
pub const MATCH: Color = Color::Rgb(0xff, 0xc7, 0x77);

pub fn border(focused: bool) -> Style {
    if focused {
        Style::default()
            .fg(BORDER_FOCUS)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(BORDER)
    }
}

pub fn title(focused: bool) -> Style {
    if focused {
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(DIM).add_modifier(Modifier::BOLD)
    }
}

pub fn label() -> Style {
    Style::default().fg(DIM)
}

pub fn value() -> Style {
    Style::default().fg(FG)
}
