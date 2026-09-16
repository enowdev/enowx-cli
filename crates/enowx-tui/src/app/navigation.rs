use super::*;
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Position;

impl App {
    pub(crate) fn select_tab(&mut self, index: usize) {
        self.sidebar_tab = index.min(4);
        self.sidebar_page = 0;
        self.show_sidebar = true;
    }

    pub(crate) fn page_sidebar(&mut self, next: bool) {
        self.sidebar_page = if next {
            (self.sidebar_page + 1).min(self.sidebar_pages.saturating_sub(1))
        } else {
            self.sidebar_page.saturating_sub(1)
        };
    }

    pub(crate) fn toggle_sidebar(&mut self) -> Result<()> {
        let visible = !self.show_sidebar;
        let mut config = self.config.clone();
        config.ui.show_sidebar = visible;
        config.save()?;
        self.config = config;
        self.show_sidebar = visible;
        Ok(())
    }

    pub(crate) fn select_theme(&mut self, index: usize) -> Result<()> {
        let theme = THEMES[index];
        let mut config = self.config.clone();
        config.ui.theme = theme.name.into();
        config.save()?;
        self.config = config;
        self.theme = theme;
        self.settings.theme = theme.name.into();
        self.status = format!("theme: {}", theme.label);
        Ok(())
    }

