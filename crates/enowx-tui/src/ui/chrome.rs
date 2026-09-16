use super::*;

pub(super) fn draw_main(frame: &mut Frame, app: &mut App, area: Rect) {
    app.sidebar_area = None;
    app.sidebar_tabs.clear();
    app.sidebar_pages_area = None;
    let border = Block::default()
        .borders(if area.height <= 8 {
            Borders::NONE
        } else {
            Borders::ALL
        })
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(app.theme.border))
        .style(Style::default().bg(app.theme.panel));
    let inner = border.inner(area);
    frame.render_widget(border, area);
    let header = if inner.height >= 10 { 2 } else { 0 };
    let footer = if inner.height >= 10 { 1 } else { 0 };
    let parts = Layout::vertical([
        Constraint::Length(header),
        Constraint::Min(3),
        Constraint::Length(footer),
    ])
    .split(inner);
    if header > 0 {
        draw_title(frame, app, parts[0]);
    }
    // The chat pane owns the window: telemetry only appears when both panes fit
    // side by side, so shrinking the terminal never buries the conversation.
    let sidebar = if app.show_sidebar && parts[1].width >= 100 && parts[1].height >= 12 {
        (parts[1].width * 2 / 5).clamp(38, 60)
    } else {
        0
    };
    if sidebar > 0 {
        let columns =
            Layout::horizontal([Constraint::Min(52), Constraint::Length(sidebar)]).split(parts[1]);
        draw_composer_pane(frame, app, columns[0]);
        draw_sidebar(frame, app, columns[1]);
    } else {
        draw_composer_pane(frame, app, parts[1]);
    }
    if footer > 0 {
        draw_footer(frame, app, parts[2]);
    }
}

fn draw_title(frame: &mut Frame, app: &App, area: Rect) {
    let t = app.theme;
    frame.render_widget(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(t.border))
            .style(Style::default().bg(t.subtle)),
        area,
    );
    let project = app
        .workspace
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    let dots = Line::from(vec![
        Span::styled("● ", Style::default().fg(t.red)),
        Span::styled("● ", Style::default().fg(t.yellow)),
        Span::styled("●", Style::default().fg(t.green)),
    ]);
    frame.render_widget(Paragraph::new(dots), Rect::new(area.x + 1, area.y, 6, 1));
    let title = if area.width >= 100 {
        format!("PROJECT: {project}   PATH: {}", app.workspace.display())
    } else {
        format!("PROJECT: {project}")
    };
    draw_split_line(
        frame,
        Line::styled(title, Style::default().fg(t.text)),
        Line::styled(
            format!(
                "{} · PID {}",
                if app.busy { "RUNNING" } else { "READY" },
                std::process::id()
            ),
            Style::default().fg(t.green),
        ),
        Rect::new(area.x + 8, area.y, area.width.saturating_sub(10), 1),
    );
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let t = app.theme;
    frame.render_widget(Block::default().style(Style::default().bg(t.subtle)), area);

    // LEFT: spinner (busy only) + role + model. Idle just shows role + model.
    let mut left_spans: Vec<Span<'static>> = Vec::new();
    if app.busy {
        left_spans.push(Span::styled(
            format!("{} ", app.spinner()),
            Style::default().fg(t.yellow),
        ));
    } else {
        left_spans.push(Span::styled(
            "● ",
            Style::default().fg(if app.status == "failed" {
                t.red
            } else {
                t.green
            }),
        ));
    }
    left_spans.push(Span::styled(
        app.role.label().to_string(),
        Style::default()
            .fg(t.accent)
            .add_modifier(ratatui::style::Modifier::BOLD),
    ));
    left_spans.push(Span::styled(" · ", Style::default().fg(t.muted)));
    left_spans.push(Span::styled(app.model_label(), Style::default().fg(t.text)));
    if app.busy {
        left_spans.push(Span::styled(" · ", Style::default().fg(t.muted)));
        left_spans.push(Span::styled(
            crate::app::fmt_elapsed(app.turn_started.elapsed().as_secs()),
            Style::default().fg(t.yellow),
        ));
    }

    // RIGHT: rotating tip so the footer never reads as blank.
    let right = Line::from(vec![Span::styled(
        app.footer_tip().to_string(),
        Style::default().fg(t.muted),
    )]);

    draw_split_line(
        frame,
        Line::from(left_spans),
        right,
        area.inner(Margin {
            horizontal: 1,
            vertical: 0,
        }),
    );
}

pub(super) fn draw_split_line(frame: &mut Frame, left: Line, right: Line, area: Rect) {
    let rw = if right.width() + left.width().min(20) + 2 <= area.width as usize {
        right.width() as u16
    } else {
        0
    };
    let lw = area.width.saturating_sub(if rw > 0 { rw + 2 } else { 0 });
    let left = if left.width() > lw as usize {
        Line::styled(trim(&left.to_string(), lw as usize), left.style)
    } else {
        left
    };
    frame.render_widget(
        Paragraph::new(left),
        Rect::new(area.x, area.y, lw, area.height),
    );
    if rw > 0 {
        frame.render_widget(
            Paragraph::new(right).alignment(Alignment::Right),
            Rect::new(area.right() - rw, area.y, rw, area.height),
        );
    }
}
