use super::*;
use crate::text::thousands;
use enowx_core::ROLES;

const TABS: [(&str, &str); 5] = [
    ("1", "TOKENS"),
    ("2", "TOOLS"),
    ("3", "SKILLS"),
    ("4", "AGENT"),
    ("5", "CONFIG"),
];

pub(super) fn draw_sidebar(frame: &mut Frame, app: &mut App, area: Rect) {
    if area.width < 24 || area.height < 6 {
        return;
    }
    let t = app.theme;
    app.sidebar_area = Some(area);
    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(t.border))
        .style(Style::default().bg(t.panel));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Tabs get their own three-row band so each label reads as a real control
    // rather than a cramped word fragment.
    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(inner);
    draw_tabs(frame, app, rows[0]);

    let body = rows[1].inner(Margin {
        horizontal: 2,
        vertical: 1,
    });
    let lines = sidebar_lines(app, body.width as usize);
    let page_size = body.height.max(1) as usize;
    app.sidebar_pages = lines.len().div_ceil(page_size).max(1);
    app.sidebar_page = app.sidebar_page.min(app.sidebar_pages - 1);
    frame.render_widget(
        Paragraph::new(
            lines
                .into_iter()
                .skip(app.sidebar_page * page_size)
                .take(page_size)
                .collect::<Vec<_>>(),
        ),
        body,
    );

    app.sidebar_pages_area = Some(rows[2]);
    // Only paging lives here; global shortcuts already sit in the window footer.
    let footer = if app.sidebar_pages > 1 {
        format!(
            "◀ Alt+←   page {}/{}   Alt+→ ▶",
            app.sidebar_page + 1,
            app.sidebar_pages
        )
    } else {
        String::new()
    };
    frame.render_widget(
        Paragraph::new(footer)
            .alignment(Alignment::Center)
            .style(Style::default().fg(t.muted).bg(t.subtle)),
        rows[2],
    );
}

fn draw_tabs(frame: &mut Frame, app: &mut App, area: Rect) {
    let t = app.theme;
    frame.render_widget(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(t.border))
            .style(Style::default().bg(t.subtle)),
        area,
    );
    // Full names need ~44 columns; below that the number alone still identifies
    // the tab and keeps every target the same clickable width.
    let compact = area.width < 44;
    for (index, (key, name)) in TABS.iter().enumerate() {
        let start = area.x + area.width * index as u16 / 5;
        let end = area.x + area.width * (index as u16 + 1) / 5;
        let rect = Rect::new(start, area.y, end.saturating_sub(start), 2);
        app.sidebar_tabs.push((rect, index));
        let active = index == app.sidebar_tab;
        let style = Style::default()
            .fg(if active { t.accent } else { t.muted })
            .bg(if active { t.active_tab } else { t.subtle });
        let label = if compact {
            (*key).to_owned()
        } else {
            format!("{key} {name}")
        };
        frame.render_widget(
            Paragraph::new(vec![
                Line::default(),
                Line::styled(
                    trim(&label, rect.width as usize),
                    if active {
                        style.add_modifier(Modifier::BOLD)
                    } else {
                        style
                    },
                ),
            ])
            .alignment(Alignment::Center)
            .style(style),
            rect,
        );
        if active && rect.width > 0 {
            frame.render_widget(
                Paragraph::new("─".repeat(rect.width as usize))
                    .style(Style::default().fg(t.accent).bg(t.active_tab)),
                Rect::new(rect.x, area.y + 2, rect.width, 1),
            );
        }
    }
}

/// A label/value row. Long values wrap under their label so nothing is clipped
/// and the value column stays aligned.
fn row(lines: &mut Vec<Line<'static>>, label: &str, value: &str, width: usize, theme: &Theme) {
    let gap = 1;
    if label.is_empty() {
        for wrapped in textwrap::wrap(value, width.max(1)) {
            lines.push(Line::styled(
                wrapped.into_owned(),
                Style::default().fg(theme.text),
            ));
        }
        return;
    }
    let room = width.saturating_sub(label.width() + gap);
    if value.width() <= room {
        let padding = width.saturating_sub(label.width() + value.width());
        lines.push(Line::from(vec![
            Span::styled(label.to_owned(), Style::default().fg(theme.muted)),
            Span::raw(" ".repeat(padding)),
            Span::styled(value.to_owned(), Style::default().fg(theme.text)),
        ]));
        return;
    }
    lines.push(Line::styled(
        label.to_owned(),
        Style::default().fg(theme.muted),
    ));
    for wrapped in textwrap::wrap(value, width.saturating_sub(2).max(1)) {
        lines.push(Line::styled(
            format!("  {wrapped}"),
            Style::default().fg(theme.text),
        ));
    }
}

