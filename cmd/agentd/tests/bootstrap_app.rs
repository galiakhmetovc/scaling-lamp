use agent_persistence::{
    AppConfig, ConfigError, ContextSummaryRepository, JobRecord, JobRepository, MissionRecord,
    MissionRepository, PersistenceStore, PlanRecord, PlanRepository, RunRecord, RunRepository,
    SessionRecord, SessionRepository, TranscriptRepository,
};
use agent_runtime::mission::{JobSpec, MissionExecutionIntent, MissionSchedule, MissionStatus};
use agent_runtime::permission::{
    PermissionAction, PermissionConfig, PermissionMode, PermissionRule,
};
use agent_runtime::plan::{PlanItem, PlanItemStatus, PlanSnapshot};
use agent_runtime::provider::{ConfiguredProvider, ProviderKind};
use agent_runtime::run::{ApprovalRequest, DelegateRun, RunEngine, RunSnapshot, RunStatus};
use agent_runtime::scheduler::{MissionVerificationSummary, SupervisorAction};
use agent_runtime::session::SessionSettings;
use agent_runtime::tool::{FsWriteInput, ToolCall};
use agent_runtime::verification::VerificationStatus;
use agent_runtime::verification::{CheckOutcome, EvidenceBundle};
use agent_runtime::workspace::WorkspaceRef;
use agentd::bootstrap::{BootstrapError, SessionPreferencesPatch, build_from_config};
use agentd::execution;
use std::fs;
use std::io::{BufRead, BufReader, Cursor, Read, Write};
use std::net::TcpListener;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

#[test]
fn build_from_config_creates_runtime_layout_from_one_root() {
    let temp = tempfile::tempdir().expect("tempdir");
    let data_dir = temp.path().join("state-root");
    let config = AppConfig {
        data_dir: data_dir.clone(),
        ..AppConfig::default()
    };

    let app = build_from_config(config.clone()).expect("build app");

    assert_eq!(app.config, config);
    assert_eq!(app.persistence.config, config);
    assert!(app.persistence.stores.artifacts_dir.is_dir());
    assert!(app.persistence.stores.runs_dir.is_dir());
    assert!(app.persistence.stores.transcripts_dir.is_dir());
    assert!(app.persistence.audit.path.parent().is_some());
}

#[test]
fn build_from_config_rejects_invalid_paths_before_side_effects() {
    let temp = tempfile::tempdir().expect("tempdir");
    let occupied_path = temp.path().join("occupied");
    fs::write(&occupied_path, "not a directory").expect("write marker");

    let error = build_from_config(AppConfig {
        data_dir: occupied_path.clone(),
        ..AppConfig::default()
    })
    .expect_err("invalid data dir must fail");

    assert!(matches!(
        error,
        BootstrapError::Config(ConfigError::InvalidDataDir { .. })
    ));
    assert!(!occupied_path.join("artifacts").exists());
}

#[test]
fn run_with_args_creates_and_shows_sessions_and_missions() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");

    let created_session = app
        .run_with_args([
            "session",
            "create",
            "session-1",
            "Autonomous",
            "CLI",
            "session",
        ])
        .expect("create session");
    assert!(created_session.contains("created session session-1"));

    let shown_session = app
        .run_with_args(["session", "show", "session-1"])
        .expect("show session");
    assert!(shown_session.contains("session-1"));
    assert!(shown_session.contains("Autonomous CLI session"));

    let created_mission = app
        .run_with_args([
            "mission",
            "create",
            "mission-1",
            "session-1",
            "Ship",
            "the",
            "autonomous",
            "supervisor",
        ])
        .expect("create mission");
    assert!(created_mission.contains("created mission mission-1"));

    let shown_mission = app
        .run_with_args(["mission", "show", "mission-1"])
        .expect("show mission");
    assert!(shown_mission.contains("mission-1"));
    assert!(shown_mission.contains("session-1"));
    assert!(shown_mission.contains("Ship the autonomous supervisor"));

    let status = app.run_with_args(["status"]).expect("status");
    assert!(status.contains("permission_mode=default"));
    assert!(status.contains("sessions=1"));
    assert!(status.contains("missions=1"));

    let store = PersistenceStore::open(&app.persistence).expect("open store");
    assert!(
        store
            .get_session("session-1")
            .expect("load session")
            .is_some()
    );
    assert!(
        store
            .get_mission("mission-1")
            .expect("load mission")
            .is_some()
    );
}

#[test]
fn run_with_args_inspects_and_updates_runs_jobs_approvals_and_delegates() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-ops".to_string(),
            title: "Operator session".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            active_mission_id: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");
    store
        .put_mission(&MissionRecord {
            id: "mission-ops".to_string(),
            session_id: "session-ops".to_string(),
            objective: "Handle operator flows".to_string(),
            status: MissionStatus::Ready.as_str().to_string(),
            execution_intent: MissionExecutionIntent::Autonomous.as_str().to_string(),
            schedule_json: serde_json::to_string(&MissionSchedule::once())
                .expect("serialize schedule"),
            acceptance_json: "[]".to_string(),
            created_at: 2,
            updated_at: 2,
            completed_at: None,
        })
        .expect("put mission");

    let mut approval_run = RunEngine::new("run-approval", "session-ops", Some("mission-ops"), 3);
    approval_run.start(4).expect("start run");
    approval_run
        .wait_for_approval(
            ApprovalRequest::new("approval-1", "tool-call-1", "allow exec", 5),
            5,
        )
        .expect("wait for approval");
    let mut evidence = EvidenceBundle::new("bundle-1", "run-approval", 6);
    evidence
        .record_check("fmt", CheckOutcome::Passed, Some("clean"), 6)
        .expect("record fmt");
    approval_run
        .record_evidence(&evidence, 6)
        .expect("record evidence");
    store
        .put_run(&RunRecord::try_from(approval_run.snapshot()).expect("run record"))
        .expect("put approval run");

    let mut delegate_run = RunEngine::new("run-delegate", "session-ops", Some("mission-ops"), 7);
    delegate_run.start(8).expect("start delegate run");
    delegate_run
        .wait_for_delegate(DelegateRun::new("delegate-1", "worker-a", 9), 9)
        .expect("wait for delegate");
    store
        .put_run(&RunRecord::try_from(delegate_run.snapshot()).expect("delegate record"))
        .expect("put delegate run");

    let job = JobSpec::mission_turn(
        "job-1",
        "mission-ops",
        Some("run-approval"),
        None,
        "Handle operator flows",
        10,
    );
    store
        .put_job(&JobRecord::try_from(&job).expect("job record"))
        .expect("put job");

    let run_show = app
        .run_with_args(["run", "show", "run-approval"])
        .expect("show run");
    assert!(run_show.contains("run-approval"));
    assert!(run_show.contains("waiting_approval"));
    assert!(run_show.contains("pending_approvals=1"));

    let approval_list = app
        .run_with_args(["approval", "list", "run-approval"])
        .expect("list approvals");
    assert!(approval_list.contains("approval-1"));
    assert!(approval_list.contains("tool-call-1"));

    let verification_show = app
        .run_with_args(["verification", "show", "run-approval"])
        .expect("show verification");
    assert!(verification_show.contains("bundle:bundle-1"));
    assert!(verification_show.contains("check:fmt"));

    let delegate_list = app
        .run_with_args(["delegate", "list", "run-delegate"])
        .expect("list delegates");
    assert!(delegate_list.contains("delegate-1"));
    assert!(delegate_list.contains("worker-a"));

    let job_show = app
        .run_with_args(["job", "show", "job-1"])
        .expect("show job");
    assert!(job_show.contains("job-1"));
    assert!(job_show.contains("mission_turn"));

    let approval_update = app
        .run_with_args(["approval", "approve", "run-approval", "approval-1"])
        .expect("approve");
    assert!(approval_update.contains("approved approval-1"));

    let updated_run = app
        .run_with_args(["run", "show", "run-approval"])
        .expect("show updated run");
    assert!(updated_run.contains("status=resuming"));
    assert!(updated_run.contains("pending_approvals=0"));

    let persisted = store
        .get_run("run-approval")
        .expect("get updated run")
        .expect("run record exists");
    let snapshot = RunSnapshot::try_from(persisted).expect("snapshot");
    assert_eq!(snapshot.status, RunStatus::Resuming);
    assert!(snapshot.pending_approvals.is_empty());
}

#[test]
fn build_from_config_interrupts_unrecoverable_runs_but_keeps_approvals_pending() {
    let temp = tempfile::tempdir().expect("tempdir");
    let data_dir = temp.path().join("state-root");
    let app = build_from_config(AppConfig {
        data_dir: data_dir.clone(),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-recovery".to_string(),
            title: "Recovery session".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            active_mission_id: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");
    store
        .put_mission(&MissionRecord {
            id: "mission-recovery".to_string(),
            session_id: "session-recovery".to_string(),
            objective: "Recover autonomous work".to_string(),
            status: MissionStatus::Running.as_str().to_string(),
            execution_intent: MissionExecutionIntent::Autonomous.as_str().to_string(),
            schedule_json: serde_json::to_string(&MissionSchedule::once())
                .expect("serialize schedule"),
            acceptance_json: "[]".to_string(),
            created_at: 2,
            updated_at: 2,
            completed_at: None,
        })
        .expect("put mission");

    for record in [
        RunRecord {
            id: "run-running".to_string(),
            session_id: "session-recovery".to_string(),
            mission_id: Some("mission-recovery".to_string()),
            status: RunStatus::Running.as_str().to_string(),
            error: None,
            result: None,
            recent_steps_json: "[]".to_string(),
            evidence_refs_json: "[]".to_string(),
            pending_approvals_json: "[]".to_string(),
            provider_loop_json: "null".to_string(),
            delegate_runs_json: "[]".to_string(),
            started_at: 3,
            updated_at: 4,
            finished_at: None,
        },
        RunRecord {
            id: "run-resuming".to_string(),
            session_id: "session-recovery".to_string(),
            mission_id: Some("mission-recovery".to_string()),
            status: RunStatus::Resuming.as_str().to_string(),
            error: None,
            result: None,
            recent_steps_json: "[]".to_string(),
            evidence_refs_json: "[]".to_string(),
            pending_approvals_json: "[]".to_string(),
            provider_loop_json: "null".to_string(),
            delegate_runs_json: "[]".to_string(),
            started_at: 5,
            updated_at: 6,
            finished_at: None,
        },
        RunRecord {
            id: "run-process".to_string(),
            session_id: "session-recovery".to_string(),
            mission_id: Some("mission-recovery".to_string()),
            status: RunStatus::WaitingProcess.as_str().to_string(),
            error: None,
            result: None,
            recent_steps_json: "[]".to_string(),
            evidence_refs_json: "[]".to_string(),
            pending_approvals_json: "[]".to_string(),
            provider_loop_json: "null".to_string(),
            delegate_runs_json: "[]".to_string(),
            started_at: 7,
            updated_at: 8,
            finished_at: None,
        },
        RunRecord {
            id: "run-delegate".to_string(),
            session_id: "session-recovery".to_string(),
            mission_id: Some("mission-recovery".to_string()),
            status: RunStatus::WaitingDelegate.as_str().to_string(),
            error: None,
            result: None,
            recent_steps_json: "[]".to_string(),
            evidence_refs_json: "[]".to_string(),
            pending_approvals_json: "[]".to_string(),
            provider_loop_json: "null".to_string(),
            delegate_runs_json: serde_json::to_string(&vec![DelegateRun::new(
                "delegate-1",
                "worker-a",
                9,
            )])
            .expect("serialize delegates"),
            started_at: 9,
            updated_at: 10,
            finished_at: None,
        },
        RunRecord {
            id: "run-approval".to_string(),
            session_id: "session-recovery".to_string(),
            mission_id: Some("mission-recovery".to_string()),
            status: RunStatus::WaitingApproval.as_str().to_string(),
            error: None,
            result: None,
            recent_steps_json: "[]".to_string(),
            evidence_refs_json: "[]".to_string(),
            pending_approvals_json: serde_json::to_string(&vec![ApprovalRequest::new(
                "approval-1",
                "tool-call-1",
                "allow exec",
                11,
            )])
            .expect("serialize approvals"),
            provider_loop_json: "null".to_string(),
            delegate_runs_json: "[]".to_string(),
            started_at: 11,
            updated_at: 12,
            finished_at: None,
        },
    ] {
        store.put_run(&record).expect("put run");
    }

    drop(store);
    drop(app);

    let reopened = build_from_config(AppConfig {
        data_dir,
        ..AppConfig::default()
    })
    .expect("reopen app");
    let reopened_store = PersistenceStore::open(&reopened.persistence).expect("reopen store");

    for run_id in ["run-running", "run-resuming", "run-process", "run-delegate"] {
        let interrupted = RunSnapshot::try_from(
            reopened_store
                .get_run(run_id)
                .expect("get interrupted run")
                .expect("interrupted run exists"),
        )
        .expect("interrupted snapshot");
        assert_eq!(interrupted.status, RunStatus::Interrupted);
        assert_eq!(
            interrupted.error.as_deref(),
            Some("runtime restart interrupted a non-recoverable run state")
        );
    }

    let pending = RunSnapshot::try_from(
        reopened_store
            .get_run("run-approval")
            .expect("get approval run")
            .expect("approval run exists"),
    )
    .expect("approval snapshot");
    assert_eq!(pending.status, RunStatus::WaitingApproval);
    assert_eq!(pending.pending_approvals.len(), 1);
}

#[test]
fn run_show_surfaces_error_details_for_interrupted_runs() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-show".to_string(),
            title: "Show session".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            active_mission_id: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");
    store
        .put_run(&RunRecord {
            id: "run-interrupted".to_string(),
            session_id: "session-show".to_string(),
            mission_id: None,
            status: RunStatus::Interrupted.as_str().to_string(),
            error: Some("runtime restart interrupted a non-recoverable run state".to_string()),
            result: None,
            recent_steps_json: "[]".to_string(),
            evidence_refs_json: "[]".to_string(),
            pending_approvals_json: "[]".to_string(),
            provider_loop_json: "null".to_string(),
            delegate_runs_json: "[]".to_string(),
            started_at: 3,
            updated_at: 4,
            finished_at: Some(4),
        })
        .expect("put run");

    let shown = app
        .run_with_args(["run", "show", "run-interrupted"])
        .expect("show run");
    assert!(shown.contains("status=interrupted"));
    assert!(shown.contains("error=runtime restart interrupted a non-recoverable run state"));
}

#[test]
fn run_with_args_provider_smoke_uses_the_configured_driver() {
    let (api_base, requests, handle) = spawn_json_server(
        r#"{
                "id":"resp_123",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_1",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"hello world"
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":11,"output_tokens":7,"total_tokens":18}
            }"#,
    );
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(format!("{api_base}/v1")),
            api_key: Some("test-key".to_string()),
            default_model: Some("gpt-5.4".to_string()),
        },
        ..AppConfig::default()
    })
    .expect("build app");

    let output = app
        .run_with_args(["provider", "smoke", "Say", "hi"])
        .expect("provider smoke");
    let raw_request = requests.recv().expect("raw request");
    handle.join().expect("join server");

    assert!(output.contains("provider name=openai-responses"));
    assert!(output.contains("response_id=resp_123"));
    assert!(output.contains("model=gpt-5.4"));
    assert!(output.contains("output=hello world"));

    let normalized_request = raw_request.to_ascii_lowercase();
    assert!(normalized_request.contains("/v1/responses"));
    assert!(normalized_request.contains("\"text\":\"say hi\""));
}

