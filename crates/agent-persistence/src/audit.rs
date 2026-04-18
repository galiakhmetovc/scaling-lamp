use crate::config::AppConfig;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditLogConfig {
    pub path: PathBuf,
}

impl AuditLogConfig {
    pub fn from_config(config: &AppConfig) -> Self {
        Self {
            path: config.data_dir.join("audit/runtime.jsonl"),
        }
    }
}
