use super::{
    DEFAULT_MISSION_ACCEPTANCE_JSON, DEFAULT_MISSION_EXECUTION_INTENT,
    DEFAULT_MISSION_SCHEDULE_JSON, LEGACY_MISSION_PREFIX,
};
use crate::{
    AgentProfileRecord, AgentRepository, AgentScheduleRecord, ArtifactRecord, ArtifactRepository,
    ContextOffloadRecord, ContextOffloadRepository, JobRecord, JobRepository, MissionRecord,
    MissionRepository, PersistenceScaffold, PlanRecord, PlanRepository, RunRecord, RunRepository,
    SessionInboxRepository, SessionRecord, SessionRepository, TranscriptRecord,
    TranscriptRepository,
};
use agent_runtime::agent::{
    AgentChainContinuationGrant, AgentProfile, AgentSchedule, AgentTemplateKind,
};
use agent_runtime::context::{ContextOffloadPayload, ContextOffloadRef, ContextOffloadSnapshot};
use agent_runtime::mission::JobExecutionInput;
use agent_runtime::plan::{PlanItem, PlanItemStatus, PlanSnapshot};
use rusqlite::params;
use std::fs;
use std::path::PathBuf;

#[test]
fn open_bootstraps_schema_and_round_trips_structured_and_file_backed_data() {
    let temp = tempfile::tempdir().expect("tempdir");
    let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..crate::AppConfig::default()
    });

    let session = SessionRecord {
        id: "session-1".to_string(),
        title: "Boot mission".to_string(),
        prompt_override: None,
        settings_json: "{\"model\":\"gpt-5.4\"}".to_string(),
        agent_profile_id: "default".to_string(),
        active_mission_id: None,
        parent_session_id: None,
        parent_job_id: None,
        delegation_label: None,
        created_at: 1,
        updated_at: 2,
    };
    let mission = MissionRecord {
        id: "mission-1".to_string(),
        session_id: session.id.clone(),
        objective: "Build stores".to_string(),
        status: "running".to_string(),
        execution_intent: "autonomous".to_string(),
        schedule_json: "{\"not_before\":null,\"interval_seconds\":null}".to_string(),
        acceptance_json: "[]".to_string(),
        created_at: 2,
        updated_at: 3,
        completed_at: None,
    };
    let run = RunRecord {
        id: "run-1".to_string(),
        session_id: session.id.clone(),
        mission_id: Some(mission.id.clone()),
        status: "running".to_string(),
        error: None,
        result: None,
        provider_usage_json: "null".to_string(),
        active_processes_json: "[]".to_string(),
        recent_steps_json: "[]".to_string(),
        evidence_refs_json: "[\"bundle:bootstrap\"]".to_string(),
        pending_approvals_json: "[]".to_string(),
        provider_loop_json: "null".to_string(),
        delegate_runs_json: "[]".to_string(),
        started_at: 3,
        updated_at: 4,
        finished_at: None,
    };
    let job = JobRecord {
        id: "job-1".to_string(),
        session_id: session.id.clone(),
        mission_id: Some(mission.id.clone()),
        run_id: Some(run.id.clone()),
        parent_job_id: None,
        kind: "maintenance".to_string(),
        status: "queued".to_string(),
        input_json: Some(
            serde_json::to_string(&JobExecutionInput::Maintenance {
                summary: "bootstrap schema".to_string(),
            })
            .expect("serialize maintenance input"),
        ),
        result_json: None,
        error: None,
        created_at: 4,
        updated_at: 5,
        started_at: None,
        finished_at: None,
        attempt_count: 0,
        max_attempts: 1,
        lease_owner: None,
        lease_expires_at: None,
        heartbeat_at: None,
        cancel_requested_at: None,
        last_progress_message: None,
        callback_json: None,
        callback_sent_at: None,
    };
    let transcript = TranscriptRecord {
        id: "transcript-1".to_string(),
        session_id: session.id.clone(),
        run_id: Some(run.id.clone()),
        kind: "user".to_string(),
        content: "build the persistence layer".to_string(),
        created_at: 6,
    };
    let artifact = ArtifactRecord {
        id: "artifact-1".to_string(),
        session_id: session.id.clone(),
        kind: "report".to_string(),
        metadata_json: "{\"source\":\"verification\"}".to_string(),
        path: PathBuf::from("artifacts/artifact-1.bin"),
        bytes: b"verification output".to_vec(),
        created_at: 7,
    };
    let plan = PlanRecord {
        session_id: session.id.clone(),
        items_json: serde_json::to_string(&vec![PlanItem {
            id: "inspect".to_string(),
            content: "Inspect planning seams".to_string(),
            status: PlanItemStatus::Pending,
            depends_on: Vec::new(),
            notes: Vec::new(),
            blocked_reason: None,
            parent_task_id: None,
        }])
        .expect("serialize plan"),
        updated_at: 8,
    };
    let offload = ContextOffloadRecord {
        session_id: session.id.clone(),
        refs_json: serde_json::to_string(&vec![ContextOffloadRef {
            id: "offload-1".to_string(),
            label: "Earlier transcript".to_string(),
            summary: "Design notes".to_string(),
            artifact_id: "artifact-offload-1".to_string(),
            token_estimate: 120,
            message_count: 4,
            created_at: 8,
        }])
        .expect("serialize offload"),
        updated_at: 9,
    };
    let offload_payload = ContextOffloadPayload {
        artifact_id: "artifact-offload-1".to_string(),
        bytes: b"earlier transcript chunk".to_vec(),
    };

    {
        let store = super::PersistenceStore::open(&scaffold).expect("open store");
        store.put_session(&session).expect("store session");
        store.put_mission(&mission).expect("store mission");
        store
            .put_session(&SessionRecord {
                active_mission_id: Some(mission.id.clone()),
                agent_profile_id: "default".to_string(),
                ..session.clone()
            })
            .expect("attach active mission");
        store.put_run(&run).expect("store run");
        store.put_job(&job).expect("store job");
        store.put_transcript(&transcript).expect("store transcript");
        store.put_plan(&plan).expect("store plan");
        store
            .put_context_offload(&offload, std::slice::from_ref(&offload_payload))
            .expect("store offload");
        store.put_artifact(&artifact).expect("store artifact");
    }

    let reopened = super::PersistenceStore::open(&scaffold).expect("reopen store");

    assert_eq!(
        reopened.get_session(&session.id).expect("get session"),
        Some(SessionRecord {
            active_mission_id: Some(mission.id.clone()),
            agent_profile_id: "default".to_string(),
            ..session
        })
    );
    assert_eq!(
        reopened.get_mission(&mission.id).expect("get mission"),
        Some(mission)
    );
    assert_eq!(reopened.get_run(&run.id).expect("get run"), Some(run));
    assert_eq!(reopened.get_job(&job.id).expect("get job"), Some(job));
    assert_eq!(
        reopened
            .get_transcript(&transcript.id)
            .expect("get transcript"),
        Some(transcript)
    );
    assert_eq!(
        reopened
            .list_transcripts_for_session("session-1")
            .expect("list transcript history"),
        vec![TranscriptRecord {
            id: "transcript-1".to_string(),
            session_id: "session-1".to_string(),
            run_id: Some("run-1".to_string()),
            kind: "user".to_string(),
            content: "build the persistence layer".to_string(),
            created_at: 6,
        }]
    );
    assert_eq!(
        reopened.get_plan("session-1").expect("get plan"),
        Some(plan)
    );
    assert_eq!(
        reopened
            .get_context_offload("session-1")
            .expect("get offload"),
        Some(offload)
    );
    assert_eq!(
        reopened
            .get_context_offload_payload("artifact-offload-1")
            .expect("get offload payload"),
        Some(offload_payload)
    );
    assert_eq!(
        reopened.get_artifact(&artifact.id).expect("get artifact"),
        Some(artifact)
    );
    assert!(scaffold.stores.metadata_db.exists());
}

