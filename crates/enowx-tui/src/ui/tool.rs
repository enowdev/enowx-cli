//! Tool block rendering: one-line summary for cheap tools, header + collapsible
//! body for the interesting ones (`bash`, `edit`, `write`, MCP calls). Kept
//! separate from `transcript.rs` so the summary rules stay easy to scan.

use super::*;
use serde_json::Value;

/// What a tool block looks like on screen right now.
pub(super) enum ToolRender<'a> {
    /// One line, no body. `read`, `glob`, `grep`, `todo`, `skill_read`, etc.
    Summary(String),
    /// Header line the user can toggle to reveal `body`. `subtitle` is a
    /// muted second line shown when expanded (e.g. the full `bash` command
    /// when the header only carries the program name).
    Detail {
        header: String,
        subtitle: Option<String>,
        body: ToolBody<'a>,
    },
}

pub(super) enum ToolBody<'a> {
    Plain(&'a str),
    /// Per-line unified diff derived from an `edit` tool's args.
    /// Per-line unified diff derived from an `edit` tool's args. `start_line`
    /// is where the region begins in the target file so the gutter can show
    /// real file line numbers instead of resetting to 1.
    Diff {
        path: String,
        old: String,
        new: String,
        start_line: usize,
    },
    /// File preview capped to a max number of lines; the header carries the
    /// total count so the reader knows there is more.
    Preview {
        content: String,
        total: usize,
    },
    /// Tree list under the header: each entry rendered with a `├─` / `└─`
    /// connector so multi-file `read`, `glob`, and `grep` share one gaya.
    Tree {
        items: Vec<String>,
    },
    /// Todo checklist rendered under the header. Done rows get
    /// strikethrough, in-progress bolded, blocked yellow.
    Todo {
        items: Vec<TodoItem>,
    },
}

pub(super) struct TodoItem {
    pub state: TodoState,
    pub label: String,
}

pub(super) enum TodoState {
    Pending,
    InProgress,
    Done,
    Blocked,
    Dropped,
}

fn parse_todo_state(s: &str) -> TodoState {
    match s.to_ascii_lowercase().as_str() {
        "in_progress" | "active" | "wip" => TodoState::InProgress,
        "done" | "completed" => TodoState::Done,
        "blocked" => TodoState::Blocked,
        "dropped" | "abandoned" | "cancelled" => TodoState::Dropped,
        _ => TodoState::Pending,
    }
}

/// Render a bullet-tree list of strings under the tool header. Uses `├─`
/// for interior rows and `└─` for the last so the eye can quickly scan the
/// items as one group.
pub(super) fn render_tree(
    items: &[String],
    width: usize,
    lines: &mut Vec<Line<'static>>,
    theme: &Theme,
    file_markers: &mut Vec<(usize, String)>,
) {
    let text_w = width.saturating_sub(5).max(1);
    for (i, item) in items.iter().enumerate() {
        let connector = if i + 1 == items.len() {
            "└─"
        } else {
            "├─"
        };
        let clipped = trim(item, text_w);
        // A grep hit shows as `path:line:col:content`; strip trailing parts
        // so the marker is just the file path. Glob rows are already paths.
        let path = clipped
            .split(':')
            .next()
            .unwrap_or(clipped.as_str())
            .to_string();
        let row_idx = lines.len();
        file_markers.push((row_idx, path));
        lines.push(Line::from(vec![
            Span::styled(format!("  {connector} "), Style::default().fg(theme.muted)),
            Span::styled(
                clipped,
                Style::default()
                    .fg(theme.text)
                    .add_modifier(Modifier::UNDERLINED),
            ),
        ]));
    }
}

