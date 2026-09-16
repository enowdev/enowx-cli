//! Skills / MCP / MCP-form popups. Split from `pickers.rs` so the search box,
//! toggle styling, and add-new form stay legible instead of squeezed into the
//! existing generic list picker.

use super::*;
use crate::app::mcp_ui::McpRow;
use crate::modal::{McpFormField, MCP_FORM_FIELDS, MCP_FORM_LABELS};
use enowx_core::discovery::{McpTransport, SkillScope};

/// Dispatch for the three new popups. `pickers::draw_modal` still owns every
/// legacy modal; this only handles the ones the composer commands opened.
pub(super) fn draw_popup(frame: &mut Frame, app: &mut App) -> bool {
    app.popup_rows.clear();
    app.popup_body = None;
    app.mcp_field_rows.clear();
    match app.modal {
        Modal::Skills => {
            draw_skills(frame, app);
            true
        }
        Modal::Mcp => {
            draw_mcp(frame, app);
            true
        }
        Modal::McpForm => {
            draw_mcp_form(frame, app);
            true
        }
        Modal::QuitConfirm => {
            draw_quit_confirm(frame, app);
            true
        }
        _ => false,
    }
}

fn popup_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(4));
    let height = height.min(area.height.saturating_sub(2));
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

fn frame_block(app: &App) -> Block<'static> {
    Block::default()
        .title(app.modal.title())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(app.theme.accent))
}

fn search_row(app: &App, area: Rect, frame: &mut Frame, footer: &str) {
    let text = format!("  search: {}_", app.modal_search);
    frame.render_widget(
        Paragraph::new(text).style(Style::default().fg(app.theme.text)),
        area,
    );
    if !footer.is_empty() {
        let footer_area = Rect {
            x: area.x,
            y: area.y + 1,
            width: area.width,
            height: 1,
        };
        frame.render_widget(
            Paragraph::new(format!("  {footer}")).style(Style::default().fg(app.theme.muted)),
            footer_area,
        );
    }
}

fn highlight_style(app: &App) -> Style {
    Style::default()
        .bg(app.theme.active_tab)
        .add_modifier(Modifier::BOLD)
}

// ------------------------------ Skills ------------------------------

fn draw_skills(frame: &mut Frame, app: &mut App) {
    let rows = app.skill_rows();
    if !rows.is_empty() && app.modal_cursor >= rows.len() {
        app.modal_cursor = rows.len() - 1;
    }
    let area = frame.area();
    let width = 92;
    let height = ((rows.len() as u16) + 6)
        .min(area.height.saturating_sub(2))
        .max(8);
    let popup = popup_rect(area, width, height);
    frame.render_widget(Clear, popup);
    let block = frame_block(app);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let layout = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(2),
        Constraint::Min(1),
    ])
    .split(inner);
    search_row(
        app,
        layout[1],
        frame,
        "Enter read · Tab enable/disable · Esc close",
    );

    if rows.is_empty() {
        frame.render_widget(
            Paragraph::new("no skills discovered — drop a SKILL.md under .agents/skills/")
                .style(Style::default().fg(app.theme.muted)),
            layout[2],
        );
        return;
    }

    let items: Vec<ListItem> = rows
        .iter()
        .map(|row| {
            let mark = if row.enabled { "●" } else { "○" };
            let color = if row.enabled {
                app.theme.accent
            } else {
                app.theme.muted
            };
            let name_style = if row.enabled {
                Style::default()
                    .fg(app.theme.text)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(app.theme.muted)
            };
            let scope = match row.scope {
                SkillScope::Project => "project",
                SkillScope::User => "user",
            };
            let desc = row.description.split('\n').next().unwrap_or("");
            let line = Line::from(vec![
                Span::styled(format!("  {mark} "), Style::default().fg(color)),
                Span::styled(format!("{:<28}", row.name), name_style),
                Span::styled(format!(" {scope:<8}"), Style::default().fg(app.theme.muted)),
                Span::styled(trim(desc, 44), Style::default().fg(app.theme.text)),
            ]);
            ListItem::new(line)
        })
        .collect();
    let mut state = ListState::default().with_selected(Some(app.modal_cursor));
    let list_area = layout[2];
    app.popup_body = Some(list_area);
    // Each row is one terminal line tall; record the row rect and a small
    // mark rect (first ~4 cols) so mouse clicks can distinguish toggle vs
    // select. Rendered rows can exceed the visible area; we only record what
    // is actually visible.
    let visible = list_area.height as usize;
    let start = app.modal_cursor.saturating_sub(visible.saturating_sub(1));
    for (offset, row_idx) in (start..(start + visible).min(rows.len())).enumerate() {
        let y = list_area.y + offset as u16;
        let row_rect = Rect {
            x: list_area.x,
            y,
            width: list_area.width,
            height: 1,
        };
        let mark_rect = Rect {
            x: list_area.x,
            y,
            width: 5.min(list_area.width),
            height: 1,
        };
        app.popup_rows.push((row_rect, mark_rect, row_idx));
    }
    frame.render_stateful_widget(
        List::new(items).highlight_style(highlight_style(app)),
        list_area,
        &mut state,
    );
}

