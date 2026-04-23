use crate::bootstrap::{
    AgentScheduleCreateOptions, AgentScheduleUpdatePatch, AgentScheduleView, App, BootstrapError,
    McpConnectorCreateOptions, McpConnectorUpdatePatch, McpConnectorView, SessionPendingApproval,
    SessionPreferencesPatch, SessionSkillStatus, SessionSummary, SessionTranscriptView,
    render_mcp_connector_view, render_mcp_connectors_view,
};
use crate::execution::{ApprovalContinuationReport, ChatExecutionEvent, ChatTurnExecutionReport};
use crate::http::client::DaemonClient;
use agent_runtime::tool::{
    KnowledgeReadInput, KnowledgeSearchInput, SessionReadInput, SessionSearchInput,
};
use std::sync::atomic::AtomicBool;
use std::time::{SystemTime, UNIX_EPOCH};

pub trait TuiBackend: Clone + Send + Sync + 'static {
    fn render_agents(&self) -> Result<String, BootstrapError>;
    fn render_agent(&self, identifier: Option<&str>) -> Result<String, BootstrapError>;
    fn select_agent(&self, identifier: &str) -> Result<String, BootstrapError>;
    fn create_agent(
        &self,
        name: &str,
        template_identifier: Option<&str>,
    ) -> Result<String, BootstrapError>;
    fn open_agent_home(&self, identifier: Option<&str>) -> Result<String, BootstrapError>;
    fn send_agent_message(
        &self,
        session_id: &str,
        target_agent_id: &str,
        message: &str,
    ) -> Result<String, BootstrapError>;
    fn grant_chain_continuation(
        &self,
        session_id: &str,
        chain_id: &str,
        reason: &str,
    ) -> Result<String, BootstrapError>;
    fn render_agent_schedules(&self) -> Result<String, BootstrapError>;
    fn render_agent_schedule(&self, id: &str) -> Result<String, BootstrapError>;
    fn load_agent_schedule(&self, id: &str) -> Result<AgentScheduleView, BootstrapError>;
    fn create_agent_schedule_with_options(
        &self,
        id: &str,
        options: AgentScheduleCreateOptions,
    ) -> Result<String, BootstrapError>;
    fn update_agent_schedule(
        &self,
        id: &str,
        patch: AgentScheduleUpdatePatch,
    ) -> Result<String, BootstrapError>;
    fn set_agent_schedule_enabled(&self, id: &str, enabled: bool)
    -> Result<String, BootstrapError>;
    fn create_agent_schedule(
        &self,
        id: &str,
        interval_seconds: u64,
        prompt: &str,
        agent_identifier: Option<&str>,
    ) -> Result<String, BootstrapError>;
    fn delete_agent_schedule(&self, id: &str) -> Result<String, BootstrapError>;
    fn render_mcp_connectors(&self) -> Result<String, BootstrapError>;
    fn render_mcp_connector(&self, id: &str) -> Result<String, BootstrapError>;
    fn load_mcp_connector(&self, id: &str) -> Result<McpConnectorView, BootstrapError>;
    fn create_mcp_connector(
        &self,
        id: &str,
        options: McpConnectorCreateOptions,
    ) -> Result<String, BootstrapError>;
    fn update_mcp_connector(
        &self,
        id: &str,
        patch: McpConnectorUpdatePatch,
    ) -> Result<String, BootstrapError>;
    fn set_mcp_connector_enabled(&self, id: &str, enabled: bool) -> Result<String, BootstrapError>;
    fn restart_mcp_connector(&self, id: &str) -> Result<String, BootstrapError>;
    fn delete_mcp_connector(&self, id: &str) -> Result<String, BootstrapError>;
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
    fn session_transcript_tail(
        &self,
        session_id: &str,
        max_entries: usize,
    ) -> Result<SessionTranscriptView, BootstrapError> {
        let mut transcript = self.session_transcript(session_id)?;
        if transcript.entries.len() > max_entries {
            let keep_from = transcript.entries.len().saturating_sub(max_entries);
            transcript.entries = transcript.entries.split_off(keep_from);
        }
        Ok(transcript)
    }
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
    fn render_system(&self, session_id: &str) -> Result<String, BootstrapError>;
    fn render_plan(&self, session_id: &str) -> Result<String, BootstrapError>;
    fn render_artifacts(&self, session_id: &str) -> Result<String, BootstrapError>;
    fn read_artifact(&self, session_id: &str, artifact_id: &str) -> Result<String, BootstrapError>;
    fn render_session_memory_search(
        &self,
        input: SessionSearchInput,
    ) -> Result<String, BootstrapError>;
    fn render_session_memory_read(&self, input: SessionReadInput)
    -> Result<String, BootstrapError>;
    fn render_knowledge_search(
        &self,
        input: KnowledgeSearchInput,
    ) -> Result<String, BootstrapError>;
    fn render_knowledge_read(&self, input: KnowledgeReadInput) -> Result<String, BootstrapError>;
    fn render_active_jobs(&self, session_id: &str) -> Result<String, BootstrapError>;
    fn render_active_run(&self, session_id: &str) -> Result<String, BootstrapError>;
    fn cancel_active_run(&self, session_id: &str, now: i64) -> Result<String, BootstrapError>;
    fn cancel_all_session_work(&self, session_id: &str, now: i64)
    -> Result<String, BootstrapError>;
    fn render_version_info(&self) -> Result<String, BootstrapError>;
    fn render_diagnostics_tail(&self, max_lines: Option<usize>) -> Result<String, BootstrapError>;
    fn update_runtime(&self, tag: Option<&str>) -> Result<String, BootstrapError>;
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
    fn render_agents(&self) -> Result<String, BootstrapError> {
        App::render_agents(self)
    }

    fn render_agent(&self, identifier: Option<&str>) -> Result<String, BootstrapError> {
        App::render_agent_profile(self, identifier)
    }

    fn select_agent(&self, identifier: &str) -> Result<String, BootstrapError> {
        let profile = App::select_agent_profile(self, identifier)?;
        Ok(format!("текущий агент: {} ({})", profile.name, profile.id))
    }

    fn create_agent(
        &self,
        name: &str,
        template_identifier: Option<&str>,
    ) -> Result<String, BootstrapError> {
        let profile = App::create_agent_from_template(self, name, template_identifier)?;
        Ok(format!(
            "создан агент {} ({}) из шаблона {}",
            profile.name,
            profile.id,
            profile.template_kind.as_str()
        ))
    }

    fn open_agent_home(&self, identifier: Option<&str>) -> Result<String, BootstrapError> {
        App::render_agent_home(self, identifier)
    }

    fn send_agent_message(
        &self,
        session_id: &str,
        target_agent_id: &str,
        message: &str,
    ) -> Result<String, BootstrapError> {
        App::send_session_agent_message(
            self,
            session_id,
            target_agent_id,
            message,
            unix_timestamp()?,
        )
    }

    fn grant_chain_continuation(
        &self,
        session_id: &str,
        chain_id: &str,
        reason: &str,
    ) -> Result<String, BootstrapError> {
        App::grant_session_chain_continuation(self, session_id, chain_id, reason, unix_timestamp()?)
    }

    fn render_agent_schedules(&self) -> Result<String, BootstrapError> {
        App::render_agent_schedules(self)
    }

    fn render_agent_schedule(&self, id: &str) -> Result<String, BootstrapError> {
        App::render_agent_schedule(self, id)
    }

    fn load_agent_schedule(&self, id: &str) -> Result<AgentScheduleView, BootstrapError> {
        App::agent_schedule_view(self, id)
    }

    fn create_agent_schedule_with_options(
        &self,
        id: &str,
        options: AgentScheduleCreateOptions,
    ) -> Result<String, BootstrapError> {
        let schedule = App::create_agent_schedule_with_options(self, id, options)?;
        Ok(format!(
            "создано расписание {} agent={} interval={}s",
            schedule.id, schedule.agent_profile_id, schedule.interval_seconds
        ))
    }

    fn update_agent_schedule(
        &self,
        id: &str,
        patch: AgentScheduleUpdatePatch,
    ) -> Result<String, BootstrapError> {
        let schedule = App::update_agent_schedule(self, id, patch)?;
        Ok(format!(
            "обновлено расписание {} agent={} mode={} delivery={} enabled={} interval={}s",
            schedule.id,
            schedule.agent_profile_id,
            schedule.mode.as_str(),
            schedule.delivery_mode.as_str(),
            schedule.enabled,
            schedule.interval_seconds
        ))
    }

    fn set_agent_schedule_enabled(
        &self,
        id: &str,
        enabled: bool,
    ) -> Result<String, BootstrapError> {
        let schedule = App::set_agent_schedule_enabled(self, id, enabled)?;
        Ok(format!(
            "расписание {} {}",
            schedule.id,
            if schedule.enabled {
                "включено"
            } else {
                "выключено"
            }
        ))
    }

    fn create_agent_schedule(
        &self,
        id: &str,
        interval_seconds: u64,
        prompt: &str,
        agent_identifier: Option<&str>,
    ) -> Result<String, BootstrapError> {
        let schedule = App::create_agent_schedule_with_options(
            self,
            id,
            AgentScheduleCreateOptions {
                agent_identifier: agent_identifier.map(str::to_string),
                prompt: prompt.to_string(),
                mode: agent_runtime::agent::AgentScheduleMode::Interval,
                delivery_mode: agent_runtime::agent::AgentScheduleDeliveryMode::FreshSession,
                target_session_id: None,
                interval_seconds,
                enabled: true,
            },
        )?;
        Ok(format!(
            "создано расписание {} agent={} interval={}s",
            schedule.id, schedule.agent_profile_id, schedule.interval_seconds
        ))
    }

    fn delete_agent_schedule(&self, id: &str) -> Result<String, BootstrapError> {
        if App::delete_agent_schedule(self, id)? {
            Ok(format!("расписание {id} удалено"))
        } else {
            Err(BootstrapError::MissingRecord {
                kind: "agent schedule",
                id: id.to_string(),
            })
        }
    }

    fn render_mcp_connectors(&self) -> Result<String, BootstrapError> {
        App::render_mcp_connectors(self)
    }

    fn render_mcp_connector(&self, id: &str) -> Result<String, BootstrapError> {
        App::render_mcp_connector(self, id)
    }

    fn load_mcp_connector(&self, id: &str) -> Result<McpConnectorView, BootstrapError> {
        App::mcp_connector(self, id)
    }

    fn create_mcp_connector(
        &self,
        id: &str,
        options: McpConnectorCreateOptions,
    ) -> Result<String, BootstrapError> {
        let connector = App::create_mcp_connector(self, id, options)?;
        Ok(format!("создан MCP коннектор {}", connector.id))
    }

    fn update_mcp_connector(
        &self,
        id: &str,
        patch: McpConnectorUpdatePatch,
    ) -> Result<String, BootstrapError> {
        let connector = App::update_mcp_connector(self, id, patch)?;
        Ok(format!("обновлён MCP коннектор {}", connector.id))
    }

    fn set_mcp_connector_enabled(&self, id: &str, enabled: bool) -> Result<String, BootstrapError> {
        let connector = App::set_mcp_connector_enabled(self, id, enabled)?;
        Ok(format!(
            "MCP коннектор {} {}",
            connector.id,
            if connector.enabled {
                "включен"
            } else {
                "выключен"
            }
        ))
    }

    fn restart_mcp_connector(&self, id: &str) -> Result<String, BootstrapError> {
        let connector = App::restart_mcp_connector(self, id)?;
        Ok(format!("MCP коннектор {} перезапущен", connector.id))
    }

    fn delete_mcp_connector(&self, id: &str) -> Result<String, BootstrapError> {
        if App::delete_mcp_connector(self, id)? {
            Ok(format!("MCP коннектор {id} удалён"))
        } else {
            Err(BootstrapError::MissingRecord {
                kind: "mcp connector",
                id: id.to_string(),
            })
        }
    }

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

    fn session_transcript_tail(
        &self,
        session_id: &str,
        max_entries: usize,
    ) -> Result<SessionTranscriptView, BootstrapError> {
        App::session_transcript_tail(self, session_id, max_entries)
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

    fn render_system(&self, session_id: &str) -> Result<String, BootstrapError> {
        App::render_system_blocks(self, session_id)
    }

    fn render_plan(&self, session_id: &str) -> Result<String, BootstrapError> {
        App::render_plan(self, session_id)
    }

    fn render_artifacts(&self, session_id: &str) -> Result<String, BootstrapError> {
        App::render_session_artifacts(self, session_id)
    }

    fn read_artifact(&self, session_id: &str, artifact_id: &str) -> Result<String, BootstrapError> {
        App::read_session_artifact(self, session_id, artifact_id)
    }

    fn render_session_memory_search(
        &self,
        input: SessionSearchInput,
    ) -> Result<String, BootstrapError> {
        App::render_session_memory_search(self, input)
    }

    fn render_session_memory_read(
        &self,
        input: SessionReadInput,
    ) -> Result<String, BootstrapError> {
        App::render_session_memory_read(self, input)
    }

    fn render_knowledge_search(
        &self,
        input: KnowledgeSearchInput,
    ) -> Result<String, BootstrapError> {
        App::render_knowledge_search(self, input)
    }

    fn render_knowledge_read(&self, input: KnowledgeReadInput) -> Result<String, BootstrapError> {
        App::render_knowledge_read(self, input)
    }

    fn render_active_jobs(&self, session_id: &str) -> Result<String, BootstrapError> {
        App::render_session_background_jobs(self, session_id)
    }

    fn render_active_run(&self, session_id: &str) -> Result<String, BootstrapError> {
        App::render_active_run(self, session_id)
    }

    fn cancel_active_run(&self, session_id: &str, now: i64) -> Result<String, BootstrapError> {
        App::cancel_latest_session_run(self, session_id, now)
    }

    fn cancel_all_session_work(
        &self,
        session_id: &str,
        now: i64,
    ) -> Result<String, BootstrapError> {
        App::cancel_all_session_work(self, session_id, now)
    }

    fn render_version_info(&self) -> Result<String, BootstrapError> {
        App::render_version_info(self)
    }

    fn render_diagnostics_tail(&self, max_lines: Option<usize>) -> Result<String, BootstrapError> {
        App::render_diagnostics_tail(
            self,
            max_lines.unwrap_or(self.config.runtime_limits.diagnostic_tail_lines),
        )
    }

    fn update_runtime(&self, tag: Option<&str>) -> Result<String, BootstrapError> {
        App::update_runtime_binary(self, tag)
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

fn unix_timestamp() -> Result<i64, BootstrapError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| BootstrapError::Stream(std::io::Error::other(error.to_string())))?;
    Ok(duration.as_secs() as i64)
}

