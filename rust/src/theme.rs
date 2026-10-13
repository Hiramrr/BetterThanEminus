// Paletas. Mismos temas (y mismos nombres) que la extensión, más
// «terminal», que usa los colores que ya tenga tu terminal.

use crate::model::Tone;
use ratatui::style::Color;

pub struct Theme {
    pub name: &'static str,
    pub bg: Color,
    pub fg: Color,
    pub dim: Color,
    pub border: Color,
    pub accent: Color,
    pub on_accent: Color,
    pub sel: Color,
    pub red: Color,
    pub orange: Color,
    pub yellow: Color,
    pub green: Color,
}

impl Theme {
    pub fn tone(&self, tone: Tone) -> Color {
        match tone {
            Tone::Done => self.green,
            Tone::Future => self.dim,
            Tone::Overdue => self.red,
            Tone::Imminent => self.orange,
            Tone::Urgent => self.yellow,
            Tone::Normal | Tone::Plain => self.fg,
        }
    }
}

const fn rgb(hex: u32) -> Color {
    Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub const THEMES: [Theme; 7] = [
    Theme { name: "gruvbox", bg: rgb(0x1d2021), fg: rgb(0xebdbb2), dim: rgb(0xa89984), border: rgb(0x504945), accent: rgb(0xfabd2f), on_accent: rgb(0x1d2021), sel: rgb(0x3c3836), red: rgb(0xfb4934), orange: rgb(0xfe8019), yellow: rgb(0xfabd2f), green: rgb(0xb8bb26) },
    Theme { name: "tokyo night", bg: rgb(0x1a1b26), fg: rgb(0xc0caf5), dim: rgb(0x8189af), border: rgb(0x3b4261), accent: rgb(0x7aa2f7), on_accent: rgb(0x1a1b26), sel: rgb(0x283457), red: rgb(0xf7768e), orange: rgb(0xff9e64), yellow: rgb(0xe0af68), green: rgb(0x9ece6a) },
    Theme { name: "catppuccin mocha", bg: rgb(0x1e1e2e), fg: rgb(0xcdd6f4), dim: rgb(0x9399b2), border: rgb(0x45475a), accent: rgb(0xcba6f7), on_accent: rgb(0x1e1e2e), sel: rgb(0x313244), red: rgb(0xf38ba8), orange: rgb(0xfab387), yellow: rgb(0xf9e2af), green: rgb(0xa6e3a1) },
    Theme { name: "nord", bg: rgb(0x2e3440), fg: rgb(0xeceff4), dim: rgb(0xa3adbf), border: rgb(0x4c566a), accent: rgb(0x88c0d0), on_accent: rgb(0x2e3440), sel: rgb(0x3b4252), red: rgb(0xe88a94), orange: rgb(0xe0a184), yellow: rgb(0xebcb8b), green: rgb(0xa3be8c) },
    Theme { name: "phosphor", bg: rgb(0x050a05), fg: rgb(0x8dfc8d), dim: rgb(0x4fa64f), border: rgb(0x1f4a1f), accent: rgb(0x39ff14), on_accent: rgb(0x050a05), sel: rgb(0x0f2a0f), red: rgb(0xff6b6b), orange: rgb(0xffb347), yellow: rgb(0xe6ff6b), green: rgb(0x39ff14) },
    Theme { name: "paper", bg: rgb(0xf7f4ec), fg: rgb(0x26231d), dim: rgb(0x6b655a), border: rgb(0xc9c2b2), accent: rgb(0x1f5fbf), on_accent: rgb(0xffffff), sel: rgb(0xe6e0d0), red: rgb(0xb3261e), orange: rgb(0xa8500a), yellow: rgb(0x7a5c00), green: rgb(0x2e6b2e) },
    Theme { name: "terminal", bg: Color::Reset, fg: Color::Reset, dim: Color::DarkGray, border: Color::DarkGray, accent: Color::Cyan, on_accent: Color::Black, sel: Color::DarkGray, red: Color::Red, orange: Color::LightRed, yellow: Color::Yellow, green: Color::Green },
];

pub fn index_of(name: &str) -> usize {
    THEMES.iter().position(|t| t.name == name).unwrap_or(0)
}