#[test]
fn execute_chat_turn_uses_the_session_model_override() {
    let (api_base, requests, handle) = spawn_json_server(
        r#"{
                "id":"resp_model_override",
                "model":"glm-5-air",
                "output":[
                    {
                        "id":"msg_1",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"model override ok"
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":5,"output_tokens":4,"total_tokens":9}
            }"#,
    );
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(format!("{api_base}/v1")),
            api_key: Some("test-key".to_string()),
            default_model: Some("gpt-5.4".to_string()),
        },
        ..AppConfig::default()
    })
    .expect("build app");
    let session = app
        .create_session_auto(Some("Model override session"))
        .expect("create session");
    app.update_session_preferences(
        &session.id,
        SessionPreferencesPatch {
            model: Some(Some("glm-5-air".to_string())),
            ..SessionPreferencesPatch::default()
        },
    )
    .expect("update model");

    let report = app
        .execute_chat_turn(&session.id, "hello model override", 100)
        .expect("chat turn");
    let raw_request = requests.recv().expect("provider request");
    handle.join().expect("join provider");

    assert_eq!(report.output_text, "model override ok");
    assert!(raw_request.contains("\"model\":\"glm-5-air\""));
}

#[test]
fn supervisor_tick_queues_due_mission_turn_jobs_from_persisted_state() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-queue".to_string(),
            title: "Queue session".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            active_mission_id: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");
    store
        .put_mission(&MissionRecord {
            id: "mission-queue".to_string(),
            session_id: "session-queue".to_string(),
            objective: "Queue a mission turn".to_string(),
            status: MissionStatus::Ready.as_str().to_string(),
            execution_intent: MissionExecutionIntent::Autonomous.as_str().to_string(),
            schedule_json: serde_json::to_string(&MissionSchedule::once())
                .expect("serialize schedule"),
            acceptance_json: "[]".to_string(),
            created_at: 2,
            updated_at: 2,
            completed_at: None,
        })
        .expect("put mission");

    let report = app.supervisor_tick(60, &[]).expect("run supervisor tick");

    assert_eq!(
        report.actions,
        vec![SupervisorAction::QueueJob(Box::new(JobSpec::mission_turn(
            "mission-queue-mission-turn-60",
            "mission-queue",
            None,
            None,
            "Queue a mission turn",
            60,
        )))]
    );

    let queued_job = store
        .get_job("mission-queue-mission-turn-60")
        .expect("get queued job")
        .expect("queued job exists");
    assert_eq!(queued_job.status, "queued");
    assert_eq!(queued_job.created_at, 60);
}

#[test]
fn supervisor_tick_dispatches_queued_jobs_and_completes_verified_missions() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-ops".to_string(),
            title: "Execution session".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            active_mission_id: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");

    store
        .put_mission(&MissionRecord {
            id: "mission-ready".to_string(),
            session_id: "session-ops".to_string(),
            objective: "Dispatch work".to_string(),
            status: MissionStatus::Ready.as_str().to_string(),
            execution_intent: MissionExecutionIntent::Autonomous.as_str().to_string(),
            schedule_json: serde_json::to_string(&MissionSchedule::once())
                .expect("serialize schedule"),
            acceptance_json: "[]".to_string(),
            created_at: 2,
            updated_at: 2,
            completed_at: None,
        })
        .expect("put ready mission");
    store
        .put_job(
            &JobRecord::try_from(&JobSpec::mission_turn(
                "job-dispatch",
                "mission-ready",
                None,
                None,
                "Dispatch work",
                10,
            ))
            .expect("job record"),
        )
        .expect("put queued job");

    store
        .put_mission(&MissionRecord {
            id: "mission-done".to_string(),
            session_id: "session-ops".to_string(),
            objective: "Complete work".to_string(),
            status: MissionStatus::Running.as_str().to_string(),
            execution_intent: MissionExecutionIntent::Autonomous.as_str().to_string(),
            schedule_json: serde_json::to_string(&MissionSchedule::once())
                .expect("serialize schedule"),
            acceptance_json: "[]".to_string(),
            created_at: 3,
            updated_at: 3,
            completed_at: None,
        })
        .expect("put running mission");
    store
        .put_run(&RunRecord {
            id: "run-done".to_string(),
            session_id: "session-ops".to_string(),
            mission_id: Some("mission-done".to_string()),
            status: RunStatus::Completed.as_str().to_string(),
            error: None,
            result: Some("done".to_string()),
            recent_steps_json: "[]".to_string(),
            evidence_refs_json: "[]".to_string(),
            pending_approvals_json: "[]".to_string(),
            provider_loop_json: "null".to_string(),
            delegate_runs_json: "[]".to_string(),
            started_at: 20,
            updated_at: 21,
            finished_at: Some(21),
        })
        .expect("put completed run");

    let report = app
        .supervisor_tick(
            90,
            &[MissionVerificationSummary {
                mission_id: "mission-done".to_string(),
                status: VerificationStatus::Passed,
                missing_required_checks: Vec::new(),
                open_risks: Vec::new(),
            }],
        )
        .expect("run supervisor tick");

    assert!(report.actions.contains(&SupervisorAction::DispatchJob {
        job_id: "job-dispatch".to_string(),
        kind: agent_runtime::mission::JobKind::MissionTurn,
    }));
    assert!(report.actions.contains(&SupervisorAction::CompleteMission {
        mission_id: "mission-done".to_string(),
    }));

    let dispatched_job = store
        .get_job("job-dispatch")
        .expect("get dispatched job")
        .expect("dispatched job exists");
    assert_eq!(dispatched_job.status, "running");
    assert_eq!(dispatched_job.started_at, Some(90));

    let completed_mission = store
        .get_mission("mission-done")
        .expect("get completed mission")
        .expect("completed mission exists");
    assert_eq!(completed_mission.status, "completed");
    assert_eq!(completed_mission.completed_at, Some(90));
}

#[test]
fn execute_mission_turn_job_creates_a_run_calls_provider_and_persists_transcript() {
    let (api_base, requests, handle) = spawn_json_server(
        r#"{
                "id":"resp_456",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_1",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Mission result"
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":15,"output_tokens":5,"total_tokens":20}
            }"#,
    );
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(format!("{api_base}/v1")),
            api_key: Some("test-key".to_string()),
            default_model: Some("gpt-5.4".to_string()),
        },
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-turn".to_string(),
            title: "Mission turn session".to_string(),
            prompt_override: Some("Reply tersely.".to_string()),
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            active_mission_id: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");
    store
        .put_mission(&MissionRecord {
            id: "mission-turn".to_string(),
            session_id: "session-turn".to_string(),
            objective: "Ship one provider-backed mission turn".to_string(),
            status: MissionStatus::Ready.as_str().to_string(),
            execution_intent: MissionExecutionIntent::Autonomous.as_str().to_string(),
            schedule_json: serde_json::to_string(&MissionSchedule::once())
                .expect("serialize schedule"),
            acceptance_json: "[]".to_string(),
            created_at: 2,
            updated_at: 2,
            completed_at: None,
        })
        .expect("put mission");
    store
        .put_job(
            &JobRecord::try_from(&JobSpec::mission_turn(
                "job-turn",
                "mission-turn",
                None,
                None,
                "Draft a short mission update",
                3,
            ))
            .expect("job record"),
        )
        .expect("put job");

    let report = app
        .execute_mission_turn_job("job-turn", 10)
        .expect("execute mission turn");
    let raw_request = requests.recv().expect("raw request");
    handle.join().expect("join server");

    assert_eq!(report.run_id, "run-job-turn");
    assert_eq!(report.response_id, "resp_456");
    assert_eq!(report.output_text, "Mission result");

    let run = store
        .get_run("run-job-turn")
        .expect("get run")
        .expect("run exists");
    assert_eq!(run.status, "completed");
    assert_eq!(run.result.as_deref(), Some("Mission result"));

    let job = store
        .get_job("job-turn")
        .expect("get job")
        .expect("job exists");
    assert_eq!(job.status, "completed");
    assert_eq!(job.run_id.as_deref(), Some("run-job-turn"));
    assert_eq!(job.finished_at, Some(10));

    let mission = store
        .get_mission("mission-turn")
        .expect("get mission")
        .expect("mission exists");
    assert_eq!(mission.status, "running");

    let transcripts = store
        .list_transcripts_for_session("session-turn")
        .expect("list transcripts");
    assert_eq!(transcripts.len(), 2);
    assert_eq!(transcripts[0].kind, "user");
    assert_eq!(transcripts[0].content, "Draft a short mission update");
    assert_eq!(transcripts[1].kind, "assistant");
    assert_eq!(transcripts[1].content, "Mission result");

    let normalized_request = raw_request.to_ascii_lowercase();
    assert!(normalized_request.contains("/v1/responses"));
    assert!(normalized_request.contains("\"instructions\":\"reply tersely.\""));
    assert!(normalized_request.contains("\"text\":\"draft a short mission update\""));
}

#[test]
fn tool_execution_pauses_for_approval_then_resumes_and_records_evidence() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace_root = temp.path().join("workspace");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-tool".to_string(),
            title: "Tool session".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            active_mission_id: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");
    store
        .put_mission(&MissionRecord {
            id: "mission-tool".to_string(),
            session_id: "session-tool".to_string(),
            objective: "Drive an approval-gated tool".to_string(),
            status: MissionStatus::Running.as_str().to_string(),
            execution_intent: MissionExecutionIntent::Autonomous.as_str().to_string(),
            schedule_json: serde_json::to_string(&MissionSchedule::once())
                .expect("serialize schedule"),
            acceptance_json: "[]".to_string(),
            created_at: 2,
            updated_at: 2,
            completed_at: None,
        })
        .expect("put mission");

    let mut job = JobSpec::mission_turn(
        "job-tool",
        "mission-tool",
        Some("run-tool"),
        None,
        "Write a mission artifact",
        3,
    );
    job.status = agent_runtime::mission::JobStatus::Running;
    job.started_at = Some(4);
    job.updated_at = 4;

    let mut run = RunEngine::new("run-tool", "session-tool", Some("mission-tool"), 4);
    run.start(4).expect("start run");
    store
        .put_run(&RunRecord::try_from(run.snapshot()).expect("run record"))
        .expect("put run");
    store
        .put_job(&JobRecord::try_from(&job).expect("job record"))
        .expect("put job");

    let tool_call = ToolCall::FsWrite(FsWriteInput {
        path: "notes/out.txt".to_string(),
        content: "tool output\n".to_string(),
    });

    let approval = app
        .request_tool_approval("job-tool", "run-tool", &tool_call, 20)
        .expect("request approval");
    assert_eq!(approval.run_status, RunStatus::WaitingApproval);
    assert_eq!(
        approval.approval_id.as_deref(),
        Some("approval-job-tool-fs_write")
    );

    let waiting_run = RunSnapshot::try_from(
        store
            .get_run("run-tool")
            .expect("get waiting run")
            .expect("waiting run exists"),
    )
    .expect("waiting snapshot");
    assert_eq!(waiting_run.status, RunStatus::WaitingApproval);
    assert_eq!(waiting_run.pending_approvals.len(), 1);

    let blocked_job = store
        .get_job("job-tool")
        .expect("get blocked job")
        .expect("blocked job exists");
    assert_eq!(blocked_job.status, "blocked");
    assert!(!workspace_root.join("notes/out.txt").exists());

    let mut evidence = EvidenceBundle::new("bundle-tool", "run-tool", 21);
    evidence
        .record_check("fmt", CheckOutcome::Passed, Some("clean"), 21)
        .expect("record fmt");
    evidence
        .record_check("clippy", CheckOutcome::Passed, Some("clean"), 21)
        .expect("record clippy");
    evidence
        .record_check("test", CheckOutcome::Passed, Some("green"), 21)
        .expect("record test");
    evidence.add_artifact_ref("artifact:notes/out.txt");

    let resumed = app
        .resume_tool_call(execution::ToolResumeRequest {
            job_id: "job-tool",
            run_id: "run-tool",
            approval_id: approval.approval_id.as_deref().expect("approval id"),
            tool_call: &tool_call,
            workspace_root: &workspace_root,
            evidence: Some(&evidence),
            now: 21,
        })
        .expect("resume tool call");
    assert_eq!(resumed.run_status, RunStatus::Completed);
    assert_eq!(
        resumed.output_summary.as_deref(),
        Some("fs_write path=notes/out.txt bytes=12")
    );
    assert!(
        resumed
            .evidence_refs
            .contains(&"bundle:bundle-tool".to_string())
    );
    assert!(resumed.evidence_refs.contains(&"check:fmt".to_string()));
    assert!(resumed.evidence_refs.contains(&"check:clippy".to_string()));
    assert!(resumed.evidence_refs.contains(&"check:test".to_string()));
    assert!(
        resumed
            .evidence_refs
            .contains(&"artifact:notes/out.txt".to_string())
    );

    assert_eq!(
        fs::read_to_string(workspace_root.join("notes/out.txt")).expect("read workspace file"),
        "tool output\n"
    );

    let completed_run = RunSnapshot::try_from(
        store
            .get_run("run-tool")
            .expect("get completed run")
            .expect("completed run exists"),
    )
    .expect("completed snapshot");
    assert_eq!(completed_run.status, RunStatus::Completed);
    assert!(completed_run.pending_approvals.is_empty());
    assert_eq!(
        completed_run.result.as_deref(),
        Some("fs_write path=notes/out.txt bytes=12")
    );
    assert!(
        completed_run
            .evidence_refs
            .contains(&"bundle:bundle-tool".to_string())
    );

    let completed_job = store
        .get_job("job-tool")
        .expect("get completed job")
        .expect("completed job exists");
    assert_eq!(completed_job.status, "completed");
    assert_eq!(
        completed_job.result_json.as_deref(),
        Some(r#"{"Summary":{"outcome":"fs_write path=notes/out.txt bytes=12"}}"#)
    );

    let mission = store
        .get_mission("mission-tool")
        .expect("get mission")
        .expect("mission exists");
    assert_eq!(mission.status, "running");
    assert_eq!(mission.updated_at, 21);
}

