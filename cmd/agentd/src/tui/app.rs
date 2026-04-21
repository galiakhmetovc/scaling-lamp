use crate::bootstrap::SessionSummary;
use crate::tui::timeline::Timeline;
use crate::tui::worker::{
    ActiveRunHandle, ActiveRunPhase, ComposerQueue, QueuedDraft, QueuedDraftMode,
};

const COMMANDS: [&str; 19] = [
    "/session",
    "/new",
    "/rename",
    "/clear",
    "/debug",
    "/context",
    "/plan",
    "/jobs",
    "\\доводка",
    "\\автоапрув",
    "\\скиллы",
    "\\включить",
    "\\выключить",
    "/approve",
    "/model",
    "/reasoning",
    "/think",
    "/compact",
    "/exit",
];
const PAGE_SCROLL_LINES: u16 = 10;

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

pub struct TuiAppState {
    sessions: Vec<SessionSummary>,
    active_screen: TuiScreen,
    current_session_id: Option<String>,
    previous_session_id: Option<String>,
    active_summary: Option<SessionSummary>,
    selected_session_index: usize,
    dialog_state: Option<DialogState>,
    input_buffer: String,
    input_cursor: usize,
    command_cycle_index: Option<usize>,
    command_cycle_seed: Option<String>,
    scroll_offset: u16,
    timeline: Timeline,
    composer_queue: ComposerQueue,
    active_run: Option<ActiveRunHandle>,
    provider_loop_progress: Option<(usize, usize)>,
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
            input_cursor: 0,
            command_cycle_index: None,
            command_cycle_seed: None,
            scroll_offset: 0,
            timeline: Timeline::default(),
            composer_queue: ComposerQueue::default(),
            active_run: None,
            provider_loop_progress: None,
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
        self.input_cursor = 0;
        self.command_cycle_index = None;
        self.dialog_state = None;
        self.active_screen = TuiScreen::Chat;
        self.provider_loop_progress = None;
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

    pub fn input_cursor(&self) -> usize {
        self.input_cursor
    }

    pub fn replace_input_buffer(&mut self, value: impl Into<String>) {
        self.input_buffer = value.into();
        self.input_cursor = self.input_buffer.len();
        self.command_cycle_index = None;
        self.command_cycle_seed = None;
    }

    pub fn push_input_char(&mut self, value: char) {
        self.input_buffer.insert(self.input_cursor, value);
        self.input_cursor += value.len_utf8();
        self.command_cycle_index = None;
        self.command_cycle_seed = None;
    }

    pub fn insert_input_text(&mut self, value: &str) {
        self.input_buffer.insert_str(self.input_cursor, value);
        self.input_cursor += value.len();
        self.command_cycle_index = None;
        self.command_cycle_seed = None;
    }

    pub fn pop_input_char(&mut self) {
        if self.input_cursor == 0 {
            return;
        }
        let previous_index = self
            .input_buffer
            .char_indices()
            .map(|(index, _)| index)
            .take_while(|index| *index < self.input_cursor)
            .last()
            .unwrap_or(0);
        self.input_buffer.drain(previous_index..self.input_cursor);
        self.input_cursor = previous_index;
        self.command_cycle_index = None;
        self.command_cycle_seed = None;
    }

    pub fn delete_input_char(&mut self) {
        if self.input_cursor >= self.input_buffer.len() {
            return;
        }
        let next_index = self
            .input_buffer
            .char_indices()
            .map(|(index, _)| index)
            .find(|index| *index > self.input_cursor)
            .unwrap_or(self.input_buffer.len());
        self.input_buffer.drain(self.input_cursor..next_index);
        self.command_cycle_index = None;
        self.command_cycle_seed = None;
    }

    pub fn move_input_cursor_left(&mut self) {
        if self.input_cursor == 0 {
            return;
        }
        self.input_cursor = self
            .input_buffer
            .char_indices()
            .map(|(index, _)| index)
            .take_while(|index| *index < self.input_cursor)
            .last()
            .unwrap_or(0);
    }

    pub fn move_input_cursor_right(&mut self) {
        if self.input_cursor >= self.input_buffer.len() {
            return;
        }
        self.input_cursor = self
            .input_buffer
            .char_indices()
            .map(|(index, _)| index)
            .find(|index| *index > self.input_cursor)
            .unwrap_or(self.input_buffer.len());
    }

