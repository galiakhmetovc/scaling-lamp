use super::*;
use crate::bootstrap::{
    AgentScheduleCreateOptions, AgentScheduleUpdatePatch, McpConnectorCreateOptions,
    McpConnectorUpdatePatch, SessionPreferencesPatch, render_mcp_connector_view,
    render_mcp_connectors_view,
};
use crate::help::{HelpTopic, parse_help_topic, render_command_usage_error, render_help};
use agent_runtime::mcp::McpConnectorTransport;
use agent_runtime::tool::{
    KnowledgeReadInput, KnowledgeReadMode, KnowledgeSearchInput, SessionReadInput, SessionReadMode,
    SessionSearchInput,
};
use std::collections::BTreeMap;

pub(super) trait ChatReplBackend {
    fn show_chat(&self, session_id: &str) -> Result<String, BootstrapError>;
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
    fn delete_agent_schedule(&self, id: &str) -> Result<String, BootstrapError>;
    fn render_mcp_connectors(&self) -> Result<String, BootstrapError>;
    fn render_mcp_connector(&self, id: &str) -> Result<String, BootstrapError>;
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
    fn render_version_info(&self) -> Result<String, BootstrapError>;
    fn render_diagnostics_tail(&self, max_lines: Option<usize>) -> Result<String, BootstrapError>;
    fn update_runtime(&self, tag: Option<&str>) -> Result<String, BootstrapError>;
    fn render_system(&self, session_id: &str) -> Result<String, BootstrapError>;
    fn render_context(&self, session_id: &str) -> Result<String, BootstrapError>;
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
    fn render_active_run(&self, session_id: &str) -> Result<String, BootstrapError>;
    fn cancel_active_run(&self, session_id: &str) -> Result<String, BootstrapError>;
    fn cancel_all_session_work(&self, session_id: &str) -> Result<String, BootstrapError>;
    fn render_active_jobs(&self, session_id: &str) -> Result<String, BootstrapError>;
    fn write_debug_bundle(&self, session_id: &str) -> Result<String, BootstrapError>;
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
    fn update_session_preferences(
        &self,
        session_id: &str,
        patch: SessionPreferencesPatch,
    ) -> Result<(), BootstrapError>;
    fn find_pending_approval(
        &self,
        session_id: &str,
        requested_approval_id: Option<&str>,
    ) -> Result<Option<ReplPendingApproval>, BootstrapError>;
    fn approve_run_with_observer(
        &self,
        run_id: &str,
        approval_id: &str,
        observer: &mut dyn FnMut(ChatExecutionEvent),
    ) -> Result<crate::execution::ApprovalContinuationReport, BootstrapError>;
    fn send_chat_with_observer(
        &self,
        session_id: &str,
        message: &str,
        observer: &mut dyn FnMut(ChatExecutionEvent),
    ) -> Result<ChatSendOutcome, BootstrapError>;
}

impl ChatReplBackend for App {
    fn show_chat(&self, session_id: &str) -> Result<String, BootstrapError> {
        render::show_chat(self, session_id)
    }

    fn render_agents(&self) -> Result<String, BootstrapError> {
        self.render_agents()
    }

    fn render_agent(&self, identifier: Option<&str>) -> Result<String, BootstrapError> {
        self.render_agent_profile(identifier)
    }

    fn select_agent(&self, identifier: &str) -> Result<String, BootstrapError> {
        let profile = self.select_agent_profile(identifier)?;
        Ok(format!("текущий агент: {} ({})", profile.name, profile.id))
    }

    fn create_agent(
        &self,
        name: &str,
        template_identifier: Option<&str>,
    ) -> Result<String, BootstrapError> {
        let profile = self.create_agent_from_template(name, template_identifier)?;
        Ok(format!(
            "создан агент {} ({}) из шаблона {}",
            profile.name,
            profile.id,
            profile.template_kind.as_str()
        ))
    }

    fn open_agent_home(&self, identifier: Option<&str>) -> Result<String, BootstrapError> {
        self.render_agent_home(identifier)
    }

    fn send_agent_message(
        &self,
        session_id: &str,
        target_agent_id: &str,
        message: &str,
    ) -> Result<String, BootstrapError> {
        self.send_session_agent_message(session_id, target_agent_id, message, unix_timestamp()?)
    }

    fn grant_chain_continuation(
        &self,
        session_id: &str,
        chain_id: &str,
        reason: &str,
    ) -> Result<String, BootstrapError> {
        self.grant_session_chain_continuation(session_id, chain_id, reason, unix_timestamp()?)
    }

    fn render_agent_schedules(&self) -> Result<String, BootstrapError> {
        App::render_agent_schedules(self)
    }

