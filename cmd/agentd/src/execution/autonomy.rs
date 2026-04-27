use super::*;
use crate::agents;
use agent_persistence::{AgentProfileRecord, AgentScheduleRecord};
use agent_runtime::agent::{
    AgentProfile, AgentSchedule, AgentScheduleDeliveryMode, AgentScheduleInit, AgentScheduleMode,
    AgentTemplateKind,
};
use agent_runtime::tool::{
    AgentCreateInput, AgentCreateOutput, AgentListInput, AgentListOutput, AgentReadInput,
    AgentReadOutput, AgentSummaryOutput, AutonomyChildSessionOutput, AutonomyInboxEventOutput,
    AutonomyInteragentOutput, AutonomyJobOutput, AutonomyMeshPeerOutput, AutonomyStateReadInput,
    AutonomyStateReadOutput, ContinueLaterInput, ContinueLaterOutput, ScheduleCreateInput,
    ScheduleCreateOutput, ScheduleDeleteInput, ScheduleDeleteOutput, ScheduleListInput,
    ScheduleListOutput, ScheduleReadInput, ScheduleReadOutput, ScheduleUpdateInput,
    ScheduleUpdateOutput, ScheduleViewOutput, ToolError,
};
use std::path::{Path, PathBuf};

const DEFAULT_AUTONOMY_STATE_LIMIT: usize = 8;
const MAX_AUTONOMY_STATE_LIMIT: usize = 50;

