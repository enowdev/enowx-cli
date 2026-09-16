//! Small ANSI SGR (color/style) parser. Consumes an escape-embedded string
//! and produces styled span pieces the transcript renderer can hand to
//! ratatui. Unknown escapes are dropped instead of leaked as literal `[m]`
//! fragments — same behaviour the composer sanitizer has for input.
//!
//! Scope: SGR only (colors, bold, italic, underline, dim, reverse, strike).
//! CSI cursor movement, OSC, and other escapes are skipped whole.

use ratatui::style::{Color, Modifier, Style};

pub struct Piece {
    pub text: String,
    pub style: Style,
}

/// Split `input` into `(text, style)` pieces at every SGR change. Newlines
/// stay in the text; the caller wraps line-by-line.
pub fn parse(input: &str) -> Vec<Piece> {
    let mut out: Vec<Piece> = Vec::new();
    let mut style = Style::default();
    let mut buf = String::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        // SGR: ESC [ … m
        if c == '\u{1b}' && chars.get(i + 1) == Some(&'[') {
            // Find final byte.
            let mut j = i + 2;
            while j < chars.len() && !((chars[j] as u32) >= 0x40 && (chars[j] as u32) <= 0x7e) {
                j += 1;
            }
            if j >= chars.len() {
                // Unterminated: swallow the rest.
                break;
            }
            let final_byte = chars[j];
            if final_byte == 'm' {
                let params: String = chars[i + 2..j].iter().collect();
                if !buf.is_empty() {
                    out.push(Piece {
                        text: std::mem::take(&mut buf),
                        style,
                    });
                }
                style = apply_sgr(&params, style);
            }
            // Any other CSI (cursor, erase, etc.) — just skip.
            i = j + 1;
            continue;
        }
        // OSC: ESC ] … BEL | ESC \
        if c == '\u{1b}' && chars.get(i + 1) == Some(&']') {
            let mut j = i + 2;
            while j < chars.len() {
                if chars[j] == '\u{07}' {
                    j += 1;
                    break;
                }
                if chars[j] == '\u{1b}' && chars.get(j + 1) == Some(&'\\') {
                    j += 2;
                    break;
                }
                j += 1;
            }
            i = j;
            continue;
        }
        // Bare ESC or control we do not visualize: drop.
        if c == '\u{1b}' || (c.is_control() && c != '\n' && c != '\t') {
            i += 1;
            continue;
        }
        // Tabs render as 4 spaces so alignment is predictable.
        if c == '\t' {
            buf.push_str("    ");
            i += 1;
            continue;
        }
        buf.push(c);
        i += 1;
    }
    if !buf.is_empty() {
        out.push(Piece { text: buf, style });
    }
    out
}

/// Apply a `;`-separated SGR parameter list to `style`. Supports:
/// 0 reset, 1 bold, 2 dim, 3 italic, 4 underline, 7 reverse, 9 strike, 22/23/24/29 off,
/// 30-37 fg, 40-47 bg, 90-97 bright fg, 100-107 bright bg,
/// 38;5;N and 48;5;N 256-color, 38;2;R;G;B and 48;2;R;G;B truecolor.
fn apply_sgr(params: &str, mut style: Style) -> Style {
    let parts: Vec<&str> = if params.is_empty() {
        vec!["0"]
    } else {
        params.split(';').collect()
    };
    let mut i = 0;
    while i < parts.len() {
        let code: u16 = parts[i].parse().unwrap_or(0);
        match code {
            0 => style = Style::default(),
            1 => style = style.add_modifier(Modifier::BOLD),
            2 => style = style.add_modifier(Modifier::DIM),
            3 => style = style.add_modifier(Modifier::ITALIC),
            4 => style = style.add_modifier(Modifier::UNDERLINED),
            7 => style = style.add_modifier(Modifier::REVERSED),
            9 => style = style.add_modifier(Modifier::CROSSED_OUT),
            22 => style = style.remove_modifier(Modifier::BOLD | Modifier::DIM),
            23 => style = style.remove_modifier(Modifier::ITALIC),
            24 => style = style.remove_modifier(Modifier::UNDERLINED),
            27 => style = style.remove_modifier(Modifier::REVERSED),
            29 => style = style.remove_modifier(Modifier::CROSSED_OUT),
            30..=37 => style = style.fg(basic_color(code - 30)),
            39 => style = style.fg(Color::Reset),
            40..=47 => style = style.bg(basic_color(code - 40)),
            49 => style = style.bg(Color::Reset),
            90..=97 => style = style.fg(bright_color(code - 90)),
            100..=107 => style = style.bg(bright_color(code - 100)),
            38 | 48 => {
                if let Some(next) = parts.get(i + 1) {
                    match next.parse::<u16>().unwrap_or(0) {
                        5 => {
                            if let Some(idx) = parts.get(i + 2).and_then(|s| s.parse::<u8>().ok()) {
                                let color = Color::Indexed(idx);
                                style = if code == 38 {
                                    style.fg(color)
                                } else {
                                    style.bg(color)
                                };
                                i += 2;
                            }
                        }
                        2 => {
                            let r = parts.get(i + 2).and_then(|s| s.parse::<u8>().ok());
                            let g = parts.get(i + 3).and_then(|s| s.parse::<u8>().ok());
                            let b = parts.get(i + 4).and_then(|s| s.parse::<u8>().ok());
                            if let (Some(r), Some(g), Some(b)) = (r, g, b) {
                                let color = Color::Rgb(r, g, b);
                                style = if code == 38 {
                                    style.fg(color)
                                } else {
                                    style.bg(color)
                                };
                                i += 4;
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    style
}

fn basic_color(n: u16) -> Color {
    match n {
        0 => Color::Black,
        1 => Color::Red,
        2 => Color::Green,
        3 => Color::Yellow,
        4 => Color::Blue,
        5 => Color::Magenta,
        6 => Color::Cyan,
        _ => Color::Gray,
    }
}

fn bright_color(n: u16) -> Color {
    match n {
        0 => Color::DarkGray,
        1 => Color::LightRed,
        2 => Color::LightGreen,
        3 => Color::LightYellow,
        4 => Color::LightBlue,
        5 => Color::LightMagenta,
        6 => Color::LightCyan,
        _ => Color::White,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plain_text_stays_one_piece() {
        let pieces = parse("hello world");
        assert_eq!(pieces.len(), 1);
        assert_eq!(pieces[0].text, "hello world");
    }
    #[test]
    fn red_switches_and_resets() {
        let pieces = parse("\x1b[31mred\x1b[0m normal");
        assert_eq!(pieces.len(), 2);
        assert_eq!(pieces[0].text, "red");
        assert_eq!(pieces[1].text, " normal");
    }
    #[test]
    fn strips_bare_escape() {
        let pieces = parse("keep\x1b[Ause");
        // \x1b[A (cursor up) has no `m` final so it just gets dropped.
        assert_eq!(
            pieces.iter().map(|p| p.text.as_str()).collect::<String>(),
            "keepuse"
        );
    }
    #[test]
    fn keeps_newlines() {
        let pieces = parse("a\nb");
        assert_eq!(pieces[0].text, "a\nb");
    }
}