impl TuiBackend for DaemonClient {
    fn render_agents(&self) -> Result<String, BootstrapError> {
        DaemonClient::render_agents(self)
    }

    fn render_agent(&self, identifier: Option<&str>) -> Result<String, BootstrapError> {
        DaemonClient::render_agent(self, identifier)
    }

    fn select_agent(&self, identifier: &str) -> Result<String, BootstrapError> {
        DaemonClient::select_agent(self, identifier)
    }

    fn create_agent(
        &self,
        name: &str,
        template_identifier: Option<&str>,
    ) -> Result<String, BootstrapError> {
        DaemonClient::create_agent(self, name, template_identifier)
    }

    fn open_agent_home(&self, identifier: Option<&str>) -> Result<String, BootstrapError> {
        DaemonClient::open_agent_home(self, identifier)
    }

    fn send_agent_message(
        &self,
        session_id: &str,
        target_agent_id: &str,
        message: &str,
    ) -> Result<String, BootstrapError> {
        DaemonClient::send_agent_message(self, session_id, target_agent_id, message)
    }

    fn grant_chain_continuation(
        &self,
        session_id: &str,
        chain_id: &str,
        reason: &str,
    ) -> Result<String, BootstrapError> {
        DaemonClient::grant_chain_continuation(self, session_id, chain_id, reason)
    }

