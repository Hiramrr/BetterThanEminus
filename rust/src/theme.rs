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

pub const THEMES: [Theme; 19] = [
    Theme { name: "gruvbox", bg: rgb(0x1d2021), fg: rgb(0xebdbb2), dim: rgb(0xa89984), border: rgb(0x504945), accent: rgb(0xfabd2f), on_accent: rgb(0x1d2021), sel: rgb(0x3c3836), red: rgb(0xfb4934), orange: rgb(0xfe8019), yellow: rgb(0xfabd2f), green: rgb(0xb8bb26) },
    Theme { name: "tokyo night", bg: rgb(0x1a1b26), fg: rgb(0xc0caf5), dim: rgb(0x8189af), border: rgb(0x3b4261), accent: rgb(0x7aa2f7), on_accent: rgb(0x1a1b26), sel: rgb(0x283457), red: rgb(0xf7768e), orange: rgb(0xff9e64), yellow: rgb(0xe0af68), green: rgb(0x9ece6a) },
    Theme { name: "catppuccin mocha", bg: rgb(0x1e1e2e), fg: rgb(0xcdd6f4), dim: rgb(0x9399b2), border: rgb(0x45475a), accent: rgb(0xcba6f7), on_accent: rgb(0x1e1e2e), sel: rgb(0x313244), red: rgb(0xf38ba8), orange: rgb(0xfab387), yellow: rgb(0xf9e2af), green: rgb(0xa6e3a1) },
    Theme { name: "nord", bg: rgb(0x2e3440), fg: rgb(0xeceff4), dim: rgb(0xa3adbf), border: rgb(0x4c566a), accent: rgb(0x88c0d0), on_accent: rgb(0x2e3440), sel: rgb(0x3b4252), red: rgb(0xe88a94), orange: rgb(0xe0a184), yellow: rgb(0xebcb8b), green: rgb(0xa3be8c) },
    Theme { name: "phosphor", bg: rgb(0x050a05), fg: rgb(0x8dfc8d), dim: rgb(0x4fa64f), border: rgb(0x1f4a1f), accent: rgb(0x39ff14), on_accent: rgb(0x050a05), sel: rgb(0x0f2a0f), red: rgb(0xff6b6b), orange: rgb(0xffb347), yellow: rgb(0xe6ff6b), green: rgb(0x39ff14) },
    Theme { name: "paper", bg: rgb(0xf7f4ec), fg: rgb(0x26231d), dim: rgb(0x6b655a), border: rgb(0xc9c2b2), accent: rgb(0x1f5fbf), on_accent: rgb(0xffffff), sel: rgb(0xe6e0d0), red: rgb(0xb3261e), orange: rgb(0xa8500a), yellow: rgb(0x7a5c00), green: rgb(0x2e6b2e) },
    Theme { name: "gruvbox light", bg: rgb(0xfbf1c7), fg: rgb(0x3c3836), dim: rgb(0x665c54), border: rgb(0xbdae93), accent: rgb(0xaf3a03), on_accent: rgb(0xfbf1c7), sel: rgb(0xf2e5bc), red: rgb(0x9d0006), orange: rgb(0xaf3a03), yellow: rgb(0x7b4e09), green: rgb(0x4f5706) },
    Theme { name: "amber", bg: rgb(0x120b02), fg: rgb(0xffb000), dim: rgb(0xbf8500), border: rgb(0x5c3d06), accent: rgb(0xffb000), on_accent: rgb(0x0b0701), sel: rgb(0x302008), red: rgb(0xff8a69), orange: rgb(0xffa45c), yellow: rgb(0xffd479), green: rgb(0xffcf5c) },
    Theme { name: "medianoche", bg: rgb(0x0f1526), fg: rgb(0xd8dcea), dim: rgb(0x8b93b0), border: rgb(0x2f3a5a), accent: rgb(0xf2b04a), on_accent: rgb(0x0f1526), sel: rgb(0x1c2640), red: rgb(0xf07178), orange: rgb(0xf78c6c), yellow: rgb(0xf2c96b), green: rgb(0xa3d977) },
    Theme { name: "terracota", bg: rgb(0x231a16), fg: rgb(0xeadccb), dim: rgb(0xad9885), border: rgb(0x4d3b31), accent: rgb(0xe8845a), on_accent: rgb(0x1a1310), sel: rgb(0x35271f), red: rgb(0xf0776b), orange: rgb(0xf0a060), yellow: rgb(0xe8c06a), green: rgb(0xadc680) },
    Theme { name: "océano", bg: rgb(0x0b1f24), fg: rgb(0xcfe6e6), dim: rgb(0x83a8ab), border: rgb(0x22474f), accent: rgb(0x4fd1c5), on_accent: rgb(0x07171b), sel: rgb(0x123139), red: rgb(0xff7a85), orange: rgb(0xffa36b), yellow: rgb(0xf5d47a), green: rgb(0x8ee39b) },
    Theme { name: "grafito", bg: rgb(0x16171a), fg: rgb(0xd6d8dc), dim: rgb(0x91969d), border: rgb(0x34383e), accent: rgb(0xc6f432), on_accent: rgb(0x111214), sel: rgb(0x272a2f), red: rgb(0xff6b6b), orange: rgb(0xffa45c), yellow: rgb(0xf5d565), green: rgb(0x7ee2a8) },
    Theme { name: "matcha", bg: rgb(0xf5f3e7), fg: rgb(0x253126), dim: rgb(0x53614f), border: rgb(0xc3cab6), accent: rgb(0x356843), on_accent: rgb(0xffffff), sel: rgb(0xe2e8d5), red: rgb(0xa32925), orange: rgb(0x955017), yellow: rgb(0x725800), green: rgb(0x2d663a) },
    Theme { name: "lavanda", bg: rgb(0xf5f2fa), fg: rgb(0x2c2539), dim: rgb(0x64586e), border: rgb(0xc9bdd9), accent: rgb(0x724497), on_accent: rgb(0xffffff), sel: rgb(0xe7dff1), red: rgb(0xad303e), orange: rgb(0x91481f), yellow: rgb(0x73570b), green: rgb(0x346542) },
    Theme { name: "sakura", bg: rgb(0xfff5f7), fg: rgb(0x37242b), dim: rgb(0x72545f), border: rgb(0xe4bfc8), accent: rgb(0xb13a5f), on_accent: rgb(0xffffff), sel: rgb(0xf6e0e7), red: rgb(0xaa2d2a), orange: rgb(0x92451f), yellow: rgb(0x76590c), green: rgb(0x316641) },
    Theme { name: "alto contraste", bg: rgb(0x000000), fg: rgb(0xffffff), dim: rgb(0xd0d0d0), border: rgb(0xffffff), accent: rgb(0xffe600), on_accent: rgb(0x000000), sel: rgb(0x262626), red: rgb(0xff6b6b), orange: rgb(0xffa640), yellow: rgb(0xffe600), green: rgb(0x3dff7a) },
    Theme { name: "daltonismo", bg: rgb(0xf7f7f5), fg: rgb(0x1f1f1f), dim: rgb(0x575757), border: rgb(0xb5b5b0), accent: rgb(0x0068a3), on_accent: rgb(0xffffff), sel: rgb(0xe5e5e1), red: rgb(0xa34600), orange: rgb(0x875600), yellow: rgb(0x705a00), green: rgb(0x006f51) },
    Theme { name: "terminal", bg: Color::Reset, fg: Color::Reset, dim: Color::DarkGray, border: Color::DarkGray, accent: Color::Cyan, on_accent: Color::Black, sel: Color::DarkGray, red: Color::Red, orange: Color::LightRed, yellow: Color::Yellow, green: Color::Green },
    Theme { name: "eminus", bg: rgb(0xf4f6f9), fg: rgb(0x222c3b), dim: rgb(0x536176), border: rgb(0xd7dee8), accent: rgb(0x1459a6), on_accent: rgb(0xffffff), sel: rgb(0xe5edf8), red: rgb(0xa62c36), orange: rgb(0x964613), yellow: rgb(0x765610), green: rgb(0x24643d) },
];

pub fn index_of(name: &str) -> usize {
    THEMES.iter().position(|t| t.name == name).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance(c: Color) -> f64 {
        let Color::Rgb(r, g, b) = c else { panic!("color sin RGB") };
        let linear = |n: u8| { let n = f64::from(n) / 255.0; if n <= 0.04045 { n / 12.92 } else { ((n + 0.055) / 1.055).powf(2.4) } };
        0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
    }

    fn contrast(a: Color, b: Color) -> f64 {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn new_themes_keep_text_and_statuses_readable() {
        for t in THEMES[6..17].iter().chain([&THEMES[index_of("eminus")]]) {
            assert!(contrast(t.fg, t.bg) >= 7.0, "{} texto", t.name);
            for background in [t.bg, t.sel] {
                for foreground in [t.fg, t.dim, t.accent, t.red, t.orange, t.yellow, t.green] {
                    assert!(contrast(foreground, background) >= 4.5, "{} {:?} sobre {:?}: {}", t.name, foreground, background, contrast(foreground, background));
                }
            }
            assert!(contrast(t.accent, t.on_accent) >= 4.5, "{} botón", t.name);
            assert_eq!(THEMES[index_of(t.name)].name, t.name);
        }
    }
}
