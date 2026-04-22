use super::{
    AgentChainContinuationRecord, AgentProfileRecord, AgentScheduleRecord, ContextOffloadRecord,
    JobRecord, MissionRecord, PlanRecord, RunRecord, SessionRecord, TranscriptRecord,
};
use agent_runtime::agent::{
    AgentChainContinuationGrant, AgentProfile, AgentSchedule, AgentTemplateKind,
};
use agent_runtime::context::{ContextOffloadRef, ContextOffloadSnapshot};
use agent_runtime::mission::{
    AcceptanceCriterion, JobExecutionInput, JobKind, JobResult, JobSpec, JobSpecValidationError,
    JobStatus, MissionExecutionIntent, MissionSchedule, MissionSpec, MissionStatus,
};
use agent_runtime::plan::{PlanItem, PlanItemStatus, PlanSnapshot};
use agent_runtime::run::{ActiveProcess, ApprovalRequest, DelegateRun, RunEngine, RunSnapshot};
use agent_runtime::session::{MessageRole, PromptOverride, Session, TranscriptEntry};
use agent_runtime::verification::{CheckOutcome, EvidenceBundle};

#[test]
fn session_records_round_trip_with_domain_sessions() {
    let session = Session {
        id: "session-1".to_string(),
        title: "Bootstrap".to_string(),
        prompt_override: Some(PromptOverride::new("Always verify").expect("prompt override")),
        settings: Default::default(),
        agent_profile_id: "default".to_string(),
        active_mission_id: Some("mission-1".to_string()),
        parent_session_id: None,
        parent_job_id: None,
        delegation_label: None,
        created_at: 10,
        updated_at: 11,
    };

    let stored = SessionRecord::try_from(&session).expect("session to record");
    let restored = Session::try_from(stored).expect("record to session");

    assert_eq!(restored, session);
}

#[test]
fn session_records_round_trip_with_delegation_lineage_metadata() {
    let session = Session {
        id: "session-child".to_string(),
        title: "Delegate: verification".to_string(),
        prompt_override: None,
        settings: Default::default(),
        agent_profile_id: "judge".to_string(),
        active_mission_id: None,
        parent_session_id: Some("session-parent".to_string()),
        parent_job_id: Some("job-delegate".to_string()),
        delegation_label: Some("verification".to_string()),
        created_at: 20,
        updated_at: 21,
    };

    let stored = SessionRecord::try_from(&session).expect("session to record");
    let restored = Session::try_from(stored).expect("record to session");

    assert_eq!(
        restored.parent_session_id.as_deref(),
        Some("session-parent")
    );
    assert_eq!(restored.parent_job_id.as_deref(), Some("job-delegate"));
    assert_eq!(restored.delegation_label.as_deref(), Some("verification"));
    assert_eq!(restored, session);
}

#[test]
fn session_records_accept_legacy_partial_settings_json() {
    let restored = Session::try_from(SessionRecord {
        id: "session-legacy".to_string(),
        title: "Legacy".to_string(),
        prompt_override: None,
        settings_json: "{\"model\":\"gpt-5.4\"}".to_string(),
        agent_profile_id: "default".to_string(),
        active_mission_id: None,
        parent_session_id: None,
        parent_job_id: None,
        delegation_label: None,
        created_at: 1,
        updated_at: 1,
    })
    .expect("record to session");

    assert_eq!(restored.settings.model.as_deref(), Some("gpt-5.4"));
    assert!(restored.settings.reasoning_visible);
    assert_eq!(restored.settings.think_level, None);
    assert_eq!(restored.settings.compactifications, 0);
}

#[test]
fn agent_profile_records_round_trip_with_domain_agents() {
    let profile = AgentProfile::new(
        "judge",
        "Judge",
        AgentTemplateKind::Judge,
        "/var/lib/teamd/agents/judge",
        vec!["fs_read_text".to_string(), "plan_snapshot".to_string()],
        10,
        11,
    )
    .expect("agent profile");

    let stored = AgentProfileRecord::try_from(&profile).expect("profile to record");
    let restored = AgentProfile::try_from(stored).expect("record to profile");

    assert_eq!(restored, profile);
}