    fn render_agent_schedules(&self) -> Result<String, BootstrapError> {
        DaemonClient::render_agent_schedules(self)
    }

    fn render_agent_schedule(&self, id: &str) -> Result<String, BootstrapError> {
        DaemonClient::render_agent_schedule(self, id)
    }

    fn load_agent_schedule(&self, id: &str) -> Result<AgentScheduleView, BootstrapError> {
        DaemonClient::resolve_agent_schedule(self, id)
    }

    fn create_agent_schedule_with_options(
        &self,
        id: &str,
        options: AgentScheduleCreateOptions,
    ) -> Result<String, BootstrapError> {
        DaemonClient::create_agent_schedule_with_options(self, id, options)
    }

    fn update_agent_schedule(
        &self,
        id: &str,
        patch: AgentScheduleUpdatePatch,
    ) -> Result<String, BootstrapError> {
        DaemonClient::update_agent_schedule(self, id, patch)
    }

    fn set_agent_schedule_enabled(
        &self,
        id: &str,
        enabled: bool,
    ) -> Result<String, BootstrapError> {
        DaemonClient::update_agent_schedule(
            self,
            id,
            AgentScheduleUpdatePatch {
                enabled: Some(enabled),
                ..AgentScheduleUpdatePatch::default()
            },
        )
    }