#[test]
fn agent_repository_round_trips_profiles_current_selection_and_continuations() {
    let temp = tempfile::tempdir().expect("tempdir");
    let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..crate::AppConfig::default()
    });
    let store = super::PersistenceStore::open(&scaffold).expect("open store");

    let profile = AgentProfile::new(
        "judge",
        "Judge",
        AgentTemplateKind::Judge,
        scaffold.config.data_dir.join("agents/judge"),
        vec!["fs_read_text".to_string(), "plan_snapshot".to_string()],
        10,
        11,
    )
    .expect("agent profile");
    let profile_record = AgentProfileRecord::try_from(&profile).expect("profile to record");

    store
        .put_agent_profile(&profile_record)
        .expect("put agent profile");
    store
        .set_current_agent_profile_id(Some(&profile.id))
        .expect("set current agent");

    let continuation = AgentChainContinuationGrant::new("chain-1", "judge approved", 12)
        .expect("continuation grant");
    let continuation_record = crate::AgentChainContinuationRecord::from(&continuation);
    store
        .put_agent_chain_continuation(&continuation_record)
        .expect("put continuation");

    assert_eq!(
        store
            .get_agent_profile(&profile.id)
            .expect("get agent profile"),
        Some(profile_record.clone())
    );
    assert_eq!(
        store.list_agent_profiles().expect("list agent profiles"),
        vec![profile_record]
    );
    assert_eq!(
        store
            .get_current_agent_profile_id()
            .expect("get current agent"),
        Some("judge".to_string())
    );
    assert_eq!(
        store
            .get_agent_chain_continuation("chain-1")
            .expect("get continuation"),
        Some(continuation_record)
    );
}

#[test]
fn agent_repository_round_trips_schedules() {
    let temp = tempfile::tempdir().expect("tempdir");
    let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..crate::AppConfig::default()
    });
    let store = super::PersistenceStore::open(&scaffold).expect("open store");

    let schedule = AgentSchedule::new(
        "judge-pulse",
        "judge",
        scaffold.config.data_dir.join("workspace"),
        "check the latest diff",
        300,
        30,
        Some(20),
        Some("session-schedule-prev".to_string()),
        Some("job-schedule-prev".to_string()),
        10,
        11,
    )
    .expect("schedule");
    let record = AgentScheduleRecord::from(&schedule);

    store.put_agent_schedule(&record).expect("put schedule");

    assert_eq!(
        store
            .get_agent_schedule("judge-pulse")
            .expect("get schedule"),
        Some(record.clone())
    );
    assert_eq!(
        store.list_agent_schedules().expect("list schedules"),
        vec![record]
    );
}

#[test]
fn store_rejects_mutating_session_agent_profile_id() {
    let temp = tempfile::tempdir().expect("tempdir");
    let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..crate::AppConfig::default()
    });
    let store = super::PersistenceStore::open(&scaffold).expect("open store");

    let session = SessionRecord {
        id: "session-immutable-agent".to_string(),
        title: "Immutable agent".to_string(),
        prompt_override: None,
        settings_json: "{}".to_string(),
        agent_profile_id: "default".to_string(),
        active_mission_id: None,
        parent_session_id: None,
        parent_job_id: None,
        delegation_label: None,
        created_at: 1,
        updated_at: 1,
    };
    store.put_session(&session).expect("put session");

    let error = store
        .put_session(&SessionRecord {
            title: "Retargeted".to_string(),
            agent_profile_id: "judge".to_string(),
            updated_at: 2,
            ..session.clone()
        })
        .expect_err("changing session agent should fail");

    assert!(matches!(
        error,
        super::StoreError::ImmutableSessionAgentProfile {
            session_id,
            existing_agent_profile_id,
            attempted_agent_profile_id,
        } if session_id == "session-immutable-agent"
            && existing_agent_profile_id == "default"
            && attempted_agent_profile_id == "judge"
    ));
    assert_eq!(
        store
            .get_session(&session.id)
            .expect("get session after failure"),
        Some(session)
    );
}

#[test]
fn open_migrates_legacy_sessions_with_default_agent_profile_id() {
    let temp = tempfile::tempdir().expect("tempdir");
    let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..crate::AppConfig::default()
    });

    fs::create_dir_all(
        scaffold
            .stores
            .metadata_db
            .parent()
            .unwrap_or(scaffold.stores.metadata_db.as_path()),
    )
    .expect("create db dir");

    let connection = rusqlite::Connection::open(&scaffold.stores.metadata_db).expect("open sqlite");
    connection
        .execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE missions (
                 id TEXT PRIMARY KEY,
                 session_id TEXT NOT NULL,
                 objective TEXT NOT NULL,
                 status TEXT NOT NULL,
                 execution_intent TEXT NOT NULL,
                 schedule_json TEXT NOT NULL,
                 acceptance_json TEXT NOT NULL,
                 created_at INTEGER NOT NULL,
                 updated_at INTEGER NOT NULL,
                 completed_at INTEGER
             );
             CREATE TABLE sessions (
                 id TEXT PRIMARY KEY,
                 title TEXT NOT NULL,
                 prompt_override TEXT,
                 settings_json TEXT NOT NULL,
                 active_mission_id TEXT,
                 created_at INTEGER NOT NULL,
                 updated_at INTEGER NOT NULL,
                 FOREIGN KEY(active_mission_id) REFERENCES missions(id) ON DELETE SET NULL
             );
             INSERT INTO sessions (
                 id, title, prompt_override, settings_json, active_mission_id, created_at, updated_at
             ) VALUES (
                 'session-legacy', 'Legacy session', NULL, '{\"model\":\"gpt-5.4\"}', NULL, 1, 2
             );",
        )
        .expect("create legacy sessions");
    drop(connection);

    let store = super::PersistenceStore::open(&scaffold).expect("open migrated store");

    assert_eq!(
        store
            .get_session("session-legacy")
            .expect("get migrated session"),
        Some(SessionRecord {
            id: "session-legacy".to_string(),
            title: "Legacy session".to_string(),
            prompt_override: None,
            settings_json: "{\"model\":\"gpt-5.4\"}".to_string(),
            agent_profile_id: "default".to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: None,
            created_at: 1,
            updated_at: 2,
        })
    );
}

