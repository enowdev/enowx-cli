//! Width-aware text helpers shared by every renderer.

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Truncate to `max` display columns, adding an ellipsis when text is cut.
/// Width-aware so CJK and emoji never overflow their cell budget.
pub fn trim(text: &str, max: usize) -> String {
    if text.width() <= max {
        return text.to_owned();
    }
    if max == 0 {
        return String::new();
    }
    let mut output = String::new();
    let mut width = 0;
    for character in text.chars() {
        let size = character.width().unwrap_or(0);
        if width + size > max - 1 {
            break;
        }
        output.push(character);
        width += size;
    }
    output.push('…');
    output
}

/// Normalize a pasted block so it cannot smuggle terminal escape sequences
/// or line-ending oddities into the composer. Terminals emit paste text with
/// the raw contents of the clipboard, which on Windows or via some editors
/// includes `\r` or bare escapes that would advance the terminal cursor when
/// re-rendered.
pub fn sanitize_paste(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                // Collapse CRLF and bare CR to a single LF.
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push('\n');
            }
            '\n' => out.push('\n'),
            '\t' => out.push_str("    "),
            c if c.is_control() => {} // drop
            c => out.push(c),
        }
    }
    out
}

/// Soft-wrap the composer into display rows and report the caret's row/column.
pub fn input_rows(text: &str, cursor: usize, width: usize) -> (Vec<String>, usize, usize) {
    let mut rows = vec![String::new()];
    let mut col = 0;
    let mut caret = (0, 0);
    for (index, character) in text.char_indices() {
        let size = character.width().unwrap_or(0);
        if character != '\n' && col + size > width {
            rows.push(String::new());
            col = 0;
        }
        if index == cursor {
            caret = (rows.len() - 1, col);
        }
        if character == '\n' {
            rows.push(String::new());
            col = 0;
        } else {
            rows.last_mut().expect("input row").push(character);
            col += size;
        }
    }
    if cursor == text.len() {
        if col >= width {
            rows.push(String::new());
            col = 0;
        }
        caret = (rows.len() - 1, col);
    }
    (rows, caret.0, caret.1)
}

/// Thousands separators for token counters, matching the reference readout.
pub fn thousands(value: u64) -> String {
    let digits = value.to_string();
    let mut output = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            output.push(',');
        }
        output.push(character);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{input_rows, trim};

    #[test]
    fn caret_follows_wrapped_and_wide_input() {
        let (rows, row, column) = input_rows("abcdef", 6, 4);
        assert_eq!(rows, ["abcd", "ef"]);
        assert_eq!((row, column), (1, 2));

        let (rows, row, column) = input_rows("ab\ncd", 5, 8);
        assert_eq!(rows, ["ab", "cd"]);
        assert_eq!((row, column), (1, 2));

        // 世 occupies two columns, so a four-column field holds exactly two and
        // the caret sits at the start of the wrapped row, before `a`.
        let (rows, row, column) = input_rows("世界a", 6, 4);
        assert_eq!(rows, ["世界", "a"]);
        assert_eq!((row, column), (1, 0));
    }

    #[test]
    fn trim_respects_display_width_not_char_count() {
        assert_eq!(trim("abc", 8), "abc");
        assert_eq!(trim("abcdef", 4), "abc…");
        // Each 界 is two columns wide: only one fits before the ellipsis.
        assert_eq!(trim("界界界", 4), "界…");
        assert_eq!(trim("abc", 0), "");
    }
}
