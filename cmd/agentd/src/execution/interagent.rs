use super::*;
use agent_runtime::agent::AgentChainContinuationGrant;
use agent_runtime::interagent::{
    AgentMessageChain, AgentMessageError, AgentMessageRequest, continued_chain_from_grant,
    format_agent_input_message,
};
use agent_runtime::mission::{JobSpec, JobStatus};
use agent_runtime::run::RunStatus;
use agent_runtime::session::TranscriptEntry;
use agent_runtime::tool::{
    GrantAgentChainContinuationOutput, MessageAgentOutput, SessionReadInput, SessionReadMode,
    SessionWaitOutput,
};
use std::thread;
use std::time::Instant;

impl ExecutionService {
    pub(crate) fn queue_interagent_message(
        &self,
        store: &PersistenceStore,
        session_id: &str,
        input: &agent_runtime::tool::MessageAgentInput,
        now: i64,
    ) -> Result<MessageAgentOutput, ExecutionError> {
        let request =
            AgentMessageRequest::new(input.target_agent_id.as_str(), input.message.as_str())
                .map_err(interagent_error)?;
        let source_session = self.load_session(store, session_id)?;
        let source_profile = self.load_agent_profile(store, &source_session.agent_profile_id)?;
        let target_profile = self.load_agent_profile(store, &request.target_agent_id)?;

        let active_chain = self
            .load_session_interagent_chain(store, &source_session.id)?
            .unwrap_or_else(|| {
                AgentMessageChain::root(
                    format!("chain-{}-{}", source_session.id, unique_execution_token()),
                    source_session.id.clone(),
                    source_session.agent_profile_id.clone(),
                )
                .expect("valid root interagent chain")
            });

        let (next_chain, grant_used) = if active_chain.can_advance_without_grant() {
            (
                active_chain
                    .next_hop(source_session.id.clone(), false)
                    .map_err(interagent_error)?,
                false,
            )
        } else {
            let grant = store
                .get_agent_chain_continuation(&active_chain.chain_id)
                .map_err(ExecutionError::Store)?
                .map(AgentChainContinuationGrant::try_from)
                .transpose()
                .map_err(ExecutionError::RecordConversion)?;
            let Some(grant) = grant else {
                return Err(ExecutionError::ProviderLoop {
                    reason: format!(
                        "inter-agent chain {} is blocked at max_hops={} and has no continuation grant",
                        active_chain.chain_id, active_chain.max_hops
                    ),
                });
            };
            let continued =
                continued_chain_from_grant(&active_chain, &grant, source_session.id.clone())
                    .map_err(interagent_error)?
                    .ok_or_else(|| ExecutionError::ProviderLoop {
                        reason: format!(
                            "continuation grant for chain {} does not match the current chain",
                            active_chain.chain_id
                        ),
                    })?;
            store
                .delete_agent_chain_continuation(&grant.chain_id)
                .map_err(ExecutionError::Store)?;
            (continued, true)
        };

        let recipient_session_id = format!("session-agentmsg-{}", unique_execution_token());
        let recipient_job_id = format!("job-agentmsg-{}", unique_execution_token());
        let recipient_session = Session {
            id: recipient_session_id.clone(),
            title: format!("Agent: {}", target_profile.name),
            prompt_override: None,
            settings: source_session.settings.clone(),
            workspace_root: target_profile
                .default_workspace_root
                .clone()
                .unwrap_or_else(|| source_session.workspace_root.clone()),
            agent_profile_id: target_profile.id.clone(),
            active_mission_id: None,
            parent_session_id: Some(source_session.id.clone()),
            parent_job_id: None,
            delegation_label: Some(format!("agent-chain:{}", next_chain.chain_id)),
            created_at: now,
            updated_at: now,
        };
        store
            .put_session(
                &agent_persistence::SessionRecord::try_from(&recipient_session)
                    .map_err(ExecutionError::RecordConversion)?,
            )
            .map_err(ExecutionError::Store)?;

        let chain_entry = TranscriptEntry::system(
            format!("transcript-{recipient_job_id}-interagent-chain"),
            recipient_session.id.clone(),
            None,
            next_chain.to_transcript_metadata(),
            now,
        );
        store
            .put_transcript(&TranscriptRecord::from(&chain_entry))
            .map_err(ExecutionError::Store)?;

        let mut job = JobSpec::interagent_message(
            &recipient_job_id,
            &recipient_session.id,
            None,
            None,
            source_session.id.clone(),
            source_profile.id.clone(),
            source_profile.name.clone(),
            target_profile.id.clone(),
            target_profile.name.clone(),
            request.message,
            next_chain.clone(),
            now,
        );
        job.status = JobStatus::Running;
        job.last_progress_message = Some(if grant_used {
            "inter-agent message queued with continuation grant".to_string()
        } else {
            "inter-agent message queued".to_string()
        });
        store
            .put_job(&JobRecord::try_from(&job).map_err(ExecutionError::RecordConversion)?)
            .map_err(ExecutionError::Store)?;

        let started_entry = TranscriptEntry::system(
            format!("transcript-{recipient_job_id}-interagent-started"),
            source_session.id,
            None,
            format!(
                "message_agent queued: target={} recipient_session={} recipient_job={} chain_id={} hop_count={}",
                target_profile.id,
                recipient_session_id,
                recipient_job_id,
                next_chain.chain_id,
                next_chain.hop_count
            ),
            now,
        );
        store
            .put_transcript(&TranscriptRecord::from(&started_entry))
            .map_err(ExecutionError::Store)?;

        Ok(MessageAgentOutput {
            target_agent_id: target_profile.id,
            recipient_session_id,
            recipient_job_id,
            chain_id: next_chain.chain_id,
            hop_count: next_chain.hop_count,
        })
    }

