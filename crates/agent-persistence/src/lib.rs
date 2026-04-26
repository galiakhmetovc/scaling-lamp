pub mod audit;
pub mod config;
pub mod records;
pub mod recovery;
pub mod repository;
pub mod store;

pub use config::{
    A2APeerConfig, AppConfig, ConfigEnv, ConfigError, DaemonConfig, McpConnectorSeedConfig,
    RuntimeLimitsConfig, RuntimeTimingConfig, TelegramConfig,
};
pub use records::{
    AgentChainContinuationRecord, AgentProfileRecord, AgentScheduleRecord, ArtifactRecord,
    ContextOffloadRecord, ContextSummaryRecord, JobRecord, KnowledgeSearchDocRecord,
    KnowledgeSourceRecord, McpConnectorRecord, MissionRecord, PlanRecord, RecordConversionError,
    RunRecord, SessionInboxEventRecord, SessionRecord, SessionRetentionRecord,
    SessionSearchDocRecord, TelegramChatBindingRecord, TelegramChatStatusRecord,
    TelegramUpdateCursorRecord, TelegramUserPairingRecord, ToolCallRecord, TranscriptRecord,
};
pub use repository::{
    AgentRepository, ArtifactRepository, ContextOffloadRepository, ContextSummaryRepository,
    JobRepository, KnowledgeRepository, McpRepository, MissionRepository, PlanRepository,
    RunRepository, RunSummaryRollup, SessionActiveJobCounts, SessionInboxRepository,
    SessionRepository, SessionRetentionRepository, SessionSearchRepository, TelegramRepository,
    ToolCallRepository, TranscriptRepository, TranscriptSessionStats,
};
pub use store::{ExecutionStateSnapshot, PersistenceStore, StoreError, StoreLayout};

#[derive(Debug, Clone, PartialEq)]
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