    fn render_agent_schedule(&self, id: &str) -> Result<String, BootstrapError> {
        App::render_agent_schedule(self, id)
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

    fn render_system(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.render_system_blocks(session_id)
    }

    fn render_context(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.render_context_state(session_id)
    }

    fn render_plan(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.render_plan(session_id)
    }

    fn render_artifacts(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.render_session_artifacts(session_id)
    }

    fn read_artifact(&self, session_id: &str, artifact_id: &str) -> Result<String, BootstrapError> {
        self.read_session_artifact(session_id, artifact_id)
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

    fn render_active_run(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.render_active_run(session_id)
    }

    fn cancel_active_run(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.cancel_latest_session_run(session_id, unix_timestamp()?)
    }

    fn cancel_all_session_work(&self, session_id: &str) -> Result<String, BootstrapError> {
        App::cancel_all_session_work(self, session_id, unix_timestamp()?)
    }

    fn render_active_jobs(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.render_session_background_jobs(session_id)
    }

    fn write_debug_bundle(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.write_debug_bundle(session_id)
            .map(|path| path.display().to_string())
    }

    fn render_session_skills(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.render_session_skills(session_id)
    }

    fn enable_session_skill(
        &self,
        session_id: &str,
        skill_name: &str,
    ) -> Result<String, BootstrapError> {
        self.enable_session_skill(session_id, skill_name)?;
        self.render_session_skills(session_id)
    }

    fn disable_session_skill(
        &self,
        session_id: &str,
        skill_name: &str,
    ) -> Result<String, BootstrapError> {
        self.disable_session_skill(session_id, skill_name)?;
        self.render_session_skills(session_id)
    }

    fn update_session_preferences(
        &self,
        session_id: &str,
        patch: SessionPreferencesPatch,
    ) -> Result<(), BootstrapError> {
        App::update_session_preferences(self, session_id, patch).map(|_| ())
    }

    fn find_pending_approval(
        &self,
        session_id: &str,
        requested_approval_id: Option<&str>,
    ) -> Result<Option<ReplPendingApproval>, BootstrapError> {
        find_pending_approval(self, session_id, requested_approval_id)
    }

    fn approve_run_with_observer(
        &self,
        run_id: &str,
        approval_id: &str,
        observer: &mut dyn FnMut(ChatExecutionEvent),
    ) -> Result<crate::execution::ApprovalContinuationReport, BootstrapError> {
        self.approve_run_with_observer(run_id, approval_id, unix_timestamp()?, observer)
    }

    fn send_chat_with_observer(
        &self,
        session_id: &str,
        message: &str,
        observer: &mut dyn FnMut(ChatExecutionEvent),
    ) -> Result<ChatSendOutcome, BootstrapError> {
        send_chat_outcome_with_observer(self, session_id, message, observer)
    }
}

impl ChatReplBackend for DaemonClient {
    fn show_chat(&self, session_id: &str) -> Result<String, BootstrapError> {
        render::show_chat_via_client(self, session_id)
    }

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

    fn render_version_info(&self) -> Result<String, BootstrapError> {
        DaemonClient::about(self)
    }

    fn render_diagnostics_tail(&self, max_lines: Option<usize>) -> Result<String, BootstrapError> {
        DaemonClient::render_diagnostics_tail(self, max_lines)
    }

    fn update_runtime(&self, tag: Option<&str>) -> Result<String, BootstrapError> {
        DaemonClient::update_runtime(self, tag)
    }

    fn render_system(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.render_system_blocks(session_id)
    }

    fn render_context(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.render_context_state(session_id)
    }

    fn render_plan(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.render_plan(session_id)
    }

    fn render_artifacts(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.render_session_artifacts(session_id)
    }

    fn read_artifact(&self, session_id: &str, artifact_id: &str) -> Result<String, BootstrapError> {
        self.read_session_artifact(session_id, artifact_id)
    }

    fn render_session_memory_search(
        &self,
        input: SessionSearchInput,
    ) -> Result<String, BootstrapError> {
        self.render_session_memory_search(input)
    }

    fn render_session_memory_read(
        &self,
        input: SessionReadInput,
    ) -> Result<String, BootstrapError> {
        self.render_session_memory_read(input)
    }

    fn render_knowledge_search(
        &self,
        input: KnowledgeSearchInput,
    ) -> Result<String, BootstrapError> {
        self.render_knowledge_search(input)
    }

    fn render_knowledge_read(&self, input: KnowledgeReadInput) -> Result<String, BootstrapError> {
        self.render_knowledge_read(input)
    }

    fn render_active_run(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.render_active_run(session_id)
    }

    fn cancel_active_run(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.cancel_active_run(session_id)
    }

    fn cancel_all_session_work(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.cancel_all_session_work(session_id)
    }

    fn render_active_jobs(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.render_session_background_jobs(session_id)
    }

    fn write_debug_bundle(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.write_debug_bundle(session_id)
    }

    fn render_session_skills(&self, session_id: &str) -> Result<String, BootstrapError> {
        render::render_session_skills_list(self.session_skills(session_id)?)
    }

    fn enable_session_skill(
        &self,
        session_id: &str,
        skill_name: &str,
    ) -> Result<String, BootstrapError> {
        let skills = self.enable_session_skill(session_id, skill_name)?;
        render::render_session_skills_list(skills)
    }

    fn disable_session_skill(
        &self,
        session_id: &str,
        skill_name: &str,
    ) -> Result<String, BootstrapError> {
        let skills = self.disable_session_skill(session_id, skill_name)?;
        render::render_session_skills_list(skills)
    }

    fn update_session_preferences(
        &self,
        session_id: &str,
        patch: SessionPreferencesPatch,
    ) -> Result<(), BootstrapError> {
        DaemonClient::update_session_preferences(self, session_id, patch).map(|_| ())
    }

    fn find_pending_approval(
        &self,
        session_id: &str,
        requested_approval_id: Option<&str>,
    ) -> Result<Option<ReplPendingApproval>, BootstrapError> {
        Ok(self
            .latest_pending_approval(session_id, requested_approval_id)?
            .map(|pending| ReplPendingApproval {
                run_id: pending.run_id,
                approval_id: pending.approval_id,
            }))
    }

    fn approve_run_with_observer(
        &self,
        run_id: &str,
        approval_id: &str,
        observer: &mut dyn FnMut(ChatExecutionEvent),
    ) -> Result<crate::execution::ApprovalContinuationReport, BootstrapError> {
        self.approve_run_with_control_and_observer(
            run_id,
            approval_id,
            unix_timestamp()?,
            None,
            observer,
        )
    }

    fn send_chat_with_observer(
        &self,
        session_id: &str,
        message: &str,
        observer: &mut dyn FnMut(ChatExecutionEvent),
    ) -> Result<ChatSendOutcome, BootstrapError> {
        send_chat_outcome_via_client_with_observer(self, session_id, message, observer)
    }
}

struct ReplRenderer<'a, W: Write> {
    output: &'a mut W,
    active_tool: Option<String>,
    reasoning_open: bool,
    assistant_open: bool,
    assistant_streamed_this_turn: bool,
}

impl<'a, W: Write> ReplRenderer<'a, W> {
    fn new(output: &'a mut W) -> Self {
        Self {
            output,
            active_tool: None,
            reasoning_open: false,
            assistant_open: false,
            assistant_streamed_this_turn: false,
        }
    }

    fn emit(&mut self, event: ChatExecutionEvent) -> Result<(), BootstrapError> {
        match event {
            ChatExecutionEvent::ReasoningDelta(delta) => self.write_reasoning_delta(&delta),
            ChatExecutionEvent::AssistantTextDelta(delta) => self.write_assistant_delta(&delta),
            ChatExecutionEvent::ProviderLoopProgress { .. } => Ok(()),
            ChatExecutionEvent::ToolStatus {
                tool_name,
                summary,
                status,
                ..
            } => self.write_tool_status(&tool_name, &summary, status),
        }
    }

    fn finish_turn(&mut self) -> Result<(), BootstrapError> {
        if self.reasoning_open {
            writeln!(self.output).map_err(BootstrapError::Stream)?;
            self.reasoning_open = false;
        }
        if self.assistant_open {
            writeln!(self.output).map_err(BootstrapError::Stream)?;
            self.assistant_open = false;
        }
        Ok(())
    }

    fn begin_turn(&mut self) {
        self.assistant_streamed_this_turn = false;
    }

    fn assistant_streamed_this_turn(&self) -> bool {
        self.assistant_streamed_this_turn
    }

    fn write_reasoning_delta(&mut self, delta: &str) -> Result<(), BootstrapError> {
        if self.assistant_open {
            writeln!(self.output).map_err(BootstrapError::Stream)?;
            self.assistant_open = false;
        }
        if self.reasoning_open {
            write!(self.output, "{delta}").map_err(BootstrapError::Stream)?;
        } else {
            write!(self.output, "размышления: {delta}").map_err(BootstrapError::Stream)?;
            self.reasoning_open = true;
        }
        self.output.flush().map_err(BootstrapError::Stream)
    }

    fn write_assistant_delta(&mut self, delta: &str) -> Result<(), BootstrapError> {
        if self.reasoning_open {
            writeln!(self.output).map_err(BootstrapError::Stream)?;
            self.reasoning_open = false;
        }
        if self.assistant_open {
            write!(self.output, "{delta}").map_err(BootstrapError::Stream)?;
        } else {
            write!(self.output, "ассистент: {delta}").map_err(BootstrapError::Stream)?;
            self.assistant_open = true;
        }
        self.assistant_streamed_this_turn = true;
        self.output.flush().map_err(BootstrapError::Stream)
    }

    fn write_tool_status(
        &mut self,
        tool_name: &str,
        summary: &str,
        status: ToolExecutionStatus,
    ) -> Result<(), BootstrapError> {
        self.finish_turn()?;
        let line = if summary.is_empty() || summary == tool_name {
            format!(
                "инструмент: {tool_name} | {}",
                translate_tool_status(&status)
            )
        } else {
            format!(
                "инструмент: {tool_name} | {} | {summary}",
                translate_tool_status(&status)
            )
        };
        match &self.active_tool {
            Some(current) if current == tool_name => {
                write!(self.output, "\x1b[1A\r\x1b[2K{line}\n").map_err(BootstrapError::Stream)?;
            }
            _ => {
                writeln!(self.output, "{line}").map_err(BootstrapError::Stream)?;
            }
        }
        if matches!(
            status,
            ToolExecutionStatus::Completed | ToolExecutionStatus::Failed
        ) {
            self.active_tool = None;
        } else {
            self.active_tool = Some(tool_name.to_string());
        }
        self.output.flush().map_err(BootstrapError::Stream)
    }
}

fn translate_tool_status(status: &ToolExecutionStatus) -> &'static str {
    match status {
        ToolExecutionStatus::Requested => "запрошен",
        ToolExecutionStatus::WaitingApproval => "ожидает апрува",
        ToolExecutionStatus::Approved => "подтверждён",
        ToolExecutionStatus::Running => "выполняется",
        ToolExecutionStatus::Completed => "завершён",
        ToolExecutionStatus::Failed => "ошибка",
    }
}