// ------------------------------ MCP list ------------------------------

fn draw_mcp(frame: &mut Frame, app: &mut App) {
    let rows = app.mcp_rows();
    if !rows.is_empty() && app.modal_cursor >= rows.len() {
        app.modal_cursor = rows.len() - 1;
    }
    let area = frame.area();
    let width = 100;
    let height = ((rows.len() as u16) + 6)
        .min(area.height.saturating_sub(2))
        .max(8);
    let popup = popup_rect(area, width, height);
    frame.render_widget(Clear, popup);
    let block = frame_block(app);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let layout = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(2),
        Constraint::Min(1),
    ])
    .split(inner);
    search_row(
        app,
        layout[1],
        frame,
        "Enter inspect/add · Tab enable/disable · Esc close",
    );

    let items: Vec<ListItem> = rows
        .iter()
        .map(|row| match row {
            McpRow::Server {
                name,
                transport,
                scope,
                enabled,
                detail,
            } => {
                let mark = if *enabled { "●" } else { "○" };
                let color = if *enabled {
                    app.theme.accent
                } else {
                    app.theme.muted
                };
                let name_style = if *enabled {
                    Style::default()
                        .fg(app.theme.text)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(app.theme.muted)
                };
                let transport = match transport {
                    McpTransport::Stdio => "stdio",
                    McpTransport::Http => "http",
                    McpTransport::Sse => "sse",
                };
                let scope = match scope {
                    SkillScope::Project => "project",
                    SkillScope::User => "user",
                };
                let _ = detail;
                ListItem::new(Line::from(vec![
                    Span::styled(format!("  {mark} "), Style::default().fg(color)),
                    Span::styled(format!("{:<24}", name), name_style),
                    Span::styled(
                        format!(" {transport:<6}"),
                        Style::default().fg(app.theme.muted),
                    ),
                    Span::styled(format!(" {scope:<8}"), Style::default().fg(app.theme.muted)),
                ]))
            }
            McpRow::AddNew => ListItem::new(Line::from(vec![Span::styled(
                "  + Add new MCP server",
                Style::default()
                    .fg(app.theme.accent)
                    .add_modifier(Modifier::BOLD),
            )])),
        })
        .collect();
    let mut state = ListState::default().with_selected(Some(app.modal_cursor));
    let list_area = layout[2];
    app.popup_body = Some(list_area);
    let visible = list_area.height as usize;
    let start = app.modal_cursor.saturating_sub(visible.saturating_sub(1));
    for (offset, row_idx) in (start..(start + visible).min(rows.len())).enumerate() {
        let y = list_area.y + offset as u16;
        let row_rect = Rect {
            x: list_area.x,
            y,
            width: list_area.width,
            height: 1,
        };
        let mark_rect = Rect {
            x: list_area.x,
            y,
            width: 5.min(list_area.width),
            height: 1,
        };
        app.popup_rows.push((row_rect, mark_rect, row_idx));
    }
    frame.render_stateful_widget(
        List::new(items).highlight_style(highlight_style(app)),
        list_area,
        &mut state,
    );
}

// ------------------------------ MCP form ------------------------------