#[test]
fn accept_edits_mode_skips_approval_for_filesystem_edits() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        permissions: PermissionConfig {
            mode: PermissionMode::AcceptEdits,
            rules: Vec::new(),
        },
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-allow".to_string(),
            title: "Allow session".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            active_mission_id: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");
    store
        .put_mission(&MissionRecord {
            id: "mission-allow".to_string(),
            session_id: "session-allow".to_string(),
            objective: "Allow edit".to_string(),
            status: MissionStatus::Running.as_str().to_string(),
            execution_intent: MissionExecutionIntent::Autonomous.as_str().to_string(),
            schedule_json: serde_json::to_string(&MissionSchedule::once())
                .expect("serialize schedule"),
            acceptance_json: "[]".to_string(),
            created_at: 2,
            updated_at: 2,
            completed_at: None,
        })
        .expect("put mission");

    let mut job = JobSpec::mission_turn(
        "job-allow",
        "mission-allow",
        Some("run-allow"),
        None,
        "Write without approval",
        3,
    );
    job.status = agent_runtime::mission::JobStatus::Running;
    job.started_at = Some(4);
    job.updated_at = 4;

    let mut run = RunEngine::new("run-allow", "session-allow", Some("mission-allow"), 4);
    run.start(4).expect("start run");
    store
        .put_run(&RunRecord::try_from(run.snapshot()).expect("run record"))
        .expect("put run");
    store
        .put_job(&JobRecord::try_from(&job).expect("job record"))
        .expect("put job");

    let tool_call = ToolCall::FsWrite(FsWriteInput {
        path: "notes/out.txt".to_string(),
        content: "allowed\n".to_string(),
    });

    let report = app
        .request_tool_approval("job-allow", "run-allow", &tool_call, 20)
        .expect("request tool gate");
    assert_eq!(report.run_status, RunStatus::Running);
    assert_eq!(report.approval_id, None);

    let run_snapshot = RunSnapshot::try_from(
        store
            .get_run("run-allow")
            .expect("get run")
            .expect("run exists"),
    )
    .expect("run snapshot");
    assert_eq!(run_snapshot.status, RunStatus::Running);
    assert!(run_snapshot.pending_approvals.is_empty());
}

#[test]
fn deny_rule_fails_tool_execution_before_approval_is_created() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        permissions: PermissionConfig {
            mode: PermissionMode::AcceptEdits,
            rules: vec![PermissionRule {
                action: PermissionAction::Deny,
                tool: Some("fs_write".to_string()),
                family: None,
                path_prefix: Some("secrets/".to_string()),
            }],
        },
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-deny".to_string(),
            title: "Deny session".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            active_mission_id: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");
    store
        .put_mission(&MissionRecord {
            id: "mission-deny".to_string(),
            session_id: "session-deny".to_string(),
            objective: "Deny edit".to_string(),
            status: MissionStatus::Running.as_str().to_string(),
            execution_intent: MissionExecutionIntent::Autonomous.as_str().to_string(),
            schedule_json: serde_json::to_string(&MissionSchedule::once())
                .expect("serialize schedule"),
            acceptance_json: "[]".to_string(),
            created_at: 2,
            updated_at: 2,
            completed_at: None,
        })
        .expect("put mission");

    let mut job = JobSpec::mission_turn(
        "job-deny",
        "mission-deny",
        Some("run-deny"),
        None,
        "Write forbidden file",
        3,
    );
    job.status = agent_runtime::mission::JobStatus::Running;
    job.started_at = Some(4);
    job.updated_at = 4;

    let mut run = RunEngine::new("run-deny", "session-deny", Some("mission-deny"), 4);
    run.start(4).expect("start run");
    store
        .put_run(&RunRecord::try_from(run.snapshot()).expect("run record"))
        .expect("put run");
    store
        .put_job(&JobRecord::try_from(&job).expect("job record"))
        .expect("put job");

    let tool_call = ToolCall::FsWrite(FsWriteInput {
        path: "secrets/out.txt".to_string(),
        content: "denied\n".to_string(),
    });

    let error = app
        .request_tool_approval("job-deny", "run-deny", &tool_call, 20)
        .expect_err("deny rule must fail");
    assert!(error.to_string().contains("permission denied"));

    let run_snapshot = RunSnapshot::try_from(
        store
            .get_run("run-deny")
            .expect("get run")
            .expect("run exists"),
    )
    .expect("run snapshot");
    assert_eq!(run_snapshot.status, RunStatus::Failed);
    assert!(run_snapshot.pending_approvals.is_empty());

    let failed_job = store
        .get_job("job-deny")
        .expect("get job")
        .expect("job exists");
    assert_eq!(failed_job.status, "failed");
    assert!(
        failed_job
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("secrets/")
    );
}

#[test]
fn run_with_args_executes_mission_ticks_and_jobs() {
    let (api_base, requests, handle) = spawn_json_server(
        r#"{
                "id":"resp_cli",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_1",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"CLI mission result"
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":12,"output_tokens":4,"total_tokens":16}
            }"#,
    );
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(format!("{api_base}/v1")),
            api_key: Some("test-key".to_string()),
            default_model: Some("gpt-5.4".to_string()),
        },
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-cli".to_string(),
            title: "CLI session".to_string(),
            prompt_override: Some("Reply tersely.".to_string()),
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            active_mission_id: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");
    store
        .put_mission(&MissionRecord {
            id: "mission-cli".to_string(),
            session_id: "session-cli".to_string(),
            objective: "Drive one CLI mission".to_string(),
            status: MissionStatus::Ready.as_str().to_string(),
            execution_intent: MissionExecutionIntent::Autonomous.as_str().to_string(),
            schedule_json: serde_json::to_string(&MissionSchedule::once())
                .expect("serialize schedule"),
            acceptance_json: "[]".to_string(),
            created_at: 2,
            updated_at: 2,
            completed_at: None,
        })
        .expect("put mission");

    let tick = app
        .run_with_args(["mission", "tick", "60"])
        .expect("mission tick");
    assert!(tick.contains("queued_jobs=1"));
    assert!(tick.contains("mission-cli-mission-turn-60"));

    let executed = app
        .run_with_args(["job", "execute", "mission-cli-mission-turn-60", "61"])
        .expect("job execute");
    let raw_request = requests.recv().expect("raw request");
    handle.join().expect("join server");

    assert!(executed.contains("job execute id=mission-cli-mission-turn-60"));
    assert!(executed.contains("run_id=run-mission-cli-mission-turn-60"));
    assert!(executed.contains("response_id=resp_cli"));
    assert!(executed.contains("output=CLI mission result"));

    let executed_job = store
        .get_job("mission-cli-mission-turn-60")
        .expect("get executed job")
        .expect("job exists");
    assert_eq!(executed_job.status, "completed");

    let executed_run = store
        .get_run("run-mission-cli-mission-turn-60")
        .expect("get executed run")
        .expect("run exists");
    assert_eq!(executed_run.status, "completed");
    assert_eq!(executed_run.result.as_deref(), Some("CLI mission result"));

    let transcripts = store
        .list_transcripts_for_session("session-cli")
        .expect("list transcripts");
    assert_eq!(transcripts.len(), 2);

    let normalized_request = raw_request.to_ascii_lowercase();
    assert!(normalized_request.contains("/v1/responses"));
    assert!(normalized_request.contains("\"instructions\":\"reply tersely.\""));
    assert!(normalized_request.contains("\"text\":\"drive one cli mission\""));
}

#[test]
fn session_transcript_view_renders_entries_in_chronological_order() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-chat".to_string(),
            title: "Chat session".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            active_mission_id: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");
    store
        .put_transcript(&agent_persistence::TranscriptRecord {
            id: "msg-2".to_string(),
            session_id: "session-chat".to_string(),
            run_id: None,
            kind: "assistant".to_string(),
            content: "Hi".to_string(),
            created_at: 11,
        })
        .expect("put assistant transcript");
    store
        .put_transcript(&agent_persistence::TranscriptRecord {
            id: "msg-1".to_string(),
            session_id: "session-chat".to_string(),
            run_id: None,
            kind: "user".to_string(),
            content: "Hello".to_string(),
            created_at: 10,
        })
        .expect("put user transcript");

    let transcript = app
        .session_transcript("session-chat")
        .expect("load transcript view");

    assert_eq!(transcript.session_id, "session-chat");
    assert_eq!(transcript.entries.len(), 2);
    assert_eq!(transcript.entries[0].role, "user");
    assert_eq!(transcript.entries[0].content, "Hello");
    assert_eq!(transcript.entries[1].role, "assistant");
    assert_eq!(transcript.entries[1].content, "Hi");
    assert_eq!(transcript.render(), "[10] user: Hello\n[11] assistant: Hi");
}

#[test]
fn execute_chat_turn_creates_a_run_and_appends_transcript_history() {
    let (api_base, requests, handle) = spawn_json_server(
        r#"{
                "id":"resp_chat",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_1",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Hi back"
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":18,"output_tokens":3,"total_tokens":21}
            }"#,
    );
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(format!("{api_base}/v1")),
            api_key: Some("test-key".to_string()),
            default_model: Some("gpt-5.4".to_string()),
        },
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-chat-turn".to_string(),
            title: "Chat turn session".to_string(),
            prompt_override: Some("Be concise.".to_string()),
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            active_mission_id: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");
    store
        .put_transcript(&agent_persistence::TranscriptRecord {
            id: "msg-1".to_string(),
            session_id: "session-chat-turn".to_string(),
            run_id: None,
            kind: "user".to_string(),
            content: "Hello there".to_string(),
            created_at: 2,
        })
        .expect("put prior user transcript");
    store
        .put_transcript(&agent_persistence::TranscriptRecord {
            id: "msg-2".to_string(),
            session_id: "session-chat-turn".to_string(),
            run_id: None,
            kind: "assistant".to_string(),
            content: "General Kenobi".to_string(),
            created_at: 3,
        })
        .expect("put prior assistant transcript");

    let report = app
        .execute_chat_turn("session-chat-turn", "How are you?", 10)
        .expect("execute chat turn");
    let raw_request = requests.recv().expect("raw request");
    handle.join().expect("join server");

    assert_eq!(report.run_id, "run-chat-session-chat-turn-10");
    assert_eq!(report.response_id, "resp_chat");
    assert_eq!(report.output_text, "Hi back");

    let run = store
        .get_run("run-chat-session-chat-turn-10")
        .expect("get run")
        .expect("run exists");
    assert_eq!(run.status, "completed");
    assert_eq!(run.result.as_deref(), Some("Hi back"));

    let transcript = app
        .session_transcript("session-chat-turn")
        .expect("load transcript");
    assert_eq!(transcript.entries.len(), 4);
    assert_eq!(transcript.entries[2].role, "user");
    assert_eq!(transcript.entries[2].content, "How are you?");
    assert_eq!(transcript.entries[3].role, "assistant");
    assert_eq!(transcript.entries[3].content, "Hi back");

    let normalized_request = raw_request.to_ascii_lowercase();
    assert!(normalized_request.contains("/v1/responses"));
    assert!(normalized_request.contains("\"instructions\":\"be concise.\""));
    assert!(normalized_request.contains("\"text\":\"hello there\""));
    assert!(normalized_request.contains("\"text\":\"general kenobi\""));
    assert!(normalized_request.contains("\"text\":\"how are you?\""));
}

