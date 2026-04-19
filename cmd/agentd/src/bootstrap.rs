use crate::cli;
use agent_persistence::{
    AppConfig, ConfigError, PersistenceScaffold, PersistenceStore, RecordConversionError,
    StoreError,
};
use agent_runtime::RuntimeScaffold;
use agent_runtime::run::RunTransitionError;
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTimeError;

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

pub fn build() -> Result<App, BootstrapError> {
    let config = AppConfig::load()?;
    build_from_config(config)
}

pub fn build_from_config(config: AppConfig) -> Result<App, BootstrapError> {
    config.validate()?;

    let persistence = PersistenceScaffold::from_config(config.clone());
    ensure_runtime_layout(&persistence)?;

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

#[cfg(test)]
mod tests {
    use super::build_from_config;
    use agent_persistence::{
        AppConfig, ConfigError, JobRecord, JobRepository, MissionRecord, MissionRepository,
        PersistenceStore, RunRecord, RunRepository, SessionRecord, SessionRepository,
    };
    use agent_runtime::mission::{JobSpec, MissionExecutionIntent, MissionSchedule, MissionStatus};
    use agent_runtime::run::{ApprovalRequest, DelegateRun, RunEngine, RunSnapshot, RunStatus};
    use agent_runtime::session::SessionSettings;
    use agent_runtime::verification::{CheckOutcome, EvidenceBundle};
    use std::fs;

    #[test]
    fn build_from_config_creates_runtime_layout_from_one_root() {
        let temp = tempfile::tempdir().expect("tempdir");
        let data_dir = temp.path().join("state-root");
        let config = AppConfig {
            data_dir: data_dir.clone(),
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
}