fn draw_mcp_form(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let width = 78;
    let height = 14;
    let popup = popup_rect(area, width, height);
    frame.render_widget(Clear, popup);
    let block = frame_block(app);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let layout = Layout::vertical([
        Constraint::Length(MCP_FORM_FIELDS.len() as u16 + 1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(inner);

    for (i, field) in MCP_FORM_FIELDS.iter().enumerate() {
        let selected = i == app.mcp_field;
        let label_style = if selected {
            Style::default()
                .fg(app.theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(app.theme.muted)
        };
        let value = match field {
            McpFormField::Name => app.mcp_draft.name.clone(),
            McpFormField::Command => app.mcp_draft.command.clone(),
            McpFormField::Args => app.mcp_draft.args.clone(),
            McpFormField::Env => app.mcp_draft.env.chars().take(60).collect(),
            McpFormField::Transport => match app.mcp_draft.transport {
                McpTransport::Stdio => "stdio".into(),
                McpTransport::Http => "http".into(),
                McpTransport::Sse => "sse".into(),
            },
        };
        let caret = if selected { "▸ " } else { "  " };
        let empty = value_is_empty(field, app);
        let value_display = if empty {
            field.placeholder().to_string()
        } else {
            value
        };
        let value_style = if empty {
            Style::default().fg(app.theme.muted)
        } else {
            Style::default().fg(app.theme.text)
        };
        let line = Line::from(vec![
            Span::styled(caret, label_style),
            Span::styled(format!("{:<28}", MCP_FORM_LABELS[i]), label_style),
            Span::styled(value_display, value_style),
        ]);
        let row = Rect {
            x: inner.x,
            y: inner.y + i as u16,
            width: inner.width,
            height: 1,
        };
        app.mcp_field_rows.push((row, i));
        frame.render_widget(Paragraph::new(line), row);
    }

    let footer = "Tab/↓ next · ↑ prev · Space cycles transport · Enter save · Esc cancel";
    frame.render_widget(
        Paragraph::new(footer).style(Style::default().fg(app.theme.muted)),
        layout[1],
    );
    if !app.modal_error.is_empty() {
        frame.render_widget(
            Paragraph::new(format!("  {}", app.modal_error))
                .style(Style::default().fg(app.theme.red)),
            layout[2],
        );
    }
}

fn value_is_empty(field: &McpFormField, app: &App) -> bool {
    match field {
        McpFormField::Name => app.mcp_draft.name.is_empty(),
        McpFormField::Command => app.mcp_draft.command.is_empty(),
        McpFormField::Args => app.mcp_draft.args.is_empty(),
        McpFormField::Env => app.mcp_draft.env.is_empty(),
        McpFormField::Transport => false,
    }
}

/// Simple confirm dialog for Ctrl+C on an empty composer. Y/Enter quits,
/// N/Esc cancels. Kept small so it never covers the transcript.
fn draw_quit_confirm(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let width = 46;
    let height = 6;
    let popup = popup_rect(area, width, height);
    frame.render_widget(Clear, popup);
    let block = frame_block(app);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let t = app.theme;

    let prompt = Rect {
        x: inner.x + 2,
        y: inner.y + 1,
        width: inner.width.saturating_sub(4),
        height: 1,
    };
    frame.render_widget(
        Paragraph::new("Quit Enx? Unsent input will be lost.").style(Style::default().fg(t.text)),
        prompt,
    );

    // Two buttons centered on the third row: `[  Yes  ]  [  No  ]`. Active
    // one uses `active_tab` bg + accent fg so keyboard focus is obvious;
    // idle one is muted. Both rects are registered for mouse click.
    let yes_label = "  Yes  ";
    let no_label = "  No  ";
    let gap = 2usize;
    let total_w = yes_label.chars().count() + gap + no_label.chars().count() + 4; // 2 brackets pairs
    let start = inner.x + (inner.width.saturating_sub(total_w as u16)) / 2;
    let y = inner.y + 3;
    let yes_rect = Rect {
        x: start,
        y,
        width: (yes_label.chars().count() + 2) as u16,
        height: 1,
    };
    let no_rect = Rect {
        x: yes_rect.x + yes_rect.width + gap as u16,
        y,
        width: (no_label.chars().count() + 2) as u16,
        height: 1,
    };
    app.quit_confirm_rects = [(yes_rect, true), (no_rect, false)];

    let active_style = Style::default()
        .fg(t.accent)
        .bg(t.active_tab)
        .add_modifier(Modifier::BOLD);
    let idle_style = Style::default().fg(t.muted);
    let (yes_style, no_style) = if app.quit_confirm_yes {
        (active_style, idle_style)
    } else {
        (idle_style, active_style)
    };
    frame.render_widget(
        Paragraph::new(vec![Line::from(vec![Span::styled(
            format!("[{yes_label}]"),
            yes_style,
        )])]),
        yes_rect,
    );
    frame.render_widget(
        Paragraph::new(vec![Line::from(vec![Span::styled(
            format!("[{no_label}]"),
            no_style,
        )])]),
        no_rect,
    );
}