#[test]
fn plan_repository_round_trips_structured_plan_snapshots() {
    let temp = tempfile::tempdir().expect("tempdir");
    let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..crate::AppConfig::default()
    });
    let store = super::PersistenceStore::open(&scaffold).expect("open store");
    store
        .put_session(&SessionRecord {
            id: "session-plan".to_string(),
            title: "Plan Session".to_string(),
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
        .expect("put session");

    let snapshot = PlanSnapshot {
        session_id: "session-plan".to_string(),
        goal: Some("Ship plan persistence".to_string()),
        items: vec![
            PlanItem {
                id: "inspect".to_string(),
                content: "Inspect seams".to_string(),
                status: PlanItemStatus::Pending,
                depends_on: Vec::new(),
                notes: Vec::new(),
                blocked_reason: None,
                parent_task_id: None,
            },
            PlanItem {
                id: "persist".to_string(),
                content: "Persist plan".to_string(),
                status: PlanItemStatus::Completed,
                depends_on: vec!["inspect".to_string()],
                notes: Vec::new(),
                blocked_reason: None,
                parent_task_id: None,
            },
        ],
        updated_at: 9,
    };

    store
        .put_plan(&PlanRecord::try_from(&snapshot).expect("plan record"))
        .expect("put plan");
    let restored = PlanSnapshot::try_from(
        store
            .get_plan("session-plan")
            .expect("get plan")
            .expect("plan exists"),
    )
    .expect("restore plan");

    assert_eq!(restored, snapshot);
}

#[test]
fn context_offload_repository_round_trips_snapshot_and_payloads() {
    let temp = tempfile::tempdir().expect("tempdir");
    let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..crate::AppConfig::default()
    });
    let store = super::PersistenceStore::open(&scaffold).expect("open store");
    store
        .put_session(&SessionRecord {
            id: "session-offload".to_string(),
            title: "Offload Session".to_string(),
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
        .expect("put session");

    let snapshot = ContextOffloadSnapshot {
        session_id: "session-offload".to_string(),
        refs: vec![ContextOffloadRef {
            id: "offload-1".to_string(),
            label: "Earlier transcript".to_string(),
            summary: "Requirements and design".to_string(),
            artifact_id: "artifact-offload-1".to_string(),
            token_estimate: 180,
            message_count: 7,
            created_at: 5,
        }],
        updated_at: 6,
    };
    let payload = ContextOffloadPayload {
        artifact_id: "artifact-offload-1".to_string(),
        bytes: b"offloaded transcript bytes".to_vec(),
    };

    store
        .put_context_offload(
            &ContextOffloadRecord::try_from(&snapshot).expect("offload record"),
            std::slice::from_ref(&payload),
        )
        .expect("put offload");

    let restored = ContextOffloadSnapshot::try_from(
        store
            .get_context_offload("session-offload")
            .expect("get offload")
            .expect("offload exists"),
    )
    .expect("restore offload");

    assert_eq!(restored, snapshot);
    assert_eq!(
        store
            .get_context_offload_payload("artifact-offload-1")
            .expect("get offload payload"),
        Some(payload)
    );
}

#[test]
fn replacing_context_offload_snapshot_prunes_obsolete_artifacts() {
    let temp = tempfile::tempdir().expect("tempdir");
    let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..crate::AppConfig::default()
    });
    let store = super::PersistenceStore::open(&scaffold).expect("open store");
    store
        .put_session(&SessionRecord {
            id: "session-offload-prune".to_string(),
            title: "Offload Prune".to_string(),
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
        .expect("put session");

    let first = ContextOffloadSnapshot {
        session_id: "session-offload-prune".to_string(),
        refs: vec![ContextOffloadRef {
            id: "offload-1".to_string(),
            label: "Earlier transcript".to_string(),
            summary: "Version one".to_string(),
            artifact_id: "artifact-offload-old".to_string(),
            token_estimate: 42,
            message_count: 2,
            created_at: 2,
        }],
        updated_at: 3,
    };
    store
        .put_context_offload(
            &ContextOffloadRecord::try_from(&first).expect("first offload"),
            &[ContextOffloadPayload {
                artifact_id: "artifact-offload-old".to_string(),
                bytes: b"old payload".to_vec(),
            }],
        )
        .expect("put first offload");

    let second = ContextOffloadSnapshot {
        session_id: "session-offload-prune".to_string(),
        refs: vec![ContextOffloadRef {
            id: "offload-2".to_string(),
            label: "Replacement".to_string(),
            summary: "Version two".to_string(),
            artifact_id: "artifact-offload-new".to_string(),
            token_estimate: 55,
            message_count: 3,
            created_at: 4,
        }],
        updated_at: 5,
    };
    store
        .put_context_offload(
            &ContextOffloadRecord::try_from(&second).expect("second offload"),
            &[ContextOffloadPayload {
                artifact_id: "artifact-offload-new".to_string(),
                bytes: b"new payload".to_vec(),
            }],
        )
        .expect("replace offload");

    assert!(
        store
            .get_context_offload_payload("artifact-offload-old")
            .expect("get old payload")
            .is_none()
    );
    assert_eq!(
        store
            .get_context_offload_payload("artifact-offload-new")
            .expect("get new payload")
            .expect("new payload exists")
            .bytes,
        b"new payload".to_vec()
    );
}

#[test]
fn open_migrates_legacy_mission_and_job_schema() {
    let temp = tempfile::tempdir().expect("tempdir");
    let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..crate::AppConfig::default()
    });

    fs::create_dir_all(
        scaffold
            .stores
            .metadata_db
            .parent()
            .unwrap_or(scaffold.stores.metadata_db.as_path()),
    )
    .expect("create db dir");

    let connection = rusqlite::Connection::open(&scaffold.stores.metadata_db).expect("open sqlite");
    connection
        .execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE sessions (
                 id TEXT PRIMARY KEY,
                 title TEXT NOT NULL,
                 prompt_override TEXT,
                 settings_json TEXT NOT NULL,
                 active_mission_id TEXT,
                 created_at INTEGER NOT NULL,
                 updated_at INTEGER NOT NULL,
                 FOREIGN KEY(active_mission_id) REFERENCES missions(id) ON DELETE SET NULL
             );
             CREATE TABLE missions (
                 id TEXT PRIMARY KEY,
                 session_id TEXT NOT NULL,
                 objective TEXT NOT NULL,
                 status TEXT NOT NULL,
                 created_at INTEGER NOT NULL,
                 updated_at INTEGER NOT NULL,
                 completed_at INTEGER,
                 FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE
             );
             CREATE TABLE runs (
                 id TEXT PRIMARY KEY,
                 session_id TEXT NOT NULL,
                 mission_id TEXT,
                 status TEXT NOT NULL,
                 error TEXT,
                 result TEXT,
                 active_processes_json TEXT NOT NULL DEFAULT '[]',
                 started_at INTEGER NOT NULL,
                 updated_at INTEGER NOT NULL,
                 finished_at INTEGER,
                 FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE,
                 FOREIGN KEY(mission_id) REFERENCES missions(id) ON DELETE SET NULL
             );
             CREATE TABLE jobs (
                 id TEXT PRIMARY KEY,
                 run_id TEXT NOT NULL,
                 parent_job_id TEXT,
                 kind TEXT NOT NULL,
                 status TEXT NOT NULL,
                 input_json TEXT,
                 result_json TEXT,
                 error TEXT,
                 created_at INTEGER NOT NULL,
                 updated_at INTEGER NOT NULL,
                 started_at INTEGER,
                 finished_at INTEGER,
                 FOREIGN KEY(run_id) REFERENCES runs(id) ON DELETE CASCADE,
                 FOREIGN KEY(parent_job_id) REFERENCES jobs(id) ON DELETE SET NULL
             );
             CREATE TABLE session_inbox_events (
                 id TEXT PRIMARY KEY,
                 session_id TEXT NOT NULL,
                 job_id TEXT,
                 kind TEXT NOT NULL,
                 payload_json TEXT NOT NULL,
                 status TEXT NOT NULL,
                 created_at INTEGER NOT NULL,
                 available_at INTEGER NOT NULL,
                 claimed_at INTEGER,
                 processed_at INTEGER,
                 error TEXT,
                 FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE,
                 FOREIGN KEY(job_id) REFERENCES jobs(id) ON DELETE SET NULL
             );
             INSERT INTO sessions (
                 id, title, prompt_override, settings_json, active_mission_id, created_at, updated_at
             ) VALUES (
                 'session-1', 'Legacy mission', NULL, '{\"model\":\"gpt-5.4\"}', NULL, 1, 1
             );
             INSERT INTO missions (
                 id, session_id, objective, status, created_at, updated_at, completed_at
             ) VALUES (
                 'mission-1', 'session-1', 'Carry forward existing missions', 'ready', 2, 2, NULL
             );
             INSERT INTO runs (
                 id, session_id, mission_id, status, error, result, started_at, updated_at, finished_at
             ) VALUES (
                 'run-1', 'session-1', NULL, 'running', NULL, NULL, 3, 4, NULL
             );
             INSERT INTO jobs (
                 id, run_id, parent_job_id, kind, status, input_json, result_json, error,
                 created_at, updated_at, started_at, finished_at
             ) VALUES (
                 'job-1',
                 'run-1',
                 NULL,
                 'maintenance',
                 'queued',
                 '{\"Maintenance\":{\"summary\":\"legacy bootstrap\"}}',
                 NULL,
                 NULL,
                 4,
                 5,
                 NULL,
                 NULL
             );
             INSERT INTO session_inbox_events (
                 id, session_id, job_id, kind, payload_json, status, created_at, available_at,
                 claimed_at, processed_at, error
             ) VALUES (
                 'event-legacy',
                 'session-1',
                 'job-1',
                 'job_completed',
                 '{\"job_id\":\"job-1\"}',
                 'queued',
                 6,
                 6,
                 NULL,
                 NULL,
                 NULL
             );",
        )
        .expect("create legacy schema");
    drop(connection);

    let reopened = super::PersistenceStore::open(&scaffold).expect("migrate legacy schema");

    assert_eq!(
        reopened
            .get_mission("mission-1")
            .expect("get migrated mission"),
        Some(MissionRecord {
            id: "mission-1".to_string(),
            session_id: "session-1".to_string(),
            objective: "Carry forward existing missions".to_string(),
            status: "ready".to_string(),
            execution_intent: DEFAULT_MISSION_EXECUTION_INTENT.to_string(),
            schedule_json: DEFAULT_MISSION_SCHEDULE_JSON.to_string(),
            acceptance_json: DEFAULT_MISSION_ACCEPTANCE_JSON.to_string(),
            created_at: 2,
            updated_at: 2,
            completed_at: None,
        })
    );

    assert_eq!(
        reopened.get_run("run-1").expect("get migrated run"),
        Some(RunRecord {
            id: "run-1".to_string(),
            session_id: "session-1".to_string(),
            mission_id: Some(format!("{LEGACY_MISSION_PREFIX}run-1")),
            status: "running".to_string(),
            error: None,
            result: None,
            provider_usage_json: "null".to_string(),
            active_processes_json: "[]".to_string(),
            recent_steps_json: "[]".to_string(),
            evidence_refs_json: "[]".to_string(),
            pending_approvals_json: "[]".to_string(),
            provider_loop_json: "null".to_string(),
            delegate_runs_json: "[]".to_string(),
            started_at: 3,
            updated_at: 4,
            finished_at: None,
        })
    );
    assert_eq!(
        reopened.get_job("job-1").expect("get migrated job"),
        Some(JobRecord {
            id: "job-1".to_string(),
            session_id: "session-1".to_string(),
            mission_id: Some(format!("{LEGACY_MISSION_PREFIX}run-1")),
            run_id: Some("run-1".to_string()),
            parent_job_id: None,
            kind: "maintenance".to_string(),
            status: "queued".to_string(),
            input_json: Some(
                serde_json::to_string(&JobExecutionInput::Maintenance {
                    summary: "legacy bootstrap".to_string(),
                })
                .expect("serialize maintenance input"),
            ),
            result_json: None,
            error: None,
            created_at: 4,
            updated_at: 5,
            started_at: None,
            finished_at: None,
            attempt_count: 0,
            max_attempts: 1,
            lease_owner: None,
            lease_expires_at: None,
            heartbeat_at: None,
            cancel_requested_at: None,
            last_progress_message: None,
            callback_json: None,
            callback_sent_at: None,
        })
    );
    assert_eq!(
        reopened
            .get_mission(&format!("{LEGACY_MISSION_PREFIX}run-1"))
            .expect("get synthesized mission"),
        Some(MissionRecord {
            id: format!("{LEGACY_MISSION_PREFIX}run-1"),
            session_id: "session-1".to_string(),
            objective: "Recovered legacy mission for run run-1".to_string(),
            status: "ready".to_string(),
            execution_intent: DEFAULT_MISSION_EXECUTION_INTENT.to_string(),
            schedule_json: DEFAULT_MISSION_SCHEDULE_JSON.to_string(),
            acceptance_json: DEFAULT_MISSION_ACCEPTANCE_JSON.to_string(),
            created_at: 3,
            updated_at: 4,
            completed_at: None,
        })
    );

    let mut foreign_key_statement = reopened
        .connection
        .prepare("PRAGMA foreign_key_list(session_inbox_events)")
        .expect("prepare foreign key list");
    let mut foreign_key_rows = foreign_key_statement
        .query([])
        .expect("query foreign key list");
    let mut job_foreign_key_target = None;
    while let Some(row) = foreign_key_rows.next().expect("read foreign key row") {
        let from_column: String = row.get(3).expect("from column");
        if from_column == "job_id" {
            let target_table: String = row.get(2).expect("target table");
            job_foreign_key_target = Some(target_table);
        }
    }
    assert_eq!(job_foreign_key_target.as_deref(), Some("jobs"));

    assert_eq!(
        reopened
            .get_session_inbox_event("event-legacy")
            .expect("load migrated inbox event"),
        Some(crate::SessionInboxEventRecord {
            id: "event-legacy".to_string(),
            session_id: "session-1".to_string(),
            job_id: Some("job-1".to_string()),
            kind: "job_completed".to_string(),
            payload_json: "{\"job_id\":\"job-1\"}".to_string(),
            status: "queued".to_string(),
            created_at: 6,
            available_at: 6,
            claimed_at: None,
            processed_at: None,
            error: None,
        })
    );

    reopened
        .put_session_inbox_event(&crate::SessionInboxEventRecord {
            id: "event-new".to_string(),
            session_id: "session-1".to_string(),
            job_id: Some("job-1".to_string()),
            kind: "job_completed".to_string(),
            payload_json: "{\"job_id\":\"job-1\",\"summary\":\"done\"}".to_string(),
            status: "queued".to_string(),
            created_at: 7,
            available_at: 7,
            claimed_at: None,
            processed_at: None,
            error: None,
        })
        .expect("store inbox event after migration");
}

