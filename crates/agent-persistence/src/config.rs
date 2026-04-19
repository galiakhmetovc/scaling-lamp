use agent_runtime::permission::{PermissionConfig, PermissionMode};
use agent_runtime::provider::{ConfiguredProvider, ProviderKind};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_ZAI_API_BASE: &str = "https://api.z.ai/api/coding/paas/v4";
const DEFAULT_ZAI_MODEL: &str = "glm-5-turbo";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub data_dir: PathBuf,
    pub permissions: PermissionConfig,
    pub provider: ConfiguredProvider,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigEnv {
    pub config_path: Option<PathBuf>,
    pub data_dir_override: Option<PathBuf>,
    pub home_dir: Option<PathBuf>,
    pub provider_api_base_override: Option<String>,
    pub provider_api_key_override: Option<String>,
    pub provider_kind_override: Option<String>,
    pub provider_model_override: Option<String>,
    pub permission_mode_override: Option<String>,
    pub temp_dir: PathBuf,
    pub xdg_config_home: Option<PathBuf>,
    pub xdg_state_home: Option<PathBuf>,
}

#[derive(Debug)]
pub enum ConfigError {
    InvalidDataDir {
        path: PathBuf,
        reason: &'static str,
    },
    InvalidConfigPath {
        path: PathBuf,
        reason: &'static str,
    },
    InvalidProviderKind {
        value: String,
    },
    InvalidPermissionMode {
        value: String,
    },
    ParseConfig {
        path: PathBuf,
        source: toml::de::Error,
    },
    ReadConfig {
        path: PathBuf,
        source: std::io::Error,
    },
}

#[derive(Debug, Clone, Deserialize)]
struct FileConfig {
    data_dir: Option<PathBuf>,
    permissions: Option<PermissionConfig>,
    provider: Option<ConfiguredProvider>,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDataDir { path, reason } => {
                write!(formatter, "invalid data dir {}: {reason}", path.display())
            }
            Self::InvalidConfigPath { path, reason } => {
                write!(
                    formatter,
                    "invalid config path {}: {reason}",
                    path.display()
                )
            }
            Self::InvalidProviderKind { value } => {
                write!(formatter, "invalid provider kind {value}")
            }
            Self::InvalidPermissionMode { value } => {
                write!(formatter, "invalid permission mode {value}")
            }
            Self::ParseConfig { path, source } => {
                write!(
                    formatter,
                    "failed to parse config {}: {source}",
                    path.display()
                )
            }
            Self::ReadConfig { path, source } => {
                write!(
                    formatter,
                    "failed to read config {}: {source}",
                    path.display()
                )
            }
        }
    }
}

impl Error for ConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::ParseConfig { source, .. } => Some(source),
            Self::ReadConfig { source, .. } => Some(source),
            Self::InvalidConfigPath { .. }
            | Self::InvalidDataDir { .. }
            | Self::InvalidPermissionMode { .. }
            | Self::InvalidProviderKind { .. } => None,
        }
    }
}

impl ConfigEnv {
    pub fn capture() -> Result<Self, ConfigError> {
        let dotenv = load_dotenv_from_cwd()?;

        Ok(Self {
            config_path: read_path_var("TEAMD_CONFIG", &dotenv)?,
            data_dir_override: read_path_var("TEAMD_DATA_DIR", &dotenv)?,
            home_dir: read_path_var("HOME", &dotenv)?,
            provider_api_base_override: read_string_var("TEAMD_PROVIDER_API_BASE", &dotenv),
            provider_api_key_override: read_string_var("TEAMD_PROVIDER_API_KEY", &dotenv),
            provider_kind_override: read_string_var("TEAMD_PROVIDER_KIND", &dotenv),
            provider_model_override: read_string_var("TEAMD_PROVIDER_MODEL", &dotenv),
            permission_mode_override: read_string_var("TEAMD_PERMISSION_MODE", &dotenv),
            temp_dir: env::temp_dir(),
            xdg_config_home: read_path_var("XDG_CONFIG_HOME", &dotenv)?,
            xdg_state_home: read_path_var("XDG_STATE_HOME", &dotenv)?,
        })
    }

    fn default_config_path(&self) -> Option<PathBuf> {
        self.xdg_config_home
            .clone()
            .or_else(|| self.home_dir.clone().map(|home| home.join(".config")))
            .map(|root| root.join("teamd/config.toml"))
    }

    fn default_data_dir(&self) -> PathBuf {
        if let Some(state_home) = &self.xdg_state_home {
            return state_home.join("teamd");
        }

        if let Some(home) = &self.home_dir {
            return home.join(".local/state/teamd");
        }

        self.temp_dir.join("teamd")
    }
}