fn heading(lines: &mut Vec<Line<'static>>, title: &str, width: usize, theme: &Theme) {
    if !lines.is_empty() {
        lines.push(Line::default());
    }
    let rule = width.saturating_sub(title.width() + 1);
    lines.push(Line::from(vec![
        Span::styled(
            title.to_owned(),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {}", "─".repeat(rule)),
            Style::default().fg(theme.border),
        ),
    ]));
}

fn gauge(percent: f64, width: usize, theme: &Theme) -> Line<'static> {
    let track = width.saturating_sub(7).max(4);
    let filled = ((track as f64) * percent / 100.0).round() as usize;
    Line::from(vec![
        Span::styled("█".repeat(filled), Style::default().fg(theme.accent)),
        Span::styled(
            "░".repeat(track.saturating_sub(filled)),
            Style::default().fg(theme.faint),
        ),
        Span::styled(format!(" {percent:>4.1}%"), Style::default().fg(theme.text)),
    ])
}

fn sidebar_lines(app: &App, width: usize) -> Vec<Line<'static>> {
    let t = &app.theme;
    let mut lines: Vec<Line<'static>> = Vec::new();
    let calls: usize = app.tool_counts.values().sum();

    match app.sidebar_tab {
        0 => {
            let window = app.context_window;
            let percent = if window > 0 {
                (100.0 * app.context_tokens as f64 / window as f64).min(100.0)
            } else {
                0.0
            };
            heading(&mut lines, "CONTEXT", width, t);
            row(
                &mut lines,
                "Used",
                &format!(
                    "{} / {}",
                    thousands(app.context_tokens as u64),
                    thousands(window as u64)
                ),
                width,
                t,
            );
            lines.push(gauge(percent, width, t));

            heading(&mut lines, "LAST MODEL CALL", width, t);
            row(
                &mut lines,
                "Input",
                &thousands(app.tokens_in as u64),
                width,
                t,
            );
            row(
                &mut lines,
                "Output",
                &thousands(app.tokens_out as u64),
                width,
                t,
            );

            heading(&mut lines, "SESSION", width, t);
            row(&mut lines, "Tool calls", &calls.to_string(), width, t);
            row(
                &mut lines,
                "Messages",
                &app.blocks
                    .iter()
                    .filter(|block| {
                        matches!(block.kind, TranscriptKind::User | TranscriptKind::Assistant)
                    })
                    .count()
                    .to_string(),
                width,
                t,
            );
            row(&mut lines, "State", app.activity.label(), width, t);

            heading(&mut lines, "COST", width, t);
            let cost = crate::pricing::cost_usd(&app.config, app.tokens_in, app.tokens_out, 0);
            row(
                &mut lines,
                "Session",
                &crate::pricing::format_cost(&app.config, cost),
                width,
                t,
            );
            row(
                &mut lines,
                "Per 1M in",
                &crate::pricing::format_cost(&app.config, app.config.model.price_input),
                width,
                t,
            );
            row(
                &mut lines,
                "Per 1M out",
                &crate::pricing::format_cost(&app.config, app.config.model.price_output),
                width,
                t,
            );
        }
        1 => {
            heading(&mut lines, "INVOCATIONS", width, t);
            row(&mut lines, "Total calls", &calls.to_string(), width, t);
            row(
                &mut lines,
                "Failed",
                &app.blocks
                    .iter()
                    .filter(|block| matches!(block.kind, TranscriptKind::Tool { error: true, .. }))
                    .count()
                    .to_string(),
                width,
                t,
            );

            heading(&mut lines, "TOOL ACCESS", width, t);
            for name in ROLES[0].allowed_tools() {
                let count = app.tool_counts.get(*name).copied().unwrap_or(0);
                let state = if app.role.allowed_tools().contains(name) {
                    format!("{count} calls")
                } else {
                    "blocked".to_owned()
                };
                row(&mut lines, name, &state, width, t);
            }

            heading(&mut lines, "MCP SERVERS", width, t);
            if app.discovery.mcp_servers.is_empty() {
                row(&mut lines, "", "no MCP servers detected", width, t);
            } else {
                for server in &app.discovery.mcp_servers {
                    let state = if server.enabled {
                        "enabled"
                    } else {
                        "disabled"
                    };
                    row(&mut lines, &server.name, state, width, t);
                    if server.enabled {
                        let tools = app.agent.mcp_tools(&server.name);
                        if tools.is_empty() {
                            row(&mut lines, "", "  (no tools reported)", width, t);
                        } else {
                            for tool in tools.iter().take(12) {
                                row(&mut lines, "", &format!("  · {}", tool.name), width, t);
                            }
                            if tools.len() > 12 {
                                row(
                                    &mut lines,
                                    "",
                                    &format!("  +{} more", tools.len() - 12),
                                    width,
                                    t,
                                );
                            }
                        }
                    }
                }
            }
        }
        2 => {
            let skills = &app.discovery.skills;
            heading(&mut lines, "SKILLS", width, t);
            row(&mut lines, "Loaded", &skills.len().to_string(), width, t);
            if !app.discovery.shadowed_skills.is_empty() {
                row(
                    &mut lines,
                    "Shadowed",
                    &app.discovery.shadowed_skills.len().to_string(),
                    width,
                    t,
                );
            }

            heading(&mut lines, "AVAILABLE", width, t);
            if skills.is_empty() {
                row(
                    &mut lines,
                    "",
                    "no skills found in this workspace or ~/",
                    width,
                    t,
                );
            } else {
                for skill in skills.iter().take(48) {
                    let scope = match skill.scope {
                        enowx_core::SkillScope::Project => "project",
                        enowx_core::SkillScope::User => "user",
                    };
                    row(&mut lines, &skill.name, scope, width, t);
                }
                if skills.len() > 48 {
                    row(
                        &mut lines,
                        "",
                        &format!("+{} more (tab 5)", skills.len() - 48),
                        width,
                        t,
                    );
                }
            }

            heading(&mut lines, "LOAD ON DEMAND", width, t);
            row(&mut lines, "Tool", "skill_read", width, t);
        }
        3 => {
            heading(&mut lines, "PRIMARY AGENT", width, t);
            row(&mut lines, "State", app.activity.label(), width, t);
            row(&mut lines, "Model", &app.config.model.default, width, t);
            row(
                &mut lines,
                "Step limit",
                &app.config.agent.max_steps.to_string(),
                width,
                t,
            );

            heading(&mut lines, "INSTRUCTIONS", width, t);
            let files = &app.discovery.instructions;
            row(&mut lines, "Files", &files.len().to_string(), width, t);
            for file in files.iter().take(24) {
                let scope = match file.scope {
                    enowx_core::SkillScope::Project => "project",
                    enowx_core::SkillScope::User => "user",
                };
                let name = file
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let label = if file.imports > 0 {
                    format!("{name} +{}", file.imports)
                } else {
                    name
                };
                row(&mut lines, &label, scope, width, t);
            }

            heading(&mut lines, "ACTIVE ROLE", width, t);
            for role in ROLES {
                let state = if role == app.role { "active" } else { "idle" };
                row(&mut lines, role.label(), state, width, t);
            }
        }
        _ => {
            heading(&mut lines, "WORKSPACE", width, t);
            row(
                &mut lines,
                "",
                &app.workspace.display().to_string(),
                width,
                t,
            );
            row(
                &mut lines,
                "Platform",
                &format!("{} / {}", std::env::consts::OS, std::env::consts::ARCH),
                width,
                t,
            );

            heading(&mut lines, "MODEL", width, t);
            row(&mut lines, "Provider", &app.config.provider.name, width, t);
            row(&mut lines, "Model", &app.config.model.default, width, t);
            row(
                &mut lines,
                "Context",
                &thousands(app.context_window as u64),
                width,
                t,
            );
            row(
                &mut lines,
                "Temperature",
                &app.config
                    .model
                    .temperature
                    .map_or("provider default".to_owned(), |value| value.to_string()),
                width,
                t,
            );

            heading(&mut lines, "SESSION", width, t);
            row(
                &mut lines,
                "ID",
                app.session_id.as_deref().unwrap_or("new session"),
                width,
                t,
            );
            row(&mut lines, "Theme", t.label, width, t);
            row(
                &mut lines,
                "Shell timeout",
                &format!("{}s", app.config.agent.shell_timeout_secs),
                width,
                t,
            );

            heading(&mut lines, "BOUNDARIES", width, t);
            row(&mut lines, "File tools", "workspace only", width, t);
            row(&mut lines, "Shell", "host, no sandbox", width, t);
        }
    }
    lines
}
