use crate::{
    app::App,
    modal::{Modal, SettingsField, SETTINGS_FIELDS},
    session::TranscriptKind,
    text::{input_rows, trim},
    theme::Theme,
};
use ratatui::{
    layout::{Alignment, Constraint, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};
use unicode_width::UnicodeWidthStr;
mod chrome;
mod composer;
mod pickers;
mod popups;
mod settings;
mod sidebar;
mod tool;
mod transcript;
use chrome::*;
use composer::*;
use pickers::*;
use popups::draw_popup;
use sidebar::*;
use transcript::*;

pub(crate) fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(app.theme.canvas).fg(app.theme.text)),
        area,
    );
    if area.width < 20 || area.height < 8 {
        return;
    }
    draw_main(
        frame,
        app,
        area.inner(Margin {
            horizontal: if area.width >= 60 { 2 } else { 1 },
            vertical: 0,
        }),
    );
    if app.modal != Modal::None && !draw_popup(frame, app) {
        draw_modal(frame, app);
    }
}