impl ExecutionService {
    pub(crate) fn read_autonomy_state_tool(
        &self,
        store: &PersistenceStore,
        session_id: &str,
        input: &AutonomyStateReadInput,
    ) -> Result<AutonomyStateReadOutput, ExecutionError> {
        let session = load_session_or_internal(store, session_id)?;
        let max_items = input
            .max_items
            .unwrap_or(DEFAULT_AUTONOMY_STATE_LIMIT)
            .clamp(1, MAX_AUTONOMY_STATE_LIMIT);
        let include_inactive_schedules = input.include_inactive_schedules.unwrap_or(false);
        let current_workspace = current_workspace_root(self.workspace.root.as_path());

        let mut schedules = store
            .list_agent_schedules()
            .map_err(ExecutionError::Store)?
            .into_iter()
            .map(AgentSchedule::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ExecutionError::RecordConversion)?;
        schedules.retain(|schedule| {
            schedule.workspace_root == current_workspace
                && (schedule.agent_profile_id == session.agent_profile_id
                    || schedule.target_session_id.as_deref() == Some(session.id.as_str())
                    || schedule.last_session_id.as_deref() == Some(session.id.as_str()))
                && (include_inactive_schedules || schedule.enabled)
        });
        schedules.sort_by(|left, right| {
            left.next_fire_at
                .cmp(&right.next_fire_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        let total_schedules = schedules.len();
        let schedules = schedules
            .iter()
            .take(max_items)
            .map(schedule_view_output)
            .collect::<Vec<_>>();

        let mut active_jobs = store
            .list_active_jobs_for_session(session.id.as_str())
            .map_err(ExecutionError::Store)?;
        active_jobs.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        let total_active_jobs = active_jobs.len();
        let active_jobs = active_jobs
            .into_iter()
            .take(max_items)
            .map(|job| AutonomyJobOutput {
                id: job.id,
                kind: job.kind,
                status: job.status,
                run_id: job.run_id,
                parent_job_id: job.parent_job_id,
                last_progress_message: job.last_progress_message,
                updated_at: job.updated_at,
            })
            .collect::<Vec<_>>();

        let mut child_sessions = store
            .list_sessions()
            .map_err(ExecutionError::Store)?
            .into_iter()
            .filter(|record| record.parent_session_id.as_deref() == Some(session.id.as_str()))
            .collect::<Vec<_>>();
        child_sessions.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        let total_child_sessions = child_sessions.len();
        let child_sessions = child_sessions
            .into_iter()
            .take(max_items)
            .map(|record| AutonomyChildSessionOutput {
                id: record.id,
                title: record.title,
                agent_profile_id: record.agent_profile_id,
                parent_job_id: record.parent_job_id,
                delegation_label: record.delegation_label,
                updated_at: record.updated_at,
            })
            .collect::<Vec<_>>();

        let mut inbox_events = store
            .list_session_inbox_events_for_session(session.id.as_str())
            .map_err(ExecutionError::Store)?;
        inbox_events.sort_by(|left, right| {
            right
                .available_at
                .cmp(&left.available_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        let total_inbox_events = inbox_events.len();
        let inbox_events = inbox_events
            .into_iter()
            .take(max_items)
            .map(|event| AutonomyInboxEventOutput {
                id: event.id,
                kind: event.kind,
                job_id: event.job_id,
                status: event.status,
                available_at: event.available_at,
                error: event.error,
            })
            .collect::<Vec<_>>();

        let interagent = self
            .load_session_interagent_chain(store, session.id.as_str())?
            .map(|chain| AutonomyInteragentOutput {
                chain_id: chain.chain_id,
                origin_session_id: chain.origin_session_id,
                origin_agent_id: chain.origin_agent_id,
                hop_count: chain.hop_count,
                max_hops: chain.max_hops,
                parent_interagent_session_id: chain.parent_interagent_session_id,
                state: match chain.state {
                    agent_runtime::interagent::AgentChainState::Active => "active",
                    agent_runtime::interagent::AgentChainState::BlockedMaxHops => {
                        "blocked_max_hops"
                    }
                    agent_runtime::interagent::AgentChainState::ContinuedOnce => "continued_once",
                }
                .to_string(),
            });

        let total_mesh_peers = self.config.a2a_peers.len();
        let mesh_peers = self
            .config
            .a2a_peers
            .iter()
            .take(max_items)
            .map(|(peer_id, peer)| AutonomyMeshPeerOutput {
                peer_id: peer_id.clone(),
                base_url: peer.base_url.clone(),
                has_bearer_token: peer.bearer_token.is_some(),
            })
            .collect::<Vec<_>>();

        Ok(AutonomyStateReadOutput {
            session_id: session.id.clone(),
            title: session.title.clone(),
            agent_profile_id: session.agent_profile_id.clone(),
            turn_source: autonomy_turn_source(&session),
            parent_session_id: session.parent_session_id.clone(),
            parent_job_id: session.parent_job_id.clone(),
            delegation_label: session.delegation_label.clone(),
            schedules,
            active_jobs,
            child_sessions,
            inbox_events,
            interagent,
            mesh_peers,
            truncated: total_schedules > max_items
                || total_active_jobs > max_items
                || total_child_sessions > max_items
                || total_inbox_events > max_items
                || total_mesh_peers > max_items,
            max_items,
        })
    }

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
            self.config.runtime_limits.agent_list_default_limit,
            self.config.runtime_limits.agent_list_max_limit,
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
        let agent_workspace = agents::agent_workspace(&self.config.data_dir, &agent_id);
        agents::clone_agent_home(
            &template.agent_home,
            &agent_home,
            template_fallback.system_md,
            template_fallback.agents_md,
        )
        .map_err(io_agent_tool_error)?;
        agents::ensure_agent_workspace_layout(&agent_workspace).map_err(io_agent_tool_error)?;
        agent_persistence::validate_workspace_root_path(
            "agent.default_workspace_root",
            &agent_workspace,
            &self.config.data_dir,
        )
        .map_err(|error| {
            invalid_agent_tool(format!("agent_create workspace path is invalid: {error}"))
        })?;
        let profile = AgentProfile::new_with_provenance(
            &agent_id,
            input.name.trim(),
            AgentTemplateKind::Custom,
            &agent_home,
            template.allowed_tools.clone(),
            Some(agent_workspace),
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
            self.config.runtime_limits.schedule_list_default_limit,
            self.config.runtime_limits.schedule_list_max_limit,
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

fn autonomy_turn_source(session: &Session) -> Option<String> {
    session
        .delegation_label
        .as_deref()
        .and_then(|label| {
            if label.starts_with("agent-schedule:") {
                Some("schedule")
            } else if label.starts_with("agent-chain:") {
                Some("agent2agent")
            } else {
                None
            }
        })
        .or_else(|| session.parent_session_id.as_ref().map(|_| "subagent"))
        .map(str::to_string)
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
        default_workspace_root: profile
            .default_workspace_root
            .as_deref()
            .map(|path| path.display().to_string()),
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
