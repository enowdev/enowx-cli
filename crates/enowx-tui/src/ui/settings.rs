use super::*;

pub(super) fn draw_settings(frame: &mut Frame, app: &App, area: Rect) {
    let width = area.width.saturating_sub(2).min(72);
    let height = (if matches!(app.modal, Modal::ModelUrl | Modal::ProviderKey) {
        10
    } else {
        26
    })
    .min(area.height.saturating_sub(2));
    let modal = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, modal);
    let block = Block::default()
        .title(if app.modal == Modal::ModelUrl {
            " Auto detect · model-list URL "
        } else if app.modal == Modal::ProviderKey {
            &app.settings.provider
        } else {
            " Settings & Configuration "
        })
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(app.theme.accent));
    let inner = block.inner(modal);
    frame.render_widget(block, modal);
    let footer_height = if inner.height >= 10 { 4 } else { 2 };
    let rows =
        Layout::vertical([Constraint::Min(2), Constraint::Length(footer_height)]).split(inner);
    let visible = if matches!(app.modal, Modal::ModelUrl | Modal::ProviderKey) {
        1
    } else {
        (rows[0].height / 3).max(1) as usize
    };
    let start = if app.modal == Modal::ModelUrl {
        3
    } else if app.modal == Modal::ProviderKey {
        2
    } else {
        app.modal_cursor.saturating_sub(visible - 1)
    };
    let labels = crate::modal::SETTINGS_LABELS;
    for (index, field) in SETTINGS_FIELDS
        .iter()
        .copied()
        .enumerate()
        .skip(start)
        .take(visible)
    {
        let active = index == app.modal_cursor;
        let raw = app.settings.value(field);
        let shown = if field == SettingsField::ApiKey {
            "•".repeat(raw.chars().count())
        } else if field == SettingsField::Theme {
            format!("{} (Enter/F2 to change)", Theme::find(raw).label)
        } else {
            raw.to_owned()
        };
        let y = rows[0].y + ((index - start) * 3) as u16;
        frame.render_widget(
            Paragraph::new(format!(
                "{} {}",
                if active { ">" } else { " " },
                labels[index]
            ))
            .style(Style::default().fg(if active {
                app.theme.accent
            } else {
                app.theme.faint
            })),
            Rect::new(inner.x, y, inner.width, 1),
        );
        let field_area = Rect::new(inner.x + 2, y + 1, inner.width.saturating_sub(3), 1);
        let cursor_column = if !active {
            0
        } else if field == SettingsField::ApiKey {
            raw[..app.field_cursor].chars().count()
        } else {
            raw[..app.field_cursor].width()
        };
        let offset = if active {
            cursor_column.saturating_sub(field_area.width.saturating_sub(1) as usize)
        } else {
            0
        };
        let content = if shown.is_empty() { "(empty)" } else { &shown };
        frame.render_widget(
            Paragraph::new(content)
                .scroll((0, offset.min(u16::MAX as usize) as u16))
                .style(Style::default().fg(if shown.is_empty() {
                    app.theme.faint
                } else {
                    app.theme.text
                })),
            field_area,
        );
        if active && field_area.width > 0 {
            frame.set_cursor_position((
                field_area.x + (cursor_column - offset) as u16,
                field_area.y,
            ));
        }
    }
    let hint = if app.modal_error.is_empty() {
        if app.modal == Modal::ModelUrl {
            "Enter detect · Ctrl+U clear · Esc cancel\nFull JSON endpoint URL; metadata depends on what it returns."
        } else if app.modal == Modal::ProviderKey {
            "Enter connect · Ctrl+U clear · Esc cancel\nProvider endpoints are preset. Add a model after connecting."
        } else if SETTINGS_FIELDS.get(app.modal_cursor) == Some(&SettingsField::Theme) {
            "Enter / F2 choose theme · Tab field · Esc cancel\nSwitch between 5 Minimalist Y2K palettes."
        } else if inner.width < 40 {
            "Tab field · Enter save\nF5 add model · Esc cancel"
        } else {
            "Tab / ↑↓ field · Ctrl+U clear · Enter save · Esc cancel\nF5 add model: Auto detect from URL or Manual entry."
        }
    } else {
        &app.modal_error
    };
    frame.render_widget(
        Paragraph::new(hint)
            .wrap(ratatui::widgets::Wrap { trim: false })
            .style(Style::default().fg(if app.modal_error.is_empty() {
                app.theme.faint
            } else {
                app.theme.red
            })),
        rows[1],
    );
}