impl AppConfig {
    pub fn load() -> Result<Self, ConfigError> {
        let env = ConfigEnv::capture()?;
        Self::load_from_env(&env)
    }

    pub fn load_from_env(env: &ConfigEnv) -> Result<Self, ConfigError> {
        let file_config = match env.config_path.as_deref() {
            Some(path) => {
                validate_config_path(path)?;
                Some(load_file_config(path, true)?)
            }
            None => env
                .default_config_path()
                .filter(|path| path.exists())
                .map(|path| load_file_config(&path, false))
                .transpose()?,
        };

        let data_dir = env
            .data_dir_override
            .clone()
            .or_else(|| {
                file_config
                    .as_ref()
                    .and_then(|config| config.data_dir.clone())
            })
            .unwrap_or_else(|| env.default_data_dir());
        let mut provider = file_config
            .as_ref()
            .and_then(|config| config.provider.clone())
            .unwrap_or_default();
        let mut permissions = file_config
            .as_ref()
            .and_then(|config| config.permissions.clone())
            .unwrap_or_default();
        if let Some(kind) = env.provider_kind_override.as_deref() {
            provider.kind = parse_provider_kind(kind)?;
        }
        if let Some(api_base) = &env.provider_api_base_override {
            provider.api_base = Some(api_base.clone());
        }
        if let Some(api_key) = &env.provider_api_key_override {
            provider.api_key = Some(api_key.clone());
        }
        if let Some(default_model) = &env.provider_model_override {
            provider.default_model = Some(default_model.clone());
        }
        if let Some(mode) = env.permission_mode_override.as_deref() {
            permissions.mode = parse_permission_mode(mode)?;
        }
        if provider.kind == ProviderKind::ZaiChatCompletions && provider.api_base.is_none() {
            provider.api_base = Some(DEFAULT_ZAI_API_BASE.to_string());
        }
        if provider.kind == ProviderKind::ZaiChatCompletions && provider.default_model.is_none() {
            provider.default_model = Some(DEFAULT_ZAI_MODEL.to_string());
        }

        let config = Self {
            data_dir,
            permissions,
            provider,
        };
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.data_dir.as_os_str().is_empty() {
            return Err(ConfigError::InvalidDataDir {
                path: self.data_dir.clone(),
                reason: "must not be empty",
            });
        }

        if !self.data_dir.is_absolute() {
            return Err(ConfigError::InvalidDataDir {
                path: self.data_dir.clone(),
                reason: "must be absolute",
            });
        }

        if self.data_dir.exists() && !self.data_dir.is_dir() {
            return Err(ConfigError::InvalidDataDir {
                path: self.data_dir.clone(),
                reason: "must point to a directory",
            });
        }

        Ok(())
    }
}

fn default_data_dir() -> PathBuf {
    match ConfigEnv::capture() {
        Ok(env) => env.default_data_dir(),
        Err(_) => env::temp_dir().join("teamd"),
    }
}

fn load_file_config(path: &Path, required: bool) -> Result<FileConfig, ConfigError> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(source) if !required && source.kind() == std::io::ErrorKind::NotFound => {
            return Ok(FileConfig {
                data_dir: None,
                permissions: None,
                provider: None,
            });
        }
        Err(source) => {
            return Err(ConfigError::ReadConfig {
                path: path.to_path_buf(),
                source,
            });
        }
    };

    toml::from_str(&contents).map_err(|source| ConfigError::ParseConfig {
        path: path.to_path_buf(),
        source,
    })
}

fn read_path_var(
    name: &'static str,
    dotenv: &BTreeMap<String, String>,
) -> Result<Option<PathBuf>, ConfigError> {
    path_from_env_value(
        name,
        env::var_os(name).or_else(|| dotenv.get(name).map(std::ffi::OsString::from)),
    )
}

fn read_string_var(name: &'static str, dotenv: &BTreeMap<String, String>) -> Option<String> {
    env::var(name)
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| dotenv.get(name).cloned())
}

fn path_from_env_value(
    _name: &'static str,
    value: Option<std::ffi::OsString>,
) -> Result<Option<PathBuf>, ConfigError> {
    Ok(match value {
        Some(value) if value.is_empty() => None,
        Some(value) => Some(PathBuf::from(value)),
        None => None,
    })
}

fn validate_config_path(path: &Path) -> Result<(), ConfigError> {
    if !path.is_absolute() {
        return Err(ConfigError::InvalidConfigPath {
            path: path.to_path_buf(),
            reason: "must be absolute",
        });
    }

    if path.exists() && path.is_dir() {
        return Err(ConfigError::InvalidConfigPath {
            path: path.to_path_buf(),
            reason: "must point to a config file",
        });
    }

    Ok(())
}