pub(super) fn run_chat_repl<R, W>(
    app: &App,
    session_id: &str,
    input: &mut R,
    output: &mut W,
) -> Result<(), BootstrapError>
where
    R: BufRead,
    W: Write,
{
    run_chat_repl_with_backend(app, session_id, input, output)
}

pub(super) fn run_chat_repl_with_backend<B, R, W>(
    backend: &B,
    session_id: &str,
    input: &mut R,
    output: &mut W,
) -> Result<(), BootstrapError>
where
    B: ChatReplBackend,
    R: BufRead,
    W: Write,
{
    writeln!(output, "{}", crate::about::short_version_label()).map_err(BootstrapError::Stream)?;
    writeln!(output, "чатовый режим session_id={session_id}").map_err(BootstrapError::Stream)?;
    writeln!(output, "{REPL_HELP}").map_err(BootstrapError::Stream)?;

    let mut line = String::new();
    let mut renderer = ReplRenderer::new(output);

    loop {
        write!(renderer.output, "> ").map_err(BootstrapError::Stream)?;
        renderer.output.flush().map_err(BootstrapError::Stream)?;

        line.clear();
        let bytes = read_repl_line(input, &mut line).map_err(BootstrapError::Stream)?;
        if bytes == 0 {
            renderer.finish_turn()?;
            writeln!(
                renderer.output,
                "выход из чатового режима session_id={session_id}"
            )
            .map_err(BootstrapError::Stream)?;
            return Ok(());
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let command_result = (|| -> Result<Option<()>, BootstrapError> {
            match canonical_repl_command(trimmed) {
                Some("/exit") => {
                    renderer.finish_turn()?;
                    writeln!(
                        renderer.output,
                        "выход из чатового режима session_id={session_id}"
                    )
                    .map_err(BootstrapError::Stream)?;
                    return Ok(Some(()));
                }
                Some("/help") => {
                    renderer.finish_turn()?;
                    let topic = parse_help_topic(split_command_arg(trimmed)).map_err(|reason| {
                        BootstrapError::Usage {
                            reason: render_command_usage_error("/help", reason.as_str()),
                        }
                    })?;
                    writeln!(renderer.output, "{}", render_help(topic))
                        .map_err(BootstrapError::Stream)?;
                }
                Some("/agents") => {
                    renderer.finish_turn()?;
                    let agents = backend.render_agents()?;
                    writeln!(renderer.output, "{agents}").map_err(BootstrapError::Stream)?;
                }
                Some("/agent") => {
                    renderer.finish_turn()?;
                    let message =
                        handle_agent_command(backend, session_id, split_command_arg(trimmed))?;
                    writeln!(renderer.output, "{message}").map_err(BootstrapError::Stream)?;
                }
                Some("/judge") => {
                    renderer.finish_turn()?;
                    let message = backend.send_agent_message(
                        session_id,
                        "judge",
                        require_arg(split_command_arg(trimmed).unwrap_or_default(), "/judge")?
                            .as_str(),
                    )?;
                    writeln!(renderer.output, "{message}").map_err(BootstrapError::Stream)?;
                }
                Some("/chain") => {
                    renderer.finish_turn()?;
                    let message =
                        handle_chain_command(backend, session_id, split_command_arg(trimmed))?;
                    writeln!(renderer.output, "{message}").map_err(BootstrapError::Stream)?;
                }
                Some("/schedules") => {
                    renderer.finish_turn()?;
                    let rendered = backend.render_agent_schedules()?;
                    writeln!(renderer.output, "{rendered}").map_err(BootstrapError::Stream)?;
                }
                Some("/schedule") => {
                    renderer.finish_turn()?;
                    let message = handle_schedule_command(backend, split_command_arg(trimmed))?;
                    writeln!(renderer.output, "{message}").map_err(BootstrapError::Stream)?;
                }
                Some("/mcp") => {
                    renderer.finish_turn()?;
                    let message = handle_mcp_command(backend, split_command_arg(trimmed))?;
                    writeln!(renderer.output, "{message}").map_err(BootstrapError::Stream)?;
                }
                Some("/version") => {
                    renderer.finish_turn()?;
                    let about = backend.render_version_info()?;
                    writeln!(renderer.output, "{about}").map_err(BootstrapError::Stream)?;
                }
                Some("/logs") => {
                    renderer.finish_turn()?;
                    let logs = backend.render_diagnostics_tail(parse_optional_positive_usize(
                        split_command_arg(trimmed),
                        "/logs",
                    )?)?;
                    writeln!(renderer.output, "{logs}").map_err(BootstrapError::Stream)?;
                }
                Some("/update") => {
                    renderer.finish_turn()?;
                    let message = backend.update_runtime(split_command_arg(trimmed))?;
                    writeln!(renderer.output, "{message}").map_err(BootstrapError::Stream)?;
                }
                Some("/show") => {
                    renderer.finish_turn()?;
                    let transcript = backend.show_chat(session_id)?;
                    writeln!(renderer.output, "{transcript}").map_err(BootstrapError::Stream)?;
                }
                Some("/system") => {
                    renderer.finish_turn()?;
                    let system = backend.render_system(session_id)?;
                    writeln!(renderer.output, "{system}").map_err(BootstrapError::Stream)?;
                }
                Some("/context") => {
                    renderer.finish_turn()?;
                    let context = backend.render_context(session_id)?;
                    writeln!(renderer.output, "{context}").map_err(BootstrapError::Stream)?;
                }
                Some("/plan") => {
                    renderer.finish_turn()?;
                    let plan = backend.render_plan(session_id)?;
                    writeln!(renderer.output, "{plan}").map_err(BootstrapError::Stream)?;
                }
                Some("/status") => {
                    renderer.finish_turn()?;
                    let run = backend.render_active_run(session_id)?;
                    writeln!(renderer.output, "{run}").map_err(BootstrapError::Stream)?;
                }
                Some("/processes") => {
                    renderer.finish_turn()?;
                    let run = backend.render_active_run(session_id)?;
                    writeln!(renderer.output, "{run}").map_err(BootstrapError::Stream)?;
                }
                Some("/pause") => {
                    renderer.finish_turn()?;
                    let message = backend.cancel_active_run(session_id)?;
                    writeln!(
                        renderer.output,
                        "пауза пока реализована как операторская остановка: {message}"
                    )
                    .map_err(BootstrapError::Stream)?;
                }
                Some("/stop") => {
                    renderer.finish_turn()?;
                    let message = backend.cancel_active_run(session_id)?;
                    writeln!(renderer.output, "{message}").map_err(BootstrapError::Stream)?;
                }
                Some("/cancel") => {
                    renderer.finish_turn()?;
                    let message = backend.cancel_all_session_work(session_id)?;
                    writeln!(renderer.output, "{message}").map_err(BootstrapError::Stream)?;
                }
                Some("/jobs") => {
                    renderer.finish_turn()?;
                    let jobs = backend.render_active_jobs(session_id)?;
                    writeln!(renderer.output, "{jobs}").map_err(BootstrapError::Stream)?;
                }
                Some("/artifacts") => {
                    renderer.finish_turn()?;
                    let artifacts = backend.render_artifacts(session_id)?;
                    writeln!(renderer.output, "{artifacts}").map_err(BootstrapError::Stream)?;
                }
                Some("/memory") => {
                    renderer.finish_turn()?;
                    let memory = handle_memory_command(backend, split_command_arg(trimmed))?;
                    writeln!(renderer.output, "{memory}").map_err(BootstrapError::Stream)?;
                }
                Some("/artifact") => {
                    renderer.finish_turn()?;
                    let artifact_id =
                        split_command_arg(trimmed).ok_or_else(|| BootstrapError::Usage {
                            reason: render_command_usage_error(
                                "/artifact",
                                "не хватает аргументов",
                            ),
                        })?;
                    let artifact = backend.read_artifact(session_id, artifact_id)?;
                    writeln!(renderer.output, "{artifact}").map_err(BootstrapError::Stream)?;
                }
                Some("/debug") => {
                    renderer.finish_turn()?;
                    let path = backend.write_debug_bundle(session_id)?;
                    writeln!(renderer.output, "отладочный пакет сохранён: {path}")
                        .map_err(BootstrapError::Stream)?;
                }
                Some("/settings") => {
                    renderer.finish_turn()?;
                    writeln!(renderer.output, "{}", render_help(HelpTopic::Settings))
                        .map_err(BootstrapError::Stream)?;
                }
                Some("/skills") => {
                    renderer.finish_turn()?;
                    let skills = backend.render_session_skills(session_id)?;
                    writeln!(renderer.output, "{skills}").map_err(BootstrapError::Stream)?;
                }
                Some("/completion") => {
                    renderer.finish_turn()?;
                    let value =
                        split_command_arg(trimmed).ok_or_else(|| BootstrapError::Usage {
                            reason: render_command_usage_error(
                                "/completion",
                                "не хватает аргументов",
                            ),
                        })?;
                    let completion_nudges = parse_completion_nudges(value)?;
                    backend.update_session_preferences(
                        session_id,
                        SessionPreferencesPatch {
                            completion_nudges: Some(completion_nudges),
                            ..SessionPreferencesPatch::default()
                        },
                    )?;
                    writeln!(
                        renderer.output,
                        "режим доводки: {}",
                        describe_completion_mode(completion_nudges)
                    )
                    .map_err(BootstrapError::Stream)?;
                }
                Some("/autoapprove") => {
                    renderer.finish_turn()?;
                    let value =
                        split_command_arg(trimmed).ok_or_else(|| BootstrapError::Usage {
                            reason: render_command_usage_error(
                                "/autoapprove",
                                "не хватает аргументов",
                            ),
                        })?;
                    let auto_approve = parse_auto_approve(value)?;
                    backend.update_session_preferences(
                        session_id,
                        SessionPreferencesPatch {
                            auto_approve: Some(auto_approve),
                            ..SessionPreferencesPatch::default()
                        },
                    )?;
                    writeln!(
                        renderer.output,
                        "автоапрув {}",
                        if auto_approve {
                            "включён"
                        } else {
                            "выключен"
                        }
                    )
                    .map_err(BootstrapError::Stream)?;
                }
                Some("/enable") => {
                    renderer.finish_turn()?;
                    let skill_name =
                        split_command_arg(trimmed).ok_or_else(|| BootstrapError::Usage {
                            reason: render_command_usage_error("/enable", "не хватает аргументов"),
                        })?;
                    let skills = backend.enable_session_skill(session_id, skill_name)?;
                    writeln!(renderer.output, "{skills}").map_err(BootstrapError::Stream)?;
                }
                Some("/disable") => {
                    renderer.finish_turn()?;
                    let skill_name =
                        split_command_arg(trimmed).ok_or_else(|| BootstrapError::Usage {
                            reason: render_command_usage_error("/disable", "не хватает аргументов"),
                        })?;
                    let skills = backend.disable_session_skill(session_id, skill_name)?;
                    writeln!(renderer.output, "{skills}").map_err(BootstrapError::Stream)?;
                }
                Some("/approve") => {
                    let requested = trimmed.split_whitespace().nth(1).map(ToString::to_string);
                    let current =
                        match backend.find_pending_approval(session_id, requested.as_deref())? {
                            Some(current) => current,
                            None => {
                                renderer.finish_turn()?;
                                writeln!(
                                    renderer.output,
                                    "для session_id={session_id} нет ожидающего апрува"
                                )
                                .map_err(BootstrapError::Stream)?;
                                return Ok(None);
                            }
                        };
                    let approval_id = current.approval_id.clone();
                    renderer.begin_turn();
                    let mut emit_error = None;
                    let mut emit = |event| {
                        if emit_error.is_none() {
                            emit_error = renderer.emit(event).err();
                        }
                    };
                    let report = backend.approve_run_with_observer(
                        &current.run_id,
                        &approval_id,
                        &mut emit,
                    )?;
                    if let Some(error) = emit_error {
                        return Err(error);
                    }
                    renderer.finish_turn()?;
                    if let Some(text) = report.output_text.as_deref() {
                        if text.is_empty() || renderer.assistant_streamed_this_turn() {
                            return Ok(None);
                        }
                        writeln!(renderer.output, "ассистент: {text}")
                            .map_err(BootstrapError::Stream)?;
                    }
                }
                _ => {
                    let message = trimmed;
                    if backend.find_pending_approval(session_id, None)?.is_some() {
                        renderer.finish_turn()?;
                        writeln!(
                            renderer.output,
                            "сначала завершите ожидающий апрув, потом отправляйте новое сообщение"
                        )
                        .map_err(BootstrapError::Stream)?;
                        return Ok(None);
                    }

                    renderer.begin_turn();
                    let mut emit_error = None;
                    let mut emit = |event| {
                        if emit_error.is_none() {
                            emit_error = renderer.emit(event).err();
                        }
                    };
                    match backend.send_chat_with_observer(session_id, message, &mut emit)? {
                        ChatSendOutcome::Completed { output_text, .. } => {
                            if let Some(error) = emit_error {
                                return Err(error);
                            }
                            renderer.finish_turn()?;
                            if output_text.is_empty() || renderer.assistant_streamed_this_turn() {
                                return Ok(None);
                            }
                            writeln!(renderer.output, "ассистент: {output_text}")
                                .map_err(BootstrapError::Stream)?;
                        }
                        ChatSendOutcome::WaitingApproval { approval_id, .. } => {
                            if let Some(error) = emit_error {
                                return Err(error);
                            }
                            let _ = approval_id;
                            renderer.finish_turn()?;
                        }
                    }
                }
            }
            Ok(None)
        })();

        match command_result {
            Ok(Some(())) => return Ok(()),
            Ok(None) => {}
            Err(BootstrapError::Usage { reason }) => {
                renderer.finish_turn()?;
                writeln!(renderer.output, "{reason}").map_err(BootstrapError::Stream)?;
            }
            Err(error) => return Err(error),
        }
    }
}

pub(super) fn send_chat_outcome(
    app: &App,
    session_id: &str,
    message: &str,
) -> Result<ChatSendOutcome, BootstrapError> {
    let mut observer = None;
    send_chat_outcome_internal(app, session_id, message, &mut observer)
}

pub(super) fn send_chat_outcome_with_observer(
    app: &App,
    session_id: &str,
    message: &str,
    observer: &mut dyn FnMut(ChatExecutionEvent),
) -> Result<ChatSendOutcome, BootstrapError> {
    let mut observer = Some(observer);
    send_chat_outcome_internal(app, session_id, message, &mut observer)
}

pub(super) fn send_chat_outcome_via_client(
    client: &DaemonClient,
    session_id: &str,
    message: &str,
) -> Result<ChatSendOutcome, BootstrapError> {
    let mut observer = None;
    send_chat_outcome_via_client_internal(client, session_id, message, &mut observer)
}

pub(super) fn send_chat_outcome_via_client_with_observer(
    client: &DaemonClient,
    session_id: &str,
    message: &str,
    observer: &mut dyn FnMut(ChatExecutionEvent),
) -> Result<ChatSendOutcome, BootstrapError> {
    let mut observer = Some(observer);
    send_chat_outcome_via_client_internal(client, session_id, message, &mut observer)
}

fn send_chat_outcome_internal(
    app: &App,
    session_id: &str,
    message: &str,
    observer: &mut Option<&mut dyn FnMut(ChatExecutionEvent)>,
) -> Result<ChatSendOutcome, BootstrapError> {
    let now = unix_timestamp()?;
    let run_id = format!("run-chat-{session_id}-{now}");
    let result = match observer.as_deref_mut() {
        Some(observer) => app.execute_chat_turn_with_observer(session_id, message, now, observer),
        None => app.execute_chat_turn(session_id, message, now),
    };
    let report = match result {
        Ok(report) => report,
        Err(BootstrapError::Execution(ExecutionError::ApprovalRequired {
            approval_id, ..
        })) => {
            return Ok(ChatSendOutcome::WaitingApproval {
                session_id: session_id.to_string(),
                run_id: Some(run_id),
                approval_id,
            });
        }
        Err(error) => return Err(error),
    };
    Ok(ChatSendOutcome::Completed {
        session_id: report.session_id,
        run_id: Some(report.run_id),
        response_id: report.response_id,
        output_text: report.output_text,
    })
}

fn send_chat_outcome_via_client_internal(
    client: &DaemonClient,
    session_id: &str,
    message: &str,
    observer: &mut Option<&mut dyn FnMut(ChatExecutionEvent)>,
) -> Result<ChatSendOutcome, BootstrapError> {
    let now = unix_timestamp()?;
    let result = match observer.as_deref_mut() {
        Some(observer) => client
            .execute_chat_turn_with_control_and_observer(session_id, message, now, None, observer),
        None => {
            let mut noop = |_| {};
            client.execute_chat_turn_with_control_and_observer(
                session_id, message, now, None, &mut noop,
            )
        }
    };
    let report = match result {
        Ok(report) => report,
        Err(BootstrapError::Execution(ExecutionError::ApprovalRequired {
            approval_id, ..
        })) => {
            return Ok(ChatSendOutcome::WaitingApproval {
                session_id: session_id.to_string(),
                run_id: None,
                approval_id,
            });
        }
        Err(error) => return Err(error),
    };
    Ok(ChatSendOutcome::Completed {
        session_id: report.session_id,
        run_id: Some(report.run_id),
        response_id: report.response_id,
        output_text: report.output_text,
    })
}

fn canonical_repl_command(raw: &str) -> Option<&'static str> {
    let command = raw
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim_end_matches(['\\', '/']);
    match command {
        "/exit" | "\\выход" => Some("/exit"),
        "/help" | "\\помощь" => Some("/help"),
        "/agents" | "\\агенты" => Some("/agents"),
        "/agent" | "\\агент" => Some("/agent"),
        "/schedules" | "\\расписания" => Some("/schedules"),
        "/schedule" | "\\расписание" => Some("/schedule"),
        "/mcp" | "\\mcp" => Some("/mcp"),
        "/version" | "/версия" | "\\версия" => Some("/version"),
        "/logs" | "/логи" | "\\логи" => Some("/logs"),
        "/update" | "/обновить" | "\\обновить" => Some("/update"),
        "/settings" | "\\настройки" => Some("/settings"),
        "/show" | "\\показать" => Some("/show"),
        "/system" | "/система" | "\\система" => Some("/system"),
        "/context" | "\\контекст" => Some("/context"),
        "/plan" | "\\план" => Some("/plan"),
        "/status" | "\\статус" => Some("/status"),
        "/processes" | "\\процессы" => Some("/processes"),
        "/pause" | "\\пауза" => Some("/pause"),
        "/stop" | "\\стоп" => Some("/stop"),
        "/cancel" | "\\отмена" => Some("/cancel"),
        "/jobs" | "\\задачи" => Some("/jobs"),
        "/memory" | "/память" | "\\память" => Some("/memory"),
        "/artifacts" | "/артефакты" | "\\артефакты" => Some("/artifacts"),
        "/artifact" | "/артефакт" | "\\артефакт" => Some("/artifact"),
        "/debug" | "\\отладка" => Some("/debug"),
        "/judge" | "/судья" | "\\судья" => Some("/judge"),
        "/chain" | "/цепочка" | "\\цепочка" => Some("/chain"),
        "/completion" | "\\доводка" => Some("/completion"),
        "/autoapprove" | "\\автоапрув" => Some("/autoapprove"),
        "/skills" | "\\скиллы" => Some("/skills"),
        "/enable" | "\\включить" => Some("/enable"),
        "/disable" | "\\выключить" => Some("/disable"),
        "/approve" | "\\апрув" => Some("/approve"),
        _ => None,
    }
}

