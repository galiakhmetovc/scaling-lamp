use agent_runtime::mcp::McpConnectorTransport;
use agent_runtime::permission::{PermissionConfig, PermissionMode};
use agent_runtime::provider::{ConfiguredProvider, ProviderKind};
use agent_runtime::{context::CompactionPolicy, session::SessionSettings};
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
    pub session_defaults: SessionDefaultsConfig,
    pub context: ContextConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct DaemonConfig {
    pub bind_host: String,
    pub bind_port: u16,
    pub bearer_token: Option<String>,
    pub skills_dir: PathBuf,
    pub public_base_url: Option<String>,
    pub a2a_peers: BTreeMap<String, A2APeerConfig>,
    pub mcp_connectors: BTreeMap<String, McpConnectorSeedConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct SessionDefaultsConfig {
    pub working_memory_limit: usize,
    pub project_memory_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct ContextConfig {
    pub compaction_min_messages: usize,
    pub compaction_keep_tail_messages: usize,
    pub compaction_max_output_tokens: u32,
    pub compaction_max_summary_chars: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct A2APeerConfig {
    pub base_url: String,
    pub bearer_token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct McpConnectorSeedConfig {
    pub transport: McpConnectorTransport,
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub cwd: Option<PathBuf>,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigEnv {
    pub config_path: Option<PathBuf>,
    pub data_dir_override: Option<PathBuf>,
    pub daemon_bearer_token_override: Option<String>,
    pub daemon_bind_host_override: Option<String>,
    pub daemon_bind_port_override: Option<u16>,
    pub daemon_public_base_url_override: Option<String>,
    pub daemon_skills_dir_override: Option<PathBuf>,
    pub home_dir: Option<PathBuf>,
    pub context_compaction_keep_tail_messages_override: Option<usize>,
    pub context_compaction_max_output_tokens_override: Option<u32>,
    pub context_compaction_max_summary_chars_override: Option<usize>,
    pub context_compaction_min_messages_override: Option<usize>,
    pub provider_api_base_override: Option<String>,
    pub provider_api_key_override: Option<String>,
    pub provider_connect_timeout_override: Option<u64>,
    pub provider_kind_override: Option<String>,
    pub provider_max_tool_rounds_override: Option<u32>,
    pub provider_max_output_tokens_override: Option<u32>,
    pub provider_model_override: Option<String>,
    pub provider_request_timeout_override: Option<u64>,
    pub provider_stream_idle_timeout_override: Option<u64>,
    pub permission_mode_override: Option<String>,
    pub session_project_memory_enabled_override: Option<bool>,
    pub session_working_memory_limit_override: Option<usize>,
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
    session_defaults: Option<SessionDefaultsConfig>,
    context: Option<ContextConfig>,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            bind_host: DEFAULT_DAEMON_BIND_HOST.to_string(),
            bind_port: DEFAULT_DAEMON_BIND_PORT,
            bearer_token: None,
            skills_dir: PathBuf::from(DEFAULT_DAEMON_SKILLS_DIR),
            public_base_url: None,
            a2a_peers: BTreeMap::new(),
            mcp_connectors: BTreeMap::new(),
        }
    }
}

impl Default for McpConnectorSeedConfig {
    fn default() -> Self {
        Self {
            transport: McpConnectorTransport::Stdio,
            command: String::new(),
            args: Vec::new(),
            env: BTreeMap::new(),
            cwd: None,
            enabled: true,
        }
    }
}

impl Default for SessionDefaultsConfig {
    fn default() -> Self {
        let defaults = SessionSettings::default();
        Self {
            working_memory_limit: defaults.working_memory_limit,
            project_memory_enabled: defaults.project_memory_enabled,
        }
    }
}

impl Default for ContextConfig {
    fn default() -> Self {
        let defaults = CompactionPolicy::default();
        Self {
            compaction_min_messages: defaults.min_messages,
            compaction_keep_tail_messages: defaults.keep_tail_messages,
            compaction_max_output_tokens: defaults.max_output_tokens,
            compaction_max_summary_chars: defaults.max_summary_chars,
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
            daemon_public_base_url_override: read_string_var(
                "TEAMD_DAEMON_PUBLIC_BASE_URL",
                &dotenv,
            ),
            daemon_skills_dir_override: read_path_var("TEAMD_DAEMON_SKILLS_DIR", &dotenv)?,
            home_dir: read_path_var("HOME", &dotenv)?,
            context_compaction_keep_tail_messages_override: read_usize_var(
                "TEAMD_CONTEXT_COMPACTION_KEEP_TAIL_MESSAGES",
                &dotenv,
            )?,
            context_compaction_max_output_tokens_override: read_u32_var(
                "TEAMD_CONTEXT_COMPACTION_MAX_OUTPUT_TOKENS",
                &dotenv,
            )?,
            context_compaction_max_summary_chars_override: read_usize_var(
                "TEAMD_CONTEXT_COMPACTION_MAX_SUMMARY_CHARS",
                &dotenv,
            )?,
            context_compaction_min_messages_override: read_usize_var(
                "TEAMD_CONTEXT_COMPACTION_MIN_MESSAGES",
                &dotenv,
            )?,
            provider_api_base_override: read_string_var("TEAMD_PROVIDER_API_BASE", &dotenv),
            provider_api_key_override: read_string_var("TEAMD_PROVIDER_API_KEY", &dotenv),
            provider_connect_timeout_override: read_u64_var(
                "TEAMD_PROVIDER_CONNECT_TIMEOUT_SECONDS",
                &dotenv,
            )?,
            provider_kind_override: read_string_var("TEAMD_PROVIDER_KIND", &dotenv),
            provider_max_tool_rounds_override: read_u32_var(
                "TEAMD_PROVIDER_MAX_TOOL_ROUNDS",
                &dotenv,
            )?,
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
            session_project_memory_enabled_override: read_bool_var(
                "TEAMD_SESSION_PROJECT_MEMORY_ENABLED",
                &dotenv,
            )?,
            session_working_memory_limit_override: read_usize_var(
                "TEAMD_SESSION_WORKING_MEMORY_LIMIT",
                &dotenv,
            )?,
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
        let mut session_defaults = file_config
            .as_ref()
            .and_then(|config| config.session_defaults.clone())
            .unwrap_or_default();
        let mut context = file_config
            .as_ref()
            .and_then(|config| config.context.clone())
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
        if let Some(public_base_url) = &env.daemon_public_base_url_override {
            daemon.public_base_url = Some(public_base_url.clone());
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
        if let Some(rounds) = env.provider_max_tool_rounds_override {
            provider.max_tool_rounds = Some(rounds);
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
        if let Some(limit) = env.session_working_memory_limit_override {
            session_defaults.working_memory_limit = limit;
        }
        if let Some(enabled) = env.session_project_memory_enabled_override {
            session_defaults.project_memory_enabled = enabled;
        }
        if let Some(value) = env.context_compaction_min_messages_override {
            context.compaction_min_messages = value;
        }
        if let Some(value) = env.context_compaction_keep_tail_messages_override {
            context.compaction_keep_tail_messages = value;
        }
        if let Some(value) = env.context_compaction_max_output_tokens_override {
            context.compaction_max_output_tokens = value;
        }
        if let Some(value) = env.context_compaction_max_summary_chars_override {
            context.compaction_max_summary_chars = value;
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
            session_defaults,
            context,
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

        if let Some(public_base_url) = &self.daemon.public_base_url
            && public_base_url.trim().is_empty()
        {
            return Err(ConfigError::InvalidProviderValue {
                name: "daemon.public_base_url",
                value: public_base_url.clone(),
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

        for (peer_id, peer) in &self.daemon.a2a_peers {
            if peer.base_url.trim().is_empty() {
                return Err(ConfigError::InvalidProviderValue {
                    name: "daemon.a2a_peers.base_url",
                    value: peer_id.clone(),
                    reason: "must not be empty",
                });
            }
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
        validate_positive_provider_value("max_tool_rounds", self.provider.max_tool_rounds)?;
        validate_positive_provider_value("max_output_tokens", self.provider.max_output_tokens)?;
        validate_positive_usize_value(
            "session_defaults.working_memory_limit",
            self.session_defaults.working_memory_limit,
        )?;
        validate_positive_usize_value(
            "context.compaction_min_messages",
            self.context.compaction_min_messages,
        )?;
        validate_positive_usize_value(
            "context.compaction_keep_tail_messages",
            self.context.compaction_keep_tail_messages,
        )?;
        validate_positive_u32_value(
            "context.compaction_max_output_tokens",
            self.context.compaction_max_output_tokens,
        )?;
        validate_positive_usize_value(
            "context.compaction_max_summary_chars",
            self.context.compaction_max_summary_chars,
        )?;
        if self.context.compaction_keep_tail_messages > self.context.compaction_min_messages {
            return Err(ConfigError::InvalidProviderValue {
                name: "context.compaction_keep_tail_messages",
                value: self.context.compaction_keep_tail_messages.to_string(),
                reason: "must be less than or equal to compaction_min_messages",
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
                daemon: None,
                permissions: None,
                provider: None,
                session_defaults: None,
                context: None,
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

fn read_usize_var(
    name: &'static str,
    dotenv: &BTreeMap<String, String>,
) -> Result<Option<usize>, ConfigError> {
    read_string_var(name, dotenv)
        .map(|value| parse_positive_numeric(name, &value))
        .transpose()
}

fn read_bool_var(
    name: &'static str,
    dotenv: &BTreeMap<String, String>,
) -> Result<Option<bool>, ConfigError> {
    read_string_var(name, dotenv)
        .map(|value| parse_bool(name, &value))
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

fn parse_bool(name: &'static str, value: &str) -> Result<bool, ConfigError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(ConfigError::InvalidProviderValue {
            name,
            value: value.to_string(),
            reason: "must be a boolean",
        }),
    }
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

fn validate_positive_usize_value(name: &'static str, value: usize) -> Result<(), ConfigError> {
    if value == 0 {
        return Err(ConfigError::InvalidProviderValue {
            name,
            value: value.to_string(),
            reason: "must be greater than zero",
        });
    }
    Ok(())
}

fn validate_positive_u32_value(name: &'static str, value: u32) -> Result<(), ConfigError> {
    if value == 0 {
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
            session_defaults: SessionDefaultsConfig::default(),
            context: ContextConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests;
