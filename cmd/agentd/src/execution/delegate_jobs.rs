use super::delegation::{DelegationExecutorKind, a2a_peer_id, resolve_delegate_dispatch};
use super::*;
use crate::http::types::{A2ACallbackTargetRequest, A2ADelegationCreateRequest};
use crate::prompting;
use agent_runtime::delegation::{DelegateRequest, DelegateResultPackage};
use agent_runtime::mission::JobResult;
use agent_runtime::session::TranscriptEntry;

impl ExecutionService {
    pub fn execute_background_delegate_job(
        &self,
        store: &PersistenceStore,
        provider: &dyn ProviderDriver,
        job_id: &str,
        now: i64,
    ) -> Result<(), ExecutionError> {
        let mut job = JobSpec::try_from(
            store
                .get_job(job_id)
                .map_err(ExecutionError::Store)?
                .ok_or_else(|| ExecutionError::MissingJob {
                    id: job_id.to_string(),
                })?,
        )
        .map_err(ExecutionError::RecordConversion)?;
        let (label, goal, bounded_context, write_scope, expected_output, owner) = match &job.input {
            JobExecutionInput::Delegate {
                label,
                goal,
                bounded_context,
                write_scope,
                expected_output,
                owner,
            } => (
                label.clone(),
                goal.clone(),
                bounded_context.clone(),
                write_scope.clone(),
                expected_output.clone(),
                owner.clone(),
            ),
            _ => {
                return Err(ExecutionError::UnsupportedJobInput {
                    id: job.id.clone(),
                    kind: job.kind.as_str().to_string(),
                });
            }
        };

        let child_run_id = format!("run-delegate-child-{}", job.id);
        let parent_run_id = job
            .run_id
            .clone()
            .unwrap_or_else(|| format!("run-delegate-parent-{}", job.id));
        let request = DelegateRequest::new(
            job.id.clone(),
            parent_run_id,
            child_run_id.clone(),
            label.clone(),
            goal.clone(),
            bounded_context.clone(),
            write_scope.clone(),
            expected_output.clone(),
            owner.clone(),
        )
        .map_err(|source| ExecutionError::ProviderLoop {
            reason: source.to_string(),
        })?;

        job.status = JobStatus::Running;
        job.error = None;
        job.updated_at = now;
        if job.started_at.is_none() {
            job.started_at = Some(now);
        }
        job.last_progress_message = Some("delegation child session running".to_string());
        store
            .put_job(&JobRecord::try_from(&job).map_err(ExecutionError::RecordConversion)?)
            .map_err(ExecutionError::Store)?;

        let dispatch = resolve_delegate_dispatch(&request);
        match dispatch.kind {
            DelegationExecutorKind::LocalChildSession => {
                self.execute_background_delegate_job_local(
                    store, provider, &mut job, &request, now,
                )?;
            }
            DelegationExecutorKind::RemoteA2A => {
                self.execute_background_delegate_job_remote(
                    store,
                    &mut job,
                    &request,
                    &dispatch.owner_selector,
                    now,
                )?;
            }
        }

        Ok(())
    }

