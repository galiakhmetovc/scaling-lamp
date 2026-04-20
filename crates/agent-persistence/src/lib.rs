pub mod audit;
pub mod config;
pub mod records;
pub mod recovery;
pub mod repository;
pub mod store;

pub use config::{AppConfig, ConfigEnv, ConfigError, DaemonConfig};
pub use records::{
    ArtifactRecord, ContextOffloadRecord, ContextSummaryRecord, JobRecord, MissionRecord,
    PlanRecord, RecordConversionError, RunRecord, SessionRecord, TranscriptRecord,
};
pub use repository::{
    ArtifactRepository, ContextOffloadRepository, ContextSummaryRepository, JobRepository,
    MissionRepository, PlanRepository, RunRepository, SessionRepository, TranscriptRepository,
};
pub use store::{ExecutionStateSnapshot, PersistenceStore, StoreError, StoreLayout};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistenceScaffold {
    pub audit: audit::AuditLogConfig,
    pub config: config::AppConfig,
    pub recovery: recovery::RecoveryPolicy,
    pub stores: store::StoreLayout,
}

impl Default for PersistenceScaffold {
    fn default() -> Self {
        Self::from_config(config::AppConfig::default())
    }
}

impl PersistenceScaffold {
    pub fn from_config(config: config::AppConfig) -> Self {
        let stores = store::StoreLayout::from_config(&config);

        Self {
            audit: audit::AuditLogConfig::from_config(&config),
            config,
            recovery: recovery::RecoveryPolicy::default(),
            stores,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PersistenceScaffold;
    use std::ffi::OsStr;

    #[test]
    fn scaffold_derives_store_layout_from_data_dir() {
        let scaffold = PersistenceScaffold::default();

        assert!(scaffold.config.data_dir.is_absolute());
        assert_eq!(
            scaffold.config.data_dir.file_name(),
            Some(OsStr::new("teamd"))
        );
        assert!(scaffold.stores.metadata_db.ends_with("teamd/state.sqlite"));
        assert!(scaffold.stores.runs_dir.ends_with("teamd/runs"));
        assert!(scaffold.audit.path.ends_with("teamd/audit/runtime.jsonl"));
    }
}
