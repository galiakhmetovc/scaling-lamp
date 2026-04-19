use crate::bootstrap::SessionSummary;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuiScreen {
    Sessions,
    Chat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogState {
    CreateSession,
    ConfirmDelete { session_id: String },
}

#[derive(Debug, Clone)]
pub struct TuiAppState {
    sessions: Vec<SessionSummary>,
    active_screen: TuiScreen,
    current_session_id: Option<String>,
    previous_session_id: Option<String>,
    selected_session_index: usize,
    dialog_state: Option<DialogState>,
    input_buffer: String,
    scroll_offset: u16,
    should_exit: bool,
}

impl TuiAppState {
    pub fn new(sessions: Vec<SessionSummary>, current_session_id: Option<String>) -> Self {
        let active_screen = if current_session_id.is_some() {
            TuiScreen::Chat
        } else {
            TuiScreen::Sessions
        };
        let selected_session_index = current_session_id
            .as_deref()
            .and_then(|id| sessions.iter().position(|session| session.id == id))
            .unwrap_or(0);

        Self {
            sessions,
            active_screen,
            current_session_id: current_session_id.clone(),
            previous_session_id: current_session_id,
            selected_session_index,
            dialog_state: None,
            input_buffer: String::new(),
            scroll_offset: 0,
            should_exit: false,
        }
    }

    pub fn active_screen(&self) -> TuiScreen {
        self.active_screen
    }

    pub fn sessions(&self) -> &[SessionSummary] {
        &self.sessions
    }

    pub fn current_session_id(&self) -> Option<&str> {
        self.current_session_id.as_deref()
    }

    pub fn selected_session(&self) -> Option<&SessionSummary> {
        self.sessions.get(self.selected_session_index)
    }

    pub fn dialog_state(&self) -> Option<DialogState> {
        self.dialog_state.clone()
    }

    pub fn close_dialog(&mut self) {
        self.dialog_state = None;
    }

    pub fn open_new_session_dialog(&mut self) {
        self.dialog_state = Some(DialogState::CreateSession);
    }

    pub fn open_delete_dialog(&mut self) -> Result<(), &'static str> {
        let selected = self.selected_session().ok_or("no selected session")?;
        self.dialog_state = Some(DialogState::ConfirmDelete {
            session_id: selected.id.clone(),
        });
        Ok(())
    }

    pub fn open_session_screen(&mut self) {
        self.previous_session_id = self.current_session_id.clone();
        self.active_screen = TuiScreen::Sessions;
    }

    pub fn select_next_session(&mut self) {
        if self.sessions.is_empty() {
            return;
        }
        self.selected_session_index = (self.selected_session_index + 1) % self.sessions.len();
    }

    pub fn select_previous_session(&mut self) {
        if self.sessions.is_empty() {
            return;
        }
        self.selected_session_index = if self.selected_session_index == 0 {
            self.sessions.len() - 1
        } else {
            self.selected_session_index - 1
        };
    }

    pub fn activate_selected_session(&mut self) -> Result<(), &'static str> {
        let selected = self.selected_session().ok_or("no selected session")?;
        self.current_session_id = Some(selected.id.clone());
        self.active_screen = TuiScreen::Chat;
        self.dialog_state = None;
        Ok(())
    }

    pub fn handle_escape(&mut self) {
        if self.dialog_state.is_some() {
            self.dialog_state = None;
            return;
        }
        if self.active_screen == TuiScreen::Sessions && self.previous_session_id.is_some() {
            self.active_screen = TuiScreen::Chat;
        }
    }

    pub fn input_buffer(&self) -> &str {
        &self.input_buffer
    }

    pub fn input_buffer_mut(&mut self) -> &mut String {
        &mut self.input_buffer
    }

    pub fn scroll_offset(&self) -> u16 {
        self.scroll_offset
    }

    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    pub fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(1);
    }

    pub fn should_exit(&self) -> bool {
        self.should_exit
    }

    pub fn request_exit(&mut self) {
        self.should_exit = true;
    }
}
