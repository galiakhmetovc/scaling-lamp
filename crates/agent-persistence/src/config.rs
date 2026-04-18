use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub data_dir: PathBuf,
}

fn default_data_dir() -> PathBuf {
    if let Some(state_home) = std::env::var_os("XDG_STATE_HOME") {
        return PathBuf::from(state_home).join("teamd");
    }

    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".local/state/teamd");
    }

    std::env::temp_dir().join("teamd")
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            data_dir: default_data_dir(),
        }
    }
}
