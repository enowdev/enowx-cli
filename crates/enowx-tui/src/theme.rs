//! Terminal palettes. Every colour the interface draws comes from one of these,
//! so `/theme` restyles the whole surface without touching layout code.

use ratatui::style::Color;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    pub name: &'static str,
    pub label: &'static str,
    /// Desktop behind the terminal window frame.
    pub canvas: Color,
    /// Window body: transcript and sidebar sit on this.
    pub panel: Color,
    /// Title bar, tab strip, input bar, footer.
    pub subtle: Color,
    /// Selected sidebar tab.
    pub active_tab: Color,
    pub border: Color,
    pub text: Color,
    pub muted: Color,
    pub faint: Color,
    pub accent: Color,
    pub accent2: Color,
    pub green: Color,
    pub red: Color,
    pub yellow: Color,
}

pub const THEMES: [Theme; 5] = [
    Theme {
        name: "obsidian_ice",
        label: "Obsidian Ice",
        canvas: Color::Rgb(9, 12, 16),
        panel: Color::Rgb(13, 17, 23),
        subtle: Color::Rgb(19, 25, 35),
        active_tab: Color::Rgb(24, 34, 50),
        border: Color::Rgb(28, 36, 51),
        text: Color::Rgb(226, 232, 240),
        muted: Color::Rgb(114, 130, 153),
        faint: Color::Rgb(114, 130, 153),
        accent: Color::Rgb(56, 189, 248),
        accent2: Color::Rgb(192, 132, 252),
        green: Color::Rgb(52, 211, 153),
        red: Color::Rgb(244, 63, 94),
        yellow: Color::Rgb(251, 191, 36),
    },
    Theme {
        name: "neo_acid",
        label: "Minimal Acid",
        canvas: Color::Rgb(8, 10, 8),
        panel: Color::Rgb(13, 18, 15),
        subtle: Color::Rgb(19, 28, 22),
        active_tab: Color::Rgb(26, 41, 31),
        border: Color::Rgb(27, 45, 34),
        text: Color::Rgb(236, 253, 245),
        muted: Color::Rgb(109, 137, 119),
        faint: Color::Rgb(109, 137, 119),
        accent: Color::Rgb(163, 230, 53),
        accent2: Color::Rgb(45, 212, 191),
        green: Color::Rgb(134, 239, 172),
        red: Color::Rgb(251, 113, 133),
        yellow: Color::Rgb(250, 204, 21),
    },
    Theme {
        name: "chrome_void",
        label: "Chrome Void",
        canvas: Color::Rgb(7, 7, 10),
        panel: Color::Rgb(13, 13, 20),
        subtle: Color::Rgb(20, 20, 32),
        active_tab: Color::Rgb(34, 30, 51),
        border: Color::Rgb(36, 36, 54),
        text: Color::Rgb(241, 245, 249),
        muted: Color::Rgb(125, 125, 150),
        faint: Color::Rgb(125, 125, 150),
        accent: Color::Rgb(255, 0, 127),
        accent2: Color::Rgb(148, 163, 184),
        green: Color::Rgb(6, 182, 212),
        red: Color::Rgb(239, 68, 68),
        yellow: Color::Rgb(251, 146, 60),
    },
    Theme {
        name: "oled_stealth",
        label: "OLED Stealth",
        canvas: Color::Rgb(0, 0, 0),
        panel: Color::Rgb(8, 8, 8),
        subtle: Color::Rgb(17, 17, 17),
        active_tab: Color::Rgb(26, 26, 26),
        border: Color::Rgb(34, 34, 34),
        text: Color::Rgb(243, 244, 246),
        muted: Color::Rgb(118, 125, 139),
        faint: Color::Rgb(118, 125, 139),
        accent: Color::Rgb(94, 234, 212),
        accent2: Color::Rgb(224, 231, 255),
        green: Color::Rgb(74, 222, 128),
        red: Color::Rgb(248, 113, 113),
        yellow: Color::Rgb(245, 158, 11),
    },
    Theme {
        name: "classic",
        label: "Classic Amber",
        canvas: Color::Rgb(16, 18, 24),
        panel: Color::Rgb(22, 24, 32),
        subtle: Color::Rgb(28, 31, 42),
        active_tab: Color::Rgb(38, 34, 46),
        border: Color::Rgb(42, 46, 58),
        text: Color::Rgb(203, 210, 224),
        muted: Color::Rgb(139, 147, 167),
        faint: Color::Rgb(139, 147, 167),
        accent: Color::Rgb(250, 178, 131),
        accent2: Color::Rgb(192, 132, 252),
        green: Color::Rgb(158, 206, 106),
        red: Color::Rgb(247, 118, 142),
        yellow: Color::Rgb(224, 175, 104),
    },
];

impl Theme {
    /// Unknown names fall back to the first palette rather than failing: a
    /// hand-edited config should never leave the interface unrenderable.
    pub fn find(name: &str) -> Self {
        THEMES
            .iter()
            .find(|theme| theme.name.eq_ignore_ascii_case(name))
            .copied()
            .unwrap_or(THEMES[0])
    }
}
