use crate::bootstrap::{
    App, BootstrapError, SessionPendingApproval, SessionPreferencesPatch, SessionSkillStatus,
    SessionSummary, SessionTranscriptView,
};
use crate::execution::{ApprovalContinuationReport, ChatExecutionEvent, ChatTurnExecutionReport};
use crate::http::client::DaemonClient;
use std::sync::atomic::AtomicBool;

pub trait TuiBackend: Clone + Send + Sync + 'static {
    fn list_session_summaries(&self) -> Result<Vec<SessionSummary>, BootstrapError>;
    fn create_session_auto(&self, title: Option<&str>) -> Result<SessionSummary, BootstrapError>;
    fn update_session_preferences(
        &self,
        session_id: &str,
        patch: SessionPreferencesPatch,
    ) -> Result<SessionSummary, BootstrapError>;
    fn delete_session(&self, session_id: &str) -> Result<(), BootstrapError>;
    fn clear_session(
        &self,
        session_id: &str,
        title: Option<&str>,
    ) -> Result<SessionSummary, BootstrapError>;
    fn session_summary(&self, session_id: &str) -> Result<SessionSummary, BootstrapError>;
    fn session_transcript(&self, session_id: &str)
    -> Result<SessionTranscriptView, BootstrapError>;
    fn pending_approvals(
        &self,
        session_id: &str,
    ) -> Result<Vec<SessionPendingApproval>, BootstrapError>;
    fn session_skills(&self, session_id: &str) -> Result<Vec<SessionSkillStatus>, BootstrapError>;
    fn enable_session_skill(
        &self,
        session_id: &str,
        skill_name: &str,
    ) -> Result<Vec<SessionSkillStatus>, BootstrapError>;
    fn disable_session_skill(
        &self,
        session_id: &str,
        skill_name: &str,
    ) -> Result<Vec<SessionSkillStatus>, BootstrapError>;
    fn latest_pending_approval(
        &self,
        session_id: &str,
        requested_approval_id: Option<&str>,
    ) -> Result<Option<SessionPendingApproval>, BootstrapError>;
    fn render_context(&self, session_id: &str) -> Result<String, BootstrapError>;
    fn render_plan(&self, session_id: &str) -> Result<String, BootstrapError>;
    fn render_active_jobs(&self, session_id: &str) -> Result<String, BootstrapError>;
    fn write_debug_bundle(&self, session_id: &str) -> Result<String, BootstrapError>;
    fn compact_session(&self, session_id: &str) -> Result<SessionSummary, BootstrapError>;
    fn execute_chat_turn_with_control_and_observer(
        &self,
        session_id: &str,
        message: &str,
        now: i64,
        interrupt_after_tool_step: Option<&AtomicBool>,
        observer: &mut dyn FnMut(ChatExecutionEvent),
    ) -> Result<ChatTurnExecutionReport, BootstrapError>;
    fn approve_run_with_control_and_observer(
        &self,
        run_id: &str,
        approval_id: &str,
        now: i64,
        interrupt_after_tool_step: Option<&AtomicBool>,
        observer: &mut dyn FnMut(ChatExecutionEvent),
    ) -> Result<ApprovalContinuationReport, BootstrapError>;
}

impl TuiBackend for App {
    fn list_session_summaries(&self) -> Result<Vec<SessionSummary>, BootstrapError> {
        App::list_session_summaries(self)
    }

    fn create_session_auto(&self, title: Option<&str>) -> Result<SessionSummary, BootstrapError> {
        App::create_session_auto(self, title)
    }

    fn update_session_preferences(
        &self,
        session_id: &str,
        patch: SessionPreferencesPatch,
    ) -> Result<SessionSummary, BootstrapError> {
        App::update_session_preferences(self, session_id, patch)
    }

    fn delete_session(&self, session_id: &str) -> Result<(), BootstrapError> {
        App::delete_session(self, session_id)
    }

    fn clear_session(
        &self,
        session_id: &str,
        title: Option<&str>,
    ) -> Result<SessionSummary, BootstrapError> {
        App::clear_session(self, session_id, title)
    }

    fn session_summary(&self, session_id: &str) -> Result<SessionSummary, BootstrapError> {
        App::session_summary(self, session_id)
    }

    fn session_transcript(
        &self,
        session_id: &str,
    ) -> Result<SessionTranscriptView, BootstrapError> {
        App::session_transcript(self, session_id)
    }

    fn pending_approvals(
        &self,
        session_id: &str,
    ) -> Result<Vec<SessionPendingApproval>, BootstrapError> {
        App::pending_approvals(self, session_id)
    }

    fn session_skills(&self, session_id: &str) -> Result<Vec<SessionSkillStatus>, BootstrapError> {
        App::session_skills(self, session_id)
    }

    fn enable_session_skill(
        &self,
        session_id: &str,
        skill_name: &str,
    ) -> Result<Vec<SessionSkillStatus>, BootstrapError> {
        App::enable_session_skill(self, session_id, skill_name)
    }

    fn disable_session_skill(
        &self,
        session_id: &str,
        skill_name: &str,
    ) -> Result<Vec<SessionSkillStatus>, BootstrapError> {
        App::disable_session_skill(self, session_id, skill_name)
    }

    fn latest_pending_approval(
        &self,
        session_id: &str,
        requested_approval_id: Option<&str>,
    ) -> Result<Option<SessionPendingApproval>, BootstrapError> {
        App::latest_pending_approval(self, session_id, requested_approval_id)
    }

    fn render_context(&self, session_id: &str) -> Result<String, BootstrapError> {
        App::render_context_state(self, session_id)
    }

    fn render_plan(&self, session_id: &str) -> Result<String, BootstrapError> {
        App::render_plan(self, session_id)
    }

