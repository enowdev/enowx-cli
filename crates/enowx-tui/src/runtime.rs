use crate::{
    app::App,
    modal::{Modal, SETTINGS_FIELDS},
    session::TranscriptKind,
    ui::draw,
};
use anyhow::Result;
use crossterm::{
    cursor::Show,
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event as TerminalEvent, KeyEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use enowx_core::Config;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{io, time::Duration};

struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        let guard = Self;
        execute!(
            io::stdout(),
            EnterAlternateScreen,
            EnableBracketedPaste,
            EnableMouseCapture
        )?;
        // Ask the terminal for a blinking bar caret; the drop path restores
        // the terminal's default shape so the outer shell prompt is unchanged.
        use std::io::Write as _;
        let _ = write!(io::stdout(), "\x1b[5 q");
        let _ = io::stdout().flush();
        Ok(guard)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        use std::io::Write as _;
        let _ = write!(io::stdout(), "\x1b[0 q");
        let _ = io::stdout().flush();
        let _ = execute!(
            io::stdout(),
            DisableBracketedPaste,
            DisableMouseCapture,
            LeaveAlternateScreen,
            Show
        );
        let _ = disable_raw_mode();
    }
}
pub async fn run(config: Config, session: Option<String>) -> Result<()> {
    use std::io::IsTerminal;
    anyhow::ensure!(
        io::stdin().is_terminal() && io::stdout().is_terminal(),
        "enowx-cli requires an interactive terminal"
    );
    let mut app = App::new(config);
    // Refresh the models.dev catalog in the background. Fire-and-forget so a
    // slow or offline network never delays the TUI opening; next run picks
    // up whatever this fetch wrote to `~/.enx/models.json`.
    tokio::spawn(async {
        let _ = enowx_core::catalog::Catalog::refresh().await;
    });
    // Every fresh terminal starts a new conversation; `/resume` opens the
    // picker for users who want to continue a saved one. `enx dev` still
    // reuses a session across rebuilds by passing `--session`.
    if let Some(id) = session {
        if let Err(error) = app.resume(&id) {
            app.push(TranscriptKind::Error, format!("{error:#}"));
        }
    }
    let _guard = TerminalGuard::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    terminal.clear()?;
    while !app.should_quit {
        app.drain_events();
        app.drain_model_events();
        terminal.draw(|frame| draw(frame, &mut app))?;
        if event::poll(Duration::from_millis(40))? {
            match event::read()? {
                TerminalEvent::Key(key) if key.kind != KeyEventKind::Release => {
                    if let Err(error) = app.key(key) {
                        if app.modal != Modal::None {
                            app.modal_error = format!("{error:#}");
                        } else {
                            app.push(TranscriptKind::Error, format!("{error:#}"));
                        }
                    }
                }
                TerminalEvent::Paste(text)
                    if matches!(
                        app.modal,
                        Modal::Settings | Modal::ModelUrl | Modal::ProviderKey
                    ) =>
                {
                    let field = SETTINGS_FIELDS[app.modal_cursor];
                    let text: String = text.chars().filter(|c| !c.is_control()).collect();
                    app.settings_changed(field);
                    app.settings
                        .value_mut(field)
                        .insert_str(app.field_cursor, &text);
                    app.field_cursor += text.len();
                }
                TerminalEvent::Paste(text) if app.modal == Modal::None => {
                    // A drag-and-drop reaches us as one or more file paths.
                    let paths = crate::attachments::dropped_paths(&text);
                    if !paths.is_empty() {
                        app.attach_paths(&paths);
                    } else if text.trim().is_empty() {
                        // macOS Cmd+V on an image sends a bracketed paste with
                        // no text: terminals cannot stream binary through stdin.
                        app.attach_from_clipboard();
                    } else {
                        // Normalize CR/CRLF to LF, drop other control chars,
                        // and expand tabs so a paste from Warp/iTerm cannot
                        // slip an out-of-band cursor movement into the field.
                        let clean = crate::text::sanitize_paste(&text);
                        app.input.insert_str(app.cursor, &clean);
                        app.cursor += clean.len();
                    }
                }
                TerminalEvent::Mouse(mouse) => {
                    if let Err(error) = app.mouse(mouse) {
                        if app.modal != Modal::None {
                            app.modal_error = format!("{error:#}");
                        } else {
                            app.push(TranscriptKind::Error, format!("{error:#}"));
                        }
                    }
                }
                _ => {}
            }
        }
    }
    app.interrupt();
    // Drain while joining: a producer awaiting a full UI channel must be able
    // to publish its final events and persist the cancelled turn before exit.
    if let Some(mut task) = app.task.take() {
        loop {
            tokio::select! {
                result = &mut task => { result?; break; }
                _ = tokio::time::sleep(Duration::from_millis(10)) => app.drain_events(),
            }
        }
    }
    Ok(())
}
