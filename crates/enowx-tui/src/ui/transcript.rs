use super::*;

pub(super) fn draw_welcome(frame: &mut Frame, app: &App, area: Rect) {
    let (title, hint) = if app.config.is_ready() {
        (
            "What are we working on?",
            "/sessions to resume · /help for commands",
        )
    } else if app.config.provider_active() && app.config.model.default.is_empty() {
        ("Choose a model", "Open /model to select a model.")
    } else {
        (
            "Connect a provider",
            "Open /provider to configure your connection.",
        )
    };
    let mut lines = Vec::new();
    if area.height > 5 {
        lines.push(Line::default());
    }
    for line in textwrap::wrap(title, area.width as usize) {
        lines.push(Line::styled(
            line.into_owned(),
            Style::default()
                .fg(app.theme.text)
                .add_modifier(Modifier::BOLD),
        ));
    }
    if area.height > 3 {
        lines.push(Line::default());
        for line in textwrap::wrap(hint, area.width as usize) {
            lines.push(Line::styled(
                line.into_owned(),
                Style::default().fg(app.theme.muted),
            ));
        }
    }
    frame.render_widget(Paragraph::new(lines), area);
}

pub(super) fn draw_transcript(frame: &mut Frame, app: &mut App, area: Rect) {
    let theme = app.theme;
    let width = area.width.saturating_sub(1).max(1) as usize;
    app.tool_header_markers.clear();
    app.tool_header_rects.clear();
    app.file_link_markers.clear();
    app.file_link_rects.clear();
    app.transcript_area = Some(area);
    let mut lines: Vec<Line> = Vec::new();
    let mut compact_tools = false;
    for block in &app.blocks {
        if matches!(block.kind, TranscriptKind::Reasoning) && !app.show_reasoning {
            continue;
        }
        if compact_tools && !matches!(block.kind, TranscriptKind::Tool { .. }) {
            lines.push(Line::default());
            compact_tools = false;
        }
        match &block.kind {
            TranscriptKind::User => {
                user_card(&mut lines, &block.text, width, &theme);
            }
            TranscriptKind::Assistant => {
                render_markdown(&block.text, width, &mut lines, &theme);
            }
            TranscriptKind::Reasoning if app.show_reasoning => {
                card(
                    &mut lines,
                    "THOUGHT TRACE",
                    &block.text,
                    width,
                    theme.accent2,
                    &theme,
                );
            }
            TranscriptKind::Reasoning => continue,
            TranscriptKind::Tool {
                id,
                name,
                args,
                result,
                running,
                error,
            } => {
                use crate::ui::tool::{classify, render_diff, ToolBody, ToolRender};
                let icon = if *running {
                    ("›", theme.yellow)
                } else if *error {
                    ("✗", theme.red)
                } else {
                    ("✓", theme.green)
                };
                let render = classify(name, args, result);
                // `bash` output can be huge (a stray `ls` on node_modules
                // spills hundreds of lines). Default-collapse it regardless
                // of the master toggle; user clicks the header to expand.
                let default_expand = if name == "bash" {
                    false
                } else {
                    app.show_tool_output
                };
                let expanded = app.tool_expanded.get(id).copied().unwrap_or(default_expand);
                match render {
                    ToolRender::Summary(text) => {
                        // Register a file marker for tools whose summary
                        // starts with `<verb> <path>` so a click opens it.
                        register_summary_file_link(
                            name,
                            args,
                            &text,
                            lines.len(),
                            &mut app.file_link_markers,
                        );
                        lines.push(Line::from(vec![
                            Span::styled(format!("{} ", icon.0), Style::default().fg(icon.1)),
                            Span::styled(text, Style::default().fg(theme.muted)),
                        ]));
                    }
                    ToolRender::Detail {
                        header,
                        subtitle,
                        body,
                    } => {
                        let chevron = if expanded { "▾" } else { "▸" };
                        let header_y_marker = lines.len();
                        lines.push(Line::from(vec![
                            Span::styled(
                                format!("{chevron} {} ", icon.0),
                                Style::default().fg(icon.1),
                            ),
                            Span::styled(header, Style::default().fg(theme.text)),
                        ]));
                        app.tool_header_markers.push((id.clone(), header_y_marker));
                        // Header path (write, bash) is also a link target.
                        register_detail_file_link(
                            name,
                            args,
                            header_y_marker,
                            &mut app.file_link_markers,
                        );
                        if expanded || *error {
                            if let Some(sub) = subtitle {
                                for wrapped in textwrap::wrap(&sub, width.saturating_sub(4).max(1))
                                {
                                    lines.push(Line::from(vec![
                                        Span::styled("    ", Style::default().fg(theme.muted)),
                                        Span::styled(
                                            wrapped.into_owned(),
                                            Style::default().fg(theme.muted),
                                        ),
                                    ]));
                                }
                            }
                            match body {
                                ToolBody::Plain(text) => {
                                    // Parse ANSI SGR so `ls --color`,
                                    // `grep --color`, and other TUI-aware
                                    // programs render with their real
                                    // colors instead of leaking `[m]`
                                    // fragments. Non-bash Plain bodies
                                    // (MCP proxy, generic tool) go through
                                    // the same path — safe: they either
                                    // have no escapes or the parser drops
                                    // them.
                                    let bg = theme.subtle;
                                    let border_style = Style::default().fg(theme.accent).bg(bg);
                                    let pieces = crate::ansi::parse(text);
                                    let mut current: Vec<Span<'static>> = Vec::new();
                                    let mut current_w: usize = 0;
                                    // Layout: `│ content …` → 2 col gutter
                                    // (`│ `) + body + right pad to full width.
                                    let max_body_w = width.saturating_sub(3).max(1);
                                    let flush = |lines: &mut Vec<Line<'static>>,
                                                 current: &mut Vec<Span<'static>>,
                                                 current_w: &mut usize| {
                                        let pad = max_body_w.saturating_sub(*current_w);
                                        let mut row: Vec<Span<'static>> = Vec::new();
                                        row.push(Span::styled("│ ", border_style));
                                        row.extend(std::mem::take(current));
                                        row.push(Span::styled(
                                            format!("{} ", " ".repeat(pad)),
                                            Style::default().bg(bg),
                                        ));
                                        lines.push(Line::from(row));
                                        *current_w = 0;
                                    };
                                    for piece in pieces {
                                        let style = piece.style.bg(bg);
                                        for segment in piece.text.split_inclusive('\n') {
                                            let is_nl = segment.ends_with('\n');
                                            let visible: String = if is_nl {
                                                segment[..segment.len() - 1].to_string()
                                            } else {
                                                segment.to_string()
                                            };
                                            let vw = visible.chars().count();
                                            let room = max_body_w.saturating_sub(current_w);
                                            let (shown, overflow) = if vw > room {
                                                let mut s: String = visible
                                                    .chars()
                                                    .take(room.saturating_sub(1).max(1))
                                                    .collect();
                                                s.push('…');
                                                (s, true)
                                            } else {
                                                (visible, false)
                                            };
                                            let shown_w = shown.chars().count();
                                            current.push(Span::styled(shown, style));
                                            current_w += shown_w;
                                            if is_nl || overflow {
                                                flush(&mut lines, &mut current, &mut current_w);
                                            }
                                        }
                                    }
                                    if !current.is_empty() {
                                        flush(&mut lines, &mut current, &mut current_w);
                                    }
                                }
                                ToolBody::Diff {
                                    path,
                                    old,
                                    new,
                                    start_line,
                                } => {
                                    render_diff(
                                        &path,
                                        &old,
                                        &new,
                                        start_line,
                                        width,
                                        &mut lines,
                                        &theme,
                                        &mut app.file_link_markers,
                                    );
                                }
                                ToolBody::Preview { content, total } => {
                                    crate::ui::tool::render_preview(
                                        &content, total, width, &mut lines, &theme,
                                    );
                                }
                                ToolBody::Tree { items } => {
                                    crate::ui::tool::render_tree(
                                        &items,
                                        width,
                                        &mut lines,
                                        &theme,
                                        &mut app.file_link_markers,
                                    );
                                }
                                ToolBody::Todo { items } => {
                                    crate::ui::tool::render_todo(&items, width, &mut lines, &theme);
                                }
                            }
                        }
                    }
                }
            }
            TranscriptKind::Notice => {
                let mut inner: Vec<Line<'static>> = Vec::new();
                render_markdown(&block.text, width, &mut inner, &theme);
                for line in inner {
                    lines.push(line);
                }
            }
            TranscriptKind::Error => {
                lines.push(Line::styled(
                    "error",
                    Style::default().fg(theme.red).add_modifier(Modifier::BOLD),
                ));
                for line in textwrap::wrap(&block.text, width.saturating_sub(3)) {
                    lines.push(Line::styled(
                        format!("│ {line}"),
                        Style::default().fg(theme.red),
                    ));
                }
            }
            TranscriptKind::System => {
                for line in block.text.lines() {
                    lines.push(Line::styled(
                        line.to_string(),
                        Style::default().fg(theme.muted),
                    ));
                }
            }
        }
        lines.push(Line::default());
    }
    // Truncate any line that overshoots `width` instead of wrapping. Wrap
    // would push a continuation onto the next row without the gutter/marker
    // that the diff and card layouts rely on, leaving what look like blank
    // gap rows. A trailing `…` says content was elided.
    let mut wrapped: Vec<Line> = Vec::new();
    let mut source_to_wrapped: Vec<usize> = Vec::with_capacity(lines.len());
    for line in lines {
        source_to_wrapped.push(wrapped.len());
        if line.width() <= width {
            wrapped.push(line);
            continue;
        }
        // Rebuild the line span-by-span, cutting when we hit `width - 1` so
        // the trailing `…` fits. Style is preserved per-span.
        let mut budget = width.saturating_sub(1);
        let mut out_spans: Vec<Span<'static>> = Vec::new();
        for span in line.spans.into_iter() {
            let span_w = span.content.chars().count();
            if span_w <= budget {
                budget -= span_w;
                out_spans.push(span);
                continue;
            }
            // Partial span: take `budget` chars, then stop.
            let taken: String = span.content.chars().take(budget).collect();
            out_spans.push(Span::styled(taken, span.style));
            break;
        }
        out_spans.push(Span::styled(
            "…".to_string(),
            ratatui::style::Style::default().fg(theme.muted),
        ));
        wrapped.push(Line::from(out_spans).style(line.style));
    }
    let content_height = wrapped.len().min(u16::MAX as usize) as u16;
    let max_scroll = content_height.saturating_sub(area.height);
    app.max_scroll = max_scroll;
    if app.auto_scroll {
        app.scroll = max_scroll;
    } else {
        app.scroll = app.scroll.min(max_scroll);
    }
    // Snapshot visible rows with the terminal row they occupy so a mouse
    // drag selection can extract exactly what the user saw.
    app.wrapped_snapshot.clear();
    let scroll = app.scroll as usize;
    for (idx, line) in wrapped
        .iter()
        .enumerate()
        .skip(scroll)
        .take(area.height as usize)
    {
        let screen_y = area.y + (idx - scroll) as u16;
        app.wrapped_snapshot.push((screen_y, line.to_string()));
    }
    // Convert tool header markers to on-screen rects (scroll-adjusted).
    for (id, marker) in std::mem::take(&mut app.tool_header_markers) {
        let wrapped_idx = source_to_wrapped.get(marker).copied().unwrap_or(marker);
        let screen_y = wrapped_idx as i32 - app.scroll as i32;
        if screen_y < 0 || screen_y >= area.height as i32 {
            continue;
        }
        let y = area.y + screen_y as u16;
        app.tool_header_rects
            .push((Rect::new(area.x, y, width as u16, 1), id));
    }
    // Convert file link markers to on-screen rects so a click can open the
    // file with the OS default app.
    for (marker, path) in std::mem::take(&mut app.file_link_markers) {
        let wrapped_idx = source_to_wrapped.get(marker).copied().unwrap_or(marker);
        let screen_y = wrapped_idx as i32 - app.scroll as i32;
        if screen_y < 0 || screen_y >= area.height as i32 {
            continue;
        }
        let y = area.y + screen_y as u16;
        app.file_link_rects
            .push((Rect::new(area.x, y, width as u16, 1), path));
    }
    // Apply active selection highlight before rendering.
    if let Some(sel) = app.selection {
        let (start, end) = normalize_selection(sel);
        for (idx, line) in wrapped.iter_mut().enumerate() {
            let row = area.y + (idx as u16).saturating_sub(app.scroll);
            if idx < scroll || row < start.0 || row > end.0 {
                continue;
            }
            let bg = app.theme.active_tab;
            highlight_line(line, row, start, end, bg);
        }
    }
    frame.render_widget(
        Paragraph::new(wrapped).scroll((app.scroll, 0)),
        Rect::new(area.x, area.y, width as u16, area.height),
    );
}