#[test]
fn execute_chat_turn_can_finish_after_an_allowed_web_tool_call() {
    let (web_base, web_requests, web_handle) = spawn_text_server("/doc", "local doc");
    let first_provider_response = format!(
        r#"{{
                "id":"resp_tool_call",
                "model":"gpt-5.4",
                "output":[
                    {{
                        "id":"fc_1",
                        "type":"function_call",
                        "status":"completed",
                        "call_id":"call_web_fetch",
                        "name":"web_fetch",
                        "arguments":"{{\"url\":\"{}\"}}"
                    }}
                ],
                "usage":{{"input_tokens":19,"output_tokens":7,"total_tokens":26}}
            }}"#,
        web_base
    );
    let (provider_api_base, provider_requests, provider_handle) = spawn_json_server_sequence(vec![
        first_provider_response,
        r#"{
                    "id":"resp_tool_final",
                    "model":"gpt-5.4",
                    "output":[
                        {
                            "id":"msg_1",
                            "type":"message",
                            "status":"completed",
                            "role":"assistant",
                            "content":[
                                {
                                    "type":"output_text",
                                    "text":"Fetched local doc"
                                }
                            ]
                        }
                    ],
                    "usage":{"input_tokens":31,"output_tokens":4,"total_tokens":35}
                }"#
        .to_string(),
    ]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(format!("{provider_api_base}/v1")),
            api_key: Some("test-key".to_string()),
            default_model: Some("gpt-5.4".to_string()),
        },
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-chat-tool".to_string(),
            title: "Chat tool session".to_string(),
            prompt_override: Some("Use tools when useful.".to_string()),
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            active_mission_id: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");

    let report = app
        .execute_chat_turn("session-chat-tool", "Fetch the local doc", 10)
        .expect("execute chat turn");
    let first_request = provider_requests.recv().expect("first provider request");
    let second_request = provider_requests.recv().expect("second provider request");
    let web_request = web_requests.recv().expect("web request");
    provider_handle.join().expect("join provider server");
    web_handle.join().expect("join web server");

    assert_eq!(report.run_id, "run-chat-session-chat-tool-10");
    assert_eq!(report.response_id, "resp_tool_final");
    assert_eq!(report.output_text, "Fetched local doc");

    let run = store
        .get_run("run-chat-session-chat-tool-10")
        .expect("get run")
        .expect("run exists");
    assert_eq!(run.status, "completed");
    assert_eq!(run.result.as_deref(), Some("Fetched local doc"));

    let transcript = app
        .session_transcript("session-chat-tool")
        .expect("load transcript");
    assert_eq!(transcript.entries.len(), 2);
    assert_eq!(transcript.entries[0].content, "Fetch the local doc");
    assert_eq!(transcript.entries[1].content, "Fetched local doc");

    let normalized_first = first_request.to_ascii_lowercase();
    assert!(normalized_first.contains("\"tools\""));
    assert!(normalized_first.contains("\"name\":\"web_fetch\""));
    assert!(normalized_first.contains("\"text\":\"fetch the local doc\""));

    let normalized_second = second_request.to_ascii_lowercase();
    assert!(normalized_second.contains("\"previous_response_id\":\"resp_tool_call\""));
    assert!(normalized_second.contains("\"type\":\"function_call_output\""));
    assert!(normalized_second.contains("local doc"));
    assert!(!normalized_second.contains("\"text\":\"fetch the local doc\""));

    let normalized_web = web_request.to_ascii_lowercase();
    assert!(normalized_web.contains("get /doc http/1.1"));
    assert!(web_base.contains("127.0.0.1"));
}

#[test]
fn execute_chat_turn_can_finish_after_an_allowed_web_tool_call_with_zai() {
    let (web_base, web_requests, web_handle) = spawn_text_server("/doc", "local doc");
    let first_provider_response = format!(
        r#"{{
                "id":"chatcmpl-tool-zai-1",
                "model":"glm-5.1",
                "choices":[
                    {{
                        "index":0,
                        "finish_reason":"tool_calls",
                        "message":{{
                            "role":"assistant",
                            "content":"",
                            "tool_calls":[
                                {{
                                    "id":"call_web_fetch",
                                    "type":"function",
                                    "function":{{
                                        "name":"web_fetch",
                                        "arguments":"{{\"url\":\"{}\"}}"
                                    }}
                                }}
                            ]
                        }}
                    }}
                ],
                "usage":{{"prompt_tokens":19,"completion_tokens":7,"total_tokens":26}}
            }}"#,
        web_base
    );
    let (provider_api_base, provider_requests, provider_handle) = spawn_json_server_sequence(vec![
        first_provider_response,
        r#"{
                    "id":"chatcmpl-tool-zai-2",
                    "model":"glm-5.1",
                    "choices":[
                        {
                            "index":0,
                            "finish_reason":"stop",
                            "message":{
                                "role":"assistant",
                                "content":"Fetched local doc through z.ai"
                            }
                        }
                    ],
                    "usage":{"prompt_tokens":31,"completion_tokens":4,"total_tokens":35}
                }"#
        .to_string(),
    ]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        provider: ConfiguredProvider {
            kind: ProviderKind::ZaiChatCompletions,
            api_base: Some(format!("{provider_api_base}/v1")),
            api_key: Some("test-key".to_string()),
            default_model: Some("glm-5.1".to_string()),
        },
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-chat-tool-zai".to_string(),
            title: "Chat tool zai session".to_string(),
            prompt_override: Some("Use tools when useful.".to_string()),
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            active_mission_id: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");

    let report = app
        .execute_chat_turn("session-chat-tool-zai", "Fetch the local doc", 10)
        .expect("execute chat turn");
    let first_request = provider_requests.recv().expect("first provider request");
    let second_request = provider_requests.recv().expect("second provider request");
    let web_request = web_requests.recv().expect("web request");
    provider_handle.join().expect("join provider server");
    web_handle.join().expect("join web server");

    assert_eq!(report.run_id, "run-chat-session-chat-tool-zai-10");
    assert_eq!(report.response_id, "chatcmpl-tool-zai-2");
    assert_eq!(report.output_text, "Fetched local doc through z.ai");

    let run = store
        .get_run("run-chat-session-chat-tool-zai-10")
        .expect("get run")
        .expect("run exists");
    assert_eq!(run.status, "completed");
    assert_eq!(
        run.result.as_deref(),
        Some("Fetched local doc through z.ai")
    );

    let transcript = app
        .session_transcript("session-chat-tool-zai")
        .expect("load transcript");
    assert_eq!(transcript.entries.len(), 2);
    assert_eq!(transcript.entries[0].content, "Fetch the local doc");
    assert_eq!(
        transcript.entries[1].content,
        "Fetched local doc through z.ai"
    );

    let normalized_first = first_request.to_ascii_lowercase();
    assert!(normalized_first.contains("/chat/completions"));
    assert!(normalized_first.contains("\"tool_choice\":\"auto\""));
    assert!(normalized_first.contains("\"name\":\"web_fetch\""));
    assert!(normalized_first.contains("\"content\":\"fetch the local doc\""));

    let normalized_second = second_request.to_ascii_lowercase();
    assert!(normalized_second.contains("\"role\":\"assistant\""));
    assert!(normalized_second.contains("\"tool_calls\""));
    assert!(normalized_second.contains("\"tool_call_id\":\"call_web_fetch\""));
    assert!(normalized_second.contains("local doc"));

    let normalized_web = web_request.to_ascii_lowercase();
    assert!(normalized_web.contains("get /doc http/1.1"));
}

#[test]
fn approval_approve_resumes_an_openai_chat_tool_call_and_completes_the_run() {
    let (web_base, web_requests, web_handle) = spawn_text_server("/doc", "approved doc");
    let first_provider_response = format!(
        r#"{{
                "id":"resp_tool_approval_call",
                "model":"gpt-5.4",
                "output":[
                    {{
                        "id":"fc_1",
                        "type":"function_call",
                        "status":"completed",
                        "call_id":"call_web_fetch",
                        "name":"web_fetch",
                        "arguments":"{{\"url\":\"{}\"}}"
                    }}
                ],
                "usage":{{"input_tokens":19,"output_tokens":7,"total_tokens":26}}
            }}"#,
        web_base
    );
    let (provider_api_base, provider_requests, provider_handle) = spawn_json_server_sequence(vec![
        first_provider_response,
        r#"{
                    "id":"resp_tool_approval_final",
                    "model":"gpt-5.4",
                    "output":[
                        {
                            "id":"msg_1",
                            "type":"message",
                            "status":"completed",
                            "role":"assistant",
                            "content":[
                                {
                                    "type":"output_text",
                                    "text":"Fetched approved doc after approval"
                                }
                            ]
                        }
                    ],
                    "usage":{"input_tokens":31,"output_tokens":4,"total_tokens":35}
                }"#
        .to_string(),
    ]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(format!("{provider_api_base}/v1")),
            api_key: Some("test-key".to_string()),
            default_model: Some("gpt-5.4".to_string()),
        },
        permissions: PermissionConfig {
            mode: PermissionMode::Auto,
            rules: vec![PermissionRule {
                action: PermissionAction::Ask,
                tool: Some("web_fetch".to_string()),
                family: None,
                path_prefix: None,
            }],
        },
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-chat-approval".to_string(),
            title: "Chat approval session".to_string(),
            prompt_override: Some("Use tools when useful.".to_string()),
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            active_mission_id: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");

    let error = app
        .execute_chat_turn("session-chat-approval", "Fetch the approved doc", 10)
        .expect_err("chat turn should pause for approval");
    assert!(error.to_string().contains("approval"));

    let waiting_run = store
        .get_run("run-chat-session-chat-approval-10")
        .expect("get waiting run")
        .expect("waiting run exists");
    assert_eq!(waiting_run.status, "waiting_approval");

    let approvals = app
        .run_with_args(["approval", "list", "run-chat-session-chat-approval-10"])
        .expect("approval list");
    assert!(approvals.contains("approval-run-chat-session-chat-approval-10-web_fetch"));
    assert!(approvals.contains("call_web_fetch"));

    let approved = app
        .run_with_args([
            "approval",
            "approve",
            "run-chat-session-chat-approval-10",
            "approval-run-chat-session-chat-approval-10-web_fetch",
        ])
        .expect("approval approve");
    let first_request = provider_requests.recv().expect("first provider request");
    let second_request = provider_requests.recv().expect("second provider request");
    let web_request = web_requests.recv().expect("web request");
    provider_handle.join().expect("join provider server");
    web_handle.join().expect("join web server");

    assert!(approved.contains("run-chat-session-chat-approval-10"));
    assert!(approved.contains("resp_tool_approval_final"));
    assert!(approved.contains("Fetched approved doc after approval"));

    let completed_run = store
        .get_run("run-chat-session-chat-approval-10")
        .expect("get completed run")
        .expect("completed run exists");
    assert_eq!(completed_run.status, "completed");
    assert_eq!(
        completed_run.result.as_deref(),
        Some("Fetched approved doc after approval")
    );

    let transcript = app
        .session_transcript("session-chat-approval")
        .expect("load transcript");
    assert_eq!(transcript.entries.len(), 2);
    assert_eq!(transcript.entries[0].content, "Fetch the approved doc");
    assert_eq!(
        transcript.entries[1].content,
        "Fetched approved doc after approval"
    );

    let normalized_first = first_request.to_ascii_lowercase();
    assert!(normalized_first.contains("\"name\":\"web_fetch\""));
    assert!(normalized_first.contains("\"text\":\"fetch the approved doc\""));

    let normalized_second = second_request.to_ascii_lowercase();
    assert!(normalized_second.contains("\"previous_response_id\":\"resp_tool_approval_call\""));
    assert!(normalized_second.contains("\"type\":\"function_call_output\""));
    assert!(normalized_second.contains("approved doc"));

    let normalized_web = web_request.to_ascii_lowercase();
    assert!(normalized_web.contains("get /doc http/1.1"));
}

#[test]
fn approval_approve_resumes_a_zai_chat_tool_call_and_completes_the_run() {
    let (web_base, web_requests, web_handle) = spawn_text_server("/doc", "approved zai doc");
    let first_provider_response = format!(
        r#"{{
                "id":"chatcmpl-approval-zai-1",
                "model":"glm-5.1",
                "choices":[
                    {{
                        "index":0,
                        "finish_reason":"tool_calls",
                        "message":{{
                            "role":"assistant",
                            "content":"",
                            "tool_calls":[
                                {{
                                    "id":"call_web_fetch",
                                    "type":"function",
                                    "function":{{
                                        "name":"web_fetch",
                                        "arguments":"{{\"url\":\"{}\"}}"
                                    }}
                                }}
                            ]
                        }}
                    }}
                ],
                "usage":{{"prompt_tokens":19,"completion_tokens":7,"total_tokens":26}}
            }}"#,
        web_base
    );
    let (provider_api_base, provider_requests, provider_handle) = spawn_json_server_sequence(vec![
        first_provider_response,
        r#"{
                    "id":"chatcmpl-approval-zai-2",
                    "model":"glm-5.1",
                    "choices":[
                        {
                            "index":0,
                            "finish_reason":"stop",
                            "message":{
                                "role":"assistant",
                                "content":"Fetched approved zai doc after approval"
                            }
                        }
                    ],
                    "usage":{"prompt_tokens":31,"completion_tokens":4,"total_tokens":35}
                }"#
        .to_string(),
    ]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        provider: ConfiguredProvider {
            kind: ProviderKind::ZaiChatCompletions,
            api_base: Some(format!("{provider_api_base}/v1")),
            api_key: Some("test-key".to_string()),
            default_model: Some("glm-5.1".to_string()),
        },
        permissions: PermissionConfig {
            mode: PermissionMode::Auto,
            rules: vec![PermissionRule {
                action: PermissionAction::Ask,
                tool: Some("web_fetch".to_string()),
                family: None,
                path_prefix: None,
            }],
        },
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-chat-approval-zai".to_string(),
            title: "Chat approval zai session".to_string(),
            prompt_override: Some("Use tools when useful.".to_string()),
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            active_mission_id: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");

    let error = app
        .execute_chat_turn(
            "session-chat-approval-zai",
            "Fetch the approved zai doc",
            10,
        )
        .expect_err("chat turn should pause for approval");
    assert!(error.to_string().contains("approval"));

    let approvals = app
        .run_with_args(["approval", "list", "run-chat-session-chat-approval-zai-10"])
        .expect("approval list");
    assert!(approvals.contains("approval-run-chat-session-chat-approval-zai-10-web_fetch"));
    assert!(approvals.contains("call_web_fetch"));

    let approved = app
        .run_with_args([
            "approval",
            "approve",
            "run-chat-session-chat-approval-zai-10",
            "approval-run-chat-session-chat-approval-zai-10-web_fetch",
        ])
        .expect("approval approve");
    let first_request = provider_requests.recv().expect("first provider request");
    let second_request = provider_requests.recv().expect("second provider request");
    let web_request = web_requests.recv().expect("web request");
    provider_handle.join().expect("join provider server");
    web_handle.join().expect("join web server");

    assert!(approved.contains("run-chat-session-chat-approval-zai-10"));
    assert!(approved.contains("chatcmpl-approval-zai-2"));
    assert!(approved.contains("Fetched approved zai doc after approval"));

    let completed_run = store
        .get_run("run-chat-session-chat-approval-zai-10")
        .expect("get completed run")
        .expect("completed run exists");
    assert_eq!(completed_run.status, "completed");
    assert_eq!(
        completed_run.result.as_deref(),
        Some("Fetched approved zai doc after approval")
    );

    let transcript = app
        .session_transcript("session-chat-approval-zai")
        .expect("load transcript");
    assert_eq!(transcript.entries.len(), 2);
    assert_eq!(transcript.entries[0].content, "Fetch the approved zai doc");
    assert_eq!(
        transcript.entries[1].content,
        "Fetched approved zai doc after approval"
    );

    let normalized_first = first_request.to_ascii_lowercase();
    assert!(normalized_first.contains("\"tool_choice\":\"auto\""));
    assert!(normalized_first.contains("\"name\":\"web_fetch\""));
    assert!(normalized_first.contains("\"content\":\"fetch the approved zai doc\""));

    let normalized_second = second_request.to_ascii_lowercase();
    assert!(normalized_second.contains("\"role\":\"assistant\""));
    assert!(normalized_second.contains("\"tool_calls\""));
    assert!(normalized_second.contains("\"tool_call_id\":\"call_web_fetch\""));
    assert!(normalized_second.contains("approved zai doc"));

    let normalized_web = web_request.to_ascii_lowercase();
    assert!(normalized_web.contains("get /doc http/1.1"));
}