    fn execute_background_delegate_job_remote(
        &self,
        store: &PersistenceStore,
        job: &mut JobSpec,
        request: &DelegateRequest,
        owner_selector: &str,
        now: i64,
    ) -> Result<(), ExecutionError> {
        let Some(peer_id) = a2a_peer_id(owner_selector) else {
            let reason = format!("invalid remote delegation owner {owner_selector}");
            job.status = JobStatus::Blocked;
            job.error = Some(reason.clone());
            job.updated_at = now;
            job.last_progress_message = Some(reason);
            store
                .put_job(&JobRecord::try_from(&*job).map_err(ExecutionError::RecordConversion)?)
                .map_err(ExecutionError::Store)?;
            return Ok(());
        };

        let Some(peer) = self.config.a2a_peers.get(peer_id) else {
            let reason = format!("remote delegation peer {peer_id} is not configured");
            job.status = JobStatus::Blocked;
            job.error = Some(reason.clone());
            job.updated_at = now;
            job.last_progress_message = Some(reason);
            store
                .put_job(&JobRecord::try_from(&*job).map_err(ExecutionError::RecordConversion)?)
                .map_err(ExecutionError::Store)?;
            return Ok(());
        };

        let Some(public_base_url) = self.config.a2a_public_base_url.as_deref() else {
            let reason = "daemon.public_base_url is required for remote delegation".to_string();
            job.status = JobStatus::Blocked;
            job.error = Some(reason.clone());
            job.updated_at = now;
            job.last_progress_message = Some(reason);
            store
                .put_job(&JobRecord::try_from(&*job).map_err(ExecutionError::RecordConversion)?)
                .map_err(ExecutionError::Store)?;
            return Ok(());
        };

        let callback_url = format!(
            "{}/v1/a2a/delegations/{}/complete",
            public_base_url.trim_end_matches('/'),
            job.id
        );
        let accepted = match self.a2a.send_delegation(
            peer,
            &A2ADelegationCreateRequest {
                parent_session_id: job.session_id.clone(),
                parent_job_id: job.id.clone(),
                label: request.label.clone(),
                goal: request.goal.clone(),
                bounded_context: request.bounded_context.clone(),
                write_scope: request.write_scope.clone(),
                expected_output: request.expected_output.clone(),
                owner: request.owner.clone(),
                callback: A2ACallbackTargetRequest {
                    url: callback_url,
                    bearer_token: self.config.a2a_callback_bearer_token.clone(),
                },
                now,
            },
        ) {
            Ok(accepted) => accepted,
            Err(reason) => {
                job.status = JobStatus::Blocked;
                job.error = Some(reason.clone());
                job.updated_at = now;
                job.last_progress_message = Some(format!(
                    "remote delegation dispatch to {peer_id} failed: {reason}"
                ));
                store
                    .put_job(&JobRecord::try_from(&*job).map_err(ExecutionError::RecordConversion)?)
                    .map_err(ExecutionError::Store)?;
                return Ok(());
            }
        };

        self.write_delegate_parent_started_transcript(
            store,
            &job.session_id,
            &job.id,
            &accepted.remote_session_id,
            &request.label,
            now,
        )?;

        job.status = JobStatus::WaitingExternal;
        job.error = None;
        job.updated_at = now;
        job.lease_owner = None;
        job.lease_expires_at = None;
        job.heartbeat_at = Some(now);
        job.last_progress_message = Some(format!(
            "remote delegation accepted by {peer_id} as {}",
            accepted.remote_job_id
        ));
        store
            .put_job(&JobRecord::try_from(&*job).map_err(ExecutionError::RecordConversion)?)
            .map_err(ExecutionError::Store)?;
        Ok(())
    }