fn handle_agent_command<B>(
    backend: &B,
    session_id: &str,
    raw: Option<&str>,
) -> Result<String, BootstrapError>
where
    B: ChatReplBackend,
{
    let raw = raw.unwrap_or_default().trim();
    let (action, tail) = match raw.split_once(' ') {
        Some((action, tail)) => (action.trim(), tail.trim()),
        None => (raw, ""),
    };

    match action {
        "показать" | "show" | "" => backend.render_agent(split_command_arg(raw)),
        "выбрать" | "select" => {
            backend.select_agent(split_command_arg(raw).ok_or_else(|| BootstrapError::Usage {
                reason: render_command_usage_error("/agent", "не хватает аргументов"),
            })?)
        }
        "создать" | "create" => {
            let spec = tail.trim();
            if spec.is_empty() {
                return Err(BootstrapError::Usage {
                    reason: render_command_usage_error("/agent", "не хватает аргументов"),
                });
            }
            let (name, template_identifier) = parse_agent_create_spec(spec)?;
            backend.create_agent(&name, template_identifier.as_deref())
        }
        "открыть" | "open" => backend.open_agent_home(split_command_arg(raw)),
        "написать" | "message" => {
            let (target_agent_id, message) =
                split_head_tail(tail).ok_or_else(|| BootstrapError::Usage {
                    reason: render_command_usage_error("/agent", "не хватает аргументов"),
                })?;
            backend.send_agent_message(session_id, target_agent_id, message)
        }
        _ => Err(BootstrapError::Usage {
            reason: render_command_usage_error(
                "/agent",
                "неизвестная подкоманда агента; ожидается показать|выбрать|создать|открыть|написать",
            ),
        }),
    }
}

