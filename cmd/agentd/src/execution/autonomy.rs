use super::*;
use crate::agents;
use agent_persistence::{AgentProfileRecord, AgentScheduleRecord};
use agent_runtime::agent::{
    AgentProfile, AgentSchedule, AgentScheduleDeliveryMode, AgentScheduleInit, AgentScheduleMode,
    AgentTemplateKind,
};
use agent_runtime::tool::{
    AgentCreateInput, AgentCreateOutput, AgentListInput, AgentListOutput, AgentReadInput,
    AgentReadOutput, AgentSummaryOutput, ContinueLaterInput, ContinueLaterOutput,
    ScheduleCreateInput, ScheduleCreateOutput, ScheduleDeleteInput, ScheduleDeleteOutput,
    ScheduleListInput, ScheduleListOutput, ScheduleReadInput, ScheduleReadOutput,
    ScheduleUpdateInput, ScheduleUpdateOutput, ScheduleViewOutput, ToolError,
};
use std::path::{Path, PathBuf};

const DEFAULT_AGENT_LIST_LIMIT: usize = 100;
const MAX_AGENT_LIST_LIMIT: usize = 1_000;
const DEFAULT_SCHEDULE_LIST_LIMIT: usize = 100;
const MAX_SCHEDULE_LIST_LIMIT: usize = 1_000;