/// Normalize a selection so `start` is top-left and `end` is bottom-right.
fn normalize_selection(sel: crate::app::TextSelection) -> ((u16, u16), (u16, u16)) {
    let (a, b) = (sel.anchor, sel.head);
    if (a.0, a.1) <= (b.0, b.1) {
        (a, b)
    } else {
        (b, a)
    }
}

/// Paint the portion of `line` that falls inside `[start..=end]` with `bg`.
/// `row` is the terminal row the line renders on.
fn highlight_line(
    line: &mut Line<'static>,
    row: u16,
    start: (u16, u16),
    end: (u16, u16),
    bg: ratatui::style::Color,
) {
    let mut col: u16 = 0;
    for span in line.spans.iter_mut() {
        let span_start = col;
        let span_end = col + span.content.chars().count() as u16;
        col = span_end;
        let sel_start_col = if row == start.0 { start.1 } else { 0 };
        let sel_end_col = if row == end.0 { end.1 } else { u16::MAX };
        if span_end < sel_start_col || span_start > sel_end_col {
            continue;
        }
        // Partial highlight would need to split the span; for simplicity we
        // highlight the whole span when any part of it falls in range. Good
        // enough for whole-word / whole-line selection which is the common
        // case.
        span.style = span.style.bg(bg);
    }
}

