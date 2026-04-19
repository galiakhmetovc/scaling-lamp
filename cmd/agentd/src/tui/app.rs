use crate::bootstrap::SessionSummary;
use crate::tui::timeline::Timeline;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuiScreen {
    Sessions,
    Chat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogState {
    CreateSession { value: String },
    RenameSession { session_id: String, value: String },
    ConfirmDelete { session_id: String },
    ConfirmClear { session_id: String },
}

#[derive(Debug, Clone)]
pub struct TuiAppState {
    sessions: Vec<SessionSummary>,
    active_screen: TuiScreen,
    current_session_id: Option<String>,
    previous_session_id: Option<String>,
    active_summary: Option<SessionSummary>,
    selected_session_index: usize,
    dialog_state: Option<DialogState>,
    input_buffer: String,
    scroll_offset: u16,
    timeline: Timeline,
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
        let active_summary = current_session_id
            .as_deref()
            .and_then(|id| sessions.iter().find(|session| session.id == id))
            .cloned();

        Self {
            sessions,
            active_screen,
            current_session_id: current_session_id.clone(),
            previous_session_id: current_session_id,
            active_summary,
            selected_session_index,
            dialog_state: None,
            input_buffer: String::new(),
            scroll_offset: 0,
            timeline: Timeline::default(),
            should_exit: false,
        }
    }

    pub fn active_screen(&self) -> TuiScreen {
        self.active_screen
    }

    pub fn sessions(&self) -> &[SessionSummary] {
        &self.sessions
    }

    pub fn sync_sessions(&mut self, sessions: Vec<SessionSummary>) {
        self.sessions = sessions;
        if self.sessions.is_empty() {
            self.selected_session_index = 0;
            self.current_session_id = None;
            self.active_summary = None;
            return;
        }
        if let Some(current_id) = self.current_session_id.as_deref() {
            if let Some(index) = self
                .sessions
                .iter()
                .position(|session| session.id == current_id)
            {
                self.selected_session_index = index;
                self.active_summary = self.sessions.get(index).cloned();
                return;
            }
            self.current_session_id = None;
            self.active_summary = None;
        }
        if self.selected_session_index >= self.sessions.len() {
            self.selected_session_index = self.sessions.len().saturating_sub(1);
        }
    }

    pub fn current_session_id(&self) -> Option<&str> {
        self.current_session_id.as_deref()
    }

    pub fn current_session_summary(&self) -> Option<&SessionSummary> {
        self.active_summary.as_ref()
    }

    pub fn set_current_session(&mut self, summary: SessionSummary, timeline: Timeline) {
        self.current_session_id = Some(summary.id.clone());
        self.previous_session_id = Some(summary.id.clone());
        self.active_summary = Some(summary.clone());
        if let Some(index) = self
            .sessions
            .iter()
            .position(|session| session.id == summary.id)
        {
            self.selected_session_index = index;
            self.sessions[index] = summary;
        } else {
            self.sessions.push(summary);
            self.selected_session_index = self.sessions.len().saturating_sub(1);
        }
        self.timeline = timeline;
        self.scroll_offset = 0;
        self.input_buffer.clear();
        self.dialog_state = None;
        self.active_screen = TuiScreen::Chat;
    }

    pub fn replace_current_summary(&mut self, summary: SessionSummary) {
        if let Some(index) = self.sessions.iter().position(|item| item.id == summary.id) {
            self.sessions[index] = summary.clone();
        } else {
            self.sessions.push(summary.clone());
        }
        if self.current_session_id.as_deref() == Some(summary.id.as_str()) {
            self.active_summary = Some(summary);
        }
    }

    pub fn selected_session(&self) -> Option<&SessionSummary> {
        self.sessions.get(self.selected_session_index)
    }

    pub fn dialog_state(&self) -> Option<DialogState> {
        self.dialog_state.clone()
    }

    pub fn dialog_input(&self) -> Option<&str> {
        match self.dialog_state.as_ref() {
            Some(DialogState::CreateSession { value })
            | Some(DialogState::RenameSession { value, .. }) => Some(value.as_str()),
            _ => None,
        }
    }

    pub fn set_dialog_input(&mut self, value: String) {
        match self.dialog_state.as_mut() {
            Some(DialogState::CreateSession { value: current })
            | Some(DialogState::RenameSession { value: current, .. }) => {
                *current = value;
            }
            _ => {}
        }
    }

    pub fn append_dialog_input(&mut self, value: char) {
        match self.dialog_state.as_mut() {
            Some(DialogState::CreateSession { value: current })
            | Some(DialogState::RenameSession { value: current, .. }) => {
                current.push(value);
            }
            _ => {}
        }
    }

    pub fn pop_dialog_input(&mut self) {
        match self.dialog_state.as_mut() {
            Some(DialogState::CreateSession { value })
            | Some(DialogState::RenameSession { value, .. }) => {
                value.pop();
            }
            _ => {}
        }
    }

    pub fn close_dialog(&mut self) {
        self.dialog_state = None;
    }

    pub fn open_new_session_dialog(&mut self) {
        self.dialog_state = Some(DialogState::CreateSession {
            value: String::new(),
        });
    }

    pub fn open_rename_dialog(&mut self) -> Result<(), &'static str> {
        let current = self
            .current_session_summary()
            .ok_or("no current session to rename")?;
        self.dialog_state = Some(DialogState::RenameSession {
            session_id: current.id.clone(),
            value: current.title.clone(),
        });
        Ok(())
    }

    pub fn open_delete_dialog(&mut self) -> Result<(), &'static str> {
        let selected = self.selected_session().ok_or("no selected session")?;
        self.dialog_state = Some(DialogState::ConfirmDelete {
            session_id: selected.id.clone(),
        });
        Ok(())
    }

    pub fn open_clear_dialog(&mut self) -> Result<(), &'static str> {
        let current = self
            .current_session_summary()
            .ok_or("no current session to clear")?;
        self.dialog_state = Some(DialogState::ConfirmClear {
            session_id: current.id.clone(),
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

    pub fn activate_selected_session(&mut self) -> Result<String, &'static str> {
        let selected = self
            .selected_session()
            .cloned()
            .ok_or("no selected session")?;
        self.current_session_id = Some(selected.id.clone());
        self.active_summary = Some(selected.clone());
        self.active_screen = TuiScreen::Chat;
        self.dialog_state = None;
        Ok(selected.id.clone())
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

    pub fn take_input_buffer(&mut self) -> String {
        std::mem::take(&mut self.input_buffer)
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

    pub fn timeline(&self) -> &Timeline {
        &self.timeline
    }

    pub fn timeline_mut(&mut self) -> &mut Timeline {
        &mut self.timeline
    }

    pub fn replace_timeline(&mut self, timeline: Timeline) {
        self.timeline = timeline;
        self.scroll_offset = 0;
    }

    pub fn should_exit(&self) -> bool {
        self.should_exit
    }

    pub fn request_exit(&mut self) {
        self.should_exit = true;
    }
}
