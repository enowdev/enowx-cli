//! Public shim for integration tests. Exposes the small slice of `App` state
//! the mouse-dispatch tests need without leaking every internal.

use crate::app::App;
use crate::modal::Modal;
use crate::session::TranscriptKind;

use crossterm::event::MouseEvent;
use enowx_core::Config;
use ratatui::layout::Rect;

pub struct TestApp {
    inner: App,
}

impl Default for TestApp {
    fn default() -> Self {
        Self::new()
    }
}

impl TestApp {
    pub fn new() -> Self {
        let tmp =
            std::env::temp_dir().join(format!("enx-test-{}-{}", std::process::id(), fastrand()));
        let home_dir = tmp.join("home");
        let enx_home = tmp.join("enx");
        let workspace = tmp.join("ws");
        for p in [&home_dir, &enx_home, &workspace] {
            let _ = std::fs::create_dir_all(p);
        }
        // Isolate from the developer's real ~/.enx and ~/.agents so discovery
        // only sees files the test seeded.
        std::env::set_var("HOME", &home_dir);
        std::env::set_var("ENX_HOME", &enx_home);
        std::env::set_current_dir(&workspace).ok();
        let mut config = Config::default();
        config.provider.name = "test".into();
        config.provider.base_url = "http://127.0.0.1:1".into();
        Self {
            inner: App::new(config),
        }
    }

    pub fn new_with_skills(names: &[&str]) -> Self {
        let mut app = Self::new();
        let ws = app.inner.config.workspace();
        for name in names {
            let dir = ws.join(".agents/skills").join(name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("SKILL.md"),
                format!(
                    "---\nname: {name}\ndescription: test skill {name}\nallowed-tools: read\n---\n\nbody-of-{name}\n"
                ),
            )
            .unwrap();
        }
        app.inner.adopt(app.inner.config.clone());
        app
    }

    pub fn mouse(&mut self, event: MouseEvent) -> anyhow::Result<()> {
        self.inner.mouse(event)
    }

    pub fn set_max_scroll(&mut self, value: u16) {
        self.inner.max_scroll = value;
    }
    pub fn scroll(&self) -> u16 {
        self.inner.scroll
    }
    pub fn modal_cursor(&self) -> usize {
        self.inner.modal_cursor
    }
    pub fn mcp_field(&self) -> usize {
        self.inner.mcp_field
    }
    pub fn is_modal_open(&self) -> bool {
        self.inner.modal != Modal::None
    }
    pub fn config_disabled_skills(&self) -> Vec<String> {
        self.inner.config.ui.disabled_skills.clone()
    }
    pub fn transcript_contains(&self, needle: &str) -> bool {
        self.inner
            .blocks
            .iter()
            .any(|b| matches!(b.kind, TranscriptKind::Notice) && b.text.contains(needle))
    }

    pub fn set_popup_rows(&mut self, rows: Vec<(Rect, Rect, usize)>) {
        self.inner.popup_rows = rows;
    }
    pub fn set_popup_body(&mut self, rect: Rect) {
        self.inner.popup_body = Some(rect);
    }
    pub fn set_mcp_field_rows(&mut self, rows: Vec<(Rect, usize)>) {
        self.inner.mcp_field_rows = rows;
    }

    pub fn enter_skills_modal(&mut self) {
        self.inner.open_skills();
    }
    pub fn enter_mcp_form(&mut self) {
        self.inner.open_mcp_form();
    }

    /// Index of a discovered skill by name so tests can seed the popup row
    /// for the exact entry they want to click.
    pub fn skill_index(&self, name: &str) -> Option<usize> {
        self.inner
            .discovery
            .skills
            .iter()
            .position(|s| s.name == name)
    }
}

fn fastrand() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0)
}