#[test]
fn approval_approve_resumes_a_mission_turn_and_completes_the_job() {
    let (web_base, web_requests, web_handle) = spawn_text_server("/doc", "mission approved doc");
    let first_provider_response = format!(
        r#"{{
                "id":"resp_mission_approval_call",
                "model":"gpt-5.4",
                "output":[
                    {{
                        "id":"fc_1",
                        "type":"function_call",
                        "status":"completed",
                        "call_id":"call_web_fetch",
                        "name":"web_fetch",
                        "arguments":"{{\"url\":\"{}\"}}"
                    }}
                ],
                "usage":{{"input_tokens":19,"output_tokens":7,"total_tokens":26}}
            }}"#,
        web_base
    );
    let (provider_api_base, provider_requests, provider_handle) = spawn_json_server_sequence(vec![
        first_provider_response,
        r#"{
                    "id":"resp_mission_approval_final",
                    "model":"gpt-5.4",
                    "output":[
                        {
                            "id":"msg_1",
                            "type":"message",
                            "status":"completed",
                            "role":"assistant",
                            "content":[
                                {
                                    "type":"output_text",
                                    "text":"Mission fetched approved doc"
                                }
                            ]
                        }
                    ],
                    "usage":{"input_tokens":31,"output_tokens":4,"total_tokens":35}
                }"#
        .to_string(),
    ]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(format!("{provider_api_base}/v1")),
            api_key: Some("test-key".to_string()),
            default_model: Some("gpt-5.4".to_string()),
        },
        permissions: PermissionConfig {
            mode: PermissionMode::Auto,
            rules: vec![PermissionRule {
                action: PermissionAction::Ask,
                tool: Some("web_fetch".to_string()),
                family: None,
                path_prefix: None,
            }],
        },
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-mission-approval".to_string(),
            title: "Mission approval session".to_string(),
            prompt_override: Some("Use tools when useful.".to_string()),
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            active_mission_id: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");
    store
        .put_mission(&MissionRecord {
            id: "mission-approval".to_string(),
            session_id: "session-mission-approval".to_string(),
            objective: "Fetch an approved doc".to_string(),
            status: MissionStatus::Ready.as_str().to_string(),
            execution_intent: MissionExecutionIntent::Autonomous.as_str().to_string(),
            schedule_json: serde_json::to_string(&MissionSchedule::once())
                .expect("serialize schedule"),
            acceptance_json: "[]".to_string(),
            created_at: 2,
            updated_at: 2,
            completed_at: None,
        })
        .expect("put mission");
    store
        .put_job(
            &JobRecord::try_from(&JobSpec::mission_turn(
                "job-mission-approval",
                "mission-approval",
                None,
                None,
                "Fetch the approved mission doc",
                3,
            ))
            .expect("job record"),
        )
        .expect("put job");

    let error = app
        .execute_mission_turn_job("job-mission-approval", 10)
        .expect_err("mission turn should pause for approval");
    assert!(error.to_string().contains("approval"));

    let approval_output = app
        .run_with_args(["approval", "list", "run-job-mission-approval"])
        .expect("approval list");
    assert!(approval_output.contains("approval-run-job-mission-approval-web_fetch"));

    let approved = app
        .run_with_args([
            "approval",
            "approve",
            "run-job-mission-approval",
            "approval-run-job-mission-approval-web_fetch",
        ])
        .expect("approval approve");
    let first_request = provider_requests.recv().expect("first provider request");
    let second_request = provider_requests.recv().expect("second provider request");
    let web_request = web_requests.recv().expect("web request");
    provider_handle.join().expect("join provider server");
    web_handle.join().expect("join web server");

    assert!(approved.contains("status=completed"));
    assert!(approved.contains("resp_mission_approval_final"));
    assert!(approved.contains("Mission fetched approved doc"));

    let completed_run = store
        .get_run("run-job-mission-approval")
        .expect("get completed run")
        .expect("completed run exists");
    assert_eq!(completed_run.status, "completed");
    assert_eq!(
        completed_run.result.as_deref(),
        Some("Mission fetched approved doc")
    );

    let completed_job = store
        .get_job("job-mission-approval")
        .expect("get completed job")
        .expect("completed job exists");
    assert_eq!(completed_job.status, "completed");
    assert!(
        completed_job
            .result_json
            .as_deref()
            .unwrap_or_default()
            .contains("Mission fetched approved doc")
    );

    let transcript = app
        .session_transcript("session-mission-approval")
        .expect("load transcript");
    assert_eq!(transcript.entries.len(), 2);
    assert_eq!(
        transcript.entries[0].content,
        "Fetch the approved mission doc"
    );
    assert_eq!(
        transcript.entries[1].content,
        "Mission fetched approved doc"
    );

    let normalized_first = first_request.to_ascii_lowercase();
    assert!(normalized_first.contains("\"name\":\"web_fetch\""));
    let normalized_second = second_request.to_ascii_lowercase();
    assert!(normalized_second.contains("\"previous_response_id\":\"resp_mission_approval_call\""));
    assert!(normalized_second.contains("mission approved doc"));
    let normalized_web = web_request.to_ascii_lowercase();
    assert!(normalized_web.contains("get /doc http/1.1"));
}

#[test]
fn execute_chat_turn_fails_when_the_provider_repeats_the_same_tool_signature() {
    let (web_base, web_requests, web_handle) = spawn_text_server("/doc", "loop doc");
    let repeated_tool_response = format!(
        r#"{{
                "id":"resp_tool_loop",
                "model":"gpt-5.4",
                "output":[
                    {{
                        "id":"fc_loop",
                        "type":"function_call",
                        "status":"completed",
                        "call_id":"call_web_fetch",
                        "name":"web_fetch",
                        "arguments":"{{\"url\":\"{}\"}}"
                    }}
                ],
                "usage":{{"input_tokens":19,"output_tokens":7,"total_tokens":26}}
            }}"#,
        web_base
    );
    let (provider_api_base, provider_requests, provider_handle) =
        spawn_json_server_sequence(vec![repeated_tool_response.clone(), repeated_tool_response]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(format!("{provider_api_base}/v1")),
            api_key: Some("test-key".to_string()),
            default_model: Some("gpt-5.4".to_string()),
        },
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-chat-loop".to_string(),
            title: "Chat loop session".to_string(),
            prompt_override: Some("Use tools when useful.".to_string()),
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            active_mission_id: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");

    let error = app
        .execute_chat_turn("session-chat-loop", "Fetch the local doc", 10)
        .expect_err("repeated tool signature must fail");
    let first_request = provider_requests.recv().expect("first provider request");
    let second_request = provider_requests.recv().expect("second provider request");
    let web_request = web_requests.recv().expect("web request");
    provider_handle.join().expect("join provider server");
    web_handle.join().expect("join web server");

    assert!(
        error
            .to_string()
            .contains("provider repeated tool-call signature")
    );

    let run = store
        .get_run("run-chat-session-chat-loop-10")
        .expect("get run")
        .expect("run exists");
    assert_eq!(run.status, "failed");
    assert!(
        run.error
            .as_deref()
            .unwrap_or_default()
            .contains("provider repeated tool-call signature")
    );

    let normalized_first = first_request.to_ascii_lowercase();
    assert!(normalized_first.contains("\"name\":\"web_fetch\""));
    let normalized_second = second_request.to_ascii_lowercase();
    assert!(normalized_second.contains("\"previous_response_id\":\"resp_tool_loop\""));
    let normalized_web = web_request.to_ascii_lowercase();
    assert!(normalized_web.contains("get /doc http/1.1"));
}

#[test]
fn execute_chat_turn_only_sends_new_tool_outputs_for_each_continuation_round() {
    let (web_base, web_requests, web_handle) =
        spawn_text_server_sequence(vec!["doc one", "doc two"]);
    let first_provider_response = format!(
        r#"{{
                "id":"resp_tool_chain_1",
                "model":"gpt-5.4",
                "output":[
                    {{
                        "id":"fc_chain_1",
                        "type":"function_call",
                        "status":"completed",
                        "call_id":"call_web_fetch_1",
                        "name":"web_fetch",
                        "arguments":"{{\"url\":\"{}/doc-1\"}}"
                    }}
                ],
                "usage":{{"input_tokens":19,"output_tokens":7,"total_tokens":26}}
            }}"#,
        web_base
    );
    let second_provider_response = format!(
        r#"{{
                "id":"resp_tool_chain_2",
                "model":"gpt-5.4",
                "output":[
                    {{
                        "id":"fc_chain_2",
                        "type":"function_call",
                        "status":"completed",
                        "call_id":"call_web_fetch_2",
                        "name":"web_fetch",
                        "arguments":"{{\"url\":\"{}/doc-2\"}}"
                    }}
                ],
                "usage":{{"input_tokens":27,"output_tokens":8,"total_tokens":35}}
            }}"#,
        web_base
    );
    let (provider_api_base, provider_requests, provider_handle) = spawn_json_server_sequence(vec![
        first_provider_response,
        second_provider_response,
        r#"{
                    "id":"resp_tool_chain_3",
                    "model":"gpt-5.4",
                    "output":[
                        {
                            "id":"msg_1",
                            "type":"message",
                            "status":"completed",
                            "role":"assistant",
                            "content":[
                                {
                                    "type":"output_text",
                                    "text":"two step tool chain ok"
                                }
                            ]
                        }
                    ],
                    "usage":{"input_tokens":39,"output_tokens":5,"total_tokens":44}
                }"#
        .to_string(),
    ]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(format!("{provider_api_base}/v1")),
            api_key: Some("test-key".to_string()),
            default_model: Some("gpt-5.4".to_string()),
        },
        ..AppConfig::default()
    })
    .expect("build app");

    let store = PersistenceStore::open(&app.persistence).expect("open store");
    store
        .put_session(&SessionRecord {
            id: "session-chat-chain".to_string(),
            title: "Chat chain session".to_string(),
            prompt_override: Some("Use tools when useful.".to_string()),
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            active_mission_id: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");

    let report = app
        .execute_chat_turn("session-chat-chain", "Fetch two docs", 10)
        .expect("execute chat turn");
    let _first_request = provider_requests.recv().expect("first provider request");
    let second_request = provider_requests.recv().expect("second provider request");
    let third_request = provider_requests.recv().expect("third provider request");
    let first_web_request = web_requests.recv().expect("first web request");
    let second_web_request = web_requests.recv().expect("second web request");
    provider_handle.join().expect("join provider server");
    web_handle.join().expect("join web server");

    assert_eq!(report.output_text, "two step tool chain ok");

    let normalized_second = second_request.to_ascii_lowercase();
    assert!(normalized_second.contains("\"previous_response_id\":\"resp_tool_chain_1\""));
    assert!(normalized_second.contains("doc one"));

    let normalized_third = third_request.to_ascii_lowercase();
    assert!(normalized_third.contains("\"previous_response_id\":\"resp_tool_chain_2\""));
    assert!(normalized_third.contains("doc two"));
    assert!(!normalized_third.contains("doc one"));

    let normalized_first_web = first_web_request.to_ascii_lowercase();
    assert!(normalized_first_web.contains("get /doc-1 http/1.1"));
    let normalized_second_web = second_web_request.to_ascii_lowercase();
    assert!(normalized_second_web.contains("get /doc-2 http/1.1"));
}

#[test]
fn run_with_args_shows_and_sends_chat_turns() {
    let (api_base, requests, handle) = spawn_json_server(
        r#"{
                "id":"resp_chat_cli",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_1",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Chat CLI reply"
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":16,"output_tokens":3,"total_tokens":19}
            }"#,
    );
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(format!("{api_base}/v1")),
            api_key: Some("test-key".to_string()),
            default_model: Some("gpt-5.4".to_string()),
        },
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-chat-cli".to_string(),
            title: "Chat CLI session".to_string(),
            prompt_override: Some("Keep it short.".to_string()),
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            active_mission_id: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");
    store
        .put_transcript(&agent_persistence::TranscriptRecord {
            id: "msg-1".to_string(),
            session_id: "session-chat-cli".to_string(),
            run_id: None,
            kind: "user".to_string(),
            content: "First".to_string(),
            created_at: 2,
        })
        .expect("put transcript");

    let shown_before = app
        .run_with_args(["chat", "show", "session-chat-cli"])
        .expect("chat show before");
    assert_eq!(shown_before, "[2] user: First");

    let sent = app
        .run_with_args(["chat", "send", "session-chat-cli", "Second", "message"])
        .expect("chat send");
    let raw_request = requests.recv().expect("raw request");
    handle.join().expect("join server");

    assert!(sent.contains("chat send session_id=session-chat-cli"));
    assert!(sent.contains("response_id=resp_chat_cli"));
    assert!(sent.contains("output=Chat CLI reply"));

    let shown_after = app
        .run_with_args(["chat", "show", "session-chat-cli"])
        .expect("chat show after");
    assert!(shown_after.contains("[2] user: First"));
    assert!(shown_after.contains("user: Second message"));
    assert!(shown_after.contains("assistant: Chat CLI reply"));

    let normalized_request = raw_request.to_ascii_lowercase();
    assert!(normalized_request.contains("\"instructions\":\"keep it short.\""));
    assert!(normalized_request.contains("\"text\":\"first\""));
    assert!(normalized_request.contains("\"text\":\"second message\""));
}