#[test]
fn file_backed_payloads_reject_unsafe_identifiers() {
    let temp = tempfile::tempdir().expect("tempdir");
    let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..crate::AppConfig::default()
    });
    let store = super::PersistenceStore::open(&scaffold).expect("open store");

    let transcript = TranscriptRecord {
        id: "../escape".to_string(),
        session_id: "session-1".to_string(),
        run_id: None,
        kind: "user".to_string(),
        content: "hello".to_string(),
        created_at: 1,
    };

    let artifact = ArtifactRecord {
        id: "../escape".to_string(),
        session_id: "session-1".to_string(),
        kind: "binary".to_string(),
        metadata_json: "{\"mime\":\"application/octet-stream\"}".to_string(),
        path: PathBuf::from("artifacts/escape.bin"),
        bytes: vec![1, 2, 3],
        created_at: 1,
    };

    assert!(matches!(
        store.put_transcript(&transcript),
        Err(super::StoreError::InvalidIdentifier { .. })
    ));
    assert!(matches!(
        store.put_artifact(&artifact),
        Err(super::StoreError::InvalidIdentifier { .. })
    ));
}

#[test]
fn list_transcripts_for_session_orders_by_timestamp_and_id() {
    let temp = tempfile::tempdir().expect("tempdir");
    let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..crate::AppConfig::default()
    });
    let store = super::PersistenceStore::open(&scaffold).expect("open store");

    let session = SessionRecord {
        id: "session-1".to_string(),
        title: "Boot mission".to_string(),
        prompt_override: None,
        settings_json: "{\"model\":\"gpt-5.4\"}".to_string(),
        agent_profile_id: "default".to_string(),
        active_mission_id: None,
        parent_session_id: None,
        parent_job_id: None,
        delegation_label: None,
        created_at: 1,
        updated_at: 1,
    };
    store.put_session(&session).expect("store session");

    store
        .put_transcript(&TranscriptRecord {
            id: "transcript-b".to_string(),
            session_id: session.id.clone(),
            run_id: None,
            kind: "assistant".to_string(),
            content: "second".to_string(),
            created_at: 2,
        })
        .expect("store transcript b");
    store
        .put_transcript(&TranscriptRecord {
            id: "transcript-a".to_string(),
            session_id: session.id.clone(),
            run_id: None,
            kind: "user".to_string(),
            content: "first".to_string(),
            created_at: 2,
        })
        .expect("store transcript a");
    store
        .put_transcript(&TranscriptRecord {
            id: "transcript-c".to_string(),
            session_id: session.id.clone(),
            run_id: None,
            kind: "tool".to_string(),
            content: "third".to_string(),
            created_at: 3,
        })
        .expect("store transcript c");

    let history = store
        .list_transcripts_for_session(&session.id)
        .expect("list transcripts");

    assert_eq!(
        history
            .iter()
            .map(|record| record.id.as_str())
            .collect::<Vec<_>>(),
        vec!["transcript-a", "transcript-b", "transcript-c"]
    );
}

