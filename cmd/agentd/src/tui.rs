pub mod app;
pub mod events;
pub mod render;
pub mod screens;

use crate::bootstrap::{App, BootstrapError};
use crossterm::event::{self, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io::{self, Stdout};
use std::time::Duration;

pub use app::{DialogState, TuiAppState, TuiScreen};

struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalGuard {
    fn new() -> Result<Self, BootstrapError> {
        enable_raw_mode().map_err(BootstrapError::Stream)?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen).map_err(BootstrapError::Stream)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend).map_err(BootstrapError::Stream)?;
        Ok(Self { terminal })
    }

    fn terminal(&mut self) -> &mut Terminal<CrosstermBackend<Stdout>> {
        &mut self.terminal
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

pub fn run(app: &App) -> Result<(), BootstrapError> {
    let mut state = app::TuiAppState::new(app.list_session_summaries()?, None);
    let mut terminal = TerminalGuard::new()?;

    loop {
        terminal
            .terminal()
            .draw(|frame| render::render(frame, &state))
            .map_err(BootstrapError::Stream)?;

        if state.should_exit() {
            return Ok(());
        }

        if !event::poll(Duration::from_millis(100)).map_err(BootstrapError::Stream)? {
            continue;
        }

        let Event::Key(key) = event::read().map_err(BootstrapError::Stream)? else {
            continue;
        };

        match state.active_screen() {
            TuiScreen::Sessions => screens::session::handle_key(app, &mut state, key)?,
            TuiScreen::Chat => screens::chat::handle_key(app, &mut state, key)?,
        }

        if key.code == KeyCode::Char('q') && key.modifiers.is_empty() {
            state.request_exit();
        }
    }
}