#[test]
fn run_with_args_chat_send_reports_waiting_approval_details() {
    let (web_base, _web_requests, _web_handle) = spawn_text_server("/doc", "cli ask doc");
    let first_provider_response = format!(
        r#"{{
                "id":"resp_chat_cli_approval",
                "model":"gpt-5.4",
                "output":[
                    {{
                        "id":"fc_1",
                        "type":"function_call",
                        "status":"completed",
                        "call_id":"call_web_fetch",
                        "name":"web_fetch",
                        "arguments":"{{\"url\":\"{}\"}}"
                    }}
                ],
                "usage":{{"input_tokens":19,"output_tokens":7,"total_tokens":26}}
            }}"#,
        web_base
    );
    let (api_base, _requests, handle) = spawn_json_server_sequence(vec![first_provider_response]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(format!("{api_base}/v1")),
            api_key: Some("test-key".to_string()),
            default_model: Some("gpt-5.4".to_string()),
        },
        permissions: PermissionConfig {
            mode: PermissionMode::Auto,
            rules: vec![PermissionRule {
                action: PermissionAction::Ask,
                tool: Some("web_fetch".to_string()),
                family: None,
                path_prefix: None,
            }],
        },
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-chat-cli-approval".to_string(),
            title: "Chat CLI approval session".to_string(),
            prompt_override: Some("Use tools when useful.".to_string()),
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            active_mission_id: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");

    let sent = app
        .run_with_args([
            "chat",
            "send",
            "session-chat-cli-approval",
            "Fetch",
            "the",
            "doc",
        ])
        .expect("chat send should report waiting approval");
    handle.join().expect("join server");

    assert!(sent.contains("status=waiting_approval"));
    assert!(sent.contains("session_id=session-chat-cli-approval"));
    assert!(sent.contains("run_id=run-chat-session-chat-cli-approval-"));
    assert!(sent.contains("approval_id=approval-run-chat-session-chat-cli-approval-"));
}

#[test]
fn repl_runs_chat_turns_and_supports_show_and_exit_commands() {
    let (api_base, _requests, handle) =
        spawn_sse_server_sequence(vec![openai_stream_message_response(
            "resp_chat_repl",
            "REPL reply",
        )]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(api_base),
            api_key: Some("test-key".to_string()),
            default_model: Some("gpt-5.4".to_string()),
        },
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-chat-repl".to_string(),
            title: "Chat REPL session".to_string(),
            prompt_override: Some("Keep it short.".to_string()),
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            active_mission_id: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");

    let mut input = Cursor::new(b"Hello from repl\n/show\n/exit\n".to_vec());
    let mut output = Vec::new();
    app.run_with_io(
        ["chat", "repl", "session-chat-repl"],
        &mut input,
        &mut output,
    )
    .expect("repl");
    handle.join().expect("join server");

    let rendered = String::from_utf8(output).expect("utf8");
    assert!(rendered.contains("assistant: REPL reply"));
    assert!(rendered.contains("["));
    assert!(rendered.contains("user: Hello from repl"));
    assert!(rendered.contains("leaving chat repl"));
}

#[test]
fn repl_accepts_cp1251_terminal_input_without_utf8_failure() {
    let (api_base, _requests, handle) =
        spawn_sse_server_sequence(vec![openai_stream_message_response(
            "resp_cp1251_repl",
            "cp1251 ok",
        )]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(api_base),
            api_key: Some("test-key".to_string()),
            default_model: Some("gpt-5.4".to_string()),
        },
        permissions: PermissionConfig::default(),
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-chat-repl-cp1251".to_string(),
            title: "Chat REPL cp1251 session".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            active_mission_id: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");

    let encoded = encoding_rs::WINDOWS_1251.encode("привет\n/exit\n").0;
    let mut input = Cursor::new(encoded.into_owned());
    let mut output = Vec::new();
    app.run_with_io(
        ["chat", "repl", "session-chat-repl-cp1251"],
        &mut input,
        &mut output,
    )
    .expect("repl");
    handle.join().expect("join server");

    let rendered = String::from_utf8(output).expect("utf8");
    assert!(rendered.contains("assistant: cp1251 ok"));
    assert!(!rendered.contains("stream did not contain valid UTF-8"));
}