impl ExecutionService {
    pub(crate) fn list_tool_agents(
        &self,
        store: &PersistenceStore,
        input: &AgentListInput,
    ) -> Result<AgentListOutput, ExecutionError> {
        let mut agents = store
            .list_agent_profiles()
            .map_err(ExecutionError::Store)?
            .into_iter()
            .map(AgentProfile::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ExecutionError::RecordConversion)?;
        agents.sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));
        let (offset, limit, next_offset) = normalized_pagination(
            agents.len(),
            input.offset,
            input.limit,
            DEFAULT_AGENT_LIST_LIMIT,
            MAX_AGENT_LIST_LIMIT,
        );
        let end = offset.saturating_add(limit).min(agents.len());
        let page = agents[offset..end]
            .iter()
            .map(agent_summary_output)
            .collect::<Vec<_>>();
        Ok(AgentListOutput {
            agents: page,
            truncated: next_offset.is_some(),
            offset,
            limit,
            total_agents: agents.len(),
            next_offset,
        })
    }

    pub(crate) fn read_tool_agent(
        &self,
        store: &PersistenceStore,
        input: &AgentReadInput,
    ) -> Result<AgentReadOutput, ExecutionError> {
        let agent = resolve_agent_profile_by_identifier(store, &input.identifier)?;
        Ok(AgentReadOutput {
            agent: agent_summary_output(&agent),
            allowed_tools: agent.allowed_tools,
        })
    }

    pub(crate) fn create_tool_agent(
        &self,
        store: &PersistenceStore,
        session_id: &str,
        input: &AgentCreateInput,
        now: i64,
    ) -> Result<AgentCreateOutput, ExecutionError> {
        let session = load_session_or_internal(store, session_id)?;
        let template = resolve_tool_agent_template(store, &session, input)?;
        let template_fallback = agents::builtin_template(&template.id).unwrap_or(
            agents::builtin_template(agents::DEFAULT_AGENT_ID)
                .expect("built-in default agent template must exist"),
        );
        let base_id = agents::normalize_agent_id(&input.name);
        let agent_id = next_available_agent_id(store, &base_id)?;
        let agent_home = agents::agent_home(&self.config.data_dir, &agent_id);
        agents::clone_agent_home(
            &template.agent_home,
            &agent_home,
            template_fallback.system_md,
            template_fallback.agents_md,
        )
        .map_err(io_agent_tool_error)?;
        let profile = AgentProfile::new_with_provenance(
            &agent_id,
            input.name.trim(),
            AgentTemplateKind::Custom,
            &agent_home,
            template.allowed_tools.clone(),
            Some(template.id.clone()),
            Some(session.id.clone()),
            Some(session.agent_profile_id.clone()),
            now,
            now,
        )
        .map_err(agent_tool_validation_error)?;
        store
            .put_agent_profile(
                &AgentProfileRecord::try_from(&profile)
                    .map_err(ExecutionError::RecordConversion)?,
            )
            .map_err(ExecutionError::Store)?;
        Ok(AgentCreateOutput {
            agent: agent_summary_output(&profile),
        })
    }

    pub(crate) fn continue_later_tool(
        &self,
        store: &PersistenceStore,
        session_id: &str,
        input: &ContinueLaterInput,
        now: i64,
    ) -> Result<ContinueLaterOutput, ExecutionError> {
        let session = load_session_or_internal(store, session_id)?;
        let handoff_payload = input.handoff_payload.trim();
        if handoff_payload.is_empty() {
            return Err(invalid_agent_tool(
                "continue_later handoff_payload must not be empty".to_string(),
            ));
        }
        let delivery_mode = input
            .delivery_mode
            .unwrap_or(AgentScheduleDeliveryMode::ExistingSession);
        let target_session_id = validate_schedule_target_session(
            store,
            &session.agent_profile_id,
            delivery_mode,
            None,
            Some(session.id.as_str()),
        )?;
        let schedule_id =
            next_available_schedule_id(store, &format!("continue-later-{}", session.id))?;
        let delay_seconds_i64 = i64::try_from(input.delay_seconds).unwrap_or(i64::MAX);
        let schedule = AgentSchedule::new(AgentScheduleInit {
            id: schedule_id,
            agent_profile_id: session.agent_profile_id.clone(),
            workspace_root: current_workspace_root(self.workspace.root.as_path()),
            prompt: continue_later_prompt(handoff_payload),
            mode: AgentScheduleMode::Once,
            delivery_mode,
            target_session_id,
            interval_seconds: input.delay_seconds,
            next_fire_at: now.saturating_add(delay_seconds_i64),
            enabled: true,
            last_triggered_at: None,
            last_finished_at: None,
            last_session_id: None,
            last_job_id: None,
            last_result: None,
            last_error: None,
            created_at: now,
            updated_at: now,
        })
        .map_err(agent_tool_validation_error)?;
        store
            .put_agent_schedule(&AgentScheduleRecord::from(&schedule))
            .map_err(ExecutionError::Store)?;
        Ok(ContinueLaterOutput {
            schedule: schedule_view_output(&schedule),
        })
    }

    pub(crate) fn list_tool_schedules(
        &self,
        store: &PersistenceStore,
        input: &ScheduleListInput,
    ) -> Result<ScheduleListOutput, ExecutionError> {
        let current_workspace = current_workspace_root(self.workspace.root.as_path());
        let agent_filter = match input
            .agent_identifier
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some(identifier) => Some(resolve_agent_profile_by_identifier(store, identifier)?.id),
            None => None,
        };
        let mut schedules = store
            .list_agent_schedules()
            .map_err(ExecutionError::Store)?
            .into_iter()
            .map(AgentSchedule::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ExecutionError::RecordConversion)?;
        schedules.retain(|schedule| schedule.workspace_root == current_workspace);
        if let Some(agent_profile_id) = agent_filter.as_deref() {
            schedules.retain(|schedule| schedule.agent_profile_id == agent_profile_id);
        }
        schedules.sort_by(|left, right| left.id.cmp(&right.id));
        let (offset, limit, next_offset) = normalized_pagination(
            schedules.len(),
            input.offset,
            input.limit,
            DEFAULT_SCHEDULE_LIST_LIMIT,
            MAX_SCHEDULE_LIST_LIMIT,
        );
        let end = offset.saturating_add(limit).min(schedules.len());
        let page = schedules[offset..end]
            .iter()
            .map(schedule_view_output)
            .collect::<Vec<_>>();
        Ok(ScheduleListOutput {
            schedules: page,
            truncated: next_offset.is_some(),
            offset,
            limit,
            total_schedules: schedules.len(),
            next_offset,
        })
    }

    pub(crate) fn read_tool_schedule(
        &self,
        store: &PersistenceStore,
        input: &ScheduleReadInput,
    ) -> Result<ScheduleReadOutput, ExecutionError> {
        let schedule = load_schedule_for_current_workspace(self, store, &input.id)?;
        Ok(ScheduleReadOutput {
            schedule: schedule_view_output(&schedule),
        })
    }

    pub(crate) fn create_tool_schedule(
        &self,
        store: &PersistenceStore,
        session_id: &str,
        input: &ScheduleCreateInput,
        now: i64,
    ) -> Result<ScheduleCreateOutput, ExecutionError> {
        let session = load_session_or_internal(store, session_id)?;
        let agent = resolve_agent_for_session(
            store,
            session.agent_profile_id.as_str(),
            input.agent_identifier.as_deref(),
        )?;
        let mode = input.mode.unwrap_or(AgentScheduleMode::Interval);
        let delivery_mode = input
            .delivery_mode
            .unwrap_or(AgentScheduleDeliveryMode::FreshSession);
        let target_session_id = validate_schedule_target_session(
            store,
            &agent.id,
            delivery_mode,
            input.target_session_id.clone(),
            Some(session.id.as_str()),
        )?;
        let schedule = AgentSchedule::new(AgentScheduleInit {
            id: input.id.clone(),
            agent_profile_id: agent.id,
            workspace_root: current_workspace_root(self.workspace.root.as_path()),
            prompt: input.prompt.clone(),
            mode,
            delivery_mode,
            target_session_id,
            interval_seconds: input.interval_seconds,
            next_fire_at: now,
            enabled: input.enabled.unwrap_or(true),
            last_triggered_at: None,
            last_finished_at: None,
            last_session_id: None,
            last_job_id: None,
            last_result: None,
            last_error: None,
            created_at: now,
            updated_at: now,
        })
        .map_err(agent_tool_validation_error)?;
        store
            .put_agent_schedule(&AgentScheduleRecord::from(&schedule))
            .map_err(ExecutionError::Store)?;
        Ok(ScheduleCreateOutput {
            schedule: schedule_view_output(&schedule),
        })
    }

    pub(crate) fn update_tool_schedule(
        &self,
        store: &PersistenceStore,
        session_id: &str,
        input: &ScheduleUpdateInput,
        now: i64,
    ) -> Result<ScheduleUpdateOutput, ExecutionError> {
        let session = load_session_or_internal(store, session_id)?;
        let current = load_schedule_for_current_workspace(self, store, &input.id)?;
        let agent = resolve_agent_for_session(
            store,
            session.agent_profile_id.as_str(),
            input
                .agent_identifier
                .as_deref()
                .or(Some(current.agent_profile_id.as_str())),
        )?;
        let mode = input.mode.unwrap_or(current.mode);
        let delivery_mode = input.delivery_mode.unwrap_or(current.delivery_mode);
        let target_session_id = validate_schedule_target_session(
            store,
            &agent.id,
            delivery_mode,
            input
                .target_session_id
                .clone()
                .or(current.target_session_id.clone()),
            None,
        )?;
        let updated = AgentSchedule::new(AgentScheduleInit {
            id: current.id.clone(),
            agent_profile_id: agent.id,
            workspace_root: current.workspace_root.clone(),
            prompt: input.prompt.clone().unwrap_or(current.prompt.clone()),
            mode,
            delivery_mode,
            target_session_id,
            interval_seconds: input.interval_seconds.unwrap_or(current.interval_seconds),
            next_fire_at: current.next_fire_at,
            enabled: input.enabled.unwrap_or(current.enabled),
            last_triggered_at: current.last_triggered_at,
            last_finished_at: current.last_finished_at,
            last_session_id: current.last_session_id.clone(),
            last_job_id: current.last_job_id.clone(),
            last_result: current.last_result.clone(),
            last_error: current.last_error.clone(),
            created_at: current.created_at,
            updated_at: now,
        })
        .map_err(agent_tool_validation_error)?;
        store
            .put_agent_schedule(&AgentScheduleRecord::from(&updated))
            .map_err(ExecutionError::Store)?;
        Ok(ScheduleUpdateOutput {
            schedule: schedule_view_output(&updated),
        })
    }

    pub(crate) fn delete_tool_schedule(
        &self,
        store: &PersistenceStore,
        input: &ScheduleDeleteInput,
    ) -> Result<ScheduleDeleteOutput, ExecutionError> {
        let _ = load_schedule_for_current_workspace(self, store, &input.id)?;
        let deleted = store
            .delete_agent_schedule(&input.id)
            .map_err(ExecutionError::Store)?;
        Ok(ScheduleDeleteOutput {
            id: input.id.clone(),
            deleted,
        })
    }
}

