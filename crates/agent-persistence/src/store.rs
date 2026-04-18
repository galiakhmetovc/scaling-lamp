use crate::config::AppConfig;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreLayout {
    pub artifacts_dir: PathBuf,
    pub metadata_db: PathBuf,
    pub runs_dir: PathBuf,
    pub transcripts_dir: PathBuf,
}

impl StoreLayout {
    pub fn from_config(config: &AppConfig) -> Self {
        let root = &config.data_dir;

        Self {
            artifacts_dir: root.join("artifacts"),
            metadata_db: root.join("state.sqlite"),
            runs_dir: root.join("runs"),
            transcripts_dir: root.join("transcripts"),
        }
    }
}
