use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

impl App {
    pub(crate) fn key(&mut self, key: KeyEvent) -> Result<()> {
        if self.modal != Modal::None {
            if matches!(
                self.modal,
                Modal::Settings | Modal::ModelUrl | Modal::ProviderKey
            ) {
                return self.settings_key(key);
            }
            if self.modal == Modal::QuitConfirm {
                match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        self.should_quit = true;
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                        self.modal = Modal::None;
                    }
                    KeyCode::Left | KeyCode::Right | KeyCode::Tab | KeyCode::BackTab => {
                        // Flip between Yes and No.
                        self.quit_confirm_yes = !self.quit_confirm_yes;
                    }
                    KeyCode::Enter => {
                        if self.quit_confirm_yes {
                            self.should_quit = true;
                        } else {
                            self.modal = Modal::None;
                        }
                    }
                    _ => {}
                }
                return Ok(());
            }
            if self.modal == Modal::Themes {
                match key.code {
                    KeyCode::Esc => {
                        self.theme = Theme::find(&self.config.ui.theme);
                        self.modal = Modal::None;
                        return Ok(());
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.theme = Theme::find(&self.config.ui.theme);
                        self.modal = Modal::None;
                        return Ok(());
                    }
                    KeyCode::Up => {
                        self.modal_cursor = self.modal_cursor.saturating_sub(1);
                        if let Some(t) = THEMES.get(self.modal_cursor) {
                            self.theme = *t;
                        }
                        return Ok(());
                    }
                    KeyCode::Down if self.modal_cursor + 1 < self.modal_items.len() => {
                        self.modal_cursor += 1;
                        if let Some(t) = THEMES.get(self.modal_cursor) {
                            self.theme = *t;
                        }
                        return Ok(());
                    }
                    KeyCode::Enter => {
                        return self.accept_modal();
                    }
                    _ => return Ok(()),
                }
            }
            if self.modal == Modal::Models {
                match key.code {
                    KeyCode::Esc => {
                        self.close_models();
                        return Ok(());
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.close_models();
                        return Ok(());
                    }
                    KeyCode::F(5) => {
                        self.model_events = None;
                        self.discovering_models = false;
                        self.modal = Modal::ModelUrl;
                        self.modal_cursor = 3;
                        self.field_cursor = self.settings.models_url.len();
                        return Ok(());
                    }
                    KeyCode::F(2) => {
                        self.close_models();
                        return Ok(());
                    }
                    KeyCode::Enter if self.modal_items.is_empty() => return Ok(()),
                    _ => {}
                }
            }
            if self.modal == Modal::Skills || self.modal == Modal::Mcp {
                match key.code {
                    KeyCode::Esc => {
                        self.modal = Modal::None;
                        return Ok(());
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.modal = Modal::None;
                        return Ok(());
                    }
                    KeyCode::Up => {
                        self.modal_cursor = self.modal_cursor.saturating_sub(1);
                        return Ok(());
                    }
                    KeyCode::Down => {
                        let len = if self.modal == Modal::Skills {
                            self.skill_rows().len()
                        } else {
                            self.mcp_rows().len()
                        };
                        if self.modal_cursor + 1 < len {
                            self.modal_cursor += 1;
                        }
                        return Ok(());
                    }
                    KeyCode::Tab => {
                        if self.modal == Modal::Skills {
                            self.toggle_selected_skill()?;
                        } else {
                            self.toggle_selected_mcp()?;
                        }
                        return Ok(());
                    }
                    KeyCode::Enter => return self.accept_modal(),
                    KeyCode::Backspace => {
                        self.modal_search.pop();
                        self.modal_cursor = 0;
                        return Ok(());
                    }
                    KeyCode::Char(c)
                        if !key.modifiers.contains(KeyModifiers::CONTROL)
                            && !key.modifiers.contains(KeyModifiers::ALT) =>
                    {
                        self.modal_search.push(c);
                        self.modal_cursor = 0;
                        return Ok(());
                    }
                    _ => return Ok(()),
                }
            }
            if self.modal == Modal::McpForm {
                return self.mcp_form_key(key);
            }
            match key.code {
                KeyCode::Esc => self.modal = Modal::None,
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.modal = Modal::None
                }
                KeyCode::Up => self.modal_cursor = self.modal_cursor.saturating_sub(1),
                KeyCode::Down if self.modal_cursor + 1 < self.modal_items.len() => {
                    self.modal_cursor += 1
                }
                KeyCode::Enter => self.accept_modal()?,
                _ => {}
            }
            return Ok(());
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('c') if self.busy => self.interrupt(),
                KeyCode::Char('c') => {
                    if self.modal == Modal::QuitConfirm {
                        // Second Ctrl+C on the confirm popup: quit.
                        self.should_quit = true;
                    } else if !self.input.is_empty() {
                        // Composer has content: clear it instead of quitting.
                        self.input.clear();
                        self.cursor = 0;
                        self.status = "input cleared".into();
                    } else {
                        // Empty composer: open the confirm popup.
                        self.modal = Modal::QuitConfirm;
                    }
                }
                KeyCode::Char('d') => self.should_quit = true,
                KeyCode::Char('l') => self.blocks.clear(),
                KeyCode::Char('r') => self.show_reasoning = !self.show_reasoning,
                KeyCode::Char('o') => self.show_tool_output = !self.show_tool_output,
                // Ctrl+V is not the primary paste key on macOS or most GUI
                // terminals; keep it as a fallback for setups where the native
                // shortcut cannot reach us, and let the terminal's own paste
                // (Cmd+V / Shift+Insert) flow through as a bracketed paste.
                KeyCode::Char('v') => self.attach_from_clipboard(),
                KeyCode::Char('b') => self.toggle_sidebar()?,
                // Ctrl+Enter inserts a newline; many terminals report it as
                // Ctrl+J, so both reach the same handler.
                KeyCode::Enter | KeyCode::Char('j') => {
                    self.input.insert(self.cursor, '\n');
                    self.cursor += 1;
                }
                _ => {}
            }
            return Ok(());
        }
        if key.modifiers.contains(KeyModifiers::ALT) {
            match key.code {
                KeyCode::Char(c @ '1'..='5') => {
                    self.select_tab(c as usize - '1' as usize);
                    return Ok(());
                }
                KeyCode::Left => {
                    self.page_sidebar(false);
                    return Ok(());
                }
                KeyCode::Right => {
                    self.page_sidebar(true);
                    return Ok(());
                }
                _ => {}
            }
        }

        let matches = self.command_matches();
        if !matches.is_empty() {
            match key.code {
                KeyCode::Up => {
                    self.palette_cursor = (self.palette_cursor + matches.len() - 1) % matches.len();
                    return Ok(());
                }
                KeyCode::Down => {
                    self.palette_cursor = (self.palette_cursor + 1) % matches.len();
                    return Ok(());
                }
                KeyCode::Tab => {
                    self.input = format!(
                        "/{} ",
                        matches[self.palette_cursor.min(matches.len() - 1)].0
                    );
                    self.cursor = self.input.len();
                    return Ok(());
                }
                KeyCode::Enter => {
                    let command =
                        format!("/{}", matches[self.palette_cursor.min(matches.len() - 1)].0);
                    self.input.clear();
                    self.cursor = 0;
                    self.palette_cursor = 0;
                    return self.run_command(&command);
                }
                _ => self.palette_cursor = 0,
            }
        }

        match key.code {
            KeyCode::F(index @ 1..=5) => {
                self.select_tab(index as usize - 1);
                return Ok(());
            }
            KeyCode::Enter => {
                let text = self.input.trim().to_string();
                if self.busy && !text.starts_with('/') {
                    self.status = "Still working; Ctrl+C interrupts".into();
                } else if !text.is_empty() {
                    self.input.clear();
                    self.cursor = 0;
                    if text.starts_with('/') {
                        self.run_command(&text)?;
                    } else {
                        self.start_turn(text);
                    }
                }
            }
            KeyCode::Char(character) => {
                self.input.insert(self.cursor, character);
                self.cursor += character.len_utf8();
            }
            KeyCode::Backspace if self.cursor > 0 => {
                // Treat a chip like `[Image 2]` as a single glyph so users can
                // remove an attachment with one Backspace, no separate command.
                if let Some((range, index)) = crate::attachments::chip_at(&self.input, self.cursor)
                {
                    self.input.replace_range(range.clone(), "");
                    self.cursor = range.start;
                    if index < self.attachments.len() {
                        self.attachments.remove(index);
                    }
                    self.renumber_chips();
                } else {
                    let previous = self.input[..self.cursor]
                        .char_indices()
                        .last()
                        .map(|(index, _)| index)
                        .unwrap_or(0);
                    self.input.drain(previous..self.cursor);
                    self.cursor = previous;
                }
            }
            KeyCode::Delete if self.cursor < self.input.len() => {
                let next = self.input[self.cursor..]
                    .char_indices()
                    .nth(1)
                    .map(|(index, _)| self.cursor + index)
                    .unwrap_or(self.input.len());
                self.input.drain(self.cursor..next);
            }
            KeyCode::Left if self.cursor > 0 => {
                self.cursor = self.input[..self.cursor]
                    .char_indices()
                    .last()
                    .map(|(index, _)| index)
                    .unwrap_or(0)
            }
            KeyCode::Right if self.cursor < self.input.len() => {
                self.cursor = self.input[self.cursor..]
                    .char_indices()
                    .nth(1)
                    .map(|(index, _)| self.cursor + index)
                    .unwrap_or(self.input.len())
            }
            KeyCode::Home => self.cursor = line_start(&self.input, self.cursor),
            KeyCode::End => self.cursor = line_end(&self.input, self.cursor),
            KeyCode::Up => {
                self.cursor = move_line(&self.input, self.cursor, -1);
            }
            KeyCode::Down => {
                self.cursor = move_line(&self.input, self.cursor, 1);
            }
            KeyCode::PageUp => {
                self.auto_scroll = false;
                self.scroll = self.scroll.saturating_sub(10);
            }
            KeyCode::PageDown => {
                self.scroll = self.scroll.saturating_add(10).min(self.max_scroll);
                self.auto_scroll = self.scroll == self.max_scroll;
            }
            KeyCode::Esc => {
                self.input.clear();
                self.cursor = 0;
            }
            _ => {}
        }
        Ok(())
    }
}

