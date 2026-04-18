pub mod audit;
pub mod config;
pub mod recovery;
pub mod store;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistenceScaffold {
    pub audit: audit::AuditLogConfig,
    pub config: config::AppConfig,
    pub recovery: recovery::RecoveryPolicy,
    pub stores: store::StoreLayout,
}

impl Default for PersistenceScaffold {
    fn default() -> Self {
        let config = config::AppConfig::default();
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
        assert!(scaffold.audit.path.ends_with("teamd/audit/runtime.jsonl"));
    }
}