#[test]
fn chain_continuation_records_round_trip() {
    let grant = AgentChainContinuationGrant::new("chain-1", "judge approved", 42)
        .expect("continuation grant");

    let stored = AgentChainContinuationRecord::from(&grant);
    let restored = AgentChainContinuationGrant::try_from(stored).expect("record to grant");

    assert_eq!(restored, grant);
}

#[test]
fn agent_schedule_records_round_trip() {
    let schedule = AgentSchedule::new(
        "judge-pulse",
        "judge",
        "/workspace/project",
        "Check the latest diff and summarize it.",
        300,
        42,
        Some(12),
        Some("session-schedule-prev".to_string()),
        Some("job-schedule-prev".to_string()),
        10,
        11,
    )
    .expect("schedule");

    let stored = AgentScheduleRecord::from(&schedule);
    let restored = AgentSchedule::try_from(stored).expect("record to schedule");

    assert_eq!(restored, schedule);
}

#[test]
fn transcript_records_round_trip_with_domain_entries() {
    let entry = TranscriptEntry::assistant(
        "message-1",
        "session-1",
        Some("run-1"),
        "starting verification",
        12,
    );

    let stored = TranscriptRecord::from(&entry);
    let restored = TranscriptEntry::try_from(stored).expect("record to entry");

    assert_eq!(restored, entry);
}

#[test]
fn transcript_records_reject_unknown_roles() {
    let record = TranscriptRecord {
        id: "message-1".to_string(),
        session_id: "session-1".to_string(),
        run_id: None,
        kind: "unknown".to_string(),
        content: "content".to_string(),
        created_at: 12,
    };

    assert!(TranscriptEntry::try_from(record).is_err());
}

#[test]
fn transcript_entry_serializes_role_names_stably() {
    let entry = TranscriptEntry::new(
        "message-1",
        "session-1",
        None,
        MessageRole::Tool,
        "patched files",
        13,
    );

    let stored = TranscriptRecord::from(&entry);

    assert_eq!(stored.kind, "tool");
}

#[test]
fn run_records_round_trip_with_snapshot_core_fields() {
    let mut engine = RunEngine::new("run-1", "session-1", Some("mission-1"), 1);
    let mut evidence = EvidenceBundle::new("bundle-1", "run-1", 2);
    engine.start(2).expect("start");
    engine
        .wait_for_approval(
            ApprovalRequest::new("approval-1", "tool-call-1", "write access", 2),
            2,
        )
        .expect("wait for approval");
    engine
        .resolve_approval("approval-1", 2)
        .expect("resolve approval");
    engine.resume(2).expect("resume");
    engine
        .track_active_process(ActiveProcess::new("exec-1", "exec", "pid:42", 2), 2)
        .expect("track active process");
    engine
        .finish_active_process("exec-1", Some(0), 2)
        .expect("finish active process");
    engine
        .wait_for_delegate(DelegateRun::new("delegate-1", "worker-a", 2), 2)
        .expect("wait for delegate");
    evidence
        .record_check("fmt", CheckOutcome::Passed, Some("rustfmt clean"), 2)
        .expect("record fmt");
    evidence.add_artifact_ref("artifact:verification-report");
    engine
        .record_evidence(&evidence, 2)
        .expect("record evidence");
    engine
        .complete_delegate("delegate-1", 2)
        .expect("complete delegate");
    engine.resume(2).expect("resume");
    engine.complete("done", 3).expect("complete");

    let stored = RunRecord::try_from(engine.snapshot()).expect("snapshot to record");
    let restored = RunSnapshot::try_from(stored).expect("record to snapshot");

    assert_eq!(restored.id, "run-1");
    assert_eq!(restored.session_id, "session-1");
    assert_eq!(restored.mission_id.as_deref(), Some("mission-1"));
    assert_eq!(restored.status.as_str(), "completed");
    assert_eq!(restored.result.as_deref(), Some("done"));
    assert_eq!(restored.finished_at, Some(3));
    assert!(restored.pending_approvals.is_empty());
    assert!(restored.delegate_runs.is_empty());
    assert!(restored.active_processes.is_empty());
    assert!(
        restored
            .recent_steps
            .iter()
            .any(|step| step.detail.contains("recorded evidence bundle bundle-1"))
    );
    assert!(
        restored
            .recent_steps
            .iter()
            .any(|step| step.detail.contains("run completed"))
    );
    assert_eq!(
        restored.evidence_refs,
        vec![
            "bundle:bundle-1".to_string(),
            "check:fmt".to_string(),
            "artifact:verification-report".to_string(),
        ]
    );
}