fn handle_chain_command<B>(
    backend: &B,
    session_id: &str,
    raw: Option<&str>,
) -> Result<String, BootstrapError>
where
    B: ChatReplBackend,
{
    let raw = raw.unwrap_or_default().trim();
    let (action, tail) = match raw.split_once(' ') {
        Some((action, tail)) => (action.trim(), tail.trim()),
        None => (raw, ""),
    };

    match action {
        "продолжить" | "grant" | "continue" => {
            let (chain_id, reason) =
                split_head_tail(tail).ok_or_else(|| BootstrapError::Usage {
                    reason: render_command_usage_error("/chain", "не хватает аргументов"),
                })?;
            backend.grant_chain_continuation(session_id, chain_id, reason)
        }
        _ => Err(BootstrapError::Usage {
            reason: render_command_usage_error(
                "/chain",
                "неизвестная подкоманда цепочки; ожидается продолжить",
            ),
        }),
    }
}

fn split_head_tail(raw: &str) -> Option<(&str, &str)> {
    let (head, tail) = raw.split_once(' ')?;
    let head = head.trim();
    let tail = tail.trim();
    if head.is_empty() || tail.is_empty() {
        return None;
    }
    Some((head, tail))
}

fn require_arg(raw: &str, command: &str) -> Result<String, BootstrapError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(BootstrapError::Usage {
            reason: render_command_usage_error(command, "не хватает аргументов"),
        });
    }
    Ok(trimmed.to_string())
}