#[test]
fn list_execution_records_orders_sessions_missions_jobs_and_runs_stably() {
    let temp = tempfile::tempdir().expect("tempdir");
    let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..crate::AppConfig::default()
    });
    let store = super::PersistenceStore::open(&scaffold).expect("open store");

    let session_b = SessionRecord {
        id: "session-b".to_string(),
        title: "Second session".to_string(),
        prompt_override: None,
        settings_json: "{}".to_string(),
        agent_profile_id: "default".to_string(),
        active_mission_id: None,
        parent_session_id: None,
        parent_job_id: None,
        delegation_label: None,
        created_at: 2,
        updated_at: 2,
    };
    let session_a = SessionRecord {
        id: "session-a".to_string(),
        title: "First session".to_string(),
        prompt_override: None,
        settings_json: "{}".to_string(),
        agent_profile_id: "default".to_string(),
        active_mission_id: None,
        parent_session_id: None,
        parent_job_id: None,
        delegation_label: None,
        created_at: 2,
        updated_at: 2,
    };
    store.put_session(&session_b).expect("put session b");
    store.put_session(&session_a).expect("put session a");

    let mission_b = MissionRecord {
        id: "mission-b".to_string(),
        session_id: session_b.id.clone(),
        objective: "Second mission".to_string(),
        status: "ready".to_string(),
        execution_intent: "autonomous".to_string(),
        schedule_json: DEFAULT_MISSION_SCHEDULE_JSON.to_string(),
        acceptance_json: DEFAULT_MISSION_ACCEPTANCE_JSON.to_string(),
        created_at: 3,
        updated_at: 3,
        completed_at: None,
    };
    let mission_a = MissionRecord {
        id: "mission-a".to_string(),
        session_id: session_a.id.clone(),
        objective: "First mission".to_string(),
        status: "ready".to_string(),
        execution_intent: "autonomous".to_string(),
        schedule_json: DEFAULT_MISSION_SCHEDULE_JSON.to_string(),
        acceptance_json: DEFAULT_MISSION_ACCEPTANCE_JSON.to_string(),
        created_at: 3,
        updated_at: 3,
        completed_at: None,
    };
    store.put_mission(&mission_b).expect("put mission b");
    store.put_mission(&mission_a).expect("put mission a");

    let run_b = RunRecord {
        id: "run-b".to_string(),
        session_id: session_b.id.clone(),
        mission_id: Some(mission_b.id.clone()),
        status: "queued".to_string(),
        error: None,
        result: None,
        provider_usage_json: "null".to_string(),
        active_processes_json: "[]".to_string(),
        recent_steps_json: "[]".to_string(),
        evidence_refs_json: "[]".to_string(),
        pending_approvals_json: "[]".to_string(),
        provider_loop_json: "null".to_string(),
        delegate_runs_json: "[]".to_string(),
        started_at: 5,
        updated_at: 5,
        finished_at: None,
    };
    let run_a = RunRecord {
        id: "run-a".to_string(),
        session_id: session_a.id.clone(),
        mission_id: Some(mission_a.id.clone()),
        status: "queued".to_string(),
        error: None,
        result: None,
        provider_usage_json: "null".to_string(),
        active_processes_json: "[]".to_string(),
        recent_steps_json: "[]".to_string(),
        evidence_refs_json: "[]".to_string(),
        pending_approvals_json: "[]".to_string(),
        provider_loop_json: "null".to_string(),
        delegate_runs_json: "[]".to_string(),
        started_at: 5,
        updated_at: 5,
        finished_at: None,
    };
    store.put_run(&run_b).expect("put run b");
    store.put_run(&run_a).expect("put run a");

    let job_b = JobRecord {
        id: "job-b".to_string(),
        session_id: session_b.id.clone(),
        mission_id: Some(mission_b.id.clone()),
        run_id: Some(run_b.id.clone()),
        parent_job_id: None,
        kind: "mission_turn".to_string(),
        status: "queued".to_string(),
        input_json: Some(
            serde_json::to_string(&JobExecutionInput::MissionTurn {
                mission_id: mission_b.id.clone(),
                goal: "second".to_string(),
            })
            .expect("serialize input b"),
        ),
        result_json: None,
        error: None,
        created_at: 4,
        updated_at: 4,
        started_at: None,
        finished_at: None,
        attempt_count: 0,
        max_attempts: 1,
        lease_owner: None,
        lease_expires_at: None,
        heartbeat_at: None,
        cancel_requested_at: None,
        last_progress_message: None,
        callback_json: None,
        callback_sent_at: None,
    };
    let job_a = JobRecord {
        id: "job-a".to_string(),
        session_id: session_a.id.clone(),
        mission_id: Some(mission_a.id.clone()),
        run_id: Some(run_a.id.clone()),
        parent_job_id: None,
        kind: "mission_turn".to_string(),
        status: "queued".to_string(),
        input_json: Some(
            serde_json::to_string(&JobExecutionInput::MissionTurn {
                mission_id: mission_a.id.clone(),
                goal: "first".to_string(),
            })
            .expect("serialize input a"),
        ),
        result_json: None,
        error: None,
        created_at: 4,
        updated_at: 4,
        started_at: None,
        finished_at: None,
        attempt_count: 0,
        max_attempts: 1,
        lease_owner: None,
        lease_expires_at: None,
        heartbeat_at: None,
        cancel_requested_at: None,
        last_progress_message: None,
        callback_json: None,
        callback_sent_at: None,
    };
    store.put_job(&job_b).expect("put job b");
    store.put_job(&job_a).expect("put job a");

    let sessions = store.list_sessions().expect("list sessions");
    let missions = store.list_missions().expect("list missions");
    let jobs = store.list_jobs().expect("list jobs");
    let runs = store.list_runs().expect("list runs");

    assert_eq!(
        sessions
            .iter()
            .map(|record| record.id.as_str())
            .collect::<Vec<_>>(),
        vec!["session-a", "session-b"]
    );
    assert_eq!(
        missions
            .iter()
            .map(|record| record.id.as_str())
            .collect::<Vec<_>>(),
        vec!["mission-a", "mission-b"]
    );
    assert_eq!(
        jobs.iter()
            .map(|record| record.id.as_str())
            .collect::<Vec<_>>(),
        vec!["job-a", "job-b"]
    );
    assert_eq!(
        runs.iter()
            .map(|record| record.id.as_str())
            .collect::<Vec<_>>(),
        vec!["run-a", "run-b"]
    );
}