/// Byte offset of the start of the line containing `cursor`.
fn line_start(text: &str, cursor: usize) -> usize {
    text[..cursor].rfind('\n').map(|i| i + 1).unwrap_or(0)
}

/// Byte offset of the end of the line containing `cursor` (just before `\n`).
fn line_end(text: &str, cursor: usize) -> usize {
    text[cursor..]
        .find('\n')
        .map(|i| cursor + i)
        .unwrap_or(text.len())
}

/// Move the cursor up (`delta = -1`) or down (`delta = 1`) one visual line,
/// preserving the display column when possible. Uses `char` counts so wide
/// glyphs behave predictably.
fn move_line(text: &str, cursor: usize, delta: i32) -> usize {
    let ls = line_start(text, cursor);
    let col = text[ls..cursor].chars().count();
    if delta < 0 {
        if ls == 0 {
            return cursor;
        }
        let prev_end = ls - 1; // the '\n' before this line
        let prev_start = line_start(text, prev_end);
        let prev_len = text[prev_start..prev_end].chars().count();
        let target = col.min(prev_len);
        char_index(&text[prev_start..], target)
            .map(|off| prev_start + off)
            .unwrap_or(prev_end)
    } else {
        let le = line_end(text, cursor);
        if le >= text.len() {
            return cursor;
        }
        let next_start = le + 1;
        let next_end = line_end(text, next_start);
        let next_len = text[next_start..next_end].chars().count();
        let target = col.min(next_len);
        char_index(&text[next_start..], target)
            .map(|off| next_start + off)
            .unwrap_or(next_end)
    }
}

/// Byte offset of the Nth character in `s` (or None if out of range).
fn char_index(s: &str, n: usize) -> Option<usize> {
    if n == 0 {
        return Some(0);
    }
    s.char_indices().nth(n).map(|(i, _)| i).or(Some(s.len()))
}