    fn create_agent_schedule(
        &self,
        id: &str,
        interval_seconds: u64,
        prompt: &str,
        agent_identifier: Option<&str>,
    ) -> Result<String, BootstrapError> {
        DaemonClient::create_agent_schedule(self, id, interval_seconds, prompt, agent_identifier)
    }

    fn delete_agent_schedule(&self, id: &str) -> Result<String, BootstrapError> {
        DaemonClient::delete_agent_schedule(self, id)
    }

    fn render_mcp_connectors(&self) -> Result<String, BootstrapError> {
        Ok(render_mcp_connectors_view(
            &DaemonClient::list_mcp_connectors(self)?,
        ))
    }

    fn render_mcp_connector(&self, id: &str) -> Result<String, BootstrapError> {
        Ok(render_mcp_connector_view(&DaemonClient::mcp_connector(
            self, id,
        )?))
    }

    fn load_mcp_connector(&self, id: &str) -> Result<McpConnectorView, BootstrapError> {
        DaemonClient::mcp_connector(self, id)
    }

    fn create_mcp_connector(
        &self,
        id: &str,
        options: McpConnectorCreateOptions,
    ) -> Result<String, BootstrapError> {
        let connector = DaemonClient::create_mcp_connector(self, id, options)?;
        Ok(format!("создан MCP коннектор {}", connector.id))
    }