#[test]
fn load_execution_state_returns_one_typed_snapshot_for_scheduler_inputs() {
    let temp = tempfile::tempdir().expect("tempdir");
    let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..crate::AppConfig::default()
    });
    let store = super::PersistenceStore::open(&scaffold).expect("open store");

    let session = SessionRecord {
        id: "session-1".to_string(),
        title: "Execution session".to_string(),
        prompt_override: None,
        settings_json: "{}".to_string(),
        agent_profile_id: "default".to_string(),
        active_mission_id: None,
        parent_session_id: None,
        parent_job_id: None,
        delegation_label: None,
        created_at: 1,
        updated_at: 2,
    };
    let mission = MissionRecord {
        id: "mission-1".to_string(),
        session_id: session.id.clone(),
        objective: "Tick the mission loop".to_string(),
        status: "ready".to_string(),
        execution_intent: "autonomous".to_string(),
        schedule_json: DEFAULT_MISSION_SCHEDULE_JSON.to_string(),
        acceptance_json: DEFAULT_MISSION_ACCEPTANCE_JSON.to_string(),
        created_at: 2,
        updated_at: 3,
        completed_at: None,
    };
    let job = JobRecord {
        id: "job-1".to_string(),
        session_id: session.id.clone(),
        mission_id: Some(mission.id.clone()),
        run_id: None,
        parent_job_id: None,
        kind: "mission_turn".to_string(),
        status: "queued".to_string(),
        input_json: Some(
            serde_json::to_string(&JobExecutionInput::MissionTurn {
                mission_id: mission.id.clone(),
                goal: "advance".to_string(),
            })
            .expect("serialize mission turn"),
        ),
        result_json: None,
        error: None,
        created_at: 4,
        updated_at: 4,
        started_at: None,
        finished_at: None,
        attempt_count: 0,
        max_attempts: 1,
        lease_owner: None,
        lease_expires_at: None,
        heartbeat_at: None,
        cancel_requested_at: None,
        last_progress_message: None,
        callback_json: None,
        callback_sent_at: None,
    };
    let run = RunRecord {
        id: "run-1".to_string(),
        session_id: session.id.clone(),
        mission_id: Some(mission.id.clone()),
        status: "queued".to_string(),
        error: None,
        result: None,
        provider_usage_json: "null".to_string(),
        active_processes_json: "[]".to_string(),
        recent_steps_json: "[]".to_string(),
        evidence_refs_json: "[]".to_string(),
        pending_approvals_json: "[]".to_string(),
        provider_loop_json: "null".to_string(),
        delegate_runs_json: "[]".to_string(),
        started_at: 5,
        updated_at: 5,
        finished_at: None,
    };

    store.put_session(&session).expect("put session");
    store.put_mission(&mission).expect("put mission");
    store.put_job(&job).expect("put job");
    store.put_run(&run).expect("put run");

    let snapshot = store.load_execution_state().expect("load execution state");

    assert_eq!(snapshot.sessions, vec![session]);
    assert_eq!(snapshot.missions, vec![mission]);
    assert_eq!(snapshot.jobs, vec![job]);
    assert_eq!(snapshot.runs, vec![run]);
}

#[test]
fn open_removes_orphan_payload_files() {
    let temp = tempfile::tempdir().expect("tempdir");
    let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..crate::AppConfig::default()
    });

    fs::create_dir_all(&scaffold.stores.transcripts_dir).expect("create transcript dir");
    fs::create_dir_all(&scaffold.stores.artifacts_dir).expect("create artifact dir");

    let orphan_transcript = scaffold.stores.transcripts_dir.join("orphan.txt");
    let orphan_artifact = scaffold.stores.artifacts_dir.join("orphan.bin");
    fs::write(&orphan_transcript, "orphan transcript").expect("write transcript");
    fs::write(&orphan_artifact, "orphan artifact").expect("write artifact");

    let _store = super::PersistenceStore::open(&scaffold).expect("open store");

    assert!(!orphan_transcript.exists());
    assert!(!orphan_artifact.exists());
}

#[test]
fn open_does_not_prune_payloads_that_are_mid_commit() {
    let temp = tempfile::tempdir().expect("tempdir");
    let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..crate::AppConfig::default()
    });
    let store = super::PersistenceStore::open(&scaffold).expect("open store");

    let session = SessionRecord {
        id: "session-1".to_string(),
        title: "Store payloads".to_string(),
        prompt_override: None,
        settings_json: "{\"model\":\"gpt-5.4\"}".to_string(),
        agent_profile_id: "default".to_string(),
        active_mission_id: None,
        parent_session_id: None,
        parent_job_id: None,
        delegation_label: None,
        created_at: 1,
        updated_at: 1,
    };
    store.put_session(&session).expect("store session");

    let transcript = TranscriptRecord {
        id: "transcript-race".to_string(),
        session_id: session.id.clone(),
        run_id: None,
        kind: "user".to_string(),
        content: "mid-commit transcript".to_string(),
        created_at: 2,
    };
    let path = scaffold.stores.transcripts_dir.join("transcript-race.txt");
    let storage_key = path
        .file_name()
        .and_then(|name| name.to_str())
        .expect("storage key")
        .to_string();
    let sha256 = super::sha256_hex(transcript.content.as_bytes());

    super::persist_payload_with_commit(&path, transcript.content.as_bytes(), || {
        let _concurrent = super::PersistenceStore::open(&scaffold).expect("concurrent open");
        store
            .connection
            .execute(
                "INSERT INTO transcripts (
                    id, session_id, run_id, kind, storage_key, byte_len, sha256, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    transcript.id.clone(),
                    transcript.session_id.clone(),
                    transcript.run_id.clone(),
                    transcript.kind.clone(),
                    storage_key.clone(),
                    transcript.content.len() as i64,
                    sha256.clone(),
                    transcript.created_at
                ],
            )
            .map(|_| ())
            .map_err(super::StoreError::from)
    })
    .expect("persist transcript through concurrent reconcile");

    assert!(
        path.exists(),
        "payload should survive concurrent store open"
    );
    assert_eq!(
        store
            .get_transcript("transcript-race")
            .expect("get transcript"),
        Some(transcript)
    );
}

