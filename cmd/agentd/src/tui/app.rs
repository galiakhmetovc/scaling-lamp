use crate::bootstrap::{AgentScheduleView, McpConnectorView, SessionSummary};
use crate::tui::timeline::Timeline;
use crate::tui::worker::{
    ActiveRunHandle, ActiveRunPhase, ComposerQueue, QueuedDraft, QueuedDraftMode,
};

const COMMAND_HINTS: [&str; 59] = [
    "\\сессии",
    "\\новая",
    "\\агенты",
    "\\агент показать",
    "\\агент выбрать",
    "\\агент создать",
    "\\агент написать",
    "\\агент открыть",
    "\\судья",
    "\\расписания",
    "\\расписание показать",
    "\\расписание создать",
    "\\расписание изменить",
    "\\расписание включить",
    "\\расписание выключить",
    "\\расписание удалить",
    "\\mcp",
    "\\mcp показать",
    "\\mcp создать",
    "\\mcp изменить",
    "\\mcp включить",
    "\\mcp выключить",
    "\\mcp перезапустить",
    "\\mcp удалить",
    "\\память сессии",
    "\\память сессия",
    "\\память знания",
    "\\память файл",
    "\\цепочка продолжить",
    "\\переименовать",
    "\\очистить",
    "\\версия",
    "\\логи",
    "\\обновить",
    "\\помощь",
    "\\настройки",
    "\\отладка",
    "\\система",
    "\\контекст",
    "\\план",
    "\\статус",
    "\\процессы",
    "\\пауза",
    "\\стоп",
    "\\отмена",
    "\\задачи",
    "\\артефакты",
    "\\артефакт",
    "\\доводка",
    "\\автоапрув",
    "\\скиллы",
    "\\включить",
    "\\выключить",
    "\\апрув",
    "\\модель",
    "\\размышления",
    "\\думай",
    "\\компакт",
    "\\выход",
];
const COMMAND_STEMS: [&str; 59] = [
    "сессии",
    "новая",
    "агенты",
    "агент показать",
    "агент выбрать",
    "агент создать",
    "агент написать",
    "агент открыть",
    "судья",
    "расписания",
    "расписание показать",
    "расписание создать",
    "расписание изменить",
    "расписание включить",
    "расписание выключить",
    "расписание удалить",
    "mcp",
    "mcp показать",
    "mcp создать",
    "mcp изменить",
    "mcp включить",
    "mcp выключить",
    "mcp перезапустить",
    "mcp удалить",
    "память сессии",
    "память сессия",
    "память знания",
    "память файл",
    "цепочка продолжить",
    "переименовать",
    "очистить",
    "версия",
    "логи",
    "обновить",
    "помощь",
    "настройки",
    "отладка",
    "система",
    "контекст",
    "план",
    "статус",
    "процессы",
    "пауза",
    "стоп",
    "отмена",
    "задачи",
    "артефакты",
    "артефакт",
    "доводка",
    "автоапрув",
    "скиллы",
    "включить",
    "выключить",
    "апрув",
    "модель",
    "размышления",
    "думай",
    "компакт",
    "выход",
];
const PAGE_SCROLL_LINES: u16 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuiScreen {
    Sessions,
    Chat,
    Agents,
    Schedules,
    Mcp,
    Artifacts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserKind {
    Agents,
    Schedules,
    Mcp,
    Artifacts,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserItem {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserState {
    kind: BrowserKind,
    title: String,
    action_hint: String,
    items: Vec<BrowserItem>,
    selected_index: usize,
    preview_title: String,
    preview_content: String,
    preview_scroll: u16,
    full_preview: bool,
    search_query: Option<String>,
    search_match_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogState {
    CreateSession { value: String },
    CreateAgent { value: String },
    CreateScheduleForm { form: ScheduleFormState },
    EditScheduleForm { form: ScheduleFormState },
    CreateMcpConnectorForm { form: McpConnectorFormState },
    EditMcpConnectorForm { form: McpConnectorFormState },
    SendAgentMessageForm { form: AgentMessageFormState },
    GrantChainContinuationForm { form: ChainGrantFormState },
    BrowserSearch { value: String },
    RenameSession { session_id: String, value: String },
    ConfirmDelete { session_id: String },
    ConfirmClear { session_id: String },
    ConfirmDeleteSchedule { id: String },
    ConfirmDeleteMcpConnector { id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleFormKind {
    Create,
    Edit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleFormField {
    Id,
    Agent,
    Mode,
    DeliveryMode,
    TargetSessionId,
    IntervalSeconds,
    Enabled,
    Prompt,
}

const CREATE_SCHEDULE_FORM_FIELDS: [ScheduleFormField; 8] = [
    ScheduleFormField::Id,
    ScheduleFormField::Agent,
    ScheduleFormField::Mode,
    ScheduleFormField::DeliveryMode,
    ScheduleFormField::TargetSessionId,
    ScheduleFormField::IntervalSeconds,
    ScheduleFormField::Enabled,
    ScheduleFormField::Prompt,
];

const EDIT_SCHEDULE_FORM_FIELDS: [ScheduleFormField; 7] = [
    ScheduleFormField::Agent,
    ScheduleFormField::Mode,
    ScheduleFormField::DeliveryMode,
    ScheduleFormField::TargetSessionId,
    ScheduleFormField::IntervalSeconds,
    ScheduleFormField::Enabled,
    ScheduleFormField::Prompt,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleFormState {
    kind: ScheduleFormKind,
    active_index: usize,
    id: String,
    agent_identifier: String,
    mode: String,
    delivery_mode: String,
    target_session_id: String,
    interval_seconds: String,
    enabled: String,
    prompt: String,
}

impl ScheduleFormState {
    pub fn new_create() -> Self {
        Self {
            kind: ScheduleFormKind::Create,
            active_index: 0,
            id: String::new(),
            agent_identifier: String::new(),
            mode: "interval".to_string(),
            delivery_mode: "fresh_session".to_string(),
            target_session_id: String::new(),
            interval_seconds: "300".to_string(),
            enabled: "true".to_string(),
            prompt: String::new(),
        }
    }

    pub fn from_schedule(schedule: AgentScheduleView) -> Self {
        Self {
            kind: ScheduleFormKind::Edit,
            active_index: 0,
            id: schedule.id,
            agent_identifier: schedule.agent_profile_id,
            mode: schedule.mode.as_str().to_string(),
            delivery_mode: schedule.delivery_mode.as_str().to_string(),
            target_session_id: schedule.target_session_id.unwrap_or_default(),
            interval_seconds: schedule.interval_seconds.to_string(),
            enabled: schedule.enabled.to_string(),
            prompt: schedule.prompt,
        }
    }

    pub fn kind(&self) -> ScheduleFormKind {
        self.kind
    }

    pub fn title(&self) -> &'static str {
        match self.kind {
            ScheduleFormKind::Create => "Создать расписание",
            ScheduleFormKind::Edit => "Изменить расписание",
        }
    }

    pub fn fields(&self) -> &[ScheduleFormField] {
        match self.kind {
            ScheduleFormKind::Create => &CREATE_SCHEDULE_FORM_FIELDS,
            ScheduleFormKind::Edit => &EDIT_SCHEDULE_FORM_FIELDS,
        }
    }

    pub fn active_field(&self) -> ScheduleFormField {
        self.fields()[self.active_index.min(self.fields().len().saturating_sub(1))]
    }

    pub fn active_field_label(&self) -> &'static str {
        Self::field_label(self.active_field())
    }

    pub fn next_field(&mut self) {
        let fields = self.fields();
        if fields.is_empty() {
            self.active_index = 0;
            return;
        }
        self.active_index = (self.active_index + 1) % fields.len();
    }

    pub fn previous_field(&mut self) {
        let fields = self.fields();
        if fields.is_empty() {
            self.active_index = 0;
            return;
        }
        self.active_index = if self.active_index == 0 {
            fields.len().saturating_sub(1)
        } else {
            self.active_index - 1
        };
    }

    pub fn current_value(&self) -> &str {
        self.value_for(self.active_field())
    }

    pub fn set_current_value(&mut self, value: String) {
        *self.value_for_mut(self.active_field()) = value;
    }

    pub fn push_current_char(&mut self, value: char) {
        self.value_for_mut(self.active_field()).push(value);
    }

    pub fn pop_current_char(&mut self) {
        self.value_for_mut(self.active_field()).pop();
    }

    pub fn render_lines(&self) -> Vec<String> {
        let mut lines = vec![self.title().to_string(), String::new()];
        let id_marker = if self.active_field() == ScheduleFormField::Id {
            ">"
        } else {
            " "
        };
        lines.push(format!(
            "{id_marker} {}: {}",
            Self::field_label(ScheduleFormField::Id),
            self.id
        ));
        for field in self.fields() {
            if *field == ScheduleFormField::Id {
                continue;
            }
            let marker = if *field == self.active_field() {
                ">"
            } else {
                " "
            };
            lines.push(format!(
                "{marker} {}: {}",
                Self::field_label(*field),
                self.value_for(*field)
            ));
        }
        lines.push(String::new());
        lines.push("Пустой agent -> оставить текущий/дефолтный.".to_string());
        lines.push("session нужен только для delivery=existing_session.".to_string());
        lines.push("Tab/Shift+Tab или ↑/↓ поле, Enter сохранить, Esc отмена".to_string());
        lines
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn agent_identifier(&self) -> &str {
        &self.agent_identifier
    }

    pub fn mode(&self) -> &str {
        &self.mode
    }

    pub fn delivery_mode(&self) -> &str {
        &self.delivery_mode
    }

    pub fn target_session_id(&self) -> &str {
        &self.target_session_id
    }

    pub fn interval_seconds(&self) -> &str {
        &self.interval_seconds
    }

    pub fn enabled(&self) -> &str {
        &self.enabled
    }

    pub fn prompt(&self) -> &str {
        &self.prompt
    }

    fn field_label(field: ScheduleFormField) -> &'static str {
        match field {
            ScheduleFormField::Id => "id",
            ScheduleFormField::Agent => "agent",
            ScheduleFormField::Mode => "mode",
            ScheduleFormField::DeliveryMode => "delivery",
            ScheduleFormField::TargetSessionId => "session",
            ScheduleFormField::IntervalSeconds => "interval",
            ScheduleFormField::Enabled => "enabled",
            ScheduleFormField::Prompt => "prompt",
        }
    }

    fn value_for(&self, field: ScheduleFormField) -> &str {
        match field {
            ScheduleFormField::Id => &self.id,
            ScheduleFormField::Agent => &self.agent_identifier,
            ScheduleFormField::Mode => &self.mode,
            ScheduleFormField::DeliveryMode => &self.delivery_mode,
            ScheduleFormField::TargetSessionId => &self.target_session_id,
            ScheduleFormField::IntervalSeconds => &self.interval_seconds,
            ScheduleFormField::Enabled => &self.enabled,
            ScheduleFormField::Prompt => &self.prompt,
        }
    }

    fn value_for_mut(&mut self, field: ScheduleFormField) -> &mut String {
        match field {
            ScheduleFormField::Id => &mut self.id,
            ScheduleFormField::Agent => &mut self.agent_identifier,
            ScheduleFormField::Mode => &mut self.mode,
            ScheduleFormField::DeliveryMode => &mut self.delivery_mode,
            ScheduleFormField::TargetSessionId => &mut self.target_session_id,
            ScheduleFormField::IntervalSeconds => &mut self.interval_seconds,
            ScheduleFormField::Enabled => &mut self.enabled,
            ScheduleFormField::Prompt => &mut self.prompt,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpConnectorFormKind {
    Create,
    Edit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpConnectorFormField {
    Id,
    Command,
    Args,
    Cwd,
    Env,
    Enabled,
}

const CREATE_MCP_CONNECTOR_FORM_FIELDS: [McpConnectorFormField; 6] = [
    McpConnectorFormField::Id,
    McpConnectorFormField::Command,
    McpConnectorFormField::Args,
    McpConnectorFormField::Cwd,
    McpConnectorFormField::Env,
    McpConnectorFormField::Enabled,
];

const EDIT_MCP_CONNECTOR_FORM_FIELDS: [McpConnectorFormField; 5] = [
    McpConnectorFormField::Command,
    McpConnectorFormField::Args,
    McpConnectorFormField::Cwd,
    McpConnectorFormField::Env,
    McpConnectorFormField::Enabled,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpConnectorFormState {
    kind: McpConnectorFormKind,
    active_index: usize,
    id: String,
    command: String,
    args: String,
    cwd: String,
    env: String,
    enabled: String,
}

impl McpConnectorFormState {
    pub fn new_create() -> Self {
        Self {
            kind: McpConnectorFormKind::Create,
            active_index: 0,
            id: String::new(),
            command: String::new(),
            args: String::new(),
            cwd: String::new(),
            env: String::new(),
            enabled: "true".to_string(),
        }
    }

    pub fn from_connector(connector: McpConnectorView) -> Self {
        Self {
            kind: McpConnectorFormKind::Edit,
            active_index: 0,
            id: connector.id,
            command: connector.command,
            args: connector.args.join(","),
            cwd: connector.cwd.unwrap_or_default(),
            env: connector
                .env
                .iter()
                .map(|(key, value)| format!("{key}={value}"))
                .collect::<Vec<_>>()
                .join(";"),
            enabled: connector.enabled.to_string(),
        }
    }

    pub fn title(&self) -> &'static str {
        match self.kind {
            McpConnectorFormKind::Create => "Создать MCP-коннектор",
            McpConnectorFormKind::Edit => "Изменить MCP-коннектор",
        }
    }

    pub fn fields(&self) -> &[McpConnectorFormField] {
        match self.kind {
            McpConnectorFormKind::Create => &CREATE_MCP_CONNECTOR_FORM_FIELDS,
            McpConnectorFormKind::Edit => &EDIT_MCP_CONNECTOR_FORM_FIELDS,
        }
    }

    pub fn active_field(&self) -> McpConnectorFormField {
        self.fields()[self.active_index.min(self.fields().len().saturating_sub(1))]
    }

    pub fn next_field(&mut self) {
        let fields = self.fields();
        if fields.is_empty() {
            self.active_index = 0;
            return;
        }
        self.active_index = (self.active_index + 1) % fields.len();
    }

    pub fn previous_field(&mut self) {
        let fields = self.fields();
        if fields.is_empty() {
            self.active_index = 0;
            return;
        }
        self.active_index = if self.active_index == 0 {
            fields.len().saturating_sub(1)
        } else {
            self.active_index - 1
        };
    }

    pub fn current_value(&self) -> &str {
        self.value_for(self.active_field())
    }

    pub fn set_current_value(&mut self, value: String) {
        *self.value_for_mut(self.active_field()) = value;
    }

    pub fn push_current_char(&mut self, value: char) {
        self.value_for_mut(self.active_field()).push(value);
    }

    pub fn pop_current_char(&mut self) {
        self.value_for_mut(self.active_field()).pop();
    }

    pub fn render_lines(&self) -> Vec<String> {
        let mut lines = vec![self.title().to_string(), String::new()];
        let id_marker = if self.active_field() == McpConnectorFormField::Id {
            ">"
        } else {
            " "
        };
        lines.push(format!(
            "{id_marker} {}: {}",
            Self::field_label(McpConnectorFormField::Id),
            self.id
        ));
        for field in self.fields() {
            if *field == McpConnectorFormField::Id {
                continue;
            }
            let marker = if *field == self.active_field() {
                ">"
            } else {
                " "
            };
            lines.push(format!(
                "{marker} {}: {}",
                Self::field_label(*field),
                self.value_for(*field)
            ));
        }
        lines.push(String::new());
        lines.push("args: через запятую. env: KEY=VALUE;KEY2=VALUE2.".to_string());
        lines.push("Пустой cwd очищает рабочую директорию.".to_string());
        lines.push("Tab/Shift+Tab или ↑/↓ поле, Enter сохранить, Esc отмена".to_string());
        lines
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn command(&self) -> &str {
        &self.command
    }

    pub fn args(&self) -> &str {
        &self.args
    }

    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    pub fn env(&self) -> &str {
        &self.env
    }

    pub fn enabled(&self) -> &str {
        &self.enabled
    }

    fn field_label(field: McpConnectorFormField) -> &'static str {
        match field {
            McpConnectorFormField::Id => "id",
            McpConnectorFormField::Command => "command",
            McpConnectorFormField::Args => "args",
            McpConnectorFormField::Cwd => "cwd",
            McpConnectorFormField::Env => "env",
            McpConnectorFormField::Enabled => "enabled",
        }
    }

    fn value_for(&self, field: McpConnectorFormField) -> &str {
        match field {
            McpConnectorFormField::Id => &self.id,
            McpConnectorFormField::Command => &self.command,
            McpConnectorFormField::Args => &self.args,
            McpConnectorFormField::Cwd => &self.cwd,
            McpConnectorFormField::Env => &self.env,
            McpConnectorFormField::Enabled => &self.enabled,
        }
    }

    fn value_for_mut(&mut self, field: McpConnectorFormField) -> &mut String {
        match field {
            McpConnectorFormField::Id => &mut self.id,
            McpConnectorFormField::Command => &mut self.command,
            McpConnectorFormField::Args => &mut self.args,
            McpConnectorFormField::Cwd => &mut self.cwd,
            McpConnectorFormField::Env => &mut self.env,
            McpConnectorFormField::Enabled => &mut self.enabled,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentMessageFormField {
    TargetAgentId,
    Message,
}

const AGENT_MESSAGE_FORM_FIELDS: [AgentMessageFormField; 2] = [
    AgentMessageFormField::TargetAgentId,
    AgentMessageFormField::Message,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentMessageFormState {
    active_index: usize,
    target_agent_id: String,
    message: String,
}

impl AgentMessageFormState {
    pub fn new(target_agent_id: Option<String>) -> Self {
        Self {
            active_index: 0,
            target_agent_id: target_agent_id.unwrap_or_default(),
            message: String::new(),
        }
    }

    pub fn title(&self) -> &'static str {
        "Написать агенту"
    }

    pub fn active_field(&self) -> AgentMessageFormField {
        AGENT_MESSAGE_FORM_FIELDS[self
            .active_index
            .min(AGENT_MESSAGE_FORM_FIELDS.len().saturating_sub(1))]
    }

    pub fn next_field(&mut self) {
        self.active_index = (self.active_index + 1) % AGENT_MESSAGE_FORM_FIELDS.len();
    }

    pub fn previous_field(&mut self) {
        self.active_index = if self.active_index == 0 {
            AGENT_MESSAGE_FORM_FIELDS.len().saturating_sub(1)
        } else {
            self.active_index - 1
        };
    }

    pub fn current_value(&self) -> &str {
        match self.active_field() {
            AgentMessageFormField::TargetAgentId => &self.target_agent_id,
            AgentMessageFormField::Message => &self.message,
        }
    }

    pub fn set_current_value(&mut self, value: String) {
        *self.current_value_mut() = value;
    }

    pub fn push_current_char(&mut self, value: char) {
        self.current_value_mut().push(value);
    }

    pub fn pop_current_char(&mut self) {
        self.current_value_mut().pop();
    }

    pub fn render_lines(&self) -> Vec<String> {
        let mut lines = vec![self.title().to_string(), String::new()];
        for field in AGENT_MESSAGE_FORM_FIELDS {
            let marker = if field == self.active_field() {
                ">"
            } else {
                " "
            };
            let (label, value) = match field {
                AgentMessageFormField::TargetAgentId => ("agent", self.target_agent_id.as_str()),
                AgentMessageFormField::Message => ("message", self.message.as_str()),
            };
            lines.push(format!("{marker} {label}: {value}"));
        }
        lines.push(String::new());
        lines.push("Пустой agent недопустим.".to_string());
        lines.push("Tab/Shift+Tab или ↑/↓ поле, Enter отправить, Esc отмена".to_string());
        lines
    }

    pub fn target_agent_id(&self) -> &str {
        &self.target_agent_id
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    fn current_value_mut(&mut self) -> &mut String {
        match self.active_field() {
            AgentMessageFormField::TargetAgentId => &mut self.target_agent_id,
            AgentMessageFormField::Message => &mut self.message,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainGrantFormField {
    ChainId,
    Reason,
}

const CHAIN_GRANT_FORM_FIELDS: [ChainGrantFormField; 2] =
    [ChainGrantFormField::ChainId, ChainGrantFormField::Reason];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainGrantFormState {
    active_index: usize,
    chain_id: String,
    reason: String,
}

impl ChainGrantFormState {
    pub fn new(chain_id: Option<String>) -> Self {
        Self {
            active_index: 0,
            chain_id: chain_id.unwrap_or_default(),
            reason: String::new(),
        }
    }

    pub fn title(&self) -> &'static str {
        "Продолжить цепочку"
    }

    pub fn active_field(&self) -> ChainGrantFormField {
        CHAIN_GRANT_FORM_FIELDS[self
            .active_index
            .min(CHAIN_GRANT_FORM_FIELDS.len().saturating_sub(1))]
    }

    pub fn next_field(&mut self) {
        self.active_index = (self.active_index + 1) % CHAIN_GRANT_FORM_FIELDS.len();
    }

    pub fn previous_field(&mut self) {
        self.active_index = if self.active_index == 0 {
            CHAIN_GRANT_FORM_FIELDS.len().saturating_sub(1)
        } else {
            self.active_index - 1
        };
    }

    pub fn current_value(&self) -> &str {
        match self.active_field() {
            ChainGrantFormField::ChainId => &self.chain_id,
            ChainGrantFormField::Reason => &self.reason,
        }
    }

    pub fn set_current_value(&mut self, value: String) {
        *self.current_value_mut() = value;
    }

    pub fn push_current_char(&mut self, value: char) {
        self.current_value_mut().push(value);
    }

    pub fn pop_current_char(&mut self) {
        self.current_value_mut().pop();
    }

    pub fn render_lines(&self) -> Vec<String> {
        let mut lines = vec![self.title().to_string(), String::new()];
        for field in CHAIN_GRANT_FORM_FIELDS {
            let marker = if field == self.active_field() {
                ">"
            } else {
                " "
            };
            let (label, value) = match field {
                ChainGrantFormField::ChainId => ("chain_id", self.chain_id.as_str()),
                ChainGrantFormField::Reason => ("reason", self.reason.as_str()),
            };
            lines.push(format!("{marker} {label}: {value}"));
        }
        lines.push(String::new());
        lines.push("Укажите chain_id и краткую причину continuation grant.".to_string());
        lines.push("Tab/Shift+Tab или ↑/↓ поле, Enter сохранить, Esc отмена".to_string());
        lines
    }

    pub fn chain_id(&self) -> &str {
        &self.chain_id
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }

    fn current_value_mut(&mut self) -> &mut String {
        match self.active_field() {
            ChainGrantFormField::ChainId => &mut self.chain_id,
            ChainGrantFormField::Reason => &mut self.reason,
        }
    }
}

pub struct TuiAppState {
    sessions: Vec<SessionSummary>,
    active_screen: TuiScreen,
    current_session_id: Option<String>,
    previous_session_id: Option<String>,
    previous_screen: Option<TuiScreen>,
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
    inspector_title: Option<String>,
    inspector_content: Option<String>,
    browser: Option<BrowserState>,
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
            previous_screen: None,
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
            inspector_title: None,
            inspector_content: None,
            browser: None,
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
        self.previous_screen = None;
        self.provider_loop_progress = None;
        self.inspector_title = None;
        self.inspector_content = None;
        self.browser = None;
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
            | Some(DialogState::CreateAgent { value })
            | Some(DialogState::BrowserSearch { value })
            | Some(DialogState::RenameSession { value, .. }) => Some(value.as_str()),
            Some(DialogState::CreateScheduleForm { form })
            | Some(DialogState::EditScheduleForm { form }) => Some(form.current_value()),
            Some(DialogState::CreateMcpConnectorForm { form })
            | Some(DialogState::EditMcpConnectorForm { form }) => Some(form.current_value()),
            Some(DialogState::SendAgentMessageForm { form }) => Some(form.current_value()),
            Some(DialogState::GrantChainContinuationForm { form }) => Some(form.current_value()),
            _ => None,
        }
    }

    pub fn set_dialog_input(&mut self, value: String) {
        match self.dialog_state.as_mut() {
            Some(DialogState::CreateSession { value: current })
            | Some(DialogState::CreateAgent { value: current })
            | Some(DialogState::BrowserSearch { value: current })
            | Some(DialogState::RenameSession { value: current, .. }) => {
                *current = value;
            }
            Some(DialogState::CreateScheduleForm { form })
            | Some(DialogState::EditScheduleForm { form }) => {
                form.set_current_value(value);
            }
            Some(DialogState::CreateMcpConnectorForm { form })
            | Some(DialogState::EditMcpConnectorForm { form }) => {
                form.set_current_value(value);
            }
            Some(DialogState::SendAgentMessageForm { form }) => form.set_current_value(value),
            Some(DialogState::GrantChainContinuationForm { form }) => {
                form.set_current_value(value);
            }
            _ => {}
        }
    }

    pub fn append_dialog_input(&mut self, value: char) {
        match self.dialog_state.as_mut() {
            Some(DialogState::CreateSession { value: current })
            | Some(DialogState::CreateAgent { value: current })
            | Some(DialogState::BrowserSearch { value: current })
            | Some(DialogState::RenameSession { value: current, .. }) => {
                current.push(value);
            }
            Some(DialogState::CreateScheduleForm { form })
            | Some(DialogState::EditScheduleForm { form }) => {
                form.push_current_char(value);
            }
            Some(DialogState::CreateMcpConnectorForm { form })
            | Some(DialogState::EditMcpConnectorForm { form }) => {
                form.push_current_char(value);
            }
            Some(DialogState::SendAgentMessageForm { form }) => form.push_current_char(value),
            Some(DialogState::GrantChainContinuationForm { form }) => {
                form.push_current_char(value);
            }
            _ => {}
        }
    }

    pub fn pop_dialog_input(&mut self) {
        match self.dialog_state.as_mut() {
            Some(DialogState::CreateSession { value })
            | Some(DialogState::CreateAgent { value })
            | Some(DialogState::BrowserSearch { value })
            | Some(DialogState::RenameSession { value, .. }) => {
                value.pop();
            }
            Some(DialogState::CreateScheduleForm { form })
            | Some(DialogState::EditScheduleForm { form }) => {
                form.pop_current_char();
            }
            Some(DialogState::CreateMcpConnectorForm { form })
            | Some(DialogState::EditMcpConnectorForm { form }) => {
                form.pop_current_char();
            }
            Some(DialogState::SendAgentMessageForm { form }) => form.pop_current_char(),
            Some(DialogState::GrantChainContinuationForm { form }) => form.pop_current_char(),
            _ => {}
        }
    }

    pub fn dialog_next_field(&mut self) {
        match self.dialog_state.as_mut() {
            Some(DialogState::CreateScheduleForm { form })
            | Some(DialogState::EditScheduleForm { form }) => form.next_field(),
            Some(DialogState::CreateMcpConnectorForm { form })
            | Some(DialogState::EditMcpConnectorForm { form }) => form.next_field(),
            Some(DialogState::SendAgentMessageForm { form }) => form.next_field(),
            Some(DialogState::GrantChainContinuationForm { form }) => form.next_field(),
            _ => {}
        }
    }

    pub fn dialog_previous_field(&mut self) {
        match self.dialog_state.as_mut() {
            Some(DialogState::CreateScheduleForm { form })
            | Some(DialogState::EditScheduleForm { form }) => form.previous_field(),
            Some(DialogState::CreateMcpConnectorForm { form })
            | Some(DialogState::EditMcpConnectorForm { form }) => form.previous_field(),
            Some(DialogState::SendAgentMessageForm { form }) => form.previous_field(),
            Some(DialogState::GrantChainContinuationForm { form }) => form.previous_field(),
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

    pub fn open_create_agent_dialog(&mut self) {
        self.dialog_state = Some(DialogState::CreateAgent {
            value: String::new(),
        });
    }

    pub fn open_create_schedule_dialog(&mut self) {
        self.dialog_state = Some(DialogState::CreateScheduleForm {
            form: ScheduleFormState::new_create(),
        });
    }

    pub fn open_edit_schedule_dialog(&mut self, schedule: AgentScheduleView) {
        self.dialog_state = Some(DialogState::EditScheduleForm {
            form: ScheduleFormState::from_schedule(schedule),
        });
    }

    pub fn open_create_mcp_connector_dialog(&mut self) {
        self.dialog_state = Some(DialogState::CreateMcpConnectorForm {
            form: McpConnectorFormState::new_create(),
        });
    }

    pub fn open_edit_mcp_connector_dialog(&mut self, connector: McpConnectorView) {
        self.dialog_state = Some(DialogState::EditMcpConnectorForm {
            form: McpConnectorFormState::from_connector(connector),
        });
    }

    pub fn open_send_agent_message_dialog(&mut self, target_agent_id: Option<String>) {
        self.dialog_state = Some(DialogState::SendAgentMessageForm {
            form: AgentMessageFormState::new(target_agent_id),
        });
    }

    pub fn open_grant_chain_dialog(&mut self, chain_id: Option<String>) {
        self.dialog_state = Some(DialogState::GrantChainContinuationForm {
            form: ChainGrantFormState::new(chain_id),
        });
    }

    pub fn open_browser_search_dialog(&mut self) {
        let value = self
            .browser
            .as_ref()
            .and_then(|browser| browser.search_query.clone())
            .unwrap_or_default();
        self.dialog_state = Some(DialogState::BrowserSearch { value });
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

    pub fn open_delete_schedule_dialog(&mut self, id: String) {
        self.dialog_state = Some(DialogState::ConfirmDeleteSchedule { id });
    }

    pub fn open_delete_mcp_connector_dialog(&mut self, id: String) {
        self.dialog_state = Some(DialogState::ConfirmDeleteMcpConnector { id });
    }

    pub fn open_session_screen(&mut self) {
        self.previous_session_id = self.current_session_id.clone();
        self.active_screen = TuiScreen::Sessions;
        self.previous_screen = None;
        self.inspector_title = None;
        self.inspector_content = None;
        self.browser = None;
    }

    pub fn open_agent_screen(&mut self, title: String, content: String) {
        self.open_inspector_screen(TuiScreen::Agents, title, content);
    }

    pub fn open_schedule_screen(&mut self, title: String, content: String) {
        self.open_inspector_screen(TuiScreen::Schedules, title, content);
    }

    pub fn open_mcp_screen(&mut self, title: String, content: String) {
        self.open_inspector_screen(TuiScreen::Mcp, title, content);
    }

    pub fn open_artifact_screen(&mut self, title: String, content: String) {
        self.open_inspector_screen(TuiScreen::Artifacts, title, content);
    }

    pub fn open_agent_browser(
        &mut self,
        title: String,
        action_hint: String,
        items: Vec<BrowserItem>,
        selected_index: usize,
        preview_title: String,
        preview_content: String,
    ) {
        self.open_browser_screen(
            TuiScreen::Agents,
            BrowserState {
                kind: BrowserKind::Agents,
                title,
                action_hint,
                items,
                selected_index,
                preview_title,
                preview_content,
                preview_scroll: 0,
                full_preview: false,
                search_query: None,
                search_match_index: 0,
            },
        );
    }

    pub fn open_schedule_browser(
        &mut self,
        title: String,
        action_hint: String,
        items: Vec<BrowserItem>,
        selected_index: usize,
        preview_title: String,
        preview_content: String,
    ) {
        self.open_browser_screen(
            TuiScreen::Schedules,
            BrowserState {
                kind: BrowserKind::Schedules,
                title,
                action_hint,
                items,
                selected_index,
                preview_title,
                preview_content,
                preview_scroll: 0,
                full_preview: false,
                search_query: None,
                search_match_index: 0,
            },
        );
    }

    pub fn open_artifact_browser(
        &mut self,
        title: String,
        action_hint: String,
        items: Vec<BrowserItem>,
        selected_index: usize,
        preview_title: String,
        preview_content: String,
    ) {
        self.open_browser_screen(
            TuiScreen::Artifacts,
            BrowserState {
                kind: BrowserKind::Artifacts,
                title,
                action_hint,
                items,
                selected_index,
                preview_title,
                preview_content,
                preview_scroll: 0,
                full_preview: false,
                search_query: None,
                search_match_index: 0,
            },
        );
    }

    pub fn open_mcp_browser(
        &mut self,
        title: String,
        action_hint: String,
        items: Vec<BrowserItem>,
        selected_index: usize,
        preview_title: String,
        preview_content: String,
    ) {
        self.open_browser_screen(
            TuiScreen::Mcp,
            BrowserState {
                kind: BrowserKind::Mcp,
                title,
                action_hint,
                items,
                selected_index,
                preview_title,
                preview_content,
                preview_scroll: 0,
                full_preview: false,
                search_query: None,
                search_match_index: 0,
            },
        );
    }

    pub fn active_inspector_title(&self) -> Option<&str> {
        self.inspector_title.as_deref()
    }

    pub fn active_inspector_content(&self) -> Option<&str> {
        self.inspector_content.as_deref()
    }

    pub fn browser_state(&self) -> Option<&BrowserState> {
        self.browser.as_ref()
    }

    pub fn browser_state_mut(&mut self) -> Option<&mut BrowserState> {
        self.browser.as_mut()
    }

    pub fn browser_selected_item(&self) -> Option<&BrowserItem> {
        self.browser
            .as_ref()
            .and_then(|browser| browser.items.get(browser.selected_index))
    }

    pub fn browser_select_next(&mut self) {
        let Some(browser) = self.browser.as_mut() else {
            return;
        };
        if browser.items.is_empty() {
            return;
        }
        browser.selected_index = (browser.selected_index + 1) % browser.items.len();
    }

    pub fn browser_select_previous(&mut self) {
        let Some(browser) = self.browser.as_mut() else {
            return;
        };
        if browser.items.is_empty() {
            return;
        }
        browser.selected_index = if browser.selected_index == 0 {
            browser.items.len() - 1
        } else {
            browser.selected_index - 1
        };
    }

    pub fn set_browser_preview(&mut self, title: String, content: String) {
        if let Some(browser) = self.browser.as_mut() {
            browser.preview_title = title;
            browser.preview_content = content;
            browser.preview_scroll = 0;
            browser.search_match_index = 0;
            browser.full_preview = false;
            browser.snap_preview_to_current_match();
        }
    }

    pub fn browser_preview_scroll_up(&mut self) {
        if let Some(browser) = self.browser.as_mut() {
            browser.preview_scroll = browser.preview_scroll.saturating_sub(1);
        }
    }

    pub fn browser_preview_scroll_down(&mut self) {
        if let Some(browser) = self.browser.as_mut() {
            browser.preview_scroll = browser.preview_scroll.saturating_add(1);
        }
    }

    pub fn browser_preview_scroll_page_up(&mut self) {
        if let Some(browser) = self.browser.as_mut() {
            browser.preview_scroll = browser.preview_scroll.saturating_sub(PAGE_SCROLL_LINES);
        }
    }

    pub fn browser_preview_scroll_page_down(&mut self) {
        if let Some(browser) = self.browser.as_mut() {
            browser.preview_scroll = browser.preview_scroll.saturating_add(PAGE_SCROLL_LINES);
        }
    }

    pub fn browser_preview_scroll_home(&mut self) {
        if let Some(browser) = self.browser.as_mut() {
            browser.preview_scroll = 0;
        }
    }

    pub fn browser_preview_scroll_end(&mut self) {
        if let Some(browser) = self.browser.as_mut() {
            browser.preview_scroll = u16::MAX;
        }
    }

    pub fn toggle_browser_full_preview(&mut self) {
        if let Some(browser) = self.browser.as_mut() {
            browser.full_preview = !browser.full_preview;
        }
    }

    pub fn close_browser_full_preview(&mut self) {
        if let Some(browser) = self.browser.as_mut() {
            browser.full_preview = false;
        }
    }

    pub fn browser_full_preview(&self) -> bool {
        self.browser
            .as_ref()
            .map(|browser| browser.full_preview)
            .unwrap_or(false)
    }

    pub fn apply_browser_search(&mut self, query: String) {
        if let Some(browser) = self.browser.as_mut() {
            browser.search_query = if query.trim().is_empty() {
                None
            } else {
                Some(query)
            };
            browser.search_match_index = 0;
            browser.snap_preview_to_current_match();
        }
    }

    pub fn browser_search_next(&mut self) {
        if let Some(browser) = self.browser.as_mut() {
            let match_count = browser.search_match_lines().len();
            if match_count == 0 {
                return;
            }
            browser.search_match_index = (browser.search_match_index + 1) % match_count;
            browser.snap_preview_to_current_match();
        }
    }

    pub fn browser_search_previous(&mut self) {
        if let Some(browser) = self.browser.as_mut() {
            let match_count = browser.search_match_lines().len();
            if match_count == 0 {
                return;
            }
            browser.search_match_index = if browser.search_match_index == 0 {
                match_count - 1
            } else {
                browser.search_match_index - 1
            };
            browser.snap_preview_to_current_match();
        }
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
        if self.browser_full_preview() {
            self.close_browser_full_preview();
            return;
        }
        match self.active_screen {
            TuiScreen::Sessions => {
                if self.previous_session_id.is_some() {
                    self.active_screen = TuiScreen::Chat;
                }
            }
            TuiScreen::Agents | TuiScreen::Schedules | TuiScreen::Mcp | TuiScreen::Artifacts => {
                self.active_screen = self.previous_screen.take().unwrap_or_else(|| {
                    if self.current_session_id.is_some() {
                        TuiScreen::Chat
                    } else {
                        TuiScreen::Sessions
                    }
                });
                self.inspector_title = None;
                self.inspector_content = None;
                self.browser = None;
            }
            TuiScreen::Chat => {}
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
        let matches = COMMAND_STEMS;
        let next_index = self
            .command_cycle_index
            .map(|index| (index + 1) % matches.len())
            .unwrap_or(0);
        self.command_cycle_index = Some(next_index);
        self.command_cycle_seed = None;
        self.input_buffer = format!("\\{}", matches[next_index]);
        self.input_cursor = self.input_buffer.len();
        true
    }

    pub fn reset_command_cycle(&mut self) {
        self.command_cycle_index = None;
        self.command_cycle_seed = None;
    }

    pub fn command_hints(&self) -> &'static [&'static str] {
        &COMMAND_HINTS
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

    fn open_inspector_screen(&mut self, screen: TuiScreen, title: String, content: String) {
        if self.active_screen != screen {
            self.previous_screen = Some(self.active_screen);
        }
        self.active_screen = screen;
        self.browser = None;
        self.inspector_title = Some(title);
        self.inspector_content = Some(content);
    }

    fn open_browser_screen(&mut self, screen: TuiScreen, browser: BrowserState) {
        if self.active_screen != screen {
            self.previous_screen = Some(self.active_screen);
        }
        self.active_screen = screen;
        self.inspector_title = None;
        self.inspector_content = None;
        self.browser = Some(browser);
    }
}

impl BrowserState {
    pub fn kind(&self) -> BrowserKind {
        self.kind
    }

    pub fn title(&self) -> &str {
        self.title.as_str()
    }

    pub fn action_hint(&self) -> &str {
        self.action_hint.as_str()
    }

    pub fn items(&self) -> &[BrowserItem] {
        &self.items
    }

    pub fn selected_index(&self) -> usize {
        self.selected_index
    }

    pub fn preview_title(&self) -> &str {
        self.preview_title.as_str()
    }

    pub fn preview_content(&self) -> &str {
        self.preview_content.as_str()
    }

    pub fn preview_scroll(&self) -> u16 {
        self.preview_scroll
    }

    pub fn full_preview(&self) -> bool {
        self.full_preview
    }

    pub fn search_query(&self) -> Option<&str> {
        self.search_query.as_deref()
    }

    pub fn search_status(&self) -> Option<(usize, usize)> {
        let query = self.search_query.as_ref()?;
        if query.trim().is_empty() {
            return None;
        }
        let matches = self.search_match_lines();
        if matches.is_empty() {
            Some((0, 0))
        } else {
            Some((self.search_match_index + 1, matches.len()))
        }
    }

    pub fn current_match_line(&self) -> Option<usize> {
        let matches = self.search_match_lines();
        matches.get(self.search_match_index).copied()
    }

    pub fn line_matches_query(&self, line: &str) -> bool {
        self.search_query
            .as_deref()
            .is_some_and(|query| line_matches_query(line, query))
    }

    fn search_match_lines(&self) -> Vec<usize> {
        let Some(query) = self.search_query.as_deref() else {
            return Vec::new();
        };
        if query.trim().is_empty() {
            return Vec::new();
        }
        self.preview_content
            .split('\n')
            .enumerate()
            .filter_map(|(index, line)| line_matches_query(line, query).then_some(index))
            .collect()
    }

    fn snap_preview_to_current_match(&mut self) {
        if let Some(line_index) = self.current_match_line() {
            self.preview_scroll = line_index as u16;
        } else {
            self.preview_scroll = 0;
        }
    }
}

fn line_matches_query(line: &str, query: &str) -> bool {
    if query.trim().is_empty() {
        return false;
    }
    line.to_lowercase().contains(&query.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::{BrowserItem, TuiAppState};

    #[test]
    fn command_cycle_works_without_prefix_and_uses_russian_commands() {
        let mut state = TuiAppState::new(Vec::new(), None);

        assert!(state.cycle_previous_command());
        assert_eq!(state.input_buffer(), "\\сессии");

        state.replace_input_buffer("авто");
        assert!(state.cycle_previous_command());
        assert_eq!(state.input_buffer(), "\\сессии");
    }

    #[test]
    fn artifact_browser_tracks_search_full_preview_and_scroll_state() {
        let mut state = TuiAppState::new(Vec::new(), Some("session-a".to_string()));
        state.open_artifact_browser(
            "Артефакты".to_string(),
            "↑↓ выбор | Enter полный".to_string(),
            vec![BrowserItem {
                id: "artifact-1".to_string(),
                label: "artifact-1 [ref]".to_string(),
            }],
            0,
            "Артефакт artifact-1".to_string(),
            "alpha\nbeta\nGamma".to_string(),
        );

        state.apply_browser_search("ga".to_string());
        let browser = state.browser_state().expect("browser");
        assert_eq!(browser.search_query(), Some("ga"));
        assert_eq!(browser.search_status(), Some((1, 1)));
        assert_eq!(browser.current_match_line(), Some(2));
        assert_eq!(browser.preview_scroll(), 2);

        state.toggle_browser_full_preview();
        assert!(state.browser_full_preview());
        state.handle_escape();
        assert!(!state.browser_full_preview());
    }
}
