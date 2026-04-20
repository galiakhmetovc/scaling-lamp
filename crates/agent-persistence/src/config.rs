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
const DEFAULT_DAEMON_BIND_HOST: &str = "127.0.0.1";
const DEFAULT_DAEMON_BIND_PORT: u16 = 5140;
const DEFAULT_DAEMON_SKILLS_DIR: &str = "skills";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub data_dir: PathBuf,
    pub daemon: DaemonConfig,
    pub permissions: PermissionConfig,
    pub provider: ConfiguredProvider,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct DaemonConfig {
    pub bind_host: String,
    pub bind_port: u16,
    pub bearer_token: Option<String>,
    pub skills_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigEnv {
    pub config_path: Option<PathBuf>,
    pub data_dir_override: Option<PathBuf>,
    pub daemon_bearer_token_override: Option<String>,
    pub daemon_bind_host_override: Option<String>,
    pub daemon_bind_port_override: Option<u16>,
    pub daemon_skills_dir_override: Option<PathBuf>,
    pub home_dir: Option<PathBuf>,
    pub provider_api_base_override: Option<String>,
    pub provider_api_key_override: Option<String>,
    pub provider_connect_timeout_override: Option<u64>,
    pub provider_kind_override: Option<String>,
    pub provider_max_output_tokens_override: Option<u32>,
    pub provider_model_override: Option<String>,
    pub provider_request_timeout_override: Option<u64>,
    pub provider_stream_idle_timeout_override: Option<u64>,
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
    InvalidProviderValue {
        name: &'static str,
        value: String,
        reason: &'static str,
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
    daemon: Option<DaemonConfig>,
    permissions: Option<PermissionConfig>,
    provider: Option<ConfiguredProvider>,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            bind_host: DEFAULT_DAEMON_BIND_HOST.to_string(),
            bind_port: DEFAULT_DAEMON_BIND_PORT,
            bearer_token: None,
            skills_dir: PathBuf::from(DEFAULT_DAEMON_SKILLS_DIR),
        }
    }
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
            Self::InvalidProviderValue {
                name,
                value,
                reason,
            } => {
                write!(
                    formatter,
                    "invalid provider setting {name}={value}: {reason}"
                )
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
            | Self::InvalidProviderValue { .. }
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
            daemon_bearer_token_override: read_string_var("TEAMD_DAEMON_BEARER_TOKEN", &dotenv),
            daemon_bind_host_override: read_string_var("TEAMD_DAEMON_BIND_HOST", &dotenv),
            daemon_bind_port_override: read_u16_var("TEAMD_DAEMON_BIND_PORT", &dotenv)?,
            daemon_skills_dir_override: read_path_var("TEAMD_DAEMON_SKILLS_DIR", &dotenv)?,
            home_dir: read_path_var("HOME", &dotenv)?,
            provider_api_base_override: read_string_var("TEAMD_PROVIDER_API_BASE", &dotenv),
            provider_api_key_override: read_string_var("TEAMD_PROVIDER_API_KEY", &dotenv),
            provider_connect_timeout_override: read_u64_var(
                "TEAMD_PROVIDER_CONNECT_TIMEOUT_SECONDS",
                &dotenv,
            )?,
            provider_kind_override: read_string_var("TEAMD_PROVIDER_KIND", &dotenv),
            provider_max_output_tokens_override: read_u32_var(
                "TEAMD_PROVIDER_MAX_OUTPUT_TOKENS",
                &dotenv,
            )?,
            provider_model_override: read_string_var("TEAMD_PROVIDER_MODEL", &dotenv),
            provider_request_timeout_override: read_u64_var(
                "TEAMD_PROVIDER_REQUEST_TIMEOUT_SECONDS",
                &dotenv,
            )?,
            provider_stream_idle_timeout_override: read_u64_var(
                "TEAMD_PROVIDER_STREAM_IDLE_TIMEOUT_SECONDS",
                &dotenv,
            )?,
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
        let mut daemon = file_config
            .as_ref()
            .and_then(|config| config.daemon.clone())
            .unwrap_or_default();
        let mut provider = file_config
            .as_ref()
            .and_then(|config| config.provider.clone())
            .unwrap_or_default();
        let mut permissions = file_config
            .as_ref()
            .and_then(|config| config.permissions.clone())
            .unwrap_or_default();
        if let Some(bind_host) = env.daemon_bind_host_override.as_deref() {
            daemon.bind_host = bind_host.to_string();
        }
        if let Some(bind_port) = env.daemon_bind_port_override {
            daemon.bind_port = bind_port;
        }
        if let Some(bearer_token) = &env.daemon_bearer_token_override {
            daemon.bearer_token = Some(bearer_token.clone());
        }
        if let Some(skills_dir) = &env.daemon_skills_dir_override {
            daemon.skills_dir = skills_dir.clone();
        }
        if let Some(kind) = env.provider_kind_override.as_deref() {
            provider.kind = parse_provider_kind(kind)?;
        }
        if let Some(api_base) = &env.provider_api_base_override {
            provider.api_base = Some(api_base.clone());
        }
        if let Some(api_key) = &env.provider_api_key_override {
            provider.api_key = Some(api_key.clone());
        }
        if let Some(seconds) = env.provider_connect_timeout_override {
            provider.connect_timeout_seconds = Some(seconds);
        }
        if let Some(default_model) = &env.provider_model_override {
            provider.default_model = Some(default_model.clone());
        }
        if let Some(tokens) = env.provider_max_output_tokens_override {
            provider.max_output_tokens = Some(tokens);
        }
        if let Some(seconds) = env.provider_request_timeout_override {
            provider.request_timeout_seconds = Some(seconds);
        }
        if let Some(seconds) = env.provider_stream_idle_timeout_override {
            provider.stream_idle_timeout_seconds = Some(seconds);
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
            daemon,
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

        if self.daemon.bind_host.trim().is_empty() {
            return Err(ConfigError::InvalidProviderValue {
                name: "daemon.bind_host",
                value: self.daemon.bind_host.clone(),
                reason: "must not be empty",
            });
        }

        if self.daemon.bind_port == 0 {
            return Err(ConfigError::InvalidProviderValue {
                name: "daemon.bind_port",
                value: self.daemon.bind_port.to_string(),
                reason: "must be greater than zero",
            });
        }

        if self.daemon.skills_dir.as_os_str().is_empty() {
            return Err(ConfigError::InvalidProviderValue {
                name: "daemon.skills_dir",
                value: self.daemon.skills_dir.display().to_string(),
                reason: "must not be empty",
            });
        }

        if self.daemon.skills_dir.exists() && !self.daemon.skills_dir.is_dir() {
            return Err(ConfigError::InvalidProviderValue {
                name: "daemon.skills_dir",
                value: self.daemon.skills_dir.display().to_string(),
                reason: "must point to a directory",
            });
        }

        validate_positive_provider_value(
            "connect_timeout_seconds",
            self.provider.connect_timeout_seconds,
        )?;
        validate_positive_provider_value(
            "request_timeout_seconds",
            self.provider.request_timeout_seconds,
        )?;
        validate_positive_provider_value(
            "stream_idle_timeout_seconds",
            self.provider.stream_idle_timeout_seconds,
        )?;
        validate_positive_provider_value("max_output_tokens", self.provider.max_output_tokens)?;

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
                daemon: None,
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

fn read_u64_var(
    name: &'static str,
    dotenv: &BTreeMap<String, String>,
) -> Result<Option<u64>, ConfigError> {
    read_string_var(name, dotenv)
        .map(|value| parse_positive_numeric(name, &value))
        .transpose()
}

fn read_u32_var(
    name: &'static str,
    dotenv: &BTreeMap<String, String>,
) -> Result<Option<u32>, ConfigError> {
    read_string_var(name, dotenv)
        .map(|value| parse_positive_numeric(name, &value))
        .transpose()
}

fn read_u16_var(
    name: &'static str,
    dotenv: &BTreeMap<String, String>,
) -> Result<Option<u16>, ConfigError> {
    read_string_var(name, dotenv)
        .map(|value| parse_positive_numeric(name, &value))
        .transpose()
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

fn parse_positive_numeric<T>(name: &'static str, value: &str) -> Result<T, ConfigError>
where
    T: std::str::FromStr + PartialEq + Default,
{
    let parsed = value
        .parse::<T>()
        .map_err(|_| ConfigError::InvalidProviderValue {
            name,
            value: value.to_string(),
            reason: "must be a positive integer",
        })?;
    if parsed == T::default() {
        return Err(ConfigError::InvalidProviderValue {
            name,
            value: value.to_string(),
            reason: "must be greater than zero",
        });
    }
    Ok(parsed)
}

fn validate_positive_provider_value<T>(
    name: &'static str,
    value: Option<T>,
) -> Result<(), ConfigError>
where
    T: PartialEq + Default + ToString,
{
    if let Some(value) = value
        && value == T::default()
    {
        return Err(ConfigError::InvalidProviderValue {
            name,
            value: value.to_string(),
            reason: "must be greater than zero",
        });
    }

    Ok(())
}

fn load_dotenv_from_cwd() -> Result<BTreeMap<String, String>, ConfigError> {
    let cwd = env::current_dir().ok();
    let exe_dir = env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf));
    load_dotenv_from_locations(cwd.as_deref(), exe_dir.as_deref())
}

fn load_dotenv_from_locations(
    cwd: Option<&Path>,
    exe_dir: Option<&Path>,
) -> Result<BTreeMap<String, String>, ConfigError> {
    let mut candidates = Vec::new();
    if let Some(cwd) = cwd {
        candidates.push(cwd.join(".env"));
    }
    if let Some(exe_dir) = exe_dir {
        let exe_candidate = exe_dir.join(".env");
        if !candidates
            .iter()
            .any(|candidate| candidate == &exe_candidate)
        {
            candidates.push(exe_candidate);
        }
    }

    for path in candidates {
        if !path.exists() {
            continue;
        }

        let contents = fs::read_to_string(&path).map_err(|source| ConfigError::ReadConfig {
            path: path.clone(),
            source,
        })?;
        return Ok(parse_dotenv(&contents));
    }

    Ok(BTreeMap::new())
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
            daemon: DaemonConfig::default(),
            permissions: PermissionConfig::default(),
            provider: ConfiguredProvider::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AppConfig, ConfigEnv, ConfigError, DEFAULT_ZAI_API_BASE, DEFAULT_ZAI_MODEL,
        load_dotenv_from_locations, parse_dotenv,
    };
    use agent_runtime::permission::{PermissionAction, PermissionMode};
    use agent_runtime::provider::ProviderKind;
    use std::collections::BTreeMap;
    use std::ffi::OsString;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn base_env(root: &Path) -> ConfigEnv {
        ConfigEnv {
            config_path: None,
            data_dir_override: None,
            daemon_bearer_token_override: None,
            daemon_bind_host_override: None,
            daemon_bind_port_override: None,
            daemon_skills_dir_override: None,
            home_dir: Some(root.join("home")),
            provider_api_base_override: None,
            provider_api_key_override: None,
            provider_connect_timeout_override: None,
            provider_kind_override: None,
            provider_max_output_tokens_override: None,
            provider_model_override: None,
            provider_request_timeout_override: None,
            provider_stream_idle_timeout_override: None,
            permission_mode_override: None,
            temp_dir: root.join("tmp"),
            xdg_config_home: Some(root.join("xdg-config")),
            xdg_state_home: Some(root.join("xdg-state")),
        }
    }

    #[test]
    fn load_prefers_explicit_data_dir_override() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config_path = temp.path().join("teamd.toml");
        let override_dir = temp.path().join("override");

        fs::write(&config_path, "data_dir = \"/ignored/from/file\"\n").expect("write config");

        let mut env = base_env(temp.path());
        env.config_path = Some(config_path);
        env.data_dir_override = Some(override_dir.clone());

        let config = AppConfig::load_from_env(&env).expect("load config");

        assert_eq!(config.data_dir, override_dir);
    }

    #[test]
    fn load_uses_xdg_state_home_before_home() {
        let temp = tempfile::tempdir().expect("tempdir");
        let xdg_state_home = temp.path().join("xdg-state");
        let home_dir = temp.path().join("home");

        let mut env = base_env(temp.path());
        env.home_dir = Some(home_dir);
        env.xdg_config_home = None;
        env.xdg_state_home = Some(xdg_state_home.clone());

        let config = AppConfig::load_from_env(&env).expect("load config");

        assert_eq!(config.data_dir, xdg_state_home.join("teamd"));
    }

    #[test]
    fn validate_rejects_relative_data_dir() {
        let config = AppConfig {
            data_dir: PathBuf::from("relative/teamd"),
            daemon: Default::default(),
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
        let mut env = base_env(temp.path());
        env.config_path = Some(PathBuf::from("relative-config.toml"));

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

        let mut env = base_env(temp.path());
        env.config_path = Some(config_path);
        env.data_dir_override = Some(temp.path().join("override"));
        env.provider_api_key_override = Some("zai-secret".into());
        env.provider_model_override = Some("glm-5.1-air".into());
        env.permission_mode_override = Some("accept_edits".into());

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
        let mut env = base_env(temp.path());
        env.xdg_config_home = None;
        env.provider_api_key_override = Some("zai-key".to_string());
        env.provider_kind_override = Some("zai_chat_completions".to_string());

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
        assert_eq!(config.provider.connect_timeout_seconds, Some(15));
        assert_eq!(config.provider.request_timeout_seconds, None);
        assert_eq!(config.provider.stream_idle_timeout_seconds, Some(1200));
        assert_eq!(config.provider.max_output_tokens, None);
    }

    #[test]
    fn load_applies_provider_runtime_env_overrides() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut env = base_env(temp.path());
        env.xdg_config_home = None;
        env.provider_api_base_override = Some("https://api.z.ai/api/coding/paas/v4".to_string());
        env.provider_api_key_override = Some("zai-key".to_string());
        env.provider_kind_override = Some("zai_chat_completions".to_string());
        env.provider_model_override = Some("glm-5-air".to_string());
        env.provider_connect_timeout_override = Some(20);
        env.provider_request_timeout_override = Some(3600);
        env.provider_stream_idle_timeout_override = Some(1800);
        env.provider_max_output_tokens_override = Some(8192);

        let config = AppConfig::load_from_env(&env).expect("load config");

        assert_eq!(config.provider.connect_timeout_seconds, Some(20));
        assert_eq!(config.provider.request_timeout_seconds, Some(3600));
        assert_eq!(config.provider.stream_idle_timeout_seconds, Some(1800));
        assert_eq!(config.provider.max_output_tokens, Some(8192));
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

    #[test]
    fn dotenv_falls_back_to_executable_directory_when_cwd_has_no_file() {
        let temp = tempfile::tempdir().expect("tempdir");
        let exe_dir = temp.path().join("bin");
        fs::create_dir_all(&exe_dir).expect("create exe dir");
        fs::write(
            exe_dir.join(".env"),
            "TEAMD_PROVIDER_API_BASE=https://api.z.ai/api/coding/paas/v4\n",
        )
        .expect("write dotenv");

        let values = load_dotenv_from_locations(Some(temp.path()), Some(&exe_dir))
            .expect("load dotenv from fallback");

        assert_eq!(
            values.get("TEAMD_PROVIDER_API_BASE").map(String::as_str),
            Some("https://api.z.ai/api/coding/paas/v4")
        );
    }

    #[test]
    fn dotenv_prefers_current_working_directory_over_executable_directory() {
        let temp = tempfile::tempdir().expect("tempdir");
        let exe_dir = temp.path().join("bin");
        fs::create_dir_all(&exe_dir).expect("create exe dir");
        fs::write(
            temp.path().join(".env"),
            "TEAMD_PROVIDER_MODEL=glm-5-turbo\n",
        )
        .expect("write cwd dotenv");
        fs::write(exe_dir.join(".env"), "TEAMD_PROVIDER_MODEL=glm-5.1\n")
            .expect("write exe dotenv");

        let values = load_dotenv_from_locations(Some(temp.path()), Some(&exe_dir))
            .expect("load dotenv with cwd precedence");

        assert_eq!(
            values.get("TEAMD_PROVIDER_MODEL").map(String::as_str),
            Some("glm-5-turbo")
        );
    }

    #[test]
    fn load_uses_safe_local_daemon_defaults() {
        let temp = tempfile::tempdir().expect("tempdir");
        let env = base_env(temp.path());

        let config = AppConfig::load_from_env(&env).expect("load config");

        assert_eq!(config.daemon.bind_host, "127.0.0.1");
        assert_eq!(config.daemon.bind_port, 5140);
        assert_eq!(config.daemon.bearer_token, None);
        assert_eq!(config.daemon.skills_dir, PathBuf::from("skills"));
    }

    #[test]
    fn load_merges_daemon_settings_from_file_and_env() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config_path = temp.path().join("teamd.toml");
        fs::write(
            &config_path,
            r#"
[daemon]
bind_host = "0.0.0.0"
bind_port = 5140
bearer_token = "file-token"
skills_dir = "/srv/teamd/skills"
"#,
        )
        .expect("write config");

        let mut env = base_env(temp.path());
        env.config_path = Some(config_path);
        env.daemon_bind_host_override = Some("10.6.5.3".to_string());
        env.daemon_bind_port_override = Some(6140);
        env.daemon_bearer_token_override = Some("env-token".to_string());
        env.daemon_skills_dir_override = Some(temp.path().join("runtime-skills"));

        let config = AppConfig::load_from_env(&env).expect("load config");

        assert_eq!(config.daemon.bind_host, "10.6.5.3");
        assert_eq!(config.daemon.bind_port, 6140);
        assert_eq!(config.daemon.bearer_token.as_deref(), Some("env-token"));
        assert_eq!(config.daemon.skills_dir, temp.path().join("runtime-skills"));
    }
}