fn handle_schedule_command<B>(backend: &B, raw: Option<&str>) -> Result<String, BootstrapError>
where
    B: ChatReplBackend,
{
    let raw = raw.unwrap_or_default().trim();
    let (action, tail) = match raw.split_once(' ') {
        Some((action, tail)) => (action.trim(), tail.trim()),
        None => (raw, ""),
    };

    match action {
        "" => backend.render_agent_schedules(),
        "показать" | "show" => {
            backend.render_agent_schedule(split_command_arg(raw).ok_or_else(|| {
                BootstrapError::Usage {
                    reason: render_command_usage_error("/schedule", "не хватает аргументов"),
                }
            })?)
        }
        "создать" | "create" => {
            let spec = tail.trim();
            if spec.is_empty() {
                return Err(BootstrapError::Usage {
                    reason: render_command_usage_error("/schedule", "не хватает аргументов"),
                });
            }
            let (id, options) = parse_schedule_create_spec(spec)?;
            backend.create_agent_schedule_with_options(&id, options)
        }
        "изменить" | "edit" => {
            let spec = tail.trim();
            if spec.is_empty() {
                return Err(BootstrapError::Usage {
                    reason: render_command_usage_error("/schedule", "не хватает аргументов"),
                });
            }
            let (id, patch) = parse_schedule_edit_spec(spec)?;
            backend.update_agent_schedule(&id, patch)
        }
        "включить" | "enable" => backend.update_agent_schedule(
            split_command_arg(raw).ok_or_else(|| BootstrapError::Usage {
                reason: render_command_usage_error("/schedule", "не хватает аргументов"),
            })?,
            AgentScheduleUpdatePatch {
                enabled: Some(true),
                ..AgentScheduleUpdatePatch::default()
            },
        ),
        "выключить" | "disable" => backend.update_agent_schedule(
            split_command_arg(raw).ok_or_else(|| BootstrapError::Usage {
                reason: render_command_usage_error("/schedule", "не хватает аргументов"),
            })?,
            AgentScheduleUpdatePatch {
                enabled: Some(false),
                ..AgentScheduleUpdatePatch::default()
            },
        ),
        "удалить" | "delete" | "remove" => {
            backend.delete_agent_schedule(split_command_arg(raw).ok_or_else(|| {
                BootstrapError::Usage {
                    reason: render_command_usage_error("/schedule", "не хватает аргументов"),
                }
            })?)
        }
        _ => Err(BootstrapError::Usage {
            reason: render_command_usage_error(
                "/schedule",
                "неизвестная подкоманда расписания; ожидается показать|создать|изменить|включить|выключить|удалить",
            ),
        }),
    }
}

fn handle_mcp_command<B>(backend: &B, raw: Option<&str>) -> Result<String, BootstrapError>
where
    B: ChatReplBackend,
{
    let raw = raw.unwrap_or_default().trim();
    let (action, tail) = match raw.split_once(' ') {
        Some((action, tail)) => (action.trim(), tail.trim()),
        None => (raw, ""),
    };

    match action {
        "" => backend.render_mcp_connectors(),
        "показать" | "show" => {
            backend.render_mcp_connector(split_command_arg(raw).ok_or_else(|| {
                BootstrapError::Usage {
                    reason: render_command_usage_error("/mcp", "не хватает аргументов"),
                }
            })?)
        }
        "создать" | "create" => {
            let spec = require_arg(tail, "/mcp")?;
            let (id, options) = parse_mcp_create_spec(spec.as_str())?;
            backend.create_mcp_connector(&id, options)
        }
        "изменить" | "edit" => {
            let spec = require_arg(tail, "/mcp")?;
            let (id, patch) = parse_mcp_edit_spec(spec.as_str())?;
            backend.update_mcp_connector(&id, patch)
        }
        "включить" | "enable" => backend.set_mcp_connector_enabled(
            split_command_arg(raw).ok_or_else(|| BootstrapError::Usage {
                reason: render_command_usage_error("/mcp", "не хватает аргументов"),
            })?,
            true,
        ),
        "выключить" | "disable" => backend.set_mcp_connector_enabled(
            split_command_arg(raw).ok_or_else(|| BootstrapError::Usage {
                reason: render_command_usage_error("/mcp", "не хватает аргументов"),
            })?,
            false,
        ),
        "перезапустить" | "restart" => {
            backend.restart_mcp_connector(split_command_arg(raw).ok_or_else(|| {
                BootstrapError::Usage {
                    reason: render_command_usage_error("/mcp", "не хватает аргументов"),
                }
            })?)
        }
        "удалить" | "delete" | "remove" => {
            backend.delete_mcp_connector(split_command_arg(raw).ok_or_else(|| {
                BootstrapError::Usage {
                    reason: render_command_usage_error("/mcp", "не хватает аргументов"),
                }
            })?)
        }
        _ => Err(BootstrapError::Usage {
            reason: render_command_usage_error(
                "/mcp",
                "неизвестная подкоманда mcp; ожидается показать|создать|изменить|включить|выключить|перезапустить|удалить",
            ),
        }),
    }
}

fn handle_memory_command<B>(backend: &B, raw: Option<&str>) -> Result<String, BootstrapError>
where
    B: ChatReplBackend,
{
    let raw = raw.unwrap_or_default().trim();
    let (action, tail) = match raw.split_once(' ') {
        Some((action, tail)) => (action.trim(), tail.trim()),
        None => (raw, ""),
    };

    match action {
        "сессии" | "sessions" => backend.render_session_memory_search(SessionSearchInput {
            query: require_arg(tail, "/memory")?,
            limit: None,
            offset: Some(0),
            tiers: None,
            agent_identifier: None,
            updated_after: None,
            updated_before: None,
        }),
        "сессия" | "session" => {
            let session_id = require_arg(tail, "/memory")?;
            let (session_id, mode) = parse_memory_session_read(session_id.as_str())?;
            backend.render_session_memory_read(SessionReadInput {
                session_id,
                mode: Some(mode),
                cursor: None,
                max_items: None,
                max_bytes: None,
                include_tools: Some(true),
            })
        }
        "знания" | "knowledge" => backend.render_knowledge_search(KnowledgeSearchInput {
            query: require_arg(tail, "/memory")?,
            limit: None,
            offset: Some(0),
            kinds: None,
            roots: None,
        }),
        "файл" | "file" => {
            let value = require_arg(tail, "/memory")?;
            let (path, mode) = parse_memory_knowledge_read(value.as_str());
            backend.render_knowledge_read(KnowledgeReadInput {
                path,
                mode: Some(mode),
                cursor: None,
                max_bytes: None,
                max_lines: None,
            })
        }
        _ => Err(BootstrapError::Usage {
            reason: render_command_usage_error(
                "/memory",
                "неизвестная подкоманда памяти; ожидается сессии|сессия|знания|файл",
            ),
        }),
    }
}

fn parse_memory_session_read(raw: &str) -> Result<(String, SessionReadMode), BootstrapError> {
    let trimmed = raw.trim();
    let Some((session_id, maybe_mode)) = trimmed.split_once(' ') else {
        return Ok((trimmed.to_string(), SessionReadMode::Summary));
    };
    Ok((
        session_id.trim().to_string(),
        parse_session_read_mode(maybe_mode.trim())?,
    ))
}

fn parse_session_read_mode(raw: &str) -> Result<SessionReadMode, BootstrapError> {
    match raw {
        "" | "summary" | "сводка" => Ok(SessionReadMode::Summary),
        "timeline" | "таймлайн" => Ok(SessionReadMode::Timeline),
        "transcript" | "транскрипт" => Ok(SessionReadMode::Transcript),
        "artifacts" | "артефакты" => Ok(SessionReadMode::Artifacts),
        other => Err(BootstrapError::Usage {
            reason: render_command_usage_error(
                "/memory",
                &format!("неизвестный режим чтения сессии {other}"),
            ),
        }),
    }
}

fn parse_memory_knowledge_read(raw: &str) -> (String, KnowledgeReadMode) {
    let trimmed = raw.trim();
    if let Some((path, mode)) = trimmed.rsplit_once(' ') {
        let mode = match mode.trim() {
            "full" | "полный" => Some(KnowledgeReadMode::Full),
            "excerpt" | "выдержка" => Some(KnowledgeReadMode::Excerpt),
            _ => None,
        };
        if let Some(mode) = mode {
            return (path.trim().to_string(), mode);
        }
    }
    (trimmed.to_string(), KnowledgeReadMode::Excerpt)
}