#[test]
fn open_removes_payloads_that_do_not_match_metadata() {
    let temp = tempfile::tempdir().expect("tempdir");
    let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..crate::AppConfig::default()
    });
    let store = super::PersistenceStore::open(&scaffold).expect("open store");

    let session = SessionRecord {
        id: "session-1".to_string(),
        title: "Store payloads".to_string(),
        prompt_override: None,
        settings_json: "{\"model\":\"gpt-5.4\"}".to_string(),
        agent_profile_id: "default".to_string(),
        active_mission_id: None,
        parent_session_id: None,
        parent_job_id: None,
        delegation_label: None,
        created_at: 1,
        updated_at: 1,
    };
    store.put_session(&session).expect("store session");

    let transcript = TranscriptRecord {
        id: "transcript-1".to_string(),
        session_id: session.id.clone(),
        run_id: None,
        kind: "user".to_string(),
        content: "original transcript".to_string(),
        created_at: 1,
    };
    store.put_transcript(&transcript).expect("store transcript");

    let artifact = ArtifactRecord {
        id: "artifact-1".to_string(),
        session_id: session.id.clone(),
        kind: "report".to_string(),
        metadata_json: "{\"source\":\"test\"}".to_string(),
        path: PathBuf::from("artifacts/artifact-1.bin"),
        bytes: b"original artifact".to_vec(),
        created_at: 1,
    };
    store.put_artifact(&artifact).expect("store artifact");
    drop(store);

    fs::write(
        scaffold.stores.transcripts_dir.join("transcript-1.txt"),
        "tampered transcript",
    )
    .expect("tamper transcript");
    fs::write(
        scaffold.stores.artifacts_dir.join("artifact-1.bin"),
        b"tampered artifact",
    )
    .expect("tamper artifact");

    let _store = super::PersistenceStore::open(&scaffold).expect("reopen store");

    assert!(
        !scaffold
            .stores
            .transcripts_dir
            .join("transcript-1.txt")
            .exists()
    );
    assert!(
        !scaffold
            .stores
            .artifacts_dir
            .join("artifact-1.bin")
            .exists()
    );
}

#[test]
fn jobs_schema_includes_session_scoped_background_columns() {
    let temp = tempfile::tempdir().expect("tempdir");
    let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..crate::AppConfig::default()
    });
    let store = super::PersistenceStore::open(&scaffold).expect("open store");

    let mut statement = store
        .connection
        .prepare("PRAGMA table_info(jobs)")
        .expect("prepare pragma");
    let mut rows = statement.query([]).expect("query pragma");
    let mut columns = Vec::new();
    while let Some(row) = rows.next().expect("next pragma row") {
        columns.push(row.get::<_, String>(1).expect("column name"));
    }

    assert!(columns.iter().any(|column| column == "session_id"));
    assert!(columns.iter().any(|column| column == "attempt_count"));
    assert!(columns.iter().any(|column| column == "lease_owner"));
    assert!(columns.iter().any(|column| column == "heartbeat_at"));
    assert!(columns.iter().any(|column| column == "cancel_requested_at"));
    assert!(columns.iter().any(|column| column == "callback_json"));
    assert!(columns.iter().any(|column| column == "callback_sent_at"));
    assert!(
        columns
            .iter()
            .any(|column| column == "last_progress_message")
    );
}

#[test]
fn open_rejects_incompatible_existing_schema() {
    let temp = tempfile::tempdir().expect("tempdir");
    let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..crate::AppConfig::default()
    });

    fs::create_dir_all(
        scaffold
            .stores
            .metadata_db
            .parent()
            .unwrap_or(scaffold.stores.metadata_db.as_path()),
    )
    .expect("create db dir");

    let connection = rusqlite::Connection::open(&scaffold.stores.metadata_db).expect("open sqlite");
    connection
        .execute_batch(
            "CREATE TABLE sessions (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                prompt_override TEXT,
                active_mission_id TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );",
        )
        .expect("create legacy schema");
    drop(connection);

    assert!(matches!(
        super::PersistenceStore::open(&scaffold),
        Err(super::StoreError::SchemaMismatch { .. })
    ));
}

#[test]
fn failed_metadata_updates_restore_previous_payloads() {
    let temp = tempfile::tempdir().expect("tempdir");
    let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..crate::AppConfig::default()
    });
    let store = super::PersistenceStore::open(&scaffold).expect("open store");

    let session = SessionRecord {
        id: "session-1".to_string(),
        title: "Store payloads".to_string(),
        prompt_override: None,
        settings_json: "{\"model\":\"gpt-5.4\"}".to_string(),
        agent_profile_id: "default".to_string(),
        active_mission_id: None,
        parent_session_id: None,
        parent_job_id: None,
        delegation_label: None,
        created_at: 1,
        updated_at: 1,
    };
    store.put_session(&session).expect("store session");

    let transcript = TranscriptRecord {
        id: "transcript-1".to_string(),
        session_id: session.id.clone(),
        run_id: None,
        kind: "user".to_string(),
        content: "original".to_string(),
        created_at: 1,
    };
    store.put_transcript(&transcript).expect("store transcript");

    let artifact = ArtifactRecord {
        id: "artifact-1".to_string(),
        session_id: session.id.clone(),
        kind: "report".to_string(),
        metadata_json: "{\"source\":\"test\"}".to_string(),
        path: PathBuf::from("artifacts/artifact-1.bin"),
        bytes: b"original".to_vec(),
        created_at: 1,
    };
    store.put_artifact(&artifact).expect("store artifact");

    let broken_transcript = TranscriptRecord {
        session_id: "missing-session".to_string(),
        content: "replacement".to_string(),
        ..transcript.clone()
    };
    let broken_artifact = ArtifactRecord {
        session_id: "missing-session".to_string(),
        bytes: b"replacement".to_vec(),
        ..artifact.clone()
    };

    assert!(store.put_transcript(&broken_transcript).is_err());
    assert!(store.put_artifact(&broken_artifact).is_err());

    assert_eq!(
        store
            .get_transcript(&transcript.id)
            .expect("get transcript after failure"),
        Some(transcript)
    );
    assert_eq!(
        store
            .get_artifact(&artifact.id)
            .expect("get artifact after failure"),
        Some(artifact)
    );
}