/// Card style: a one-row header with a solid background, then a body block
/// with a subtle background that extends to the right edge so the whole card
/// reads as one continuous surface. All colors come from the active theme so
/// switching themes restyles every card at once.
fn card(
    lines: &mut Vec<Line<'static>>,
    title: &str,
    text: &str,
    width: usize,
    accent: ratatui::style::Color,
    theme: &Theme,
) {
    let inner_w = width.saturating_sub(2).max(1);
    let heading = trim(title, inner_w);
    let heading_len = heading.width();
    let header_pad = inner_w.saturating_sub(heading_len);
    // Header: bold title on the accent color, padded to full width.
    lines.push(Line::from(vec![Span::styled(
        format!("  {heading}{}", " ".repeat(header_pad)),
        Style::default()
            .fg(theme.canvas)
            .bg(accent)
            .add_modifier(Modifier::BOLD),
    )]));
    // Body: markdown-rendered lines painted onto the subtle background with
    // a two-column left padding, right-padded to the same width.
    let body_w = width.saturating_sub(4).max(1);
    let mut inner: Vec<Line<'static>> = Vec::new();
    render_markdown(text, body_w, &mut inner, theme);
    if inner.is_empty() {
        inner.push(Line::default());
    }
    for row in inner {
        let row_w = row.width();
        let pad = body_w.saturating_sub(row_w);
        let mut spans: Vec<Span<'static>> = Vec::new();
        spans.push(Span::styled("  ", Style::default().bg(theme.subtle)));
        for span in row.spans {
            // Repaint each span's background so gaps between spans still
            // show the card tint.
            let style = span.style.bg(theme.subtle);
            spans.push(Span::styled(span.content.into_owned(), style));
        }
        spans.push(Span::styled(
            format!("{}  ", " ".repeat(pad)),
            Style::default().bg(theme.subtle),
        ));
        lines.push(Line::from(spans));
    }
}