    fn execute_background_delegate_job_local(
        &self,
        store: &PersistenceStore,
        provider: &dyn ProviderDriver,
        job: &mut JobSpec,
        request: &DelegateRequest,
        now: i64,
    ) -> Result<(), ExecutionError> {
        let parent_session_record = store
            .get_session(&job.session_id)
            .map_err(ExecutionError::Store)?
            .ok_or_else(|| ExecutionError::MissingSession {
                id: job.session_id.clone(),
            })?;
        let parent_session =
            Session::try_from(parent_session_record).map_err(ExecutionError::RecordConversion)?;
        let child_session = self.ensure_delegate_child_session(
            store,
            &parent_session,
            &job.id,
            &request.label,
            now,
        )?;
        self.write_delegate_parent_started_transcript(
            store,
            &parent_session.id,
            &job.id,
            &child_session.id,
            &request.label,
            now,
        )?;

        let child_report = match self.restore_or_execute_delegate_child_turn(
            store,
            provider,
            &job.id,
            &child_session,
            request,
            now,
        ) {
            Ok(report) => report,
            Err(source @ ExecutionError::ApprovalRequired { .. }) => {
                job.status = JobStatus::Blocked;
                job.error = Some(source.to_string());
                job.updated_at = now;
                job.last_progress_message =
                    Some("delegated child session requires approval".to_string());
                store
                    .put_job(&JobRecord::try_from(&*job).map_err(ExecutionError::RecordConversion)?)
                    .map_err(ExecutionError::Store)?;
                return Err(source);
            }
            Err(source) => {
                job.status = JobStatus::Failed;
                job.error = Some(source.to_string());
                job.finished_at = Some(now);
                job.updated_at = now;
                job.last_progress_message = Some("delegated child session failed".to_string());
                store
                    .put_job(&JobRecord::try_from(&*job).map_err(ExecutionError::RecordConversion)?)
                    .map_err(ExecutionError::Store)?;
                return Err(source);
            }
        };

        let artifact_refs = store
            .get_context_offload(&child_session.id)
            .map_err(ExecutionError::Store)?
            .map(agent_runtime::context::ContextOffloadSnapshot::try_from)
            .transpose()
            .map_err(ExecutionError::RecordConversion)?
            .map(|snapshot| {
                snapshot
                    .refs
                    .into_iter()
                    .map(|reference| reference.artifact_id)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let package = DelegateResultPackage::new(
            prompting::preview_text(&child_report.output_text, 240),
            Vec::new(),
            artifact_refs,
            Vec::new(),
        )
        .map_err(|source| ExecutionError::ProviderLoop {
            reason: source.to_string(),
        })?;

        job.status = JobStatus::Completed;
        job.run_id = Some(child_report.run_id.clone());
        job.result = Some(JobResult::Delegation {
            child_session_id: child_session.id.clone(),
            package: package.clone(),
        });
        job.finished_at = Some(now);
        job.updated_at = now;
        job.last_progress_message = Some("delegated child session completed".to_string());
        store
            .put_job(&JobRecord::try_from(&*job).map_err(ExecutionError::RecordConversion)?)
            .map_err(ExecutionError::Store)?;

        Ok(())
    }

    fn ensure_delegate_child_session(
        &self,
        store: &PersistenceStore,
        parent_session: &Session,
        parent_job_id: &str,
        label: &str,
        now: i64,
    ) -> Result<Session, ExecutionError> {
        let child_session_id = format!("session-delegate-{parent_job_id}");
        if let Some(record) = store
            .get_session(&child_session_id)
            .map_err(ExecutionError::Store)?
        {
            return Session::try_from(record).map_err(ExecutionError::RecordConversion);
        }

        let child_session = Session {
            id: child_session_id,
            title: format!("Delegate: {label}"),
            prompt_override: parent_session.prompt_override.clone(),
            settings: parent_session.settings.clone(),
            active_mission_id: None,
            parent_session_id: Some(parent_session.id.clone()),
            parent_job_id: Some(parent_job_id.to_string()),
            delegation_label: Some(label.to_string()),
            created_at: now,
            updated_at: now,
        };
        store
            .put_session(
                &agent_persistence::SessionRecord::try_from(&child_session)
                    .map_err(ExecutionError::RecordConversion)?,
            )
            .map_err(ExecutionError::Store)?;
        Ok(child_session)
    }

    fn write_delegate_parent_started_transcript(
        &self,
        store: &PersistenceStore,
        parent_session_id: &str,
        parent_job_id: &str,
        child_session_id: &str,
        label: &str,
        now: i64,
    ) -> Result<(), ExecutionError> {
        let entry = TranscriptEntry::system(
            format!("transcript-{parent_job_id}-delegate-started"),
            parent_session_id.to_string(),
            None,
            format!("delegation started: {label} (child session: {child_session_id})"),
            now,
        );
        store
            .put_transcript(&TranscriptRecord::from(&entry))
            .map_err(ExecutionError::Store)
    }

    fn restore_or_execute_delegate_child_turn(
        &self,
        store: &PersistenceStore,
        provider: &dyn ProviderDriver,
        parent_job_id: &str,
        child_session: &Session,
        request: &DelegateRequest,
        now: i64,
    ) -> Result<ChatTurnExecutionReport, ExecutionError> {
        if let Some(existing_run) = store
            .get_run(&request.child_run_id)
            .map_err(ExecutionError::Store)?
        {
            let snapshot =
                RunSnapshot::try_from(existing_run).map_err(ExecutionError::RecordConversion)?;
            if snapshot.status == RunStatus::Completed {
                let assistant_output = store
                    .list_transcripts_for_session(&child_session.id)
                    .map_err(ExecutionError::Store)?
                    .into_iter()
                    .rfind(|record| {
                        record.run_id.as_deref() == Some(request.child_run_id.as_str())
                            && record.kind == "assistant"
                    })
                    .map(|record| record.content)
                    .unwrap_or_default();
                return Ok(ChatTurnExecutionReport {
                    session_id: child_session.id.clone(),
                    run_id: request.child_run_id.clone(),
                    response_id: String::new(),
                    output_text: assistant_output,
                });
            }
        }

        let mut run = RunEngine::new(
            request.child_run_id.clone(),
            child_session.id.clone(),
            None,
            now,
        );
        run.start(now).map_err(ExecutionError::RunTransition)?;
        store
            .put_run(
                &RunRecord::try_from(run.snapshot()).map_err(ExecutionError::RecordConversion)?,
            )
            .map_err(ExecutionError::Store)?;

        let system_entry = TranscriptEntry::system(
            format!("transcript-{parent_job_id}-delegate-child-01-system"),
            child_session.id.clone(),
            Some(request.child_run_id.as_str()),
            format!(
                "Delegated task.\nlabel: {}\nowner: {}\nexpected_output: {}\nbounded_context: {}\nwrite_scope: {}",
                request.label,
                request.owner,
                request.expected_output,
                request.bounded_context.join(", "),
                request.write_scope.allowed_paths.join(", ")
            ),
            now,
        );
        store
            .put_transcript(&TranscriptRecord::from(&system_entry))
            .map_err(ExecutionError::Store)?;

        let user_entry = TranscriptEntry::user(
            format!("transcript-{parent_job_id}-delegate-child-02-user"),
            child_session.id.clone(),
            Some(request.child_run_id.as_str()),
            &request.goal,
            now,
        );
        store
            .put_transcript(&TranscriptRecord::from(&user_entry))
            .map_err(ExecutionError::Store)?;

        let mut observer = None;
        let response = match self.execute_provider_turn_loop(
            store,
            provider,
            &child_session.id,
            child_session.settings.model.clone(),
            child_session
                .prompt_override
                .as_ref()
                .map(|override_text| override_text.as_str().to_string()),
            &mut run,
            None,
            now,
            None,
            &mut observer,
        ) {
            Ok(response) => response,
            Err(source) => {
                if !matches!(
                    source,
                    ExecutionError::PermissionDenied { .. }
                        | ExecutionError::ApprovalRequired { .. }
                        | ExecutionError::InterruptedByQueuedInput
                ) {
                    run.fail(source.to_string(), now)
                        .map_err(ExecutionError::RunTransition)?;
                    self.persist_run(store, &run)?;
                }
                return Err(source);
            }
        };

        run.complete(&response.output_text, now)
            .map_err(ExecutionError::RunTransition)?;
        self.persist_run(store, &run)?;

        let assistant_entry = TranscriptEntry::assistant(
            format!("transcript-{parent_job_id}-delegate-child-03-assistant"),
            child_session.id.clone(),
            Some(request.child_run_id.as_str()),
            &response.output_text,
            now,
        );
        store
            .put_transcript(&TranscriptRecord::from(&assistant_entry))
            .map_err(ExecutionError::Store)?;

        Ok(ChatTurnExecutionReport {
            session_id: child_session.id.clone(),
            run_id: request.child_run_id.clone(),
            response_id: response.response_id,
            output_text: response.output_text,
        })
    }
}
