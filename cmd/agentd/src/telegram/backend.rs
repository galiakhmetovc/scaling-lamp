use crate::bootstrap::{
    BootstrapError, SessionPreferencesPatch, SessionSkillStatus, SessionSummary,
};
use crate::execution::{ChatExecutionEvent, ChatTurnExecutionReport};
use crate::http::client::DaemonClient;

pub trait TelegramBackend: Clone + Send + Sync + 'static {
    fn list_session_summaries(&self) -> Result<Vec<SessionSummary>, BootstrapError>;
    fn create_session_auto(&self, title: Option<&str>) -> Result<SessionSummary, BootstrapError>;
    fn update_session_preferences(
        &self,
        session_id: &str,
        patch: SessionPreferencesPatch,
    ) -> Result<SessionSummary, BootstrapError>;
    fn session_summary(&self, session_id: &str) -> Result<SessionSummary, BootstrapError>;
    fn execute_chat_turn(
        &self,
        session_id: &str,
        message: &str,
        now: i64,
        observer: &mut dyn FnMut(ChatExecutionEvent),
    ) -> Result<ChatTurnExecutionReport, BootstrapError>;
    fn send_agent_message(
        &self,
        session_id: &str,
        target_agent_id: &str,
        message: &str,
    ) -> Result<String, BootstrapError>;
    fn render_active_run(&self, session_id: &str) -> Result<String, BootstrapError>;
    fn cancel_active_run(&self, session_id: &str) -> Result<String, BootstrapError>;
    fn cancel_all_session_work(&self, session_id: &str) -> Result<String, BootstrapError>;
    fn render_session_background_jobs(&self, session_id: &str) -> Result<String, BootstrapError>;
    fn render_session_skills(&self, session_id: &str) -> Result<String, BootstrapError>;
    fn enable_session_skill(
        &self,
        session_id: &str,
        skill_name: &str,
    ) -> Result<String, BootstrapError>;
    fn disable_session_skill(
        &self,
        session_id: &str,
        skill_name: &str,
    ) -> Result<String, BootstrapError>;
    fn compact_session(&self, session_id: &str) -> Result<SessionSummary, BootstrapError>;
}

#[derive(Debug, Clone)]
pub struct DaemonTelegramBackend {
    client: DaemonClient,
}

impl DaemonTelegramBackend {
    pub fn new(client: DaemonClient) -> Self {
        Self { client }
    }

    pub fn client(&self) -> &DaemonClient {
        &self.client
    }
}

impl TelegramBackend for DaemonTelegramBackend {
    fn list_session_summaries(&self) -> Result<Vec<SessionSummary>, BootstrapError> {
        self.client.list_session_summaries()
    }

    fn create_session_auto(&self, title: Option<&str>) -> Result<SessionSummary, BootstrapError> {
        self.client.create_session_auto(title)
    }

    fn update_session_preferences(
        &self,
        session_id: &str,
        patch: SessionPreferencesPatch,
    ) -> Result<SessionSummary, BootstrapError> {
        self.client.update_session_preferences(session_id, patch)
    }

    fn session_summary(&self, session_id: &str) -> Result<SessionSummary, BootstrapError> {
        self.client.session_summary(session_id)
    }

    fn execute_chat_turn(
        &self,
        session_id: &str,
        message: &str,
        now: i64,
        observer: &mut dyn FnMut(ChatExecutionEvent),
    ) -> Result<ChatTurnExecutionReport, BootstrapError> {
        self.client
            .execute_chat_turn_with_trace_control_and_observer(
                session_id,
                message,
                now,
                None,
                observer,
                Some("telegram"),
                Some("telegram.message"),
            )
    }

    fn send_agent_message(
        &self,
        session_id: &str,
        target_agent_id: &str,
        message: &str,
    ) -> Result<String, BootstrapError> {
        self.client
            .send_agent_message(session_id, target_agent_id, message)
    }

    fn render_active_run(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.client.render_active_run(session_id)
    }

    fn cancel_active_run(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.client.cancel_active_run(session_id)
    }

    fn cancel_all_session_work(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.client.cancel_all_session_work(session_id)
    }

    fn render_session_background_jobs(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.client.render_session_background_jobs(session_id)
    }

    fn render_session_skills(&self, session_id: &str) -> Result<String, BootstrapError> {
        Ok(render_session_skills_text(
            self.client.session_skills(session_id)?,
        ))
    }

    fn enable_session_skill(
        &self,
        session_id: &str,
        skill_name: &str,
    ) -> Result<String, BootstrapError> {
        Ok(render_session_skills_text(
            self.client.enable_session_skill(session_id, skill_name)?,
        ))
    }

    fn disable_session_skill(
        &self,
        session_id: &str,
        skill_name: &str,
    ) -> Result<String, BootstrapError> {
        Ok(render_session_skills_text(
            self.client.disable_session_skill(session_id, skill_name)?,
        ))
    }

    fn compact_session(&self, session_id: &str) -> Result<SessionSummary, BootstrapError> {
        self.client.compact_session(session_id)
    }
}

pub(crate) fn render_session_skills_text(skills: Vec<SessionSkillStatus>) -> String {
    if skills.is_empty() {
        return "Скиллы: ничего не найдено".to_string();
    }

    let mut lines = vec!["Скиллы:".to_string()];
    lines.extend(
        skills
            .into_iter()
            .map(|skill| format!("- [{}] {}: {}", skill.mode, skill.name, skill.description)),
    );
    lines.join("\n")
}