pub(super) fn render_markdown(
    text: &str,
    width: usize,
    lines: &mut Vec<Line<'static>>,
    theme: &Theme,
) {
    let mut in_code = false;
    let mut ordered_counter: Option<usize> = None;
    let raw_lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    while i < raw_lines.len() {
        let raw = raw_lines[i];
        // Fenced code block.
        if let Some(rest) = raw.trim_start().strip_prefix("```") {
            in_code = !in_code;
            let bar = if in_code {
                let lang = rest.trim();
                if lang.is_empty() {
                    "┌ code".to_string()
                } else {
                    format!("┌ {lang}")
                }
            } else {
                "└".into()
            };
            let pad = width.saturating_sub(bar.chars().count());
            lines.push(Line::from(vec![
                Span::styled(bar, Style::default().fg(theme.muted).bg(theme.subtle)),
                Span::styled(" ".repeat(pad), Style::default().bg(theme.subtle)),
            ]));
            i += 1;
            continue;
        }
        if in_code {
            let body = if raw.chars().count() > width.saturating_sub(2) {
                let mut cut: String = raw.chars().take(width.saturating_sub(3)).collect();
                cut.push('…');
                cut
            } else {
                raw.to_string()
            };
            let used = body.chars().count() + 2;
            let pad = width.saturating_sub(used);
            lines.push(Line::from(vec![
                Span::styled("│ ", Style::default().fg(theme.muted).bg(theme.subtle)),
                Span::styled(body, Style::default().fg(theme.text).bg(theme.subtle)),
                Span::styled(" ".repeat(pad), Style::default().bg(theme.subtle)),
            ]));
            i += 1;
            continue;
        }

        // Table: header row + separator + data rows. Detected when the next
        // line looks like `| --- | --- |`. We consume all consecutive `|` rows.
        if is_table_row(raw) && i + 1 < raw_lines.len() && is_table_separator(raw_lines[i + 1]) {
            let mut rows: Vec<Vec<String>> = Vec::new();
            rows.push(split_table_row(raw));
            i += 2; // skip header + separator
            while i < raw_lines.len() && is_table_row(raw_lines[i]) {
                rows.push(split_table_row(raw_lines[i]));
                i += 1;
            }
            render_table(&rows, width, lines, theme);
            continue;
        }

        if raw.trim().is_empty() {
            ordered_counter = None;
            lines.push(Line::default());
            i += 1;
            continue;
        }

        if let Some(body) = raw.strip_prefix("##### ") {
            emit_wrapped(
                lines,
                &markdown_spans(body, theme),
                "",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
                width,
                theme,
            );
            i += 1;
            continue;
        }
        if let Some(body) = raw.strip_prefix("#### ") {
            emit_wrapped(
                lines,
                &markdown_spans(body, theme),
                "",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
                width,
                theme,
            );
            i += 1;
            continue;
        }
        if let Some(body) = raw.strip_prefix("### ") {
            emit_wrapped(
                lines,
                &markdown_spans(body, theme),
                "",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
                width,
                theme,
            );
            i += 1;
            continue;
        }
        if let Some(body) = raw.strip_prefix("## ") {
            emit_wrapped(
                lines,
                &markdown_spans(body, theme),
                "",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
                width,
                theme,
            );
            i += 1;
            continue;
        }
        if let Some(body) = raw.strip_prefix("# ") {
            emit_wrapped(
                lines,
                &markdown_spans(body, theme),
                "",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
                width,
                theme,
            );
            i += 1;
            continue;
        }

        if let Some(body) = raw.strip_prefix("> ") {
            emit_wrapped(
                lines,
                &markdown_spans(body, theme),
                "│ ",
                Style::default().fg(theme.faint),
                width,
                theme,
            );
            i += 1;
            continue;
        }

        if matches!(raw.trim(), "---" | "***" | "___") {
            lines.push(Line::styled(
                "─".repeat(width.max(1)),
                Style::default().fg(theme.muted),
            ));
            i += 1;
            continue;
        }

        let trimmed = raw.trim_start();
        let indent = raw.len() - trimmed.len();
        if let Some(item) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            let prefix = format!("{}• ", " ".repeat(indent));
            emit_wrapped(
                lines,
                &markdown_spans(item, theme),
                &prefix,
                Style::default().fg(theme.text),
                width,
                theme,
            );
            i += 1;
            continue;
        }

        if let Some((num, item)) = split_ordered(trimmed) {
            let display = ordered_counter.map_or(num, |n| n + 1);
            ordered_counter = Some(display);
            let prefix = format!("{}{}. ", " ".repeat(indent), display);
            emit_wrapped(
                lines,
                &markdown_spans(item, theme),
                &prefix,
                Style::default().fg(theme.text),
                width,
                theme,
            );
            i += 1;
            continue;
        } else {
            ordered_counter = None;
        }

        // Paragraph.
        emit_wrapped(
            lines,
            &markdown_spans(raw, theme),
            "",
            Style::default().fg(theme.text),
            width,
            theme,
        );
        i += 1;
    }
}