    pub(crate) fn wait_for_session(
        &self,
        store: &PersistenceStore,
        provider: &dyn ProviderDriver,
        input: &agent_runtime::tool::SessionWaitInput,
        now: i64,
    ) -> Result<SessionWaitOutput, ExecutionError> {
        self.load_session(store, input.session_id.as_str())?;
        let wait_timeout_ms = input
            .wait_timeout_ms
            .unwrap_or(self.config.runtime_timing.daemon_http_request_timeout_ms);
        let poll_interval = self
            .config
            .runtime_timing
            .daemon_background_worker_tick_interval();
        let started = Instant::now();

        loop {
            let state = self.collect_session_wait_state(store, input.session_id.as_str())?;
            if state.active_run_count == 0 && state.active_job_count == 0 {
                return self.build_session_wait_output(store, input, wait_timeout_ms, true);
            }
            if started.elapsed().as_millis() >= u128::from(wait_timeout_ms) {
                return self.build_session_wait_output(store, input, wait_timeout_ms, false);
            }

            let elapsed_seconds = i64::try_from(started.elapsed().as_secs()).unwrap_or(i64::MAX);
            let tick_now = now.saturating_add(elapsed_seconds);
            let progressed = self.pump_background_for_session(
                store,
                provider,
                input.session_id.as_str(),
                tick_now,
            )?;
            if !progressed {
                let remaining_ms = wait_timeout_ms.saturating_sub(
                    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
                );
                if remaining_ms == 0 {
                    return self.build_session_wait_output(store, input, wait_timeout_ms, false);
                }
                thread::sleep(poll_interval.min(std::time::Duration::from_millis(remaining_ms)));
            }
        }
    }

    pub(crate) fn grant_agent_chain_continuation(
        &self,
        store: &PersistenceStore,
        input: &agent_runtime::tool::GrantAgentChainContinuationInput,
        now: i64,
    ) -> Result<GrantAgentChainContinuationOutput, ExecutionError> {
        let grant =
            AgentChainContinuationGrant::new(input.chain_id.as_str(), input.reason.as_str(), now)
                .map_err(|_| ExecutionError::ProviderLoop {
                reason: format!(
                    "invalid chain continuation grant for chain {}",
                    input.chain_id
                ),
            })?;
        store
            .put_agent_chain_continuation(&agent_persistence::AgentChainContinuationRecord::from(
                &grant,
            ))
            .map_err(ExecutionError::Store)?;

        Ok(GrantAgentChainContinuationOutput {
            chain_id: grant.chain_id,
            granted_hops: grant.granted_hops,
        })
    }

