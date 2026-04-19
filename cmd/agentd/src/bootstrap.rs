use crate::{cli, execution};
use agent_persistence::{
    AppConfig, ConfigError, PersistenceScaffold, PersistenceStore, RecordConversionError,
    StoreError, recovery,
};
use agent_runtime::RuntimeScaffold;
use agent_runtime::provider::{ProviderBuildError, ProviderDriver, ProviderError, build_driver};
use agent_runtime::run::RunTransitionError;
use agent_runtime::scheduler::MissionVerificationSummary;
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, SystemTimeError, UNIX_EPOCH};

#[derive(Debug)]
pub enum BootstrapError {
    Config(ConfigError),
    Clock(SystemTimeError),
    InvalidPath {
        path: PathBuf,
        reason: &'static str,
    },
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    MissingRecord {
        kind: &'static str,
        id: String,
    },
    ProviderBuild(ProviderBuildError),
    ProviderRequest(ProviderError),
    Execution(execution::ExecutionError),
    Recovery(recovery::RecoveryError),
    RecordConversion(RecordConversionError),
    RunTransition(RunTransitionError),
    Sqlite(rusqlite::Error),
    Store(StoreError),
    Usage {
        reason: String,
    },
}

#[derive(Debug)]
pub struct App {
    pub config: AppConfig,
    pub persistence: PersistenceScaffold,
    pub runtime: RuntimeScaffold,
}

impl App {
    pub fn run(&self) -> Result<(), BootstrapError> {
        let output = self.run_with_args(std::env::args().skip(1))?;
        println!("{output}");
        Ok(())
    }

    pub fn run_with_args<I, S>(&self, args: I) -> Result<String, BootstrapError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        cli::execute(self, args)
    }

    pub fn store(&self) -> Result<PersistenceStore, BootstrapError> {
        PersistenceStore::open(&self.persistence).map_err(BootstrapError::Store)
    }

    pub fn provider_driver(&self) -> Result<Box<dyn ProviderDriver>, BootstrapError> {
        build_driver(&self.config.provider).map_err(BootstrapError::ProviderBuild)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn supervisor_tick(
        &self,
        now: i64,
        verifications: &[MissionVerificationSummary],
    ) -> Result<execution::SupervisorTickReport, BootstrapError> {
        let store = self.store()?;
        execution::ExecutionService::default()
            .supervisor_tick(&store, now, verifications)
            .map_err(BootstrapError::Execution)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn execute_mission_turn_job(
        &self,
        job_id: &str,
        now: i64,
    ) -> Result<execution::MissionTurnExecutionReport, BootstrapError> {
        let store = self.store()?;
        let provider = self.provider_driver()?;
        execution::ExecutionService::default()
            .execute_mission_turn_job(&store, provider.as_ref(), job_id, now)
            .map_err(BootstrapError::Execution)
    }
}

impl fmt::Display for BootstrapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(source) => write!(formatter, "{source}"),
            Self::Clock(source) => write!(formatter, "system clock error: {source}"),
            Self::InvalidPath { path, reason } => {
                write!(
                    formatter,
                    "invalid bootstrap path {}: {reason}",
                    path.display()
                )
            }
            Self::Io { path, source } => {
                write!(
                    formatter,
                    "bootstrap filesystem error at {}: {source}",
                    path.display()
                )
            }
            Self::MissingRecord { kind, id } => write!(formatter, "{kind} {id} was not found"),
            Self::ProviderBuild(source) => write!(formatter, "{source}"),
            Self::ProviderRequest(source) => write!(formatter, "{source}"),
            Self::Execution(source) => write!(formatter, "{source}"),
            Self::Recovery(source) => write!(formatter, "{source}"),
            Self::RecordConversion(source) => {
                write!(formatter, "record conversion error: {source}")
            }
            Self::RunTransition(source) => write!(formatter, "{source}"),
            Self::Sqlite(source) => write!(formatter, "sqlite error: {source}"),
            Self::Store(source) => write!(formatter, "{source}"),
            Self::Usage { reason } => write!(formatter, "{reason}"),
        }
    }
}

impl Error for BootstrapError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Config(source) => Some(source),
            Self::Clock(source) => Some(source),
            Self::Io { source, .. } => Some(source),
            Self::ProviderBuild(source) => Some(source),
            Self::ProviderRequest(source) => Some(source),
            Self::Execution(source) => Some(source),
            Self::Recovery(source) => Some(source),
            Self::RecordConversion(source) => Some(source),
            Self::RunTransition(source) => Some(source),
            Self::Sqlite(source) => Some(source),
            Self::Store(source) => Some(source),
            Self::InvalidPath { .. } | Self::MissingRecord { .. } | Self::Usage { .. } => None,
        }
    }
}

impl From<ConfigError> for BootstrapError {
    fn from(source: ConfigError) -> Self {
        Self::Config(source)
    }
}

impl From<rusqlite::Error> for BootstrapError {
    fn from(source: rusqlite::Error) -> Self {
        Self::Sqlite(source)
    }
}

impl From<StoreError> for BootstrapError {
    fn from(source: StoreError) -> Self {
        Self::Store(source)
    }
}

impl From<ProviderBuildError> for BootstrapError {
    fn from(source: ProviderBuildError) -> Self {
        Self::ProviderBuild(source)
    }
}

impl From<ProviderError> for BootstrapError {
    fn from(source: ProviderError) -> Self {
        Self::ProviderRequest(source)
    }
}