    pub fn move_input_cursor_home(&mut self) {
        self.input_cursor = 0;
    }

    pub fn move_input_cursor_end(&mut self) {
        self.input_cursor = self.input_buffer.len();
    }

    pub fn take_input_buffer(&mut self) -> String {
        self.command_cycle_index = None;
        self.command_cycle_seed = None;
        self.input_cursor = 0;
        std::mem::take(&mut self.input_buffer)
    }

    pub fn scroll_offset(&self) -> u16 {
        self.scroll_offset
    }

    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(1);
    }

    pub fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    pub fn scroll_page_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(PAGE_SCROLL_LINES);
    }

    pub fn scroll_page_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(PAGE_SCROLL_LINES);
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

    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
    }

    pub fn has_active_run(&self) -> bool {
        self.active_run.is_some()
    }

    pub fn active_run(&self) -> Option<&ActiveRunHandle> {
        self.active_run.as_ref()
    }

    pub fn active_run_mut(&mut self) -> Option<&mut ActiveRunHandle> {
        self.active_run.as_mut()
    }

    pub fn set_active_run(&mut self, active_run: ActiveRunHandle) {
        self.active_run = Some(active_run);
        self.provider_loop_progress = None;
    }

    pub fn take_active_run(&mut self) -> Option<ActiveRunHandle> {
        self.active_run.take()
    }

    pub fn provider_loop_progress(&self) -> Option<(usize, usize)> {
        self.provider_loop_progress
    }

    pub fn set_provider_loop_progress(&mut self, current_round: usize, max_rounds: usize) {
        self.provider_loop_progress = Some((current_round, max_rounds));
    }

    pub fn clear_provider_loop_progress(&mut self) {
        self.provider_loop_progress = None;
    }

    pub fn queue_draft(&mut self, content: String, queued_at: i64, mode: QueuedDraftMode) {
        self.composer_queue.enqueue(QueuedDraft {
            content,
            queued_at,
            mode,
        });
        if matches!(mode, QueuedDraftMode::Priority)
            && let Some(active_run) = self.active_run.as_ref()
        {
            active_run.queue_interrupt_after_tool_step();
        }
    }

    pub fn next_priority_draft(&mut self) -> Option<QueuedDraft> {
        self.composer_queue.pop_priority()
    }

    pub fn next_deferred_draft(&mut self) -> Option<QueuedDraft> {
        self.composer_queue.pop_deferred()
    }

    pub fn queued_draft_count(&self) -> usize {
        self.composer_queue.total_len()
    }

    pub fn queued_priority_count(&self) -> usize {
        self.composer_queue.priority_len()
    }

    pub fn queued_deferred_count(&self) -> usize {
        self.composer_queue.deferred_len()
    }

    pub fn cycle_previous_command(&mut self) -> bool {
        if !self.input_buffer.starts_with('/') {
            return false;
        }

        let (typed_prefix, suffix) = self
            .input_buffer
            .split_once(' ')
            .map(|(command, rest)| (command, Some(rest)))
            .unwrap_or((self.input_buffer.as_str(), None));
        let command_prefix = self
            .command_cycle_seed
            .clone()
            .unwrap_or_else(|| typed_prefix.to_string());
        let matches = COMMANDS
            .iter()
            .copied()
            .filter(|command| command.starts_with(command_prefix.as_str()))
            .collect::<Vec<_>>();
        if matches.is_empty() {
            return false;
        }
        let next_index = self
            .command_cycle_index
            .map(|index| (index + 1) % matches.len())
            .unwrap_or(0);
        self.command_cycle_index = Some(next_index);
        self.command_cycle_seed = Some(command_prefix);
        self.input_buffer = match suffix {
            Some(rest) if !rest.is_empty() => format!("{} {}", matches[next_index], rest),
            _ => matches[next_index].to_string(),
        };
        true
    }

    pub fn reset_command_cycle(&mut self) {
        self.command_cycle_index = None;
        self.command_cycle_seed = None;
    }

    pub fn command_hints(&self) -> &'static [&'static str] {
        &COMMANDS
    }

    pub fn current_phase(&self) -> Option<&ActiveRunPhase> {
        self.active_run.as_ref().map(ActiveRunHandle::phase)
    }

    pub fn should_exit(&self) -> bool {
        self.should_exit
    }

    pub fn request_exit(&mut self) {
        self.should_exit = true;
    }
}