    fn update_mcp_connector(
        &self,
        id: &str,
        patch: McpConnectorUpdatePatch,
    ) -> Result<String, BootstrapError> {
        let connector = DaemonClient::update_mcp_connector(self, id, patch)?;
        Ok(format!("обновлён MCP коннектор {}", connector.id))
    }

    fn set_mcp_connector_enabled(&self, id: &str, enabled: bool) -> Result<String, BootstrapError> {
        let connector = DaemonClient::update_mcp_connector(
            self,
            id,
            McpConnectorUpdatePatch {
                enabled: Some(enabled),
                ..McpConnectorUpdatePatch::default()
            },
        )?;
        Ok(format!(
            "MCP коннектор {} {}",
            connector.id,
            if connector.enabled {
                "включен"
            } else {
                "выключен"
            }
        ))
    }

    fn restart_mcp_connector(&self, id: &str) -> Result<String, BootstrapError> {
        let connector = DaemonClient::restart_mcp_connector(self, id)?;
        Ok(format!("MCP коннектор {} перезапущен", connector.id))
    }

    fn delete_mcp_connector(&self, id: &str) -> Result<String, BootstrapError> {
        DaemonClient::delete_mcp_connector(self, id)?;
        Ok(format!("MCP коннектор {id} удалён"))
    }

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

    fn session_transcript_tail(
        &self,
        session_id: &str,
        max_entries: usize,
    ) -> Result<SessionTranscriptView, BootstrapError> {
        DaemonClient::session_transcript_tail(self, session_id, max_entries)
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

    fn render_system(&self, session_id: &str) -> Result<String, BootstrapError> {
        DaemonClient::render_system_blocks(self, session_id)
    }

    fn render_plan(&self, session_id: &str) -> Result<String, BootstrapError> {
        DaemonClient::render_plan(self, session_id)
    }

    fn render_artifacts(&self, session_id: &str) -> Result<String, BootstrapError> {
        DaemonClient::render_session_artifacts(self, session_id)
    }

    fn read_artifact(&self, session_id: &str, artifact_id: &str) -> Result<String, BootstrapError> {
        DaemonClient::read_session_artifact(self, session_id, artifact_id)
    }

    fn render_session_memory_search(
        &self,
        input: SessionSearchInput,
    ) -> Result<String, BootstrapError> {
        DaemonClient::render_session_memory_search(self, input)
    }

    fn render_session_memory_read(
        &self,
        input: SessionReadInput,
    ) -> Result<String, BootstrapError> {
        DaemonClient::render_session_memory_read(self, input)
    }

    fn render_knowledge_search(
        &self,
        input: KnowledgeSearchInput,
    ) -> Result<String, BootstrapError> {
        DaemonClient::render_knowledge_search(self, input)
    }

    fn render_knowledge_read(&self, input: KnowledgeReadInput) -> Result<String, BootstrapError> {
        DaemonClient::render_knowledge_read(self, input)
    }

    fn render_active_jobs(&self, session_id: &str) -> Result<String, BootstrapError> {
        DaemonClient::render_session_background_jobs(self, session_id)
    }

    fn render_active_run(&self, session_id: &str) -> Result<String, BootstrapError> {
        DaemonClient::render_active_run(self, session_id)
    }

    fn cancel_active_run(&self, session_id: &str, _now: i64) -> Result<String, BootstrapError> {
        DaemonClient::cancel_active_run(self, session_id)
    }

    fn cancel_all_session_work(
        &self,
        session_id: &str,
        _now: i64,
    ) -> Result<String, BootstrapError> {
        DaemonClient::cancel_all_session_work(self, session_id)
    }

    fn render_version_info(&self) -> Result<String, BootstrapError> {
        DaemonClient::about(self)
    }

    fn render_diagnostics_tail(&self, max_lines: Option<usize>) -> Result<String, BootstrapError> {
        DaemonClient::render_diagnostics_tail(self, max_lines)
    }

    fn update_runtime(&self, tag: Option<&str>) -> Result<String, BootstrapError> {
        DaemonClient::update_runtime(self, tag)
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