fn load_session_or_internal(
    store: &PersistenceStore,
    session_id: &str,
) -> Result<Session, ExecutionError> {
    Session::try_from(
        store
            .get_session(session_id)
            .map_err(ExecutionError::Store)?
            .ok_or_else(|| ExecutionError::MissingSession {
                id: session_id.to_string(),
            })?,
    )
    .map_err(ExecutionError::RecordConversion)
}

fn resolve_agent_profile_by_identifier(
    store: &PersistenceStore,
    identifier: &str,
) -> Result<AgentProfile, ExecutionError> {
    if let Some(record) = store
        .get_agent_profile(identifier)
        .map_err(ExecutionError::Store)?
    {
        return AgentProfile::try_from(record).map_err(ExecutionError::RecordConversion);
    }
    let found = store
        .list_agent_profiles()
        .map_err(ExecutionError::Store)?
        .into_iter()
        .find(|record| record.name.eq_ignore_ascii_case(identifier))
        .ok_or_else(|| invalid_agent_tool(format!("agent {identifier} not found")))?;
    AgentProfile::try_from(found).map_err(ExecutionError::RecordConversion)
}

fn resolve_agent_for_session(
    store: &PersistenceStore,
    session_agent_profile_id: &str,
    requested_identifier: Option<&str>,
) -> Result<AgentProfile, ExecutionError> {
    match requested_identifier
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(identifier) => resolve_agent_profile_by_identifier(store, identifier),
        None => resolve_agent_profile_by_identifier(store, session_agent_profile_id),
    }
}