fn is_table_row(line: &str) -> bool {
    let t = line.trim();
    t.starts_with('|') && t.ends_with('|') && t.matches('|').count() >= 2
}

fn is_table_separator(line: &str) -> bool {
    let t = line.trim();
    if !is_table_row(t) {
        return false;
    }
    // Every non-empty cell is dashes with optional colon (alignment marker).
    t.trim_matches('|').split('|').all(|cell| {
        let c = cell.trim();
        !c.is_empty() && c.chars().all(|ch| ch == '-' || ch == ':')
    })
}

fn split_table_row(line: &str) -> Vec<String> {
    line.trim()
        .trim_matches('|')
        .split('|')
        .map(|c| c.trim().to_string())
        .collect()
}

fn render_table(rows: &[Vec<String>], width: usize, lines: &mut Vec<Line<'static>>, theme: &Theme) {
    if rows.is_empty() {
        return;
    }
    let cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    if cols == 0 {
        return;
    }
    // Column widths: measure content, then rebalance so total fits `width - (cols+1)` (borders).
    let mut widths = vec![0usize; cols];
    for row in rows {
        for (c, cell) in row.iter().enumerate() {
            widths[c] = widths[c].max(cell.chars().count());
        }
    }
    let budget = width.saturating_sub(cols + 1).max(cols); // 1 pipe per col + trailing
    let total: usize = widths.iter().sum();
    if total > budget {
        let scale = budget as f32 / total as f32;
        for w in widths.iter_mut() {
            *w = ((*w as f32) * scale).floor().max(3.0) as usize;
        }
    }
    let border_style = Style::default().fg(theme.muted).bg(theme.subtle);
    let cell_style = Style::default().fg(theme.text).bg(theme.subtle);
    let header_style = Style::default()
        .fg(theme.accent)
        .bg(theme.subtle)
        .add_modifier(Modifier::BOLD);

    // Top border
    let top: String = std::iter::once('┌')
        .chain(widths.iter().enumerate().flat_map(|(i, w)| {
            let seg: Vec<char> = std::iter::repeat_n('─', *w + 2).collect();
            let sep = if i + 1 == cols { '┐' } else { '┬' };
            seg.into_iter().chain(std::iter::once(sep))
        }))
        .collect();
    lines.push(Line::styled(top, border_style));

    for (r, row) in rows.iter().enumerate() {
        let mut spans: Vec<Span<'static>> = vec![Span::styled("│", border_style)];
        for (c, w) in widths.iter().enumerate() {
            let cell = row.get(c).map(String::as_str).unwrap_or("");
            // Parse the cell's inline markdown so **bold** and `code` inside
            // a table cell render like they do in paragraphs. Fall back to
            // plain text when the cell is empty. Layout is char-count based
            // for width; styling is span-based.
            let parsed = markdown_spans(cell, theme);
            let plain_text: String = parsed.iter().map(|(s, _)| s.as_str()).collect();
            let plain_w = plain_text.chars().count();
            // Clip to the column width, replacing overflow with `…`. Since we
            // clip on chars we may cut mid-style; acceptable for tables.
            let (rendered, pad) = if plain_w > *w {
                let mut cut: String = plain_text.chars().take(w.saturating_sub(1)).collect();
                cut.push('…');
                (
                    vec![(cut, if r == 0 { header_style } else { cell_style })],
                    0,
                )
            } else {
                // Adopt the header style when we're on the first row so bold
                // titles read like a header, otherwise keep the span colors.
                let base_style = if r == 0 { header_style } else { cell_style };
                let styled: Vec<(String, Style)> = parsed
                    .into_iter()
                    .map(|(s, style)| {
                        // For rows 2+, keep inline markdown coloring on the
                        // subtle background; for row 0, force the header
                        // color so accent stays consistent.
                        let merged = if r == 0 {
                            base_style
                        } else {
                            style.bg(theme.subtle)
                        };
                        (s, merged)
                    })
                    .collect();
                (styled, w.saturating_sub(plain_w))
            };
            spans.push(Span::styled(" ", cell_style));
            for (text, style) in rendered {
                spans.push(Span::styled(text, style));
            }
            spans.push(Span::styled(format!("{} ", " ".repeat(pad)), cell_style));
            spans.push(Span::styled("│", border_style));
        }
        lines.push(Line::from(spans));

        // Separator under header
        if r == 0 {
            let sep: String = std::iter::once('├')
                .chain(widths.iter().enumerate().flat_map(|(i, w)| {
                    let seg: Vec<char> = std::iter::repeat_n('─', *w + 2).collect();
                    let s = if i + 1 == cols { '┤' } else { '┼' };
                    seg.into_iter().chain(std::iter::once(s))
                }))
                .collect();
            lines.push(Line::styled(sep, border_style));
        }
    }

    let bot: String = std::iter::once('└')
        .chain(widths.iter().enumerate().flat_map(|(i, w)| {
            let seg: Vec<char> = std::iter::repeat_n('─', *w + 2).collect();
            let sep = if i + 1 == cols { '┘' } else { '┴' };
            seg.into_iter().chain(std::iter::once(sep))
        }))
        .collect();
    lines.push(Line::styled(bot, border_style));
}