#[test]
fn reads_fail_when_payloads_no_longer_match_metadata() {
    let temp = tempfile::tempdir().expect("tempdir");
    let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..crate::AppConfig::default()
    });
    let store = super::PersistenceStore::open(&scaffold).expect("open store");

    let session = SessionRecord {
        id: "session-1".to_string(),
        title: "Store payloads".to_string(),
        prompt_override: None,
        settings_json: "{\"model\":\"gpt-5.4\"}".to_string(),
        agent_profile_id: "default".to_string(),
        active_mission_id: None,
        parent_session_id: None,
        parent_job_id: None,
        delegation_label: None,
        created_at: 1,
        updated_at: 1,
    };
    store.put_session(&session).expect("store session");

    let transcript = TranscriptRecord {
        id: "transcript-1".to_string(),
        session_id: session.id.clone(),
        run_id: None,
        kind: "user".to_string(),
        content: "original transcript".to_string(),
        created_at: 1,
    };
    store.put_transcript(&transcript).expect("store transcript");

    let artifact = ArtifactRecord {
        id: "artifact-1".to_string(),
        session_id: session.id.clone(),
        kind: "report".to_string(),
        metadata_json: "{\"source\":\"test\"}".to_string(),
        path: PathBuf::from("artifacts/artifact-1.bin"),
        bytes: b"original artifact".to_vec(),
        created_at: 1,
    };
    store.put_artifact(&artifact).expect("store artifact");

    fs::write(
        scaffold.stores.transcripts_dir.join("transcript-1.txt"),
        "tampered transcript",
    )
    .expect("tamper transcript");
    fs::write(
        scaffold.stores.artifacts_dir.join("artifact-1.bin"),
        b"tampered artifact",
    )
    .expect("tamper artifact");

    assert!(matches!(
        store.get_transcript(&transcript.id),
        Err(super::StoreError::IntegrityMismatch { .. })
    ));
    assert!(matches!(
        store.get_artifact(&artifact.id),
        Err(super::StoreError::IntegrityMismatch { .. })
    ));
}

#[test]
fn put_artifact_recreates_the_payload_directory_when_it_was_removed() {
    let temp = tempfile::tempdir().expect("tempdir");
    let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..crate::AppConfig::default()
    });
    let store = super::PersistenceStore::open(&scaffold).expect("open store");

    let session = SessionRecord {
        id: "session-artifacts-recreated".to_string(),
        title: "Artifacts recreated".to_string(),
        prompt_override: None,
        settings_json: "{\"model\":\"gpt-5.4\"}".to_string(),
        agent_profile_id: "default".to_string(),
        active_mission_id: None,
        parent_session_id: None,
        parent_job_id: None,
        delegation_label: None,
        created_at: 1,
        updated_at: 1,
    };
    store.put_session(&session).expect("put session");

    fs::remove_dir_all(&scaffold.stores.artifacts_dir).expect("remove artifacts dir");
    assert!(!scaffold.stores.artifacts_dir.exists());

    let artifact = ArtifactRecord {
        id: "artifact-recreated".to_string(),
        session_id: session.id.clone(),
        kind: "report".to_string(),
        metadata_json: "{\"source\":\"test\"}".to_string(),
        path: PathBuf::from("artifacts/artifact-recreated.bin"),
        bytes: b"payload".to_vec(),
        created_at: 2,
    };
    store.put_artifact(&artifact).expect("store artifact");

    assert!(scaffold.stores.artifacts_dir.is_dir());
    assert_eq!(
        store
            .get_artifact("artifact-recreated")
            .expect("get artifact"),
        Some(artifact)
    );
}

#[test]
fn open_restores_matching_backups_before_pruning_corrupt_payloads() {
    let temp = tempfile::tempdir().expect("tempdir");
    let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..crate::AppConfig::default()
    });
    let store = super::PersistenceStore::open(&scaffold).expect("open store");

    let session = SessionRecord {
        id: "session-1".to_string(),
        title: "Store payloads".to_string(),
        prompt_override: None,
        settings_json: "{\"model\":\"gpt-5.4\"}".to_string(),
        agent_profile_id: "default".to_string(),
        active_mission_id: None,
        parent_session_id: None,
        parent_job_id: None,
        delegation_label: None,
        created_at: 1,
        updated_at: 1,
    };
    store.put_session(&session).expect("store session");

    let transcript = TranscriptRecord {
        id: "transcript-1".to_string(),
        session_id: session.id.clone(),
        run_id: None,
        kind: "user".to_string(),
        content: "original transcript".to_string(),
        created_at: 1,
    };
    store.put_transcript(&transcript).expect("store transcript");

    let artifact = ArtifactRecord {
        id: "artifact-1".to_string(),
        session_id: session.id.clone(),
        kind: "report".to_string(),
        metadata_json: "{\"source\":\"test\"}".to_string(),
        path: PathBuf::from("artifacts/artifact-1.bin"),
        bytes: b"original artifact".to_vec(),
        created_at: 1,
    };
    store.put_artifact(&artifact).expect("store artifact");
    drop(store);

    let transcript_path = scaffold.stores.transcripts_dir.join("transcript-1.txt");
    let transcript_backup = scaffold.stores.transcripts_dir.join("transcript-1.txt.bak");
    fs::rename(&transcript_path, &transcript_backup).expect("backup transcript");
    fs::write(&transcript_path, "tampered transcript").expect("write bad transcript");

    let artifact_path = scaffold.stores.artifacts_dir.join("artifact-1.bin");
    let artifact_backup = scaffold.stores.artifacts_dir.join("artifact-1.bin.bak");
    fs::rename(&artifact_path, &artifact_backup).expect("backup artifact");
    fs::write(&artifact_path, b"tampered artifact").expect("write bad artifact");

    let reopened = super::PersistenceStore::open(&scaffold).expect("reopen store");

    assert_eq!(
        reopened
            .get_transcript(&transcript.id)
            .expect("get restored transcript"),
        Some(transcript)
    );
    assert_eq!(
        reopened
            .get_artifact(&artifact.id)
            .expect("get restored artifact"),
        Some(artifact)
    );
    assert!(!transcript_backup.exists());
    assert!(!artifact_backup.exists());
}

#[test]
fn inbox_events_round_trip_and_session_queries() {
    let temp = tempfile::tempdir().expect("tempdir");
    let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..crate::AppConfig::default()
    });
    let store = super::PersistenceStore::open(&scaffold).expect("open store");

    let session = SessionRecord {
        id: "session-inbox".to_string(),
        title: "Inbox Session".to_string(),
        prompt_override: None,
        settings_json: "{\"model\":\"gpt-5.4\"}".to_string(),
        agent_profile_id: "default".to_string(),
        active_mission_id: None,
        parent_session_id: None,
        parent_job_id: None,
        delegation_label: None,
        created_at: 1,
        updated_at: 1,
    };
    store.put_session(&session).expect("put session");

    let queued = crate::SessionInboxEventRecord::try_from(
        &agent_runtime::inbox::SessionInboxEvent::job_completed(
            "inbox-job-completed",
            "session-inbox",
            None,
            "Background job finished",
            10,
        ),
    )
    .expect("queued record");
    let processed = crate::SessionInboxEventRecord::try_from(
        &agent_runtime::inbox::SessionInboxEvent::job_failed(
            "inbox-job-failed",
            "session-inbox",
            None,
            "Background job failed",
            11,
        )
        .mark_processed(12),
    )
    .expect("processed record");

    store.put_session_inbox_event(&queued).expect("put queued");
    store
        .put_session_inbox_event(&processed)
        .expect("put processed");

    let restored = agent_runtime::inbox::SessionInboxEvent::try_from(
        store
            .get_session_inbox_event("inbox-job-completed")
            .expect("get queued")
            .expect("queued exists"),
    )
    .expect("restore queued");
    assert_eq!(restored.session_id, "session-inbox");
    assert_eq!(
        restored.status,
        agent_runtime::inbox::SessionInboxEventStatus::Queued
    );

    let queued_events = store
        .list_queued_session_inbox_events_for_session("session-inbox")
        .expect("list queued");
    assert_eq!(queued_events.len(), 1);
    assert_eq!(queued_events[0].id, "inbox-job-completed");
}
