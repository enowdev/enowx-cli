//! `/skills` popup: list, search-as-you-type, toggle enabled with Tab, and
//! read the selected skill's body on Enter.

use anyhow::Result;
use enowx_core::discovery::{SkillEntry, SkillScope};
use enowx_core::persist;

use crate::{modal::Modal, session::TranscriptKind};

use super::App;

/// One row rendered in the Skills popup after the search filter.
#[derive(Clone)]
pub(crate) struct SkillRow {
    pub name: String,
    pub description: String,
    pub scope: SkillScope,
    pub enabled: bool,
    pub path: std::path::PathBuf,
}

impl App {
    pub(crate) fn open_skills(&mut self) {
        self.modal = Modal::Skills;
        self.modal_cursor = 0;
        self.modal_search.clear();
        self.modal_error.clear();
    }

    /// Snapshot of what the popup should render right now, respecting the
    /// current search filter. Recomputed each frame so a toggle or add
    /// reflects without extra bookkeeping.
    pub(crate) fn skill_rows(&self) -> Vec<SkillRow> {
        let needle = self.modal_search.trim().to_ascii_lowercase();
        let disabled = &self.config.ui.disabled_skills;
        self.discovery
            .skills
            .iter()
            .filter(|s| {
                needle.is_empty()
                    || s.name.contains(&needle)
                    || s.description.to_ascii_lowercase().contains(&needle)
            })
            .map(|s: &SkillEntry| SkillRow {
                name: s.name.clone(),
                description: s.description.clone(),
                scope: s.scope,
                enabled: !disabled.iter().any(|d| d == &s.name),
                path: s.path.clone(),
            })
            .collect()
    }

    /// Tab: flip enabled state for the currently selected row.
    pub(crate) fn toggle_selected_skill(&mut self) -> Result<()> {
        let rows = self.skill_rows();
        let Some(row) = rows.get(self.modal_cursor) else {
            return Ok(());
        };
        let name = row.name.clone();
        let disabled = persist::toggle_skill(&mut self.config, &name)?;
        // Rebuild the agent so the registry drops or picks up `skill_read`
        // according to the new active set. Cheap: build_registry only walks
        // discovery vectors already in memory.
        let mut next = self.config.clone();
        next.ui.disabled_skills = disabled;
        self.adopt(next);
        self.status = format!("skill toggled: {name}");
        Ok(())
    }

    /// Enter: read the SKILL.md body into the transcript so the user can see
    /// what a skill will inject.
    pub(crate) fn read_selected_skill(&mut self) -> Result<()> {
        let rows = self.skill_rows();
        let Some(row) = rows.get(self.modal_cursor).cloned() else {
            return Ok(());
        };
        let body = std::fs::read_to_string(&row.path)?;
        self.push(
            TranscriptKind::Notice,
            format!("# {} ({})\n{}", row.name, row.path.display(), body),
        );
        self.modal = Modal::None;
        Ok(())
    }
}
