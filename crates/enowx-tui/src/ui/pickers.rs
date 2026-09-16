use super::settings::draw_settings;
use super::*;

pub(super) fn draw_modal(frame: &mut Frame, app: &mut App) {
    app.modal_rows.clear();
    let area = frame.area();
    if app.modal.is_form() {
        draw_settings(frame, app, area);
        return;
    }
    let width = area.width.saturating_sub(4).min(
        if app.modal == Modal::Sessions || app.modal == Modal::Attach {
            96
        } else {
            72
        },
    );
    let per_row = if matches!(
        app.modal,
        Modal::Roles | Modal::ModelSource | Modal::Providers | Modal::Themes
    ) {
        2
    } else {
        1
    };
    let footer = if app.modal == Modal::Models { 2 } else { 0 };
    let body = if app.modal == Modal::Models {
        (app.modal_items.len() as u16).max(3)
    } else {
        (app.modal_items.len() as u16 * per_row).min(20)
    };
    let height = (body + footer + 2).min(area.height.saturating_sub(2));
    let modal = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, modal);
    let title = app.modal.title();
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(app.theme.accent));
    let inner = block.inner(modal);
    frame.render_widget(block, modal);
    if app.modal == Modal::Models {
        let rows = Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).split(inner);
        draw_model_list(frame, app, rows[0]);
        let hint = if app.modal_items.is_empty() {
            "F5 retry  ·  F2 enter an ID  ·  Esc cancel"
        } else {
            "Enter uses the model right away  ·  F5 refresh  ·  Esc cancel"
        };
        frame.render_widget(
            Paragraph::new(hint).style(Style::default().fg(app.theme.faint)),
            rows[1],
        );
        return;
    }
    let mut y = inner.y;
    let items: Vec<ListItem> = app
        .modal_items
        .iter()
        .enumerate()
        .map(|(index, (id, description))| {
            let selected = index == app.modal_cursor;
            let marker = if selected { "❯ " } else { "  " };
            let label = if app.modal == Modal::Sessions {
                // Session picker rows hide the internal id, keeping the visible
                // list to the title and metadata the user recognises.
                format!("{marker}{description}")
            } else {
                format!("{marker}{id}\n    {description}")
            };
            let rows = label.matches('\n').count() as u16 + 1;
            if y + rows <= inner.bottom() {
                app.modal_rows
                    .push((Rect::new(inner.x, y, inner.width, rows), index));
                y += rows;
            }
            ListItem::new(label).style(if selected {
                Style::default()
                    .fg(app.theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(app.theme.text)
            })
        })
        .collect();
    let mut state = ListState::default().with_selected(Some(app.modal_cursor));
    frame.render_stateful_widget(List::new(items), inner, &mut state);
}

pub(super) fn draw_model_list(frame: &mut Frame, app: &App, area: Rect) {
    if app.discovering_models {
        frame.render_widget(
            Paragraph::new(format!(
                "Asking {} for its model list…",
                app.settings.provider
            ))
            .style(Style::default().fg(app.theme.muted)),
            area,
        );
        return;
    }
    if app.modal_items.is_empty() {
        frame.render_widget(
            Paragraph::new(if app.modal_error.is_empty() {
                "No models detected. Enter a model-list URL or use manual entry."
            } else {
                &app.modal_error
            })
            .wrap(ratatui::widgets::Wrap { trim: false })
            .style(Style::default().fg(app.theme.red)),
            area,
        );
        return;
    }
    let items: Vec<ListItem> = app
        .modal_items
        .iter()
        .enumerate()
        .map(|(index, (id, description))| {
            let selected = index == app.modal_cursor;
            let marker = if selected { "❯ " } else { "  " };
            let label = if description.is_empty() {
                format!("{marker}{id}")
            } else {
                format!("{marker}{id}  ({description})")
            };
            ListItem::new(label).style(if selected {
                Style::default()
                    .fg(app.theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(app.theme.text)
            })
        })
        .collect();
    let mut state = ListState::default().with_selected(Some(app.modal_cursor));
    frame.render_stateful_widget(List::new(items), area, &mut state);
}