fn parse_mcp_create_spec(raw: &str) -> Result<(String, McpConnectorCreateOptions), BootstrapError> {
    let (id, assignments) = split_head_tail(raw).ok_or_else(|| BootstrapError::Usage {
        reason: render_command_usage_error("/mcp", "не хватает аргументов"),
    })?;
    let fields = parse_assignment_fields(assignments, "/mcp")?;
    let command = required_assignment(&fields, "command", "/mcp")?;
    Ok((
        id.to_string(),
        McpConnectorCreateOptions {
            transport: McpConnectorTransport::Stdio,
            command,
            args: parse_mcp_args(fields.get("args").map(String::as_str)),
            env: parse_mcp_env(fields.get("env").map(String::as_str), "/mcp")?,
            cwd: parse_mcp_cwd(fields.get("cwd").map(String::as_str)),
            enabled: parse_mcp_enabled(fields.get("enabled").map(String::as_str))?,
        },
    ))
}

fn parse_mcp_edit_spec(raw: &str) -> Result<(String, McpConnectorUpdatePatch), BootstrapError> {
    let (id, assignments) = split_head_tail(raw).ok_or_else(|| BootstrapError::Usage {
        reason: render_command_usage_error("/mcp", "не хватает аргументов"),
    })?;
    let fields = parse_assignment_fields(assignments, "/mcp")?;
    if fields.is_empty() {
        return Err(BootstrapError::Usage {
            reason: render_command_usage_error("/mcp", "не указаны поля для изменения"),
        });
    }
    Ok((
        id.to_string(),
        McpConnectorUpdatePatch {
            command: optional_assignment(&fields, "command"),
            args: fields
                .get("args")
                .map(|value| parse_mcp_args(Some(value.as_str()))),
            env: if fields.contains_key("env") {
                Some(parse_mcp_env(
                    fields.get("env").map(String::as_str),
                    "/mcp",
                )?)
            } else {
                None
            },
            cwd: if fields.contains_key("cwd") {
                Some(parse_mcp_cwd(fields.get("cwd").map(String::as_str)))
            } else {
                None
            },
            enabled: parse_mcp_enabled_patch(fields.get("enabled").map(String::as_str))?,
        },
    ))
}

fn parse_assignment_fields(
    raw: &str,
    command: &str,
) -> Result<BTreeMap<String, String>, BootstrapError> {
    let mut fields = BTreeMap::new();
    for token in raw.split_whitespace() {
        let Some((key, value)) = token.split_once('=') else {
            return Err(BootstrapError::Usage {
                reason: render_command_usage_error(
                    command,
                    &format!("ожидается field=value, получено {token}"),
                ),
            });
        };
        fields.insert(key.trim().to_string(), value.to_string());
    }
    Ok(fields)
}

fn required_assignment(
    fields: &BTreeMap<String, String>,
    key: &str,
    command: &str,
) -> Result<String, BootstrapError> {
    let value = fields
        .get(key)
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| BootstrapError::Usage {
            reason: render_command_usage_error(command, &format!("не хватает {key}")),
        })?;
    Ok(value.to_string())
}

fn optional_assignment(fields: &BTreeMap<String, String>, key: &str) -> Option<String> {
    fields
        .get(key)
        .map(String::as_str)
        .map(str::trim)
        .map(ToString::to_string)
}

fn parse_mcp_args(value: Option<&str>) -> Vec<String> {
    value
        .unwrap_or_default()
        .split(',')
        .filter_map(|item| {
            let trimmed = item.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        })
        .collect()
}

fn parse_mcp_env(
    value: Option<&str>,
    command: &str,
) -> Result<BTreeMap<String, String>, BootstrapError> {
    let mut env = BTreeMap::new();
    for pair in value.unwrap_or_default().split(';') {
        let trimmed = pair.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some((key, raw_value)) = trimmed.split_once('=') else {
            return Err(BootstrapError::Usage {
                reason: render_command_usage_error(
                    command,
                    &format!("ожидается env KEY=VALUE, получено {trimmed}"),
                ),
            });
        };
        let key = key.trim();
        if key.is_empty() {
            return Err(BootstrapError::Usage {
                reason: render_command_usage_error(command, "ключ env не должен быть пустым"),
            });
        }
        env.insert(key.to_string(), raw_value.to_string());
    }
    Ok(env)
}

fn parse_mcp_cwd(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn parse_mcp_enabled(value: Option<&str>) -> Result<bool, BootstrapError> {
    match value.unwrap_or("true").trim() {
        "true" | "yes" | "1" | "on" => Ok(true),
        "false" | "no" | "0" | "off" => Ok(false),
        other => Err(BootstrapError::Usage {
            reason: render_command_usage_error(
                "/mcp",
                &format!("неподдерживаемый enabled {other}; ожидается true|false"),
            ),
        }),
    }
}

fn parse_mcp_enabled_patch(value: Option<&str>) -> Result<Option<bool>, BootstrapError> {
    match value {
        Some(value) => parse_mcp_enabled(Some(value)).map(Some),
        None => Ok(None),
    }
}

fn parse_agent_create_spec(raw: &str) -> Result<(String, Option<String>), BootstrapError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(BootstrapError::Usage {
            reason: render_command_usage_error("/agent", "не хватает аргументов"),
        });
    }

    for delimiter in [" из ", " from "] {
        if let Some((name, template)) = trimmed.split_once(delimiter) {
            let name = name.trim().to_string();
            let template = template.trim().to_string();
            if !name.is_empty() && !template.is_empty() {
                return Ok((name, Some(template)));
            }
        }
    }

    Ok((trimmed.to_string(), None))
}

fn parse_schedule_create_spec(
    raw: &str,
) -> Result<(String, AgentScheduleCreateOptions), BootstrapError> {
    let trimmed = raw.trim();
    let Some((head, prompt)) = trimmed.split_once("::") else {
        return Err(BootstrapError::Usage {
            reason: render_command_usage_error(
                "/schedule",
                "не хватает prompt; используйте формат с разделителем ::",
            ),
        });
    };
    let prompt = prompt.trim().to_string();
    if prompt.is_empty() {
        return Err(BootstrapError::Usage {
            reason: render_command_usage_error("/schedule", "prompt не должен быть пустым"),
        });
    }

    let parsed = parse_schedule_field_tokens(head)?;
    let Some(id) = parsed.id else {
        return Err(BootstrapError::Usage {
            reason: render_command_usage_error("/schedule", "не хватает id расписания"),
        });
    };
    let Some(interval_seconds) = parsed.interval_seconds else {
        return Err(BootstrapError::Usage {
            reason: render_command_usage_error("/schedule", "не хватает interval_seconds"),
        });
    };

    Ok((
        id,
        AgentScheduleCreateOptions {
            agent_identifier: parsed.agent_identifier,
            prompt,
            mode: parsed
                .mode
                .unwrap_or(agent_runtime::agent::AgentScheduleMode::Interval),
            delivery_mode: parsed
                .delivery_mode
                .unwrap_or(agent_runtime::agent::AgentScheduleDeliveryMode::FreshSession),
            target_session_id: parsed.target_session_id,
            interval_seconds,
            enabled: parsed.enabled.unwrap_or(true),
        },
    ))
}

fn parse_schedule_edit_spec(
    raw: &str,
) -> Result<(String, AgentScheduleUpdatePatch), BootstrapError> {
    let trimmed = raw.trim();
    let (head, prompt) = match trimmed.split_once("::") {
        Some((head, prompt)) => (head.trim(), Some(prompt.trim().to_string())),
        None => (trimmed, None),
    };
    let parsed = parse_schedule_field_tokens(head)?;
    let Some(id) = parsed.id else {
        return Err(BootstrapError::Usage {
            reason: render_command_usage_error("/schedule", "не хватает id расписания"),
        });
    };
    let patch = AgentScheduleUpdatePatch {
        agent_identifier: parsed.agent_identifier,
        prompt: prompt.filter(|value| !value.is_empty()),
        mode: parsed.mode,
        delivery_mode: parsed.delivery_mode,
        target_session_id: parsed.target_session_id,
        interval_seconds: parsed.interval_seconds,
        enabled: parsed.enabled,
    };
    if patch == AgentScheduleUpdatePatch::default() {
        return Err(BootstrapError::Usage {
            reason: render_command_usage_error(
                "/schedule",
                "для edit укажите хотя бы одно поле или новый prompt",
            ),
        });
    }
    Ok((id, patch))
}