fn parse_provider_kind(value: &str) -> Result<ProviderKind, ConfigError> {
    ProviderKind::try_from(value).map_err(|_| ConfigError::InvalidProviderKind {
        value: value.to_string(),
    })
}

fn parse_permission_mode(value: &str) -> Result<PermissionMode, ConfigError> {
    PermissionMode::try_from(value).map_err(|_| ConfigError::InvalidPermissionMode {
        value: value.to_string(),
    })
}

fn load_dotenv_from_cwd() -> Result<BTreeMap<String, String>, ConfigError> {
    let Some(path) = env::current_dir().ok().map(|cwd| cwd.join(".env")) else {
        return Ok(BTreeMap::new());
    };

    if !path.exists() {
        return Ok(BTreeMap::new());
    }

    let contents = fs::read_to_string(&path).map_err(|source| ConfigError::ReadConfig {
        path: path.clone(),
        source,
    })?;

    Ok(parse_dotenv(&contents))
}

fn parse_dotenv(contents: &str) -> BTreeMap<String, String> {
    let mut values = BTreeMap::new();

    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }

        let value = value
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .to_string();
        if value.is_empty() {
            continue;
        }

        values.insert(key.to_string(), value);
    }

    values
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            data_dir: default_data_dir(),
            permissions: PermissionConfig::default(),
            provider: ConfiguredProvider::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AppConfig, ConfigEnv, ConfigError, DEFAULT_ZAI_API_BASE, DEFAULT_ZAI_MODEL, parse_dotenv,
    };
    use agent_runtime::permission::{PermissionAction, PermissionMode};
    use agent_runtime::provider::ProviderKind;
    use std::collections::BTreeMap;
    use std::ffi::OsString;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn load_prefers_explicit_data_dir_override() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config_path = temp.path().join("teamd.toml");
        let override_dir = temp.path().join("override");

        fs::write(&config_path, "data_dir = \"/ignored/from/file\"\n").expect("write config");

        let env = ConfigEnv {
            config_path: Some(config_path),
            data_dir_override: Some(override_dir.clone()),
            home_dir: Some(temp.path().join("home")),
            provider_api_base_override: None,
            provider_api_key_override: None,
            provider_kind_override: None,
            provider_model_override: None,
            permission_mode_override: None,
            temp_dir: temp.path().join("tmp"),
            xdg_config_home: Some(temp.path().join("xdg-config")),
            xdg_state_home: Some(temp.path().join("xdg-state")),
        };

        let config = AppConfig::load_from_env(&env).expect("load config");

        assert_eq!(config.data_dir, override_dir);
    }

    #[test]
    fn load_uses_xdg_state_home_before_home() {
        let temp = tempfile::tempdir().expect("tempdir");
        let xdg_state_home = temp.path().join("xdg-state");
        let home_dir = temp.path().join("home");

        let env = ConfigEnv {
            config_path: None,
            data_dir_override: None,
            home_dir: Some(home_dir),
            provider_api_base_override: None,
            provider_api_key_override: None,
            provider_kind_override: None,
            provider_model_override: None,
            permission_mode_override: None,
            temp_dir: temp.path().join("tmp"),
            xdg_config_home: None,
            xdg_state_home: Some(xdg_state_home.clone()),
        };

        let config = AppConfig::load_from_env(&env).expect("load config");

        assert_eq!(config.data_dir, xdg_state_home.join("teamd"));
    }

    #[test]
    fn validate_rejects_relative_data_dir() {
        let config = AppConfig {
            data_dir: PathBuf::from("relative/teamd"),
            permissions: Default::default(),
            provider: Default::default(),
        };

        let error = config.validate().expect_err("relative path must fail");

        assert!(matches!(error, ConfigError::InvalidDataDir { .. }));
    }

    #[test]
    fn empty_env_bindings_are_treated_as_unset() {
        let value = super::path_from_env_value("TEAMD_DATA_DIR", Some(OsString::new()))
            .expect("empty bindings should be ignored");

        assert_eq!(value, None);
    }

    #[test]
    fn load_rejects_relative_config_override_paths() {
        let temp = tempfile::tempdir().expect("tempdir");
        let env = ConfigEnv {
            config_path: Some(PathBuf::from("relative-config.toml")),
            data_dir_override: None,
            home_dir: Some(temp.path().join("home")),
            provider_api_base_override: None,
            provider_api_key_override: None,
            provider_kind_override: None,
            provider_model_override: None,
            permission_mode_override: None,
            temp_dir: temp.path().join("tmp"),
            xdg_config_home: Some(temp.path().join("xdg-config")),
            xdg_state_home: Some(temp.path().join("xdg-state")),
        };

        let error = AppConfig::load_from_env(&env).expect_err("relative config path must fail");

        assert!(matches!(error, ConfigError::InvalidConfigPath { .. }));
    }

    #[test]
    fn load_merges_provider_settings_from_file_and_env() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config_path = temp.path().join("teamd.toml");

        fs::write(
            &config_path,
            r#"
data_dir = "/tmp/teamd-config"

[provider]
kind = "zai_chat_completions"
api_base = "https://api.z.ai/api/paas/v4"
default_model = "glm-5.1"

[permissions]
mode = "plan"

[[permissions.rules]]
action = "allow"
tool = "fs_write"
path_prefix = "notes/"
"#,
        )
        .expect("write config");

        let env = ConfigEnv {
            config_path: Some(config_path),
            data_dir_override: Some(temp.path().join("override")),
            home_dir: Some(temp.path().join("home")),
            provider_api_base_override: None,
            provider_api_key_override: Some("zai-secret".into()),
            provider_kind_override: None,
            provider_model_override: Some("glm-5.1-air".into()),
            permission_mode_override: Some("accept_edits".into()),
            temp_dir: temp.path().join("tmp"),
            xdg_config_home: Some(temp.path().join("xdg-config")),
            xdg_state_home: Some(temp.path().join("xdg-state")),
        };

        let config = AppConfig::load_from_env(&env).expect("load config");

        assert_eq!(config.provider.kind, ProviderKind::ZaiChatCompletions);
        assert_eq!(
            config.provider.api_base.as_deref(),
            Some("https://api.z.ai/api/paas/v4")
        );
        assert_eq!(config.provider.api_key.as_deref(), Some("zai-secret"));
        assert_eq!(
            config.provider.default_model.as_deref(),
            Some("glm-5.1-air")
        );
        assert_eq!(config.permissions.mode, PermissionMode::AcceptEdits);
        assert_eq!(config.permissions.rules.len(), 1);
        assert_eq!(config.permissions.rules[0].action, PermissionAction::Allow);
    }

    #[test]
    fn load_uses_zai_defaults_when_provider_kind_is_selected() {
        let temp = tempfile::tempdir().expect("tempdir");
        let env = ConfigEnv {
            config_path: None,
            data_dir_override: None,
            home_dir: Some(temp.path().join("home")),
            provider_api_base_override: None,
            provider_api_key_override: Some("zai-key".to_string()),
            provider_kind_override: Some("zai_chat_completions".to_string()),
            provider_model_override: None,
            permission_mode_override: None,
            temp_dir: temp.path().join("tmp"),
            xdg_config_home: None,
            xdg_state_home: Some(temp.path().join("xdg-state")),
        };

        let config = AppConfig::load_from_env(&env).expect("load config");

        assert_eq!(config.provider.kind, ProviderKind::ZaiChatCompletions);
        assert_eq!(
            config.provider.api_base.as_deref(),
            Some(DEFAULT_ZAI_API_BASE)
        );
        assert_eq!(config.provider.api_key.as_deref(), Some("zai-key"));
        assert_eq!(
            config.provider.default_model.as_deref(),
            Some(DEFAULT_ZAI_MODEL)
        );
    }

    #[test]
    fn parse_dotenv_ignores_comments_and_trims_quotes() {
        let values = parse_dotenv(
            r#"
# comment
TEAMD_PROVIDER_KIND="zai_chat_completions"
TEAMD_PROVIDER_MODEL='glm-5-turbo'
INVALID_LINE
TEAMD_PROVIDER_API_KEY=secret-key
"#,
        );

        assert_eq!(
            values.get("TEAMD_PROVIDER_KIND").map(String::as_str),
            Some("zai_chat_completions")
        );
        assert_eq!(
            values.get("TEAMD_PROVIDER_MODEL").map(String::as_str),
            Some("glm-5-turbo")
        );
        assert_eq!(
            values.get("TEAMD_PROVIDER_API_KEY").map(String::as_str),
            Some("secret-key")
        );
    }

    #[test]
    fn dotenv_values_fill_missing_provider_env_bindings() {
        let mut dotenv = BTreeMap::new();
        dotenv.insert(
            "TEAMD_PROVIDER_API_BASE".to_string(),
            "https://api.z.ai/api/coding/paas/v4".to_string(),
        );

        let value = super::read_string_var("TEAMD_PROVIDER_API_BASE", &dotenv);

        assert_eq!(
            value.as_deref(),
            Some("https://api.z.ai/api/coding/paas/v4")
        );
    }
}