fn resolve_tool_agent_template(
    store: &PersistenceStore,
    session: &Session,
    input: &AgentCreateInput,
) -> Result<AgentProfile, ExecutionError> {
    let identifier = input
        .template_identifier
        .as_deref()
        .unwrap_or(session.agent_profile_id.as_str());
    let template = resolve_agent_profile_by_identifier(store, identifier)?;
    if template.template_kind == AgentTemplateKind::Custom
        && template.id != session.agent_profile_id
    {
        return Err(invalid_agent_tool(
            "agent_create templates must be built-in or the current session agent".to_string(),
        ));
    }
    Ok(template)
}

fn next_available_agent_id(
    store: &PersistenceStore,
    base_id: &str,
) -> Result<String, ExecutionError> {
    let mut candidate = base_id.to_string();
    let mut suffix = 2usize;
    while store
        .get_agent_profile(&candidate)
        .map_err(ExecutionError::Store)?
        .is_some()
    {
        candidate = format!("{base_id}-{suffix}");
        suffix += 1;
    }
    Ok(candidate)
}

fn next_available_schedule_id(
    store: &PersistenceStore,
    base_id: &str,
) -> Result<String, ExecutionError> {
    let mut candidate = base_id.to_string();
    let mut suffix = 2usize;
    while store
        .get_agent_schedule(&candidate)
        .map_err(ExecutionError::Store)?
        .is_some()
    {
        candidate = format!("{base_id}-{suffix}");
        suffix += 1;
    }
    Ok(candidate)
}

fn load_schedule_for_current_workspace(
    service: &ExecutionService,
    store: &PersistenceStore,
    id: &str,
) -> Result<AgentSchedule, ExecutionError> {
    let schedule = AgentSchedule::try_from(
        store
            .get_agent_schedule(id)
            .map_err(ExecutionError::Store)?
            .ok_or_else(|| invalid_agent_tool(format!("agent schedule {id} not found")))?,
    )
    .map_err(ExecutionError::RecordConversion)?;
    let current_workspace = current_workspace_root(service.workspace.root.as_path());
    if schedule.workspace_root != current_workspace {
        return Err(invalid_agent_tool(format!(
            "agent schedule {id} does not belong to the current workspace"
        )));
    }
    Ok(schedule)
}