#[derive(Default)]
struct ParsedScheduleFields {
    id: Option<String>,
    agent_identifier: Option<String>,
    mode: Option<agent_runtime::agent::AgentScheduleMode>,
    delivery_mode: Option<agent_runtime::agent::AgentScheduleDeliveryMode>,
    target_session_id: Option<String>,
    interval_seconds: Option<u64>,
    enabled: Option<bool>,
}

fn parse_schedule_field_tokens(raw: &str) -> Result<ParsedScheduleFields, BootstrapError> {
    let mut parsed = ParsedScheduleFields::default();
    for token in raw.split_whitespace() {
        if token.trim().is_empty() {
            continue;
        }
        if let Some((key, value)) = token.split_once('=') {
            let key = key.trim();
            let value = value.trim();
            if value.is_empty() {
                return Err(BootstrapError::Usage {
                    reason: render_command_usage_error(
                        "/schedule",
                        &format!("пустое значение для поля {key}"),
                    ),
                });
            }
            match key {
                "id" | "ид" => parsed.id = Some(value.to_string()),
                "agent" | "агент" => parsed.agent_identifier = Some(value.to_string()),
                "mode" | "режим" => parsed.mode = Some(parse_schedule_mode(value)?),
                "delivery" | "доставка" => {
                    parsed.delivery_mode = Some(parse_schedule_delivery_mode(value)?)
                }
                "session" | "сессия" => parsed.target_session_id = Some(value.to_string()),
                "interval" | "секунды" => {
                    parsed.interval_seconds = Some(parse_schedule_interval_seconds(value)?)
                }
                "enabled" | "включено" => {
                    parsed.enabled = Some(parse_schedule_enabled(value)?)
                }
                other => {
                    return Err(BootstrapError::Usage {
                        reason: render_command_usage_error(
                            "/schedule",
                            &format!("неизвестное поле {other}"),
                        ),
                    });
                }
            }
            continue;
        }

        if parsed.id.is_none() {
            parsed.id = Some(token.to_string());
            continue;
        }
        if parsed.interval_seconds.is_none() {
            parsed.interval_seconds = Some(parse_schedule_interval_seconds(token)?);
            continue;
        }
        if let Some(agent_identifier) = parse_schedule_agent_override(token)? {
            parsed.agent_identifier = Some(agent_identifier);
            continue;
        }
        return Err(BootstrapError::Usage {
            reason: render_command_usage_error(
                "/schedule",
                "лишние аргументы в спецификации расписания",
            ),
        });
    }
    Ok(parsed)
}

fn parse_schedule_agent_override(raw: &str) -> Result<Option<String>, BootstrapError> {
    for prefix in ["agent=", "агент="] {
        if let Some(value) = raw.strip_prefix(prefix) {
            let value = value.trim();
            if value.is_empty() {
                return Err(BootstrapError::Usage {
                    reason: render_command_usage_error(
                        "/schedule",
                        "после agent= должен быть id или имя агента",
                    ),
                });
            }
            return Ok(Some(value.to_string()));
        }
    }
    Ok(None)
}

fn parse_schedule_interval_seconds(raw: &str) -> Result<u64, BootstrapError> {
    let interval_seconds = raw.parse::<u64>().map_err(|_| BootstrapError::Usage {
        reason: render_command_usage_error(
            "/schedule",
            "interval_seconds должен быть положительным целым числом",
        ),
    })?;
    if interval_seconds == 0 {
        return Err(BootstrapError::Usage {
            reason: render_command_usage_error(
                "/schedule",
                "interval_seconds должен быть больше нуля",
            ),
        });
    }
    Ok(interval_seconds)
}

fn parse_schedule_mode(
    raw: &str,
) -> Result<agent_runtime::agent::AgentScheduleMode, BootstrapError> {
    match raw {
        "interval" => Ok(agent_runtime::agent::AgentScheduleMode::Interval),
        "after_completion" => Ok(agent_runtime::agent::AgentScheduleMode::AfterCompletion),
        "once" => Ok(agent_runtime::agent::AgentScheduleMode::Once),
        other => Err(BootstrapError::Usage {
            reason: render_command_usage_error(
                "/schedule",
                &format!("неподдерживаемый mode {other}; ожидается interval|after_completion|once"),
            ),
        }),
    }
}

fn parse_schedule_delivery_mode(
    raw: &str,
) -> Result<agent_runtime::agent::AgentScheduleDeliveryMode, BootstrapError> {
    match raw {
        "fresh_session" => Ok(agent_runtime::agent::AgentScheduleDeliveryMode::FreshSession),
        "existing_session" => Ok(agent_runtime::agent::AgentScheduleDeliveryMode::ExistingSession),
        other => Err(BootstrapError::Usage {
            reason: render_command_usage_error(
                "/schedule",
                &format!(
                    "неподдерживаемый delivery {other}; ожидается fresh_session|existing_session"
                ),
            ),
        }),
    }
}

fn parse_schedule_enabled(raw: &str) -> Result<bool, BootstrapError> {
    match raw {
        "true" | "yes" | "on" | "1" | "да" | "вкл" => Ok(true),
        "false" | "no" | "off" | "0" | "нет" | "выкл" => Ok(false),
        other => Err(BootstrapError::Usage {
            reason: render_command_usage_error(
                "/schedule",
                &format!("неподдерживаемый enabled {other}; ожидается true|false"),
            ),
        }),
    }
}

fn split_command_arg(raw: &str) -> Option<&str> {
    raw.split_once(char::is_whitespace)
        .map(|(_, rest)| rest.trim())
        .filter(|value| !value.is_empty())
}

fn parse_optional_positive_usize(
    raw: Option<&str>,
    command: &str,
) -> Result<Option<usize>, BootstrapError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let value = raw.parse::<usize>().map_err(|_| BootstrapError::Usage {
        reason: render_command_usage_error(command, "ожидается положительное целое число"),
    })?;
    if value == 0 {
        return Err(BootstrapError::Usage {
            reason: render_command_usage_error(command, "значение должно быть больше нуля"),
        });
    }
    Ok(Some(value))
}

fn parse_completion_nudges(raw: &str) -> Result<Option<u32>, BootstrapError> {
    let trimmed = raw.trim();
    if matches!(trimmed, "off" | "выкл" | "disable") {
        return Ok(None);
    }
    trimmed
        .parse::<u32>()
        .map(Some)
        .map_err(|_| BootstrapError::Usage {
            reason: render_command_usage_error(
                "/completion",
                &format!(
                    "неподдерживаемый режим доводки {trimmed}; ожидается выкл или неотрицательное число"
                ),
            ),
        })
}

fn describe_completion_mode(completion_nudges: Option<u32>) -> String {
    match completion_nudges {
        None => "выключен".to_string(),
        Some(0) => "включён: после первой ранней остановки сразу нужен апрув оператора".to_string(),
        Some(value) => format!("включён: {value} автоматических пинка перед апрувом"),
    }
}

fn parse_auto_approve(raw: &str) -> Result<bool, BootstrapError> {
    match raw.trim() {
        "on" | "1" | "yes" | "да" | "вкл" | "enable" => Ok(true),
        "off" | "0" | "no" | "нет" | "выкл" | "disable" => Ok(false),
        value => Err(BootstrapError::Usage {
            reason: render_command_usage_error(
                "/autoapprove",
                &format!("неподдерживаемый режим автоапрува {value}; ожидается вкл|выкл"),
            ),
        }),
    }
}

fn find_pending_approval(
    app: &App,
    session_id: &str,
    requested_approval_id: Option<&str>,
) -> Result<Option<ReplPendingApproval>, BootstrapError> {
    Ok(app
        .latest_pending_approval(session_id, requested_approval_id)?
        .map(|pending| ReplPendingApproval {
            run_id: pending.run_id,
            approval_id: pending.approval_id,
        }))
}