    pub(crate) fn mouse(&mut self, event: MouseEvent) -> Result<()> {
        let position = Position::new(event.column, event.row);
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                // QuitConfirm buttons: single click on Yes/No acts immediately.
                if self.modal == Modal::QuitConfirm {
                    for (rect, is_yes) in self.quit_confirm_rects {
                        if rect.contains(position) {
                            if is_yes {
                                self.should_quit = true;
                            } else {
                                self.modal = Modal::None;
                            }
                            return Ok(());
                        }
                    }
                    return Ok(());
                }
                if matches!(self.modal, Modal::Skills | Modal::Mcp) {
                    let hit = self
                        .popup_rows
                        .iter()
                        .find(|(row, _, _)| row.contains(position))
                        .map(|(_, mark, idx)| (*mark, *idx));
                    if let Some((mark, idx)) = hit {
                        // Click on the mark still toggles enable/disable, so
                        // one gesture covers the common case. Click anywhere
                        // else on the row is select-only: press Enter to open.
                        self.modal_cursor = idx;
                        if mark.contains(position) {
                            if self.modal == Modal::Skills {
                                self.toggle_selected_skill()?;
                            } else {
                                self.toggle_selected_mcp()?;
                            }
                        }
                    }
                    return Ok(());
                }
                if self.modal == Modal::McpForm {
                    if let Some((_, idx)) = self
                        .mcp_field_rows
                        .iter()
                        .find(|(rect, _)| rect.contains(position))
                    {
                        self.mcp_field = *idx;
                    }
                    return Ok(());
                }
                // Composer field: click positions the cursor.
                if let Some(field) = self.composer_field {
                    if field.contains(position) {
                        let visual_row = (position.y - field.y) as usize + self.composer_offset;
                        let visual_col = (position.x - field.x) as usize;
                        self.cursor = composer_cursor_at(
                            &self.input,
                            visual_row,
                            visual_col,
                            self.composer_width.max(1),
                        );
                        return Ok(());
                    }
                }
                if self.modal != Modal::None {
                    if let Some((_, index)) = self
                        .modal_rows
                        .iter()
                        .find(|(rect, _)| rect.contains(position))
                    {
                        self.modal_cursor = *index;
                        self.accept_modal()?;
                    }
                } else if let Some((_, path)) = self
                    .file_link_rects
                    .iter()
                    .find(|(rect, _)| rect.contains(position))
                    .map(|(r, p)| (*r, p.clone()))
                {
                    // Clicking on a tool header/row that carries a file path
                    // opens the file in the OS default application. Ties with
                    // the tool header toggle are broken toward opening.
                    let full = if std::path::Path::new(&path).is_absolute() {
                        std::path::PathBuf::from(&path)
                    } else {
                        self.config.workspace().join(&path)
                    };
                    if crate::attachments::open_path(&full) {
                        self.status = format!("opened {}", path);
                    } else {
                        self.status = format!("could not open {}", path);
                    }
                } else if let Some((_, id)) = self
                    .tool_header_rects
                    .iter()
                    .find(|(rect, _)| rect.contains(position))
                    .map(|(r, id)| (*r, id.clone()))
                {
                    let current = self
                        .tool_expanded
                        .get(&id)
                        .copied()
                        .unwrap_or(self.show_tool_output);
                    self.tool_expanded.insert(id, !current);
                } else if let Some((_, index)) = self
                    .sidebar_tabs
                    .iter()
                    .find(|(rect, _)| rect.contains(position))
                {
                    self.select_tab(*index);
                } else if let Some(rect) = self
                    .sidebar_pages_area
                    .filter(|rect| rect.contains(position))
                {
                    self.page_sidebar(event.column >= rect.x + rect.width / 2);
                } else if self.transcript_area.is_some_and(|r| r.contains(position)) {
                    // Start a drag selection anchored at the click point.
                    self.selection = Some(crate::app::TextSelection {
                        anchor: (event.row, event.column),
                        head: (event.row, event.column),
                    });
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if let Some(sel) = self.selection.as_mut() {
                    sel.head = (event.row, event.column);
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                // Copy selection to clipboard on release, then clear so the
                // highlight disappears immediately as the user wants.
                if let Some(sel) = self.selection.take() {
                    if sel.anchor != sel.head {
                        let text = extract_selection(&self.wrapped_snapshot, sel);
                        if !text.trim().is_empty() && crate::attachments::copy_to_clipboard(&text) {
                            self.status = format!("copied {} chars", text.len());
                        }
                    }
                }
            }
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                let dir_up = event.kind == MouseEventKind::ScrollUp;
                // Popup selector still throttles so a burst of trackpad wheel
                // events does not skip past every row. Transcript and composer
                // scroll are natural and expect one step per event.
                if matches!(self.modal, Modal::Skills | Modal::Mcp)
                    && self.popup_body.is_some_and(|r| r.contains(position))
                {
                    let now = std::time::Instant::now();
                    if let Some(prev) = self.last_wheel {
                        if now.duration_since(prev)
                            < std::time::Duration::from_millis(WHEEL_THROTTLE_MS)
                        {
                            return Ok(());
                        }
                    }
                    self.last_wheel = Some(now);
                    let len = if self.modal == Modal::Skills {
                        self.skill_rows().len()
                    } else {
                        self.mcp_rows().len()
                    };
                    if dir_up {
                        self.modal_cursor = self.modal_cursor.saturating_sub(1);
                    } else if self.modal_cursor + 1 < len {
                        self.modal_cursor += 1;
                    }
                    return Ok(());
                }
                if let Some(field) = self.composer_field {
                    if field.contains(position) {
                        let width = self.composer_width.max(1);
                        self.cursor = crate::app::navigation::composer_move_visual(
                            &self.input,
                            self.cursor,
                            if dir_up { -1 } else { 1 },
                            width,
                        );
                        return Ok(());
                    }
                }
                if self
                    .sidebar_area
                    .is_some_and(|rect| rect.contains(position))
                {
                    return Ok(());
                }
                if dir_up {
                    self.scroll = self.scroll.saturating_sub(3);
                    self.auto_scroll = false;
                } else {
                    self.scroll = self.scroll.saturating_add(3).min(self.max_scroll);
                    self.auto_scroll = self.scroll == self.max_scroll;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

const WHEEL_THROTTLE_MS: u64 = 80;

/// Byte offset in `text` corresponding to a click at visual (row, col) inside
/// a composer field wrapped at `width` columns. `\n` terminates a visual row;
/// otherwise a row wraps at `width` chars. Falls back to the row's end when
/// the click lands past the last character of that row.
fn composer_cursor_at(text: &str, row: usize, col: usize, width: usize) -> usize {
    let mut byte = 0usize;
    let mut visual = 0usize;
    let mut visual_col = 0usize;
    for c in text.chars() {
        if visual == row && visual_col == col {
            return byte;
        }
        if c == '\n' {
            if visual == row {
                return byte;
            }
            visual += 1;
            visual_col = 0;
        } else {
            visual_col += 1;
            if visual_col >= width {
                if visual == row {
                    return byte + c.len_utf8();
                }
                visual += 1;
                visual_col = 0;
            }
        }
        byte += c.len_utf8();
    }
    text.len()
}

/// Move the composer cursor up or down one visual row, preserving column
/// where possible. Honours real `\n` breaks and wraps at `width`. Cursor at
/// the very end of the text or on a wrap boundary is counted on the row it
/// visually occupies, so wheel-up from the last row does not jump to row 0.
pub(super) fn composer_move_visual(text: &str, cursor: usize, delta: i32, width: usize) -> usize {
    let (cur_row, cur_col) = visual_pos(text, cursor, width);
    let target_row = if delta < 0 {
        cur_row.saturating_sub(1)
    } else {
        cur_row + 1
    };
    composer_cursor_at(text, target_row, cur_col, width)
}

fn visual_pos(text: &str, cursor: usize, width: usize) -> (usize, usize) {
    let mut byte = 0usize;
    let mut visual = 0usize;
    let mut visual_col = 0usize;
    for c in text.chars() {
        if byte == cursor {
            return (visual, visual_col);
        }
        if c == '\n' {
            visual += 1;
            visual_col = 0;
        } else {
            visual_col += 1;
            if visual_col >= width {
                visual += 1;
                visual_col = 0;
            }
        }

        byte += c.len_utf8();
    }
    (visual, visual_col)
}

/// Extract the substring covered by a selection from the wrapped snapshot.
/// Rows outside the selection are skipped; the first and last rows are
/// clipped to the selection columns; middle rows are taken whole. Card
/// borders (`│`, `╭`, `╰`, `─`) are stripped so a copied user card or diff
/// yields the plain text the reader saw, not the decoration.
fn extract_selection(snapshot: &[(u16, String)], sel: crate::app::TextSelection) -> String {
    let (start, end) = if (sel.anchor.0, sel.anchor.1) <= (sel.head.0, sel.head.1) {
        (sel.anchor, sel.head)
    } else {
        (sel.head, sel.anchor)
    };
    let mut out = String::new();
    for (row, text) in snapshot {
        let row = *row;
        if row < start.0 || row > end.0 {
            continue;
        }
        let chars: Vec<char> = text.chars().collect();
        let col_start = if row == start.0 { start.1 as usize } else { 0 };
        let col_end = if row == end.0 {
            (end.1 as usize + 1).min(chars.len())
        } else {
            chars.len()
        };
        if col_start < col_end {
            let slice: String = chars[col_start..col_end].iter().collect();
            let cleaned = strip_card_chrome(&slice);
            if !cleaned.trim().is_empty() {
                out.push_str(&cleaned);
            }
        }
        if row != end.0 {
            out.push('\n');
        }
    }
    // Trim consecutive blank rows a stripped border can leave behind.
    let cleaned: Vec<&str> = out.lines().collect();
    let mut trimmed = String::new();
    let mut last_blank = false;
    for line in cleaned {
        let is_blank = line.trim().is_empty();
        if is_blank && last_blank {
            continue;
        }
        trimmed.push_str(line);
        trimmed.push('\n');
        last_blank = is_blank;
    }
    trimmed.trim_end().to_string()
}

/// Remove the box-drawing chrome from a line so copied text does not include
/// `│` gutters or `╭─╮`/`╰─╯` frame characters. Preserves everything else.
fn strip_card_chrome(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    for ch in line.chars() {
        // Skip box-drawing characters that make up the card frames used by
        // user_card, render_preview, render_diff, and the fenced code block.
        if matches!(
            ch,
            '│' | '─'
                | '╭'
                | '╮'
                | '╰'
                | '╯'
                | '├'
                | '┤'
                | '┬'
                | '┴'
                | '┼'
                | '┌'
                | '┐'
                | '└'
                | '┘'
        ) {
            out.push(' ');
            continue;
        }
        out.push(ch);
    }
    // Collapse the leading padding a stripped `│ ` leaves behind but keep
    // interior spacing (indentation inside code blocks is real content).
    out.trim_start().trim_end().to_string()
}