/// Parse inline markdown into styled spans: **bold**, *italic*, `code`,
/// [text](url). Everything else is plain text under the caller's base style.
fn markdown_spans(text: &str, theme: &Theme) -> Vec<(String, Style)> {
    let mut out: Vec<(String, Style)> = Vec::new();
    let plain = Style::default().fg(theme.text);
    let mut buf = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let flush = |buf: &mut String, out: &mut Vec<(String, Style)>| {
        if !buf.is_empty() {
            out.push((std::mem::take(buf), plain));
        }
    };
    while i < chars.len() {
        let c = chars[i];
        // Inline code: `…`
        if c == '`' {
            if let Some(end) = chars[i + 1..].iter().position(|&c| c == '`') {
                flush(&mut buf, &mut out);
                let body: String = chars[i + 1..i + 1 + end].iter().collect();
                out.push((body, Style::default().fg(theme.accent2).bg(theme.subtle)));
                i += end + 2;
                continue;
            }
        }
        // Bold: **…**
        if c == '*' && chars.get(i + 1) == Some(&'*') {
            if let Some(end) = find_pair(&chars, i + 2, "**") {
                flush(&mut buf, &mut out);
                let body: String = chars[i + 2..end].iter().collect();
                out.push((
                    body,
                    Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
                ));
                i = end + 2;
                continue;
            }
        }
        // Italic: *…* (single asterisk, avoid ** that we already handled)
        if c == '*' {
            if let Some(end) = find_pair(&chars, i + 1, "*") {
                flush(&mut buf, &mut out);
                let body: String = chars[i + 1..end].iter().collect();
                out.push((
                    body,
                    Style::default()
                        .fg(theme.text)
                        .add_modifier(Modifier::ITALIC),
                ));
                i = end + 1;
                continue;
            }
        }
        // Link: [text](url) → underline the text, keep url out of sight
        if c == '[' {
            if let Some(close) = chars[i + 1..].iter().position(|&c| c == ']') {
                let after = i + 1 + close + 1;
                if chars.get(after) == Some(&'(') {
                    if let Some(url_end) = chars[after + 1..].iter().position(|&c| c == ')') {
                        flush(&mut buf, &mut out);
                        let label: String = chars[i + 1..i + 1 + close].iter().collect();
                        out.push((
                            label,
                            Style::default()
                                .fg(theme.accent)
                                .add_modifier(Modifier::UNDERLINED),
                        ));
                        i = after + 1 + url_end + 1;
                        continue;
                    }
                }
            }
        }
        buf.push(c);
        i += 1;
    }
    flush(&mut buf, &mut out);
    out
}

