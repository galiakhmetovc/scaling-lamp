use agent_persistence::{AppConfig, ConfigError, PersistenceScaffold};
use agent_runtime::RuntimeScaffold;
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum BootstrapError {
    Config(ConfigError),
    InvalidPath {
        path: PathBuf,
        reason: &'static str,
    },
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

#[derive(Debug)]
pub struct App {
    pub config: AppConfig,
    pub persistence: PersistenceScaffold,
    pub runtime: RuntimeScaffold,
}

impl App {
    pub fn run(&self) {
        println!(
            "agentd ready: data_dir={} state_db={} components={}",
            self.config.data_dir.display(),
            self.persistence.stores.metadata_db.display(),
            self.runtime.component_count()
        );
    }
}

impl fmt::Display for BootstrapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(source) => write!(formatter, "{source}"),
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
        }
    }
}

impl Error for BootstrapError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Config(source) => Some(source),
            Self::Io { source, .. } => Some(source),
            Self::InvalidPath { .. } => None,
        }
    }
}

impl From<ConfigError> for BootstrapError {
    fn from(source: ConfigError) -> Self {
        Self::Config(source)
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
    use agent_persistence::{AppConfig, ConfigError};
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
}