fn validate_schedule_target_session(
    store: &PersistenceStore,
    agent_profile_id: &str,
    delivery_mode: AgentScheduleDeliveryMode,
    requested_target_session_id: Option<String>,
    default_target_session_id: Option<&str>,
) -> Result<Option<String>, ExecutionError> {
    match delivery_mode {
        AgentScheduleDeliveryMode::FreshSession => Ok(None),
        AgentScheduleDeliveryMode::ExistingSession => {
            let target_session_id = requested_target_session_id
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .or_else(|| default_target_session_id.map(str::to_string))
                .ok_or_else(|| {
                    invalid_agent_tool(
                        "existing_session schedule requires target_session_id".to_string(),
                    )
                })?;
            let session = store
                .get_session(&target_session_id)
                .map_err(ExecutionError::Store)?
                .ok_or_else(|| {
                    invalid_agent_tool(format!("session {target_session_id} not found"))
                })?;
            if session.agent_profile_id != agent_profile_id {
                return Err(invalid_agent_tool(format!(
                    "target_session_id {} belongs to agent {}, not {}",
                    target_session_id, session.agent_profile_id, agent_profile_id
                )));
            }
            Ok(Some(target_session_id))
        }
    }
}

fn normalized_pagination(
    total: usize,
    offset: Option<usize>,
    limit: Option<usize>,
    default_limit: usize,
    max_limit: usize,
) -> (usize, usize, Option<usize>) {
    let offset = offset.unwrap_or(0).min(total);
    let limit = limit.unwrap_or(default_limit).clamp(1, max_limit);
    let next_offset = if offset.saturating_add(limit) < total {
        Some(offset + limit)
    } else {
        None
    };
    (offset, limit, next_offset)
}

fn current_workspace_root(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn continue_later_prompt(handoff_payload: &str) -> String {
    format!(
        "Resume the previously deferred work. Use the handoff payload below as the source of truth for what to do next.\n\nHandoff payload:\n{}",
        handoff_payload
    )
}

fn agent_summary_output(profile: &AgentProfile) -> AgentSummaryOutput {
    AgentSummaryOutput {
        id: profile.id.clone(),
        name: profile.name.clone(),
        template_kind: profile.template_kind,
        agent_home: profile.agent_home.display().to_string(),
        allowed_tool_count: profile.allowed_tools.len(),
        created_from_template_id: profile.created_from_template_id.clone(),
        created_by_session_id: profile.created_by_session_id.clone(),
        created_by_agent_profile_id: profile.created_by_agent_profile_id.clone(),
        created_at: profile.created_at,
        updated_at: profile.updated_at,
    }
}

fn schedule_view_output(schedule: &AgentSchedule) -> ScheduleViewOutput {
    ScheduleViewOutput {
        id: schedule.id.clone(),
        agent_profile_id: schedule.agent_profile_id.clone(),
        workspace_root: schedule.workspace_root.display().to_string(),
        prompt: schedule.prompt.clone(),
        mode: schedule.mode,
        delivery_mode: schedule.delivery_mode,
        target_session_id: schedule.target_session_id.clone(),
        interval_seconds: schedule.interval_seconds,
        next_fire_at: schedule.next_fire_at,
        enabled: schedule.enabled,
        last_triggered_at: schedule.last_triggered_at,
        last_finished_at: schedule.last_finished_at,
        last_session_id: schedule.last_session_id.clone(),
        last_job_id: schedule.last_job_id.clone(),
        last_result: schedule.last_result.clone(),
        last_error: schedule.last_error.clone(),
        created_at: schedule.created_at,
        updated_at: schedule.updated_at,
    }
}

fn invalid_agent_tool(reason: String) -> ExecutionError {
    ExecutionError::Tool(ToolError::InvalidAgentTool { reason })
}

fn agent_tool_validation_error<E: std::fmt::Display>(error: E) -> ExecutionError {
    invalid_agent_tool(error.to_string())
}

fn io_agent_tool_error(source: std::io::Error) -> ExecutionError {
    invalid_agent_tool(source.to_string())
}
