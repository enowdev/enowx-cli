use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

impl App {
    /// Editing provider identity invalidates everything scoped to the old
    /// provider: its key, endpoints, and any detected model.
    pub(crate) fn settings_changed(&mut self, field: SettingsField) {
        self.modal_error.clear();
        if matches!(field, SettingsField::Provider | SettingsField::BaseUrl) {
            self.settings.preset = "custom".into();
            self.settings.api_key.clear();
            self.settings.models_url.clear();
            self.settings.model.clear();
            self.settings.context_window.clear();
        }
        if field != SettingsField::Model && field != SettingsField::ContextWindow {
            self.models.clear();
            self.modal_items.clear();
            self.model_events = None;
            self.discovering_models = false;
        }
    }

    pub(crate) fn settings_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('c') => self.modal = Modal::None,
                KeyCode::Char('u') => {
                    self.settings_changed(SETTINGS_FIELDS[self.modal_cursor]);
                    self.settings
                        .value_mut(SETTINGS_FIELDS[self.modal_cursor])
                        .clear();
                    self.field_cursor = 0;
                }
                _ => {}
            }
            return Ok(());
        }
        let field = SETTINGS_FIELDS[self.modal_cursor];
        match key.code {
            KeyCode::Esc => {
                self.modal = Modal::None;
                self.model_events = None;
            }
            KeyCode::Up | KeyCode::BackTab if self.modal == Modal::Settings => {
                self.modal_cursor =
                    (self.modal_cursor + SETTINGS_FIELDS.len() - 1) % SETTINGS_FIELDS.len();
                self.field_cursor = self
                    .settings
                    .value(SETTINGS_FIELDS[self.modal_cursor])
                    .len();
            }
            KeyCode::Down | KeyCode::Tab if self.modal == Modal::Settings => {
                self.modal_cursor = (self.modal_cursor + 1) % SETTINGS_FIELDS.len();
                self.field_cursor = self
                    .settings
                    .value(SETTINGS_FIELDS[self.modal_cursor])
                    .len();
            }
            KeyCode::Enter if self.modal == Modal::ModelUrl => self.discover_models(),
            KeyCode::Enter if self.modal == Modal::ProviderKey => self.connect_preset()?,
            KeyCode::Enter if field == SettingsField::Theme => {
                self.open_themes();
                return Ok(());
            }
            KeyCode::F(2) if field == SettingsField::Theme => {
                self.open_themes();
                return Ok(());
            }
            KeyCode::Enter => self.save_settings()?,
            KeyCode::F(5) if self.modal == Modal::Settings => self.open_model_source(),
            KeyCode::Char(character) => {
                self.settings_changed(field);
                let value = self.settings.value_mut(field);
                value.insert(self.field_cursor, character);
                self.field_cursor += character.len_utf8();
            }
            KeyCode::Backspace if self.field_cursor > 0 => {
                self.settings_changed(field);
                let value = self.settings.value_mut(field);
                let previous = value[..self.field_cursor]
                    .char_indices()
                    .last()
                    .map(|(index, _)| index)
                    .unwrap_or(0);
                value.drain(previous..self.field_cursor);
                self.field_cursor = previous;
            }
            KeyCode::Delete => {
                self.settings_changed(field);
                let value = self.settings.value_mut(field);
                if self.field_cursor < value.len() {
                    let next = value[self.field_cursor..]
                        .char_indices()
                        .nth(1)
                        .map(|(index, _)| self.field_cursor + index)
                        .unwrap_or(value.len());
                    value.drain(self.field_cursor..next);
                }
            }
            KeyCode::Left if self.field_cursor > 0 => {
                self.field_cursor = self.settings.value(field)[..self.field_cursor]
                    .char_indices()
                    .last()
                    .map(|(index, _)| index)
                    .unwrap_or(0);
            }
            KeyCode::Right => {
                let value = self.settings.value(field);
                if self.field_cursor < value.len() {
                    self.field_cursor = value[self.field_cursor..]
                        .char_indices()
                        .nth(1)
                        .map(|(index, _)| self.field_cursor + index)
                        .unwrap_or(value.len());
                }
            }
            KeyCode::Home => self.field_cursor = 0,
            KeyCode::End => self.field_cursor = self.settings.value(field).len(),
            _ => {}
        }
        Ok(())
    }
}