#[test]
fn repl_surfaces_waiting_approval_and_can_approve_latest_pending_turn() {
    let (web_base, _web_requests, _web_handle) = spawn_text_server("/doc", "repl ask doc");
    let first_provider_response = openai_stream_tool_call_response(
        "resp_repl_waiting",
        "call_web_fetch",
        "web_fetch",
        &format!(r#"{{"url":"{}"}}"#, web_base),
    );
    let second_provider_response =
        openai_stream_message_response("resp_repl_approved", "repl approval completed");
    let (api_base, _requests, handle) =
        spawn_sse_server_sequence(vec![first_provider_response, second_provider_response]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(api_base),
            api_key: Some("test-key".to_string()),
            default_model: Some("gpt-5.4".to_string()),
        },
        permissions: PermissionConfig {
            mode: PermissionMode::Auto,
            rules: vec![PermissionRule {
                action: PermissionAction::Ask,
                tool: Some("web_fetch".to_string()),
                family: None,
                path_prefix: None,
            }],
        },
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-chat-repl-approval".to_string(),
            title: "Chat REPL approval session".to_string(),
            prompt_override: Some("Use tools when useful.".to_string()),
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            active_mission_id: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");

    let mut input = Cursor::new(b"Fetch the doc\n/approve\n/show\n/exit\n".to_vec());
    let mut output = Vec::new();
    app.run_with_io(
        ["chat", "repl", "session-chat-repl-approval"],
        &mut input,
        &mut output,
    )
    .expect("repl");
    handle.join().expect("join server");

    let rendered = String::from_utf8(output).expect("utf8");
    assert!(rendered.contains("tool: web_fetch | waiting_approval"));
    assert!(rendered.contains("tool: web_fetch | approved"));
    assert!(rendered.contains("tool: web_fetch | completed"));
    assert!(rendered.contains("assistant: repl approval completed"));
}

#[test]
fn repl_rehydrates_latest_pending_approval_after_restart() {
    let (web_base, _web_requests, _web_handle) = spawn_text_server("/doc", "rehydrated doc");
    let first_provider_response = openai_stream_tool_call_response(
        "resp_repl_restart_waiting",
        "call_web_fetch",
        "web_fetch",
        &format!(r#"{{"url":"{}"}}"#, web_base),
    );
    let second_provider_response = openai_stream_message_response(
        "resp_repl_restart_done",
        "approval after restart completed",
    );
    let temp = tempfile::tempdir().expect("tempdir");
    let state_root = temp.path().join("state-root");
    let (api_base, _requests, handle) = spawn_sse_server_sequence(vec![first_provider_response]);
    let app = build_from_config(AppConfig {
        data_dir: state_root.clone(),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(api_base),
            api_key: Some("test-key".to_string()),
            default_model: Some("gpt-5.4".to_string()),
        },
        permissions: PermissionConfig {
            mode: PermissionMode::Auto,
            rules: vec![PermissionRule {
                action: PermissionAction::Ask,
                tool: Some("web_fetch".to_string()),
                family: None,
                path_prefix: None,
            }],
        },
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-chat-repl-restart".to_string(),
            title: "Chat REPL restart session".to_string(),
            prompt_override: Some("Use tools when useful.".to_string()),
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            active_mission_id: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");

    let mut first_input = Cursor::new(b"Fetch the doc\n/exit\n".to_vec());
    let mut first_output = Vec::new();
    app.run_with_io(
        ["chat", "repl", "session-chat-repl-restart"],
        &mut first_input,
        &mut first_output,
    )
    .expect("first repl");
    handle.join().expect("join first server");

    let (api_base, _requests, _handle) = spawn_sse_server_sequence(vec![second_provider_response]);
    let app = build_from_config(AppConfig {
        data_dir: state_root,
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(api_base),
            api_key: Some("test-key".to_string()),
            default_model: Some("gpt-5.4".to_string()),
        },
        permissions: PermissionConfig {
            mode: PermissionMode::Auto,
            rules: vec![PermissionRule {
                action: PermissionAction::Ask,
                tool: Some("web_fetch".to_string()),
                family: None,
                path_prefix: None,
            }],
        },
    })
    .expect("rebuild app");

    let mut second_input = Cursor::new(b"/approve\n/exit\n".to_vec());
    let mut second_output = Vec::new();
    app.run_with_io(
        ["chat", "repl", "session-chat-repl-restart"],
        &mut second_input,
        &mut second_output,
    )
    .expect("second repl");

    let rendered = String::from_utf8(second_output).expect("utf8");
    assert!(rendered.contains("tool: web_fetch | approved"));
    assert!(rendered.contains("tool: web_fetch | completed"));
    assert!(rendered.contains("assistant: approval after restart completed"));
}

#[test]
fn repl_rejects_new_turns_while_an_approval_is_pending() {
    let (web_base, _web_requests, _web_handle) = spawn_text_server("/doc", "pending doc");
    let first_provider_response = openai_stream_tool_call_response(
        "resp_repl_pending_only",
        "call_web_fetch",
        "web_fetch",
        &format!(r#"{{"url":"{}"}}"#, web_base),
    );
    let (api_base, _requests, handle) = spawn_sse_server_sequence(vec![first_provider_response]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(api_base),
            api_key: Some("test-key".to_string()),
            default_model: Some("gpt-5.4".to_string()),
        },
        permissions: PermissionConfig {
            mode: PermissionMode::Auto,
            rules: vec![PermissionRule {
                action: PermissionAction::Ask,
                tool: Some("web_fetch".to_string()),
                family: None,
                path_prefix: None,
            }],
        },
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-chat-repl-pending".to_string(),
            title: "Chat REPL pending session".to_string(),
            prompt_override: Some("Use tools when useful.".to_string()),
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            active_mission_id: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");

    let mut input = Cursor::new(b"Fetch the doc\nSecond turn should be blocked\n/exit\n".to_vec());
    let mut output = Vec::new();
    app.run_with_io(
        ["chat", "repl", "session-chat-repl-pending"],
        &mut input,
        &mut output,
    )
    .expect("repl");
    handle.join().expect("join server");

    let rendered = String::from_utf8(output).expect("utf8");
    assert!(rendered.contains("tool: web_fetch | waiting_approval"));
    assert!(rendered.contains("finish the pending approval before sending another message"));
    assert!(!rendered.contains("assistant: Second turn"));
}

#[test]
fn repl_stream_renders_tool_status_instead_of_raw_command_reports() {
    let (web_base, _web_requests, _web_handle) = spawn_text_server("/doc", "streaming tool result");
    let first_stream = format!(
        "data: {{\"id\":\"chatcmpl-stream-tool-1\",\"model\":\"glm-5-turbo\",\"choices\":[{{\"index\":0,\"delta\":{{\"reasoning_content\":\"inspect doc before fetching. \",\"tool_calls\":[{{\"index\":0,\"id\":\"call_web_fetch\",\"type\":\"function\",\"function\":{{\"name\":\"web_fetch\",\"arguments\":\"{{\\\"url\\\":\\\"{}\\\"}}\"}}}}]}},\"finish_reason\":\"tool_calls\"}}]}}\n\n\
data: [DONE]\n\n",
        web_base
    );
    let second_stream =
            "data: {\"id\":\"chatcmpl-stream-tool-2\",\"model\":\"glm-5-turbo\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"streaming \"},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"chatcmpl-stream-tool-2\",\"model\":\"glm-5-turbo\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"tool result\"},\"finish_reason\":\"stop\"}]}\n\n\
data: [DONE]\n\n"
                .to_string();
    let (api_base, _requests, handle) =
        spawn_sse_server_sequence(vec![first_stream, second_stream]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        provider: ConfiguredProvider {
            kind: ProviderKind::ZaiChatCompletions,
            api_base: Some(api_base),
            api_key: Some("zai-key".to_string()),
            default_model: Some("glm-5-turbo".to_string()),
        },
        permissions: PermissionConfig {
            mode: PermissionMode::Auto,
            rules: vec![PermissionRule {
                action: PermissionAction::Ask,
                tool: Some("web_fetch".to_string()),
                family: None,
                path_prefix: None,
            }],
        },
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-chat-repl-stream".to_string(),
            title: "Chat REPL stream session".to_string(),
            prompt_override: Some("Use tools when useful.".to_string()),
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            active_mission_id: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");

    let mut input = Cursor::new(
        b"Fetch the doc and reply with the exact body only\n/approve\n/exit\n".to_vec(),
    );
    let mut output = Vec::new();
    app.run_with_io(
        ["chat", "repl", "session-chat-repl-stream"],
        &mut input,
        &mut output,
    )
    .expect("repl");
    handle.join().expect("join server");

    let rendered = String::from_utf8(output).expect("utf8");
    assert!(rendered.contains("reasoning: inspect doc before fetching."));
    assert!(rendered.contains("tool: web_fetch | waiting_approval"));
    assert!(rendered.contains("tool: web_fetch | completed"));
    assert!(rendered.contains("assistant: streaming tool result"));
    assert!(!rendered.contains("chat send session_id=session-chat-repl-stream"));
    assert!(!rendered.contains("approved approval-"));
}

#[test]
fn openai_repl_stream_renders_reasoning_summary_and_assistant_text() {
    let stream = "data: {\"type\":\"response.reasoning_summary_text.delta\",\"item_id\":\"rs_123\",\"output_index\":0,\"summary_index\":0,\"delta\":\"Compare the request with prior context. \"}\n\n\
data: {\"type\":\"response.output_text.delta\",\"item_id\":\"msg_123\",\"output_index\":1,\"content_index\":0,\"delta\":\"hello \"}\n\n\
data: {\"type\":\"response.output_text.delta\",\"item_id\":\"msg_123\",\"output_index\":1,\"content_index\":0,\"delta\":\"from openai\"}\n\n\
data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_stream_repl\",\"model\":\"gpt-5.4\",\"output\":[{\"id\":\"rs_123\",\"type\":\"reasoning\",\"summary\":[{\"type\":\"summary_text\",\"text\":\"Compare the request with prior context. \"}]},{\"id\":\"msg_123\",\"type\":\"message\",\"status\":\"completed\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"hello from openai\",\"annotations\":[]}]}],\"usage\":{\"input_tokens\":11,\"output_tokens\":7,\"total_tokens\":18}}}\n\n".to_string();
    let (api_base, _requests, handle) = spawn_sse_server_sequence(vec![stream]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(api_base),
            api_key: Some("test-key".to_string()),
            default_model: Some("gpt-5.4".to_string()),
        },
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-chat-repl-openai-stream".to_string(),
            title: "Chat REPL openai stream session".to_string(),
            prompt_override: Some("Be brief.".to_string()),
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            active_mission_id: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");

    let mut input = Cursor::new(b"Say hi\n/exit\n".to_vec());
    let mut output = Vec::new();
    app.run_with_io(
        ["chat", "repl", "session-chat-repl-openai-stream"],
        &mut input,
        &mut output,
    )
    .expect("repl");
    handle.join().expect("join server");

    let rendered = String::from_utf8(output).expect("utf8");
    assert!(rendered.contains("reasoning: Compare the request with prior context."));
    assert!(rendered.contains("assistant: hello from openai"));
}

fn openai_stream_message_response(response_id: &str, text: &str) -> String {
    let text = serde_json::to_string(text).expect("serialize text");
    format!(
        "data: {{\"type\":\"response.completed\",\"response\":{{\"id\":\"{response_id}\",\"model\":\"gpt-5.4\",\"output\":[{{\"id\":\"msg_1\",\"type\":\"message\",\"status\":\"completed\",\"role\":\"assistant\",\"content\":[{{\"type\":\"output_text\",\"text\":{text}}}]}}],\"usage\":{{\"input_tokens\":16,\"output_tokens\":3,\"total_tokens\":19}}}}}}\n\n"
    )
}

fn openai_stream_tool_call_response(
    response_id: &str,
    call_id: &str,
    tool_name: &str,
    arguments: &str,
) -> String {
    let arguments = serde_json::to_string(arguments).expect("serialize arguments");
    format!(
        "data: {{\"type\":\"response.completed\",\"response\":{{\"id\":\"{response_id}\",\"model\":\"gpt-5.4\",\"output\":[{{\"id\":\"fc_1\",\"type\":\"function_call\",\"status\":\"completed\",\"call_id\":\"{call_id}\",\"name\":\"{tool_name}\",\"arguments\":{arguments}}}],\"usage\":{{\"input_tokens\":19,\"output_tokens\":7,\"total_tokens\":26}}}}}}\n\n"
    )
}

fn spawn_json_server(body: &'static str) -> (String, Receiver<String>, thread::JoinHandle<()>) {
    spawn_json_server_sequence(vec![body.to_string()])
}

fn spawn_sse_server_sequence(
    bodies: Vec<String>,
) -> (String, Receiver<String>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let address = listener.local_addr().expect("local addr");
    let (sender, receiver) = mpsc::channel();

    let handle = thread::spawn(move || {
        for body in bodies {
            let (mut stream, _) = listener.accept().expect("accept connection");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("set read timeout");

            let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
            let mut raw_request = String::new();
            let mut content_length = 0usize;

            loop {
                let mut line = String::new();
                reader.read_line(&mut line).expect("read request line");
                raw_request.push_str(&line);

                if line == "\r\n" {
                    break;
                }

                let lower = line.to_ascii_lowercase();
                if let Some(value) = lower.strip_prefix("content-length:") {
                    content_length = value.trim().parse().expect("parse content length");
                }
            }

            let mut body_bytes = vec![0; content_length];
            reader
                .read_exact(&mut body_bytes)
                .expect("read request body");
            raw_request.push_str(&String::from_utf8_lossy(&body_bytes));

            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("write response");
            stream.flush().expect("flush response");
            sender.send(raw_request).expect("send request");
        }
    });

    (format!("http://{address}/v1"), receiver, handle)
}

fn spawn_json_server_sequence(
    bodies: Vec<String>,
) -> (String, Receiver<String>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let address = listener.local_addr().expect("local addr");
    let (sender, receiver) = mpsc::channel();

    let handle = thread::spawn(move || {
        for body in bodies {
            let (mut stream, _) = listener.accept().expect("accept connection");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("set read timeout");

            let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
            let mut raw_request = String::new();
            let mut content_length = 0usize;

            loop {
                let mut line = String::new();
                reader.read_line(&mut line).expect("read request line");
                raw_request.push_str(&line);

                if line == "\r\n" {
                    break;
                }

                let lower = line.to_ascii_lowercase();
                if let Some(value) = lower.strip_prefix("content-length:") {
                    content_length = value.trim().parse().expect("parse content length");
                }
            }

            let mut body_buf = vec![0u8; content_length];
            reader.read_exact(&mut body_buf).expect("read request body");
            raw_request.push_str(std::str::from_utf8(&body_buf).expect("utf8 body"));
            sender.send(raw_request).expect("send request");

            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("write response");
            stream.flush().expect("flush response");
        }
    });

    (format!("http://{address}"), receiver, handle)
}

fn spawn_text_server(
    path: &'static str,
    body: &'static str,
) -> (String, Receiver<String>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let address = listener.local_addr().expect("local addr");
    let (sender, receiver) = mpsc::channel();

    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept connection");
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set read timeout");

        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
        let mut raw_request = String::new();

        loop {
            let mut line = String::new();
            reader.read_line(&mut line).expect("read request line");
            raw_request.push_str(&line);
            if line == "\r\n" {
                break;
            }
        }

        sender.send(raw_request).expect("send request");
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/plain\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream
            .write_all(response.as_bytes())
            .expect("write response");
        stream.flush().expect("flush response");
    });

    (format!("http://{address}{path}"), receiver, handle)
}

fn spawn_text_server_sequence(
    bodies: Vec<&'static str>,
) -> (String, Receiver<String>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let address = listener.local_addr().expect("local addr");
    let (sender, receiver) = mpsc::channel();

    let handle = thread::spawn(move || {
        for body in bodies {
            let (mut stream, _) = listener.accept().expect("accept connection");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("set read timeout");

            let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
            let mut raw_request = String::new();

            loop {
                let mut line = String::new();
                reader.read_line(&mut line).expect("read request line");
                raw_request.push_str(&line);
                if line == "\r\n" {
                    break;
                }
            }

            sender.send(raw_request).expect("send request");
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/plain\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("write response");
            stream.flush().expect("flush response");
        }
    });

    (format!("http://{address}"), receiver, handle)
}

#[test]
fn tui_like_session_metadata_persists_and_lists_for_ui() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");

    let created = app
        .create_session_auto(Some("Terminal UI"))
        .expect("create session");

    let updated = app
        .update_session_preferences(
            &created.id,
            agentd::bootstrap::SessionPreferencesPatch {
                title: Some("Renamed in TUI".to_string()),
                model: Some(Some("gpt-5.4".to_string())),
                reasoning_visible: Some(false),
                think_level: Some(Some("high".to_string())),
                compactifications: Some(3),
            },
        )
        .expect("update session preferences");

    assert_eq!(updated.id, created.id);
    assert_eq!(updated.title, "Renamed in TUI");
    assert_eq!(updated.model.as_deref(), Some("gpt-5.4"));
    assert!(!updated.reasoning_visible);
    assert_eq!(updated.think_level.as_deref(), Some("high"));
    assert_eq!(updated.compactifications, 3);

    let sessions = app
        .list_session_summaries()
        .expect("list session summaries");
    let summary = sessions
        .into_iter()
        .find(|summary| summary.id == created.id)
        .expect("session summary");

    assert_eq!(summary.title, "Renamed in TUI");
    assert_eq!(summary.model.as_deref(), Some("gpt-5.4"));
    assert!(!summary.reasoning_visible);
    assert_eq!(summary.think_level.as_deref(), Some("high"));
    assert_eq!(summary.compactifications, 3);
}

#[test]
fn tui_like_session_delete_and_clear_remove_canonical_state() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");

    let store = PersistenceStore::open(&app.persistence).expect("open store");
    let session = app
        .create_session_auto(Some("Delete me"))
        .expect("create session");
    let now = 100;
    store
        .put_transcript(&agent_persistence::TranscriptRecord {
            id: "transcript-delete-1".to_string(),
            session_id: session.id.clone(),
            run_id: None,
            kind: "user".to_string(),
            content: "hello".to_string(),
            created_at: now,
        })
        .expect("put transcript");

    app.delete_session(&session.id).expect("delete session");

    assert!(
        store
            .get_session(&session.id)
            .expect("get deleted session")
            .is_none()
    );
    assert!(
        store
            .list_transcripts_for_session(&session.id)
            .expect("list deleted transcripts")
            .is_empty()
    );

    let clear_source = app
        .create_session_auto(Some("Clear me"))
        .expect("create clear source");
    store
        .put_transcript(&agent_persistence::TranscriptRecord {
            id: "transcript-clear-1".to_string(),
            session_id: clear_source.id.clone(),
            run_id: None,
            kind: "assistant".to_string(),
            content: "old state".to_string(),
            created_at: now + 1,
        })
        .expect("put clear transcript");

    let replacement = app
        .clear_session(&clear_source.id, Some("Fresh Session"))
        .expect("clear session");

    assert_ne!(replacement.id, clear_source.id);
    assert_eq!(replacement.title, "Fresh Session");
    assert!(
        store
            .get_session(&clear_source.id)
            .expect("get cleared source")
            .is_none()
    );
    assert!(
        store
            .list_transcripts_for_session(&clear_source.id)
            .expect("list cleared transcripts")
            .is_empty()
    );
    assert!(
        store
            .get_session(&replacement.id)
            .expect("get replacement")
            .is_some()
    );
}

#[test]
fn compact_session_persists_a_context_summary_and_increments_the_counter() {
    let (api_base, requests, handle) = spawn_json_server(
        r#"{
                "id":"resp_compact",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_compact",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Condensed earlier context."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":42,"output_tokens":9,"total_tokens":51}
            }"#,
    );
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(format!("{api_base}/v1")),
            api_key: Some("test-key".to_string()),
            default_model: Some("gpt-5.4".to_string()),
        },
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");
    let session = app
        .create_session_auto(Some("Compaction Session"))
        .expect("create session");

    for (index, (kind, content)) in [
        ("user", "covered user one"),
        ("assistant", "covered assistant one"),
        ("user", "recent user one"),
        ("assistant", "recent assistant one"),
        ("user", "recent user two"),
        ("assistant", "recent assistant two"),
        ("user", "recent user three"),
        ("assistant", "recent assistant three"),
    ]
    .into_iter()
    .enumerate()
    {
        store
            .put_transcript(&agent_persistence::TranscriptRecord {
                id: format!("compact-transcript-{index}"),
                session_id: session.id.clone(),
                run_id: None,
                kind: kind.to_string(),
                content: content.to_string(),
                created_at: 10 + index as i64,
            })
            .expect("put transcript");
    }

    let summary = app.compact_session(&session.id).expect("compact session");
    let context_summary = app
        .context_summary(&session.id)
        .expect("load context summary")
        .expect("context summary should exist");
    let raw_request = requests.recv().expect("compaction request");
    handle.join().expect("join server");

    assert_eq!(summary.compactifications, 1);
    assert_eq!(context_summary.session_id, session.id);
    assert_eq!(context_summary.summary_text, "Condensed earlier context.");
    assert_eq!(context_summary.covered_message_count, 2);
    assert!(context_summary.summary_token_estimate > 0);

    let normalized_request = raw_request.to_ascii_lowercase();
    assert!(normalized_request.contains("/v1/responses"));
    assert!(normalized_request.contains("\"text\":\"covered user one\""));
    assert!(normalized_request.contains("\"text\":\"covered assistant one\""));
    assert!(!normalized_request.contains("\"text\":\"recent user one\""));
}