/// Render a Todo checklist under the header. Icons:
/// - `☑` done (green strikethrough), `☐` pending (accent),
/// - `▶` in-progress (bold accent), `⊡` blocked (yellow),
/// - `⊠` dropped (muted strikethrough).
pub(super) fn render_todo(
    items: &[TodoItem],
    width: usize,
    lines: &mut Vec<Line<'static>>,
    theme: &Theme,
) {
    let text_w = width.saturating_sub(6).max(1);
    for (i, it) in items.iter().enumerate() {
        let connector = if i + 1 == items.len() {
            "└─"
        } else {
            "├─"
        };
        let (icon, style) = match it.state {
            TodoState::Done => (
                "☑",
                Style::default()
                    .fg(theme.green)
                    .add_modifier(Modifier::CROSSED_OUT),
            ),
            TodoState::Pending => ("☐", Style::default().fg(theme.accent)),
            TodoState::InProgress => (
                "▶",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            TodoState::Blocked => ("⊡", Style::default().fg(theme.yellow)),
            TodoState::Dropped => (
                "⊠",
                Style::default()
                    .fg(theme.muted)
                    .add_modifier(Modifier::CROSSED_OUT),
            ),
        };
        let clipped = trim(&it.label, text_w);
        lines.push(Line::from(vec![
            Span::styled(format!("  {connector} "), Style::default().fg(theme.muted)),
            Span::styled(format!("{icon} "), style),
            Span::styled(clipped, style),
        ]));
    }
}

/// Decide which representation a tool call gets. Args are the raw JSON from
/// the provider; result is the stringified tool output.
pub(super) fn classify<'a>(name: &str, args: &'a str, result: &'a str) -> ToolRender<'a> {
    let parsed: Value = serde_json::from_str(args).unwrap_or(Value::Null);
    match name {
        "bash" => {
            // Even `ls` in a large repo can spill hundreds of lines. Default
            // collapsed; click the header to expand.
            let cmd = str_arg(&parsed, "command").unwrap_or("").trim();
            let short = cmd.split_whitespace().next().unwrap_or(cmd);
            let n = result.lines().count();
            ToolRender::Detail {
                header: format!("$ {short} · {n} lines"),
                subtitle: Some(trim(cmd, 120)),
                body: ToolBody::Plain(result),
            }
        }
        "edit" => {
            let path = str_arg(&parsed, "path").unwrap_or("").to_string();
            let old = str_arg(&parsed, "old_text").unwrap_or("").to_string();
            let new = str_arg(&parsed, "new_text").unwrap_or("").to_string();
            // The `edit` tool ends its output with `at line N`; use that so
            // the diff gutter shows real file line numbers instead of `1..`.
            let start_line = parse_start_line(result).unwrap_or(1);
            ToolRender::Detail {
                header: format!("edit {path}"),
                subtitle: None,
                body: ToolBody::Diff {
                    path: path.clone(),
                    old,
                    new,
                    start_line,
                },
            }
        }
        "write" => {
            let path = str_arg(&parsed, "path").unwrap_or("").to_string();
            let content = str_arg(&parsed, "content").unwrap_or("");
            let n = content.lines().count();
            ToolRender::Detail {
                header: format!("write {path} · {n} lines"),
                subtitle: None,
                body: ToolBody::Preview {
                    content: content.to_string(),
                    total: n,
                },
            }
        }
        "read" => {
            // A single-file read is fine as a summary; a multi-file batch
            // (rare but possible via one call listing several paths) uses a
            // tree body instead.
            let path = str_arg(&parsed, "path").unwrap_or("").to_string();
            let paths: Vec<String> = parsed
                .get("paths")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            if paths.len() > 1 {
                ToolRender::Detail {
                    header: format!("Read ({})", paths.len()),
                    subtitle: None,
                    body: ToolBody::Tree { items: paths },
                }
            } else {
                let n = result.lines().count();
                ToolRender::Summary(format!("read {path} · {n} lines"))
            }
        }
        "glob" => {
            let pat = str_arg(&parsed, "pattern")
                .or_else(|| str_arg(&parsed, "path"))
                .unwrap_or("")
                .to_string();
            let matches: Vec<String> = result
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| l.trim().to_string())
                .collect();
            if matches.len() > 1 {
                ToolRender::Detail {
                    header: format!("Glob ({}) — {}", matches.len(), trim(&pat, 40)),
                    subtitle: None,
                    body: ToolBody::Tree { items: matches },
                }
            } else {
                ToolRender::Summary(format!("glob {pat} · {} matches", matches.len()))
            }
        }
        "grep" => {
            let pat = str_arg(&parsed, "pattern").unwrap_or("").to_string();
            let hits: Vec<String> = result
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| l.trim().to_string())
                .collect();
            if hits.len() > 1 {
                ToolRender::Detail {
                    header: format!("Grep ({}) — '{}'", hits.len(), trim(&pat, 40)),
                    subtitle: None,
                    body: ToolBody::Tree { items: hits },
                }
            } else {
                ToolRender::Summary(format!("grep '{}' · {} hits", trim(&pat, 40), hits.len()))
            }
        }
        "todo" => {
            // Parse the items array into TodoItems. enowx-cli todo tool ships each
            // entry as `{ state: "pending"|"in_progress"|"done"|"blocked"|
            // "dropped", label: "..." }`; some builds only send `label`.
            let items: Vec<TodoItem> = parsed
                .get("items")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .map(|v| {
                            let label = v
                                .get("label")
                                .or_else(|| v.get("task"))
                                .and_then(Value::as_str)
                                .or_else(|| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            let state = v
                                .get("state")
                                .and_then(Value::as_str)
                                .map(parse_todo_state)
                                .unwrap_or(TodoState::Pending);
                            TodoItem { state, label }
                        })
                        .collect()
                })
                .unwrap_or_default();
            if items.is_empty() {
                ToolRender::Summary("todo · 0 items".to_string())
            } else {
                ToolRender::Detail {
                    header: format!("Todo {} tasks", items.len()),
                    subtitle: None,
                    body: ToolBody::Todo { items },
                }
            }
        }
        "skill_read" => {
            let name = str_arg(&parsed, "name").unwrap_or("").to_string();
            ToolRender::Summary(format!("skill_read {name}"))
        }
        "fetch" => {
            let url = str_arg(&parsed, "url").unwrap_or("").to_string();
            ToolRender::Summary(format!("fetch {} · {} chars", trim(&url, 60), result.len()))
        }
        other if other.starts_with("mcp__") => {
            let label = other.trim_start_matches("mcp__").replacen("__", ":", 1);
            let n = result.lines().count();
            ToolRender::Detail {
                header: format!("mcp {label} · {n} lines"),
                subtitle: None,
                body: ToolBody::Plain(result),
            }
        }
        _ => {
            let n = result.lines().count();
            ToolRender::Detail {
                header: format!("{name} · {n} lines"),
                subtitle: None,
                body: ToolBody::Plain(result),
            }
        }
    }
}