    pub(crate) fn load_session_interagent_chain(
        &self,
        store: &PersistenceStore,
        session_id: &str,
    ) -> Result<Option<AgentMessageChain>, ExecutionError> {
        let chain = store
            .list_transcripts_for_session(session_id)
            .map_err(ExecutionError::Store)?
            .into_iter()
            .rev()
            .find_map(|record| AgentMessageChain::from_transcript_metadata(&record.content));
        Ok(chain)
    }

    pub(super) fn interagent_origin_user_message(
        &self,
        source_agent_name: &str,
        message: &str,
    ) -> String {
        format_agent_input_message(source_agent_name, message)
    }

    fn build_session_wait_output(
        &self,
        store: &PersistenceStore,
        input: &agent_runtime::tool::SessionWaitInput,
        wait_timeout_ms: u64,
        settled: bool,
    ) -> Result<SessionWaitOutput, ExecutionError> {
        let state = self.collect_session_wait_state(store, input.session_id.as_str())?;
        let snapshot = self.read_session(
            store,
            &SessionReadInput {
                session_id: input.session_id.clone(),
                mode: Some(input.mode.unwrap_or(SessionReadMode::Transcript)),
                cursor: input.cursor,
                max_items: input.max_items,
                max_bytes: input.max_bytes,
                include_tools: input.include_tools,
            },
        )?;

        Ok(SessionWaitOutput {
            session_id: input.session_id.clone(),
            wait_timeout_ms,
            settled,
            active_run_count: state.active_run_count,
            active_job_count: state.active_job_count,
            latest_run_id: state.latest_run_id,
            latest_run_status: state.latest_run_status,
            latest_job_id: state.latest_job_id,
            latest_job_status: state.latest_job_status,
            snapshot,
        })
    }

    fn collect_session_wait_state(
        &self,
        store: &PersistenceStore,
        session_id: &str,
    ) -> Result<SessionWaitState, ExecutionError> {
        let mut jobs = store
            .list_jobs_for_session(session_id)
            .map_err(ExecutionError::Store)?
            .into_iter()
            .map(JobSpec::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ExecutionError::RecordConversion)?;
        jobs.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then(right.created_at.cmp(&left.created_at))
                .then(left.id.cmp(&right.id))
        });
        let active_job_count = jobs.iter().filter(|job| job.status.is_active()).count();

        let mut runs = store
            .list_runs_for_session(session_id)
            .map_err(ExecutionError::Store)?
            .into_iter()
            .map(RunSnapshot::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ExecutionError::RecordConversion)?;
        runs.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then(right.started_at.cmp(&left.started_at))
                .then(left.id.cmp(&right.id))
        });
        let active_run_count = runs
            .iter()
            .filter(|run| run_status_is_active(run.status))
            .count();

        Ok(SessionWaitState {
            active_run_count,
            active_job_count,
            latest_run_id: runs.first().map(|run| run.id.clone()),
            latest_run_status: runs.first().map(|run| run.status.as_str().to_string()),
            latest_job_id: jobs.first().map(|job| job.id.clone()),
            latest_job_status: jobs.first().map(|job| job.status.as_str().to_string()),
        })
    }
}

fn interagent_error(source: AgentMessageError) -> ExecutionError {
    ExecutionError::ProviderLoop {
        reason: source.to_string(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SessionWaitState {
    active_run_count: usize,
    active_job_count: usize,
    latest_run_id: Option<String>,
    latest_run_status: Option<String>,
    latest_job_id: Option<String>,
    latest_job_status: Option<String>,
}

fn run_status_is_active(status: RunStatus) -> bool {
    matches!(
        status,
        RunStatus::Queued
            | RunStatus::Running
            | RunStatus::WaitingApproval
            | RunStatus::WaitingProcess
            | RunStatus::WaitingDelegate
            | RunStatus::Resuming
    )
}