#[test]
fn compact_session_is_a_noop_when_the_transcript_is_below_threshold() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");
    let session = app
        .create_session_auto(Some("Short Session"))
        .expect("create session");

    for (index, (kind, content)) in [
        ("user", "one"),
        ("assistant", "two"),
        ("user", "three"),
        ("assistant", "four"),
        ("user", "five"),
        ("assistant", "six"),
        ("user", "seven"),
    ]
    .into_iter()
    .enumerate()
    {
        store
            .put_transcript(&agent_persistence::TranscriptRecord {
                id: format!("short-transcript-{index}"),
                session_id: session.id.clone(),
                run_id: None,
                kind: kind.to_string(),
                content: content.to_string(),
                created_at: 20 + index as i64,
            })
            .expect("put short transcript");
    }

    let summary = app
        .compact_session(&session.id)
        .expect("compact short session");

    assert_eq!(summary.compactifications, 0);
    assert!(
        app.context_summary(&session.id)
            .expect("load context summary")
            .is_none()
    );
}

#[test]
fn session_head_derives_counts_previews_and_summary_state() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace_root = temp.path().join("workspace");
    fs::create_dir_all(workspace_root.join("crates")).expect("create crates dir");
    fs::write(workspace_root.join("README.md"), "hello\n").expect("write readme");
    let mut app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    app.runtime.workspace = WorkspaceRef::new(&workspace_root);
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-head".to_string(),
            title: "Session Head".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings {
                compactifications: 2,
                ..SessionSettings::default()
            })
            .expect("serialize settings"),
            active_mission_id: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");
    for (index, (kind, content)) in [
        ("user", "first question"),
        ("assistant", "first answer"),
        ("user", "latest question"),
        ("assistant", "recent answer"),
    ]
    .into_iter()
    .enumerate()
    {
        store
            .put_transcript(&agent_persistence::TranscriptRecord {
                id: format!("session-head-transcript-{index}"),
                session_id: "session-head".to_string(),
                run_id: None,
                kind: kind.to_string(),
                content: content.to_string(),
                created_at: 10 + index as i64,
            })
            .expect("put transcript");
    }
    store
        .put_context_summary(&agent_persistence::ContextSummaryRecord {
            session_id: "session-head".to_string(),
            summary_text: "Condensed state.".to_string(),
            covered_message_count: 2,
            summary_token_estimate: 7,
            updated_at: 20,
        })
        .expect("put context summary");

    let mut run = RunEngine::new("run-pending", "session-head", None, 30);
    run.start(30).expect("start run");
    run.wait_for_approval(
        ApprovalRequest::new("approval-1", "tool-1", "approve", 31),
        31,
    )
    .expect("wait approval");
    run.record_tool_completion("fs_read path=.env -> fs_read path=.env bytes=42", 32)
        .expect("record fs read");
    run.record_tool_completion("fs_list path=. recursive=false -> fs_list entries=2", 33)
        .expect("record fs list");
    store
        .put_run(&RunRecord::try_from(run.snapshot()).expect("run record"))
        .expect("put run");

    let head = app.session_head("session-head").expect("session head");

    assert_eq!(head.session_id, "session-head");
    assert_eq!(head.title, "Session Head");
    assert_eq!(head.message_count, 4);
    assert_eq!(head.context_tokens, 15);
    assert_eq!(head.compactifications, 2);
    assert_eq!(head.summary_covered_message_count, 2);
    assert_eq!(head.pending_approval_count, 1);
    assert_eq!(head.last_user_preview.as_deref(), Some("latest question"));
    assert_eq!(
        head.last_assistant_preview.as_deref(),
        Some("recent answer")
    );
    assert_eq!(head.recent_filesystem_activity.len(), 2);
    assert_eq!(head.recent_filesystem_activity[0].action, "list");
    assert_eq!(head.recent_filesystem_activity[0].target, ".");
    assert_eq!(head.recent_filesystem_activity[1].action, "read");
    assert_eq!(head.recent_filesystem_activity[1].target, ".env");
    assert_eq!(head.workspace_tree.len(), 2);
    assert_eq!(head.workspace_tree[0].path, "README.md");
    assert_eq!(head.workspace_tree[1].path, "crates");

    let rendered = head.render();
    assert!(rendered.contains("Session: Session Head"));
    assert!(rendered.contains("Summary Covers: 2 messages"));
    assert!(rendered.contains("Pending Approvals: 1"));
    assert!(rendered.contains("Recent Filesystem Activity:"));
    assert!(rendered.contains("- list ."));
    assert!(rendered.contains("- read .env"));
    assert!(rendered.contains("Workspace Tree:"));
    assert!(rendered.contains("- README.md"));
    assert!(rendered.contains("- crates/"));
}

#[test]
fn execute_chat_turn_uses_the_context_summary_and_only_the_uncovered_messages() {
    let (api_base, requests, handle) = spawn_json_server_sequence(vec![
        r#"{
                "id":"resp_compact_for_chat",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_compact_chat",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Compact summary covering earlier context."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":42,"output_tokens":9,"total_tokens":51}
            }"#
        .to_string(),
        r#"{
                "id":"resp_chat_after_compact",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_after_compact",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Answer after compaction."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":44,"output_tokens":6,"total_tokens":50}
            }"#
        .to_string(),
    ]);
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace_root = temp.path().join("workspace");
    fs::create_dir_all(workspace_root.join("src")).expect("create src");
    fs::write(workspace_root.join("README.md"), "workspace readme\n")
        .expect("write workspace readme");
    let mut app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(format!("{api_base}/v1")),
            api_key: Some("test-key".to_string()),
            default_model: Some("gpt-5.4".to_string()),
        },
        ..AppConfig::default()
    })
    .expect("build app");
    app.runtime.workspace = WorkspaceRef::new(&workspace_root);
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-compact-chat".to_string(),
            title: "Compacted Chat".to_string(),
            prompt_override: Some("Be concise.".to_string()),
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            active_mission_id: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");

    for (index, (kind, content)) in [
        ("user", "covered user one"),
        ("assistant", "covered assistant one"),
        ("user", "recent user one"),
        ("assistant", "recent assistant one"),
        ("user", "recent user two"),
        ("assistant", "recent assistant two"),
        ("user", "recent user three"),
        ("assistant", "recent assistant three"),
    ]
    .into_iter()
    .enumerate()
    {
        store
            .put_transcript(&agent_persistence::TranscriptRecord {
                id: format!("chat-compact-transcript-{index}"),
                session_id: "session-compact-chat".to_string(),
                run_id: None,
                kind: kind.to_string(),
                content: content.to_string(),
                created_at: 30 + index as i64,
            })
            .expect("put transcript");
    }
    let mut prior_run = RunEngine::new("run-fs-head", "session-compact-chat", None, 40);
    prior_run.start(40).expect("start prior run");
    prior_run
        .record_tool_completion(
            "fs_patch path=src/main.rs edits=1 -> fs_patch path=src/main.rs edits=1",
            41,
        )
        .expect("record patch");
    store
        .put_run(&RunRecord::try_from(prior_run.snapshot()).expect("prior run record"))
        .expect("put prior run");

    app.compact_session("session-compact-chat")
        .expect("compact session");
    let _first_request = requests.recv().expect("compaction request");

    let report = app
        .execute_chat_turn("session-compact-chat", "latest question", 50)
        .expect("execute compacted chat turn");
    let second_request = requests.recv().expect("chat request");
    handle.join().expect("join server");

    assert_eq!(report.response_id, "resp_chat_after_compact");
    assert_eq!(report.output_text, "Answer after compaction.");

    let normalized_request = second_request.to_ascii_lowercase();
    assert!(normalized_request.contains("\"instructions\":\"be concise.\""));
    let session_head_marker = normalized_request
        .find("session: compacted chat")
        .expect("session head marker");
    let summary_marker = normalized_request
        .find("compact summary covering earlier context.")
        .expect("compact summary marker");
    assert!(session_head_marker < summary_marker);
    assert!(normalized_request.contains("summary covers: 2 messages"));
    assert!(normalized_request.contains("recent filesystem activity:"));
    assert!(normalized_request.contains("- patch src/main.rs"));
    assert!(normalized_request.contains("workspace tree:"));
    assert!(normalized_request.contains("- readme.md"));
    assert!(normalized_request.contains("- src/"));
    assert!(normalized_request.contains("compact summary covering earlier context."));
    assert!(normalized_request.contains("\"text\":\"recent user one\""));
    assert!(normalized_request.contains("\"text\":\"recent assistant three\""));
    assert!(normalized_request.contains("\"text\":\"latest question\""));
    assert!(!normalized_request.contains("\"text\":\"covered user one\""));
    assert!(!normalized_request.contains("\"text\":\"covered assistant one\""));
}

#[test]
fn execute_chat_turn_includes_the_plan_snapshot_before_context_summary() {
    let (api_base, requests, handle) = spawn_json_server_sequence(vec![
        r#"{
                "id":"resp_plan_chat",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_plan_chat",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Answer with plan context."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":28,"output_tokens":6,"total_tokens":34}
            }"#
        .to_string(),
    ]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(format!("{api_base}/v1")),
            api_key: Some("test-key".to_string()),
            default_model: Some("gpt-5.4".to_string()),
        },
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-plan-chat".to_string(),
            title: "Planned Chat".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            active_mission_id: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");
    store
        .put_plan(
            &PlanRecord::try_from(&PlanSnapshot {
                session_id: "session-plan-chat".to_string(),
                items: vec![
                    PlanItem {
                        id: "inspect".to_string(),
                        content: "Inspect planning seams".to_string(),
                        status: PlanItemStatus::Pending,
                    },
                    PlanItem {
                        id: "persist".to_string(),
                        content: "Persist canonical plan state".to_string(),
                        status: PlanItemStatus::InProgress,
                    },
                ],
                updated_at: 3,
            })
            .expect("plan record"),
        )
        .expect("put plan");
    store
        .put_context_summary(&agent_persistence::ContextSummaryRecord {
            session_id: "session-plan-chat".to_string(),
            summary_text: "Compact summary text.".to_string(),
            covered_message_count: 0,
            summary_token_estimate: 4,
            updated_at: 4,
        })
        .expect("put context summary");

    let report = app
        .execute_chat_turn("session-plan-chat", "what next?", 10)
        .expect("execute chat turn");
    let request = requests.recv().expect("provider request");
    handle.join().expect("join server");

    assert_eq!(report.response_id, "resp_plan_chat");
    let normalized = request.to_ascii_lowercase();
    let session_marker = normalized.find("session: planned chat").expect("session");
    let plan_marker = normalized.find("plan:").expect("plan marker");
    let summary_marker = normalized
        .find("compact summary text.")
        .expect("summary marker");
    assert!(session_marker < plan_marker);
    assert!(plan_marker < summary_marker);
    assert!(normalized.contains("[pending] inspect: inspect planning seams"));
    assert!(normalized.contains("[in_progress] persist: persist canonical plan state"));
}

#[test]
fn execute_chat_turn_can_finish_after_plan_write_and_plan_read_tool_calls() {
    let (api_base, requests, handle) = spawn_json_server_sequence(vec![
        r#"{
                "id":"resp_plan_tools_1",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"fc_plan_write",
                        "type":"function_call",
                        "call_id":"call_plan_write",
                        "name":"plan_write",
                        "arguments":"{\"items\":[{\"id\":\"inspect\",\"content\":\"Inspect planning seams\",\"status\":\"in_progress\"},{\"id\":\"persist\",\"content\":\"Persist plan snapshot\",\"status\":\"pending\"}]}"
                    }
                ],
                "usage":{"input_tokens":30,"output_tokens":10,"total_tokens":40}
            }"#
        .to_string(),
        r#"{
                "id":"resp_plan_tools_2",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"fc_plan_read",
                        "type":"function_call",
                        "call_id":"call_plan_read",
                        "name":"plan_read",
                        "arguments":"{}"
                    }
                ],
                "usage":{"input_tokens":20,"output_tokens":8,"total_tokens":28}
            }"#
        .to_string(),
        r#"{
                "id":"resp_plan_tools_3",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_plan_tools",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Plan updated and read back."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":24,"output_tokens":6,"total_tokens":30}
            }"#
        .to_string(),
    ]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(format!("{api_base}/v1")),
            api_key: Some("test-key".to_string()),
            default_model: Some("gpt-5.4".to_string()),
        },
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-plan-tools".to_string(),
            title: "Plan Tools".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            active_mission_id: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");

    let report = app
        .execute_chat_turn("session-plan-tools", "make a plan", 10)
        .expect("execute chat turn");
    let _first_request = requests.recv().expect("first provider request");
    let second_request = requests.recv().expect("second provider request");
    let third_request = requests.recv().expect("third provider request");
    handle.join().expect("join server");

    assert_eq!(report.response_id, "resp_plan_tools_3");
    assert_eq!(report.output_text, "Plan updated and read back.");

    let plan = PlanSnapshot::try_from(
        store
            .get_plan("session-plan-tools")
            .expect("get plan")
            .expect("plan exists"),
    )
    .expect("restore plan");
    assert_eq!(plan.items.len(), 2);
    assert_eq!(plan.items[0].id, "inspect");
    assert_eq!(plan.items[0].status, PlanItemStatus::InProgress);
    assert_eq!(plan.items[1].id, "persist");
    assert_eq!(plan.items[1].status, PlanItemStatus::Pending);

    let normalized_second = second_request.to_ascii_lowercase();
    assert!(normalized_second.contains("\"type\":\"function_call_output\""));
    assert!(normalized_second.contains("\"call_id\":\"call_plan_write\""));
    assert!(normalized_second.contains("plan_write"));
    assert!(normalized_second.contains("inspect planning seams"));

    let normalized_third = third_request.to_ascii_lowercase();
    assert!(normalized_third.contains("\"call_id\":\"call_plan_read\""));
    assert!(normalized_third.contains("plan_read"));
    assert!(normalized_third.contains("inspect planning seams"));
}