#[test]
fn mission_records_round_trip_with_schedule_and_acceptance_criteria() {
    let mission = MissionSpec {
        id: "mission-1".to_string(),
        session_id: "session-1".to_string(),
        objective: "Ship the autonomous runtime".to_string(),
        status: MissionStatus::Running,
        execution_intent: MissionExecutionIntent::Scheduled,
        schedule: MissionSchedule {
            not_before: Some(20),
            interval_seconds: Some(3600),
        },
        acceptance_criteria: vec![
            AcceptanceCriterion::new("criterion-1", "all workspace tests pass").expect("criterion"),
        ],
        created_at: 10,
        updated_at: 11,
        completed_at: None,
    };

    let stored = MissionRecord::try_from(&mission).expect("mission to record");
    let restored = MissionSpec::try_from(stored).expect("record to mission");

    assert_eq!(restored, mission);
}

#[test]
fn plan_records_round_trip_with_typed_items() {
    let snapshot = PlanSnapshot {
        session_id: "session-1".to_string(),
        goal: Some("Ship planning tools".to_string()),
        items: vec![
            PlanItem {
                id: "inspect".to_string(),
                content: "Inspect planning seams".to_string(),
                status: PlanItemStatus::Pending,
                depends_on: Vec::new(),
                notes: Vec::new(),
                blocked_reason: None,
                parent_task_id: None,
            },
            PlanItem {
                id: "persist".to_string(),
                content: "Persist plan snapshot".to_string(),
                status: PlanItemStatus::InProgress,
                depends_on: vec!["inspect".to_string()],
                notes: vec!["Use sqlite".to_string()],
                blocked_reason: None,
                parent_task_id: None,
            },
        ],
        updated_at: 12,
    };

    let stored = PlanRecord::try_from(&snapshot).expect("plan to record");
    let restored = PlanSnapshot::try_from(stored).expect("record to plan");

    assert_eq!(restored, snapshot);
}

#[test]
fn context_offload_records_round_trip_with_artifact_refs() {
    let snapshot = ContextOffloadSnapshot {
        session_id: "session-1".to_string(),
        refs: vec![
            ContextOffloadRef {
                id: "offload-1".to_string(),
                label: "Earlier transcript".to_string(),
                summary: "Design and requirements".to_string(),
                artifact_id: "artifact-offload-1".to_string(),
                token_estimate: 128,
                message_count: 6,
                created_at: 20,
            },
            ContextOffloadRef {
                id: "offload-2".to_string(),
                label: "Large tool output".to_string(),
                summary: "Web fetch dump".to_string(),
                artifact_id: "artifact-offload-2".to_string(),
                token_estimate: 64,
                message_count: 1,
                created_at: 21,
            },
        ],
        updated_at: 22,
    };

    let stored = ContextOffloadRecord::try_from(&snapshot).expect("offload to record");
    let restored = ContextOffloadSnapshot::try_from(stored).expect("record to offload");

    assert_eq!(restored, snapshot);
}