impl From<execution::ExecutionError> for BootstrapError {
    fn from(source: execution::ExecutionError) -> Self {
        Self::Execution(source)
    }
}

impl From<recovery::RecoveryError> for BootstrapError {
    fn from(source: recovery::RecoveryError) -> Self {
        Self::Recovery(source)
    }
}

pub fn build() -> Result<App, BootstrapError> {
    let config = AppConfig::load()?;
    build_from_config(config)
}

pub fn build_from_config(config: AppConfig) -> Result<App, BootstrapError> {
    config.validate()?;

    let persistence = PersistenceScaffold::from_config(config.clone());
    ensure_runtime_layout(&persistence)?;
    reconcile_recovery_state(&persistence)?;

    Ok(App {
        config,
        persistence,
        runtime: RuntimeScaffold::default(),
    })
}

fn ensure_runtime_layout(persistence: &PersistenceScaffold) -> Result<(), BootstrapError> {
    let audit_dir = persistence
        .audit
        .path
        .parent()
        .ok_or_else(|| BootstrapError::InvalidPath {
            path: persistence.audit.path.clone(),
            reason: "must have a parent directory",
        })?;

    ensure_directory_target(&persistence.config.data_dir)?;
    ensure_directory_target(&persistence.stores.artifacts_dir)?;
    ensure_directory_target(&persistence.stores.runs_dir)?;
    ensure_directory_target(&persistence.stores.transcripts_dir)?;
    ensure_directory_target(audit_dir)?;

    ensure_file_target(&persistence.stores.metadata_db)?;
    ensure_file_target(&persistence.audit.path)?;

    create_directory(&persistence.config.data_dir)?;
    create_directory(&persistence.stores.artifacts_dir)?;
    create_directory(&persistence.stores.runs_dir)?;
    create_directory(&persistence.stores.transcripts_dir)?;
    create_directory(audit_dir)?;

    Ok(())
}

fn ensure_directory_target(path: &Path) -> Result<(), BootstrapError> {
    if path.exists() && !path.is_dir() {
        return Err(BootstrapError::InvalidPath {
            path: path.to_path_buf(),
            reason: "must point to a directory",
        });
    }

    Ok(())
}

fn ensure_file_target(path: &Path) -> Result<(), BootstrapError> {
    if path.exists() && path.is_dir() {
        return Err(BootstrapError::InvalidPath {
            path: path.to_path_buf(),
            reason: "must point to a file path",
        });
    }

    Ok(())
}

fn create_directory(path: &Path) -> Result<(), BootstrapError> {
    fs::create_dir_all(path).map_err(|source| BootstrapError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn reconcile_recovery_state(persistence: &PersistenceScaffold) -> Result<(), BootstrapError> {
    let store = PersistenceStore::open(persistence)?;
    recovery::reconcile_runs(&store, persistence.recovery, unix_timestamp()?)?;
    Ok(())
}

fn unix_timestamp() -> Result<i64, BootstrapError> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(BootstrapError::Clock)?
        .as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use super::build_from_config;
    use agent_persistence::{
        AppConfig, ConfigError, JobRecord, JobRepository, MissionRecord, MissionRepository,
        PersistenceStore, RunRecord, RunRepository, SessionRecord, SessionRepository,
        TranscriptRepository,
    };
    use agent_runtime::mission::{JobSpec, MissionExecutionIntent, MissionSchedule, MissionStatus};
    use agent_runtime::provider::{ConfiguredProvider, ProviderKind};
    use agent_runtime::run::{ApprovalRequest, DelegateRun, RunEngine, RunSnapshot, RunStatus};
    use agent_runtime::scheduler::{MissionVerificationSummary, SupervisorAction};
    use agent_runtime::session::SessionSettings;
    use agent_runtime::verification::VerificationStatus;
    use agent_runtime::verification::{CheckOutcome, EvidenceBundle};
    use std::fs;
    use std::io::{BufRead, BufReader, Read, Write};
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
            super::BootstrapError::Config(ConfigError::InvalidDataDir { .. })
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

        let mut approval_run =
            RunEngine::new("run-approval", "session-ops", Some("mission-ops"), 3);
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

        let mut delegate_run =
            RunEngine::new("run-delegate", "session-ops", Some("mission-ops"), 7);
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
                evidence_refs_json: "[]".to_string(),
                pending_approvals_json: "[]".to_string(),
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
                evidence_refs_json: "[]".to_string(),
                pending_approvals_json: "[]".to_string(),
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
                evidence_refs_json: "[]".to_string(),
                pending_approvals_json: "[]".to_string(),
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
                evidence_refs_json: "[]".to_string(),
                pending_approvals_json: "[]".to_string(),
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
                evidence_refs_json: "[]".to_string(),
                pending_approvals_json: serde_json::to_string(&vec![ApprovalRequest::new(
                    "approval-1",
                    "tool-call-1",
                    "allow exec",
                    11,
                )])
                .expect("serialize approvals"),
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
                evidence_refs_json: "[]".to_string(),
                pending_approvals_json: "[]".to_string(),
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
                evidence_refs_json: "[]".to_string(),
                pending_approvals_json: "[]".to_string(),
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

    fn spawn_json_server(body: &'static str) -> (String, Receiver<String>, thread::JoinHandle<()>) {
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
        });

        (format!("http://{address}"), receiver, handle)
    }
}