    fn render_active_jobs(&self, session_id: &str) -> Result<String, BootstrapError> {
        App::render_session_background_jobs(self, session_id)
    }

    fn write_debug_bundle(&self, session_id: &str) -> Result<String, BootstrapError> {
        App::write_debug_bundle(self, session_id).map(|path| path.display().to_string())
    }

    fn compact_session(&self, session_id: &str) -> Result<SessionSummary, BootstrapError> {
        App::compact_session(self, session_id)
    }

    fn execute_chat_turn_with_control_and_observer(
        &self,
        session_id: &str,
        message: &str,
        now: i64,
        interrupt_after_tool_step: Option<&AtomicBool>,
        observer: &mut dyn FnMut(ChatExecutionEvent),
    ) -> Result<ChatTurnExecutionReport, BootstrapError> {
        App::execute_chat_turn_with_control_and_observer(
            self,
            session_id,
            message,
            now,
            interrupt_after_tool_step,
            observer,
        )
    }

    fn approve_run_with_control_and_observer(
        &self,
        run_id: &str,
        approval_id: &str,
        now: i64,
        interrupt_after_tool_step: Option<&AtomicBool>,
        observer: &mut dyn FnMut(ChatExecutionEvent),
    ) -> Result<ApprovalContinuationReport, BootstrapError> {
        App::approve_run_with_control_and_observer(
            self,
            run_id,
            approval_id,
            now,
            interrupt_after_tool_step,
            observer,
        )
    }
}

impl TuiBackend for DaemonClient {
    fn list_session_summaries(&self) -> Result<Vec<SessionSummary>, BootstrapError> {
        DaemonClient::list_session_summaries(self)
    }

    fn create_session_auto(&self, title: Option<&str>) -> Result<SessionSummary, BootstrapError> {
        DaemonClient::create_session_auto(self, title)
    }

    fn update_session_preferences(
        &self,
        session_id: &str,
        patch: SessionPreferencesPatch,
    ) -> Result<SessionSummary, BootstrapError> {
        DaemonClient::update_session_preferences(self, session_id, patch)
    }

    fn delete_session(&self, session_id: &str) -> Result<(), BootstrapError> {
        DaemonClient::delete_session(self, session_id)
    }

    fn clear_session(
        &self,
        session_id: &str,
        title: Option<&str>,
    ) -> Result<SessionSummary, BootstrapError> {
        DaemonClient::clear_session(self, session_id, title)
    }

    fn session_summary(&self, session_id: &str) -> Result<SessionSummary, BootstrapError> {
        DaemonClient::session_summary(self, session_id)
    }

    fn session_transcript(
        &self,
        session_id: &str,
    ) -> Result<SessionTranscriptView, BootstrapError> {
        DaemonClient::session_transcript(self, session_id)
    }

    fn pending_approvals(
        &self,
        session_id: &str,
    ) -> Result<Vec<SessionPendingApproval>, BootstrapError> {
        DaemonClient::pending_approvals(self, session_id)
    }

    fn session_skills(&self, session_id: &str) -> Result<Vec<SessionSkillStatus>, BootstrapError> {
        DaemonClient::session_skills(self, session_id)
    }

    fn enable_session_skill(
        &self,
        session_id: &str,
        skill_name: &str,
    ) -> Result<Vec<SessionSkillStatus>, BootstrapError> {
        DaemonClient::enable_session_skill(self, session_id, skill_name)
    }

    fn disable_session_skill(
        &self,
        session_id: &str,
        skill_name: &str,
    ) -> Result<Vec<SessionSkillStatus>, BootstrapError> {
        DaemonClient::disable_session_skill(self, session_id, skill_name)
    }

    fn latest_pending_approval(
        &self,
        session_id: &str,
        requested_approval_id: Option<&str>,
    ) -> Result<Option<SessionPendingApproval>, BootstrapError> {
        DaemonClient::latest_pending_approval(self, session_id, requested_approval_id)
    }

    fn render_context(&self, session_id: &str) -> Result<String, BootstrapError> {
        DaemonClient::render_context_state(self, session_id)
    }

    fn render_plan(&self, session_id: &str) -> Result<String, BootstrapError> {
        DaemonClient::render_plan(self, session_id)
    }

    fn render_active_jobs(&self, session_id: &str) -> Result<String, BootstrapError> {
        DaemonClient::render_session_background_jobs(self, session_id)
    }

    fn write_debug_bundle(&self, session_id: &str) -> Result<String, BootstrapError> {
        DaemonClient::write_debug_bundle(self, session_id)
    }

    fn compact_session(&self, session_id: &str) -> Result<SessionSummary, BootstrapError> {
        DaemonClient::compact_session(self, session_id)
    }

    fn execute_chat_turn_with_control_and_observer(
        &self,
        session_id: &str,
        message: &str,
        now: i64,
        interrupt_after_tool_step: Option<&AtomicBool>,
        observer: &mut dyn FnMut(ChatExecutionEvent),
    ) -> Result<ChatTurnExecutionReport, BootstrapError> {
        DaemonClient::execute_chat_turn_with_control_and_observer(
            self,
            session_id,
            message,
            now,
            interrupt_after_tool_step,
            observer,
        )
    }

    fn approve_run_with_control_and_observer(
        &self,
        run_id: &str,
        approval_id: &str,
        now: i64,
        interrupt_after_tool_step: Option<&AtomicBool>,
        observer: &mut dyn FnMut(ChatExecutionEvent),
    ) -> Result<ApprovalContinuationReport, BootstrapError> {
        DaemonClient::approve_run_with_control_and_observer(
            self,
            run_id,
            approval_id,
            now,
            interrupt_after_tool_step,
            observer,
        )
    }
}