#[test]
fn job_records_round_trip_with_typed_input_and_result() {
    let mut job = JobSpec::mission_turn(
        "job-1",
        "session-1",
        "mission-1",
        Some("run-1"),
        Some("job-root"),
        "Ship the autonomous runtime",
        30,
    );
    job.status = JobStatus::Completed;
    job.result = Some(JobResult::Summary {
        outcome: "done".to_string(),
    });
    job.updated_at = 31;
    job.started_at = Some(30);
    job.finished_at = Some(31);

    let stored = JobRecord::try_from(&job).expect("job to record");
    let restored = JobSpec::try_from(stored).expect("record to job");

    assert_eq!(restored.kind, JobKind::MissionTurn);
    assert_eq!(restored.status, JobStatus::Completed);
    assert_eq!(restored.session_id, "session-1");
    assert_eq!(restored.mission_id.as_deref(), Some("mission-1"));
    assert_eq!(restored.run_id.as_deref(), Some("run-1"));
    assert_eq!(restored.parent_job_id.as_deref(), Some("job-root"));
    assert_eq!(
        restored.input,
        JobExecutionInput::MissionTurn {
            mission_id: "mission-1".to_string(),
            goal: "Ship the autonomous runtime".to_string(),
        }
    );
    assert_eq!(
        restored.result,
        Some(JobResult::Summary {
            outcome: "done".to_string(),
        })
    );
}

#[test]
fn job_records_round_trip_with_waiting_external_callback_metadata() {
    let mut job = JobSpec::delegate(
        "job-a2a",
        "session-parent",
        None,
        None,
        "judge",
        "Review the results",
        vec!["reports/judge.md".to_string()],
        agent_runtime::delegation::DelegateWriteScope::new(vec!["reports".to_string()])
            .expect("write scope"),
        "Short verdict",
        "a2a:judge",
        40,
    );
    job.status = JobStatus::WaitingExternal;
    job.updated_at = 41;
    job.callback = Some(agent_runtime::mission::JobCallbackTarget {
        url: "https://daemon-a.example/v1/a2a/delegations/job-parent/complete".to_string(),
        bearer_token: Some("callback-token".to_string()),
        parent_session_id: "session-parent".to_string(),
        parent_job_id: "job-parent".to_string(),
    });

    let stored = JobRecord::try_from(&job).expect("job to record");
    let restored = JobSpec::try_from(stored).expect("record to job");

    assert_eq!(restored.status, JobStatus::WaitingExternal);
    assert_eq!(
        restored.callback.as_ref().expect("callback").url,
        "https://daemon-a.example/v1/a2a/delegations/job-parent/complete"
    );
    assert_eq!(
        restored
            .callback
            .as_ref()
            .and_then(|callback| callback.bearer_token.as_deref()),
        Some("callback-token")
    );
}

#[test]
fn job_records_reject_mismatched_mission_turn_identifiers() {
    let mut stored = JobRecord::try_from(&JobSpec::mission_turn(
        "job-1",
        "session-1",
        "mission-1",
        Some("run-1"),
        None,
        "Ship the autonomous runtime",
        30,
    ))
    .expect("job to record");
    stored.mission_id = Some("mission-2".to_string());

    assert!(matches!(
        JobSpec::try_from(stored),
        Err(super::RecordConversionError::InvalidJobSpec(
            JobSpecValidationError::MissionIdMismatch { .. }
        ))
    ));
}

#[test]
fn job_records_accept_future_background_chat_turn_payloads() {
    let stored = JobRecord {
        id: "job-chat".to_string(),
        session_id: "session-1".to_string(),
        mission_id: None,
        run_id: Some("run-1".to_string()),
        parent_job_id: None,
        kind: "chat_turn".to_string(),
        status: "queued".to_string(),
        input_json: Some("{\"ChatTurn\":{\"message\":\"hello from the queue\"}}".to_string()),
        result_json: None,
        error: None,
        created_at: 10,
        updated_at: 10,
        started_at: None,
        finished_at: None,
        attempt_count: 0,
        max_attempts: 1,
        lease_owner: None,
        lease_expires_at: None,
        heartbeat_at: None,
        cancel_requested_at: None,
        last_progress_message: Some("queued for background execution".to_string()),
        callback_json: None,
        callback_sent_at: None,
    };

    assert!(JobSpec::try_from(stored).is_ok());
}