fn str_arg<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

/// Pull the trailing `at line N` from an `edit` tool result. The tool emits
/// `Updated <path> at line N`; anything else falls through to `None` so the
/// caller keeps its default gutter start.
fn parse_start_line(result: &str) -> Option<usize> {
    let idx = result.rfind("at line ")?;
    let tail = &result[idx + "at line ".len()..];
    let digits: String = tail.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

/// Max lines a `write` preview shows before truncating. Anything past this
/// gets `+N more lines` at the bottom; click the header to see full output.
pub(super) const WRITE_PREVIEW_MAX: usize = 12;

/// Compact write preview matching the diff style: single-line header, then
/// rows shaped ` NNN│content`. Overlong rows truncate with `…`; content past
/// `WRITE_PREVIEW_MAX` is folded into a `↳ N more lines` hint.
pub(super) fn render_preview(
    content: &str,
    total: usize,
    width: usize,
    lines: &mut Vec<Line<'static>>,
    theme: &Theme,
) {
    let num_w = total.to_string().len().max(2);
    // Layout: `  NNN│content` — 2 space + num_w + 1 `│`
    let text_w = width.saturating_sub(num_w + 3).max(1);

    let bg = theme.subtle;
    let border = Style::default().fg(theme.accent).bg(bg);
    let dim_bg = Style::default().fg(theme.muted).bg(bg);
    let text_bg = Style::default().fg(theme.text).bg(bg);
    let show: Vec<&str> = content.lines().take(WRITE_PREVIEW_MAX).collect();
    for (idx, raw) in show.iter().enumerate() {
        let line_no = idx + 1;
        let (indent_viz, rest) = visualize_indent(raw);
        let indent_w = indent_viz.chars().count();
        let body_budget = text_w.saturating_sub(indent_w);
        let (body_shown, _) = truncate(&rest, body_budget);
        let used = 2 + num_w + 1 + indent_w + body_shown.chars().count();
        let pad = width.saturating_sub(used);
        lines.push(Line::from(vec![
            Span::styled("│ ", border),
            Span::styled(format!("{line_no:>num_w$}"), dim_bg),
            Span::styled("│", dim_bg),
            Span::styled(indent_viz, dim_bg),
            Span::styled(body_shown, text_bg),
            Span::styled(" ".repeat(pad), Style::default().bg(bg)),
        ]));
    }
    if total > WRITE_PREVIEW_MAX {
        let extra = total - WRITE_PREVIEW_MAX;
        let msg = format!("↳ {extra} more lines");
        let pad = width.saturating_sub(2 + msg.chars().count());
        lines.push(Line::from(vec![
            Span::styled("│ ", border),
            Span::styled(msg, dim_bg),
            Span::styled(" ".repeat(pad), Style::default().bg(bg)),
        ]));
    }
}

/// Rich per-line diff: header with `✎ Edit: 📄 <path> [+N/-M]`, colored
/// line-number gutter, marker column, and inline whitespace visualization.
/// Everything sits on `theme.subtle` and reaches the right edge so the diff
/// reads as one continuous block.
/// Compact diff renderer: header, then rows shaped
/// `+NNN|content`. Prefix / number / separator are dim so the marker and
/// content stand out. Overlong rows are truncated with `…`; if the diff has
/// more than `DIFF_PREVIEW_MAX` rows, the tail is folded into a hint.
pub(super) const DIFF_PREVIEW_MAX: usize = 15;

#[allow(clippy::too_many_arguments)]
pub(super) fn render_diff(
    path: &str,
    old: &str,
    new: &str,
    start_line: usize,
    width: usize,
    lines: &mut Vec<Line<'static>>,
    theme: &Theme,
    file_markers: &mut Vec<(usize, String)>,
) {
    let old_lines: Vec<&str> = old.split('\n').collect();
    let new_lines: Vec<&str> = new.split('\n').collect();
    let ops = lcs_diff(&old_lines, &new_lines);
    let adds = ops.iter().filter(|op| matches!(op, DiffOp::Add(_))).count();
    let dels = ops.iter().filter(|op| matches!(op, DiffOp::Del(_))).count();

    let text = Style::default().fg(theme.text);
    let dim = Style::default().fg(theme.muted);
    let add_c = Style::default().fg(theme.green);
    let del_c = Style::default().fg(theme.red);

    // Header: single line, no frame. Register the path so a click on this
    // header row opens the file with the OS default app.
    let header_idx = lines.len();
    file_markers.push((header_idx, path.to_string()));
    let bg = theme.subtle;
    let border = Style::default().fg(theme.accent).bg(bg);
    let header_left_w = 2
        + "✎ Edit: 📄 ".chars().count()
        + path.chars().count()
        + " · +/-".chars().count()
        + adds.to_string().len()
        + dels.to_string().len();
    let header_pad = width.saturating_sub(header_left_w + 1);
    lines.push(Line::from(vec![
        Span::styled("│ ", border),
        Span::styled(
            "✎ ",
            Style::default()
                .fg(theme.accent)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            path.to_string(),
            Style::default()
                .fg(theme.text)
                .bg(bg)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        ),
        Span::styled(" · ", Style::default().fg(theme.muted).bg(bg)),
        Span::styled(
            format!("+{adds}"),
            Style::default()
                .fg(theme.green)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("/", Style::default().fg(theme.muted).bg(bg)),
        Span::styled(
            format!("-{dels}"),
            Style::default()
                .fg(theme.red)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ".repeat(header_pad), Style::default().bg(bg)),
    ]));

    let total_target = start_line + old_lines.len().max(new_lines.len());
    let num_w = total_target.to_string().len().max(2);
    // Layout: `  +NNN│content` — 2 space + 1 marker + num_w + 1 `│`
    let text_w = width.saturating_sub(num_w + 4).max(1);

    let mut old_no = start_line;
    let mut new_no = start_line;
    let mut rendered = 0usize;
    let visible_ops: Vec<&DiffOp> = ops.iter().collect();
    let cut = visible_ops.len().saturating_sub(DIFF_PREVIEW_MAX);
    let show = &visible_ops[..(DIFF_PREVIEW_MAX).min(visible_ops.len())];
    for op in show {
        let (marker, marker_st, content_st, n_str, content) = match op {
            DiffOp::Del(t) => {
                let n = old_no.to_string();
                old_no += 1;
                ("-", del_c.add_modifier(Modifier::BOLD), del_c, n, t.clone())
            }
            DiffOp::Add(t) => {
                let n = new_no.to_string();
                new_no += 1;
                ("+", add_c.add_modifier(Modifier::BOLD), add_c, n, t.clone())
            }
            DiffOp::Keep(t) => {
                let n = new_no.to_string();
                old_no += 1;
                new_no += 1;
                (" ", dim, text, n, t.clone())
            }
        };
        let (indent_viz, rest) = visualize_indent(&content);
        let indent_w = indent_viz.chars().count();
        let body_budget = text_w.saturating_sub(indent_w);
        let (body_shown, _) = truncate(&rest, body_budget);
        // Marker/content styles are the plain fg from the match; wrap them
        // in the subtle bg so the diff row reads as one continuous strip.
        let marker_style = marker_st.bg(bg);
        let num_style = dim.bg(bg);
        let indent_style = dim.bg(bg);
        let body_style = content_st.bg(bg);
        let used = 2 + 1 + num_w + 1 + indent_viz.chars().count() + body_shown.chars().count();
        let pad = width.saturating_sub(used);
        lines.push(Line::from(vec![
            Span::styled("│ ", border),
            Span::styled(marker.to_string(), marker_style),
            Span::styled(format!("{n_str:>num_w$}"), num_style),
            Span::styled("│", num_style),
            Span::styled(indent_viz, indent_style),
            Span::styled(body_shown, body_style),
            Span::styled(" ".repeat(pad), Style::default().bg(bg)),
        ]));
        rendered += 1;
    }
    let _ = rendered;
    if cut > 0 {
        let msg = format!("↳ {cut} more lines");
        let pad = width.saturating_sub(2 + msg.chars().count());
        lines.push(Line::from(vec![
            Span::styled("│ ", border),
            Span::styled(msg, dim.bg(bg)),
            Span::styled(" ".repeat(pad), Style::default().bg(bg)),
        ]));
    }
}

/// Render indent as visible glyphs (space → `·`, tab → ` → `) and return the
/// visualized indent plus the remaining text after the leading whitespace.
fn visualize_indent(s: &str) -> (String, String) {
    let mut indent = String::new();
    let mut rest = String::new();
    let mut in_leading = true;
    for c in s.chars() {
        if in_leading && c == ' ' {
            indent.push('·');
        } else if in_leading && c == '\t' {
            indent.push_str(" → ");
        } else {
            in_leading = false;
            rest.push(c);
        }
    }
    (indent, rest)
}

/// Truncate a string with `…` when it exceeds `budget`. Returns
/// `(shown, was_truncated)`.
fn truncate(text: &str, budget: usize) -> (String, bool) {
    if text.chars().count() <= budget {
        return (text.to_string(), false);
    }
    if budget == 0 {
        return (String::new(), true);
    }
    let mut s: String = text.chars().take(budget.saturating_sub(1).max(1)).collect();
    s.push('…');
    (s, true)
}

enum DiffOp {
    Keep(String),
    Del(String),
    Add(String),
}

fn lcs_diff(a: &[&str], b: &[&str]) -> Vec<DiffOp> {
    let n = a.len();
    let m = b.len();
    let mut dp = vec![vec![0u32; m + 1]; n + 1];
    for i in 0..n {
        for j in 0..m {
            dp[i + 1][j + 1] = if a[i] == b[j] {
                dp[i][j] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    let mut ops = Vec::new();
    let mut i = n;
    let mut j = m;
    while i > 0 && j > 0 {
        if a[i - 1] == b[j - 1] {
            ops.push(DiffOp::Keep(a[i - 1].to_string()));
            i -= 1;
            j -= 1;
        } else if dp[i - 1][j] >= dp[i][j - 1] {
            ops.push(DiffOp::Del(a[i - 1].to_string()));
            i -= 1;
        } else {
            ops.push(DiffOp::Add(b[j - 1].to_string()));
            j -= 1;
        }
    }
    while i > 0 {
        ops.push(DiffOp::Del(a[i - 1].to_string()));
        i -= 1;
    }
    while j > 0 {
        ops.push(DiffOp::Add(b[j - 1].to_string()));
        j -= 1;
    }
    ops.reverse();
    ops
}