fn find_pair(chars: &[char], start: usize, delim: &str) -> Option<usize> {
    let bytes: Vec<char> = delim.chars().collect();
    let mut i = start;
    while i + bytes.len() <= chars.len() {
        if chars[i..i + bytes.len()] == bytes[..] {
            // Don't match empty spans.
            if i > start {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

fn split_ordered(text: &str) -> Option<(usize, &str)> {
    let digits: String = text.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    let rest = &text[digits.len()..];
    let body = rest.strip_prefix(". ")?;
    digits.parse().ok().map(|n| (n, body))
}

/// Emit a styled-span line, wrapping at ASCII whitespace so inline styles
/// (code, bold, links) survive the wrap without losing characters like `<`,
/// `>`, `=`, `"` that word-based wrappers treat as word boundaries.
fn emit_wrapped(
    lines: &mut Vec<Line<'static>>,
    spans: &[(String, Style)],
    prefix: &str,
    base: Style,
    width: usize,
    _theme: &Theme,
) {
    let usable = width.saturating_sub(prefix.chars().count()).max(1);

    if spans.iter().all(|(s, _)| s.is_empty()) {
        lines.push(Line::styled(prefix.to_string(), base));
        return;
    }

    // Flatten to (char, style) pairs so wrap decisions do not have to know
    // about span boundaries. Rebuild spans on emit by grouping consecutive
    // chars that share a style.
    let mut chars: Vec<(char, Style)> = Vec::new();
    for (text, style) in spans {
        for ch in text.chars() {
            chars.push((ch, *style));
        }
    }

    let mut row_start = 0usize;
    let mut first_row = true;
    let mut i = 0;
    while i < chars.len() {
        // Advance up to `usable` chars.
        let mut end = (row_start + usable).min(chars.len());
        // If we would cut in the middle of a word, back up to the last space
        // in the current window so wrap happens at whitespace.
        if end < chars.len() && chars[end].0 != ' ' {
            let mut back = end;
            while back > row_start && chars[back - 1].0 != ' ' {
                back -= 1;
            }
            if back > row_start {
                end = back;
            }
        }
        // Emit chars[row_start..end] as grouped spans, trim trailing space.
        let mut segment_end = end;
        while segment_end > row_start && chars[segment_end - 1].0 == ' ' {
            segment_end -= 1;
        }
        push_row(
            lines,
            &chars,
            row_start,
            segment_end,
            prefix,
            base,
            first_row,
        );
        first_row = false;
        // Skip the whitespace we broke on so the next row does not start
        // with a leading space.
        i = end;
        while i < chars.len() && chars[i].0 == ' ' {
            i += 1;
        }
        row_start = i;
    }
}

fn push_row(
    lines: &mut Vec<Line<'static>>,
    chars: &[(char, Style)],
    start: usize,
    end: usize,
    prefix: &str,
    base: Style,
    first_row: bool,
) {
    let indent = if first_row {
        prefix.to_string()
    } else {
        " ".repeat(prefix.chars().count())
    };
    let mut spans: Vec<Span<'static>> = Vec::new();
    if !indent.is_empty() {
        spans.push(Span::styled(indent, base));
    }
    if start >= end {
        lines.push(Line::from(spans));
        return;
    }
    let mut current = String::new();
    let mut current_style = chars[start].1;
    for (ch, style) in &chars[start..end] {
        if *style != current_style && !current.is_empty() {
            spans.push(Span::styled(std::mem::take(&mut current), current_style));
        }
        current_style = *style;
        current.push(*ch);
    }
    if !current.is_empty() {
        spans.push(Span::styled(current, current_style));
    }
    lines.push(Line::from(spans));
}

/// User messages sit in a bordered, subtle-tinted card so the eye can find
/// them while scrolling. Every body line is hard-clipped to the card's inner
/// width so a nested list or fenced code inside a paste can never draw past
/// the right border. Tall messages get truncated with a "▸ N more lines"
/// hint the caller can wire to expansion later.
const USER_CARD_MAX_LINES: usize = 12;

fn user_card(lines: &mut Vec<Line<'static>>, text: &str, width: usize, theme: &Theme) {
    if width < 6 {
        return;
    }
    let body_w = width.saturating_sub(4);
    let mut inner: Vec<Line<'static>> = Vec::new();
    render_markdown(text, body_w, &mut inner, theme);
    if inner.is_empty() {
        inner.push(Line::default());
    }
    // Force every rendered line to fit inside `body_w`. `render_markdown`
    // already wraps text, but styled spans, tables, and headings can still
    // exceed the target width (styled bg extends past the char count). Re-wrap
    // by flattening to text and re-styling to a single foreground colour.
    let mut clipped: Vec<(String, Style)> = Vec::new();
    for row in inner {
        let joined: String = row
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect::<String>();
        let style = row
            .spans
            .first()
            .map(|s| s.style)
            .unwrap_or_else(|| Style::default().fg(theme.text));
        if joined.is_empty() {
            clipped.push((String::new(), style));
            continue;
        }
        for wrapped in textwrap::wrap(&joined, body_w.max(1)) {
            clipped.push((wrapped.into_owned(), style));
        }
    }

    let overflow = clipped.len().saturating_sub(USER_CARD_MAX_LINES);
    let visible: Vec<(String, Style)> = if overflow > 0 {
        clipped.into_iter().take(USER_CARD_MAX_LINES).collect()
    } else {
        clipped
    };

    let border_style = Style::default().fg(theme.accent).bg(theme.subtle);
    let top = format!("╭{}╮", "─".repeat(width.saturating_sub(2)));
    let bottom = format!("╰{}╯", "─".repeat(width.saturating_sub(2)));
    lines.push(Line::styled(top, border_style));
    for (text, style) in visible {
        let text_w = unicode_width_of(&text);
        let pad = body_w.saturating_sub(text_w);
        let style = style.bg(theme.subtle);
        lines.push(Line::from(vec![
            Span::styled("│ ", border_style),
            Span::styled(text, style),
            Span::styled(format!("{} │", " ".repeat(pad)), border_style),
        ]));
    }
    if overflow > 0 {
        let msg = format!(
            "▸ {overflow} more line{s} …",
            s = if overflow == 1 { "" } else { "s" }
        );
        let msg_w = unicode_width_of(&msg);
        let pad = body_w.saturating_sub(msg_w);
        lines.push(Line::from(vec![
            Span::styled("│ ", border_style),
            Span::styled(msg, Style::default().fg(theme.muted).bg(theme.subtle)),
            Span::styled(format!("{} │", " ".repeat(pad)), border_style),
        ]));
    }
    lines.push(Line::styled(bottom, border_style));
}

fn unicode_width_of(text: &str) -> usize {
    use unicode_width::UnicodeWidthStr;
    text.width()
}

fn args_path(args: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(args).ok()?;
    value
        .get("path")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

pub(super) fn register_summary_file_link(
    name: &str,
    args: &str,
    _text: &str,
    line_index: usize,
    markers: &mut Vec<(usize, String)>,
) {
    if !matches!(name, "read" | "write" | "skill_read" | "fetch") {
        return;
    }
    if let Some(path) = args_path(args) {
        markers.push((line_index, path));
    }
}

pub(super) fn register_detail_file_link(
    name: &str,
    args: &str,
    line_index: usize,
    markers: &mut Vec<(usize, String)>,
) {
    if !matches!(name, "write" | "edit") {
        return;
    }
    if let Some(path) = args_path(args) {
        markers.push((line_index, path));
    }
}
