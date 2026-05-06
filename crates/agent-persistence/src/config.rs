use agent_runtime::mcp::McpConnectorTransport;
use agent_runtime::permission::{PermissionConfig, PermissionMode};
use agent_runtime::provider::{ConfiguredProvider, ProviderKind};
use agent_runtime::tool::{
    BrowserToolConfig, KnowledgeRoot, KnowledgeSourceKind, ToolRuntimeLimits, WebSearchBackend,
};
use agent_runtime::{context::CompactionPolicy, session::SessionSettings};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

const DEFAULT_ZAI_API_BASE: &str = "https://api.z.ai/api/coding/paas/v4";
const DEFAULT_ZAI_MODEL: &str = "glm-5-turbo";
const DEFAULT_DAEMON_BIND_HOST: &str = "127.0.0.1";
const DEFAULT_DAEMON_BIND_PORT: u16 = 5140;
const DEFAULT_DAEMON_SKILLS_DIR: &str = "skills";
const DEFAULT_WEB_SEARCH_URL: &str = "https://duckduckgo.com/html/";
const DEFAULT_OTLP_ENDPOINT: &str = "http://127.0.0.1:4318/v1/traces";
const DEFAULT_BROWSER_COMMAND: &str = "agent-browser";
const DEFAULT_BROWSER_PROVIDER: &str = "browserless";
const DEFAULT_BROWSER_SESSION_PREFIX: &str = "teamd";
const DEFAULT_BROWSER_TIMEOUT_MS: u64 = 30_000;
const DEFAULT_BROWSER_MAX_OUTPUT_CHARS: usize = 20_000;
const DEFAULT_BROWSERLESS_API_URL: &str = "http://127.0.0.1:3000";
const DEFAULT_BROWSERLESS_BROWSER_TYPE: &str = "chromium";
const DEFAULT_BROWSERLESS_TTL_MS: u64 = 300_000;
const DEFAULT_MEM0_API_BASE: &str = "http://127.0.0.1:18888";
const DEFAULT_MEM0_DEFAULT_USER_ID: &str = "local-operator";
const DEFAULT_MEM0_REQUEST_TIMEOUT_MS: u64 = 120_000;
const DEFAULT_MEM0_DEFAULT_LIMIT: usize = 10;
const DEFAULT_MEM0_MAX_LIMIT: usize = 50;
const DEFAULT_MEMORY_CURATOR_MODE: &str = "auto";
const DEFAULT_MEMORY_CURATOR_MIN_CONFIDENCE: f64 = 0.8;
const DEFAULT_MEMORY_CURATOR_MAX_CANDIDATES: usize = 5;
const DEFAULT_MEMORY_CURATOR_MAX_OUTPUT_TOKENS: u32 = 512;
const DEFAULT_MEMORY_RECALL_MAX_RESULTS: usize = 6;
const DEFAULT_MEMORY_RECALL_MAX_QUERY_CHARS: usize = 512;
const DEFAULT_MEMORY_RECALL_MAX_MEMORY_CHARS: usize = 800;
const DEFAULT_OPERATOR_TIMEZONE: &str = "Europe/Moscow";
const DEFAULT_SILVERBULLET_SPACE_DIR: &str = "/var/lib/teamd/knowledge/silverbullet/teamd";
const DEFAULT_DATABASE_URL: &str = "postgresql://teamd@127.0.0.1:5432/teamd";
const DEFAULT_DATABASE_CONNECT_TIMEOUT_SECONDS: u64 = 5;
const DEFAULT_DATABASE_APPLICATION_NAME: &str = "teamd";

pub fn redacted_database_url(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_string();
    };
    let Some(at_index) = rest.find('@') else {
        return url.to_string();
    };
    format!("{scheme}://<redacted>@{}", &rest[at_index + 1..])
}

#[derive(Debug, Clone, PartialEq)]
pub struct AppConfig {
    pub data_dir: PathBuf,
    pub database: DatabaseConfig,
    pub daemon: DaemonConfig,
    pub telegram: TelegramConfig,
    pub permissions: PermissionConfig,
    pub provider: ConfiguredProvider,
    pub session_defaults: SessionDefaultsConfig,
    pub workspace: WorkspaceConfig,
    pub context: ContextConfig,
    pub web: WebConfig,
    pub browser: BrowserConfig,
    pub mem0: Mem0Config,
    pub memory_curator: MemoryCuratorConfig,
    pub memory_recall: MemoryRecallConfig,
    pub knowledge: KnowledgeConfig,
    pub observability: ObservabilityConfig,
    pub runtime_timing: RuntimeTimingConfig,
    pub runtime_limits: RuntimeLimitsConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct DatabaseConfig {
    pub url: String,
    pub connect_timeout_seconds: u64,
    pub application_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct DaemonConfig {
    pub bind_host: String,
    pub bind_port: u16,
    pub bearer_token: Option<String>,
    pub skills_dir: PathBuf,
    pub public_base_url: Option<String>,
    pub worker_lease_owner: String,
    pub a2a_peers: BTreeMap<String, A2APeerConfig>,
    pub mcp_connectors: BTreeMap<String, McpConnectorSeedConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct TelegramConfig {
    pub enabled: bool,
    pub bot_token: Option<String>,
    pub poll_interval_ms: u64,
    pub poll_request_timeout_seconds: u64,
    pub progress_update_min_interval_ms: u64,
    pub global_send_min_interval_ms: u64,
    pub private_chat_send_min_interval_ms: u64,
    pub group_chat_send_min_interval_ms: u64,
    pub pairing_token_ttl_seconds: u64,
    pub max_upload_bytes: usize,
    pub max_download_bytes: usize,
    pub private_chat_auto_create_session: bool,
    pub group_require_mention: bool,
    pub default_autoapprove: bool,
    pub inbound_queue_default_mode: String,
    pub inbound_coalesce_window_ms: u64,
    pub inbound_min_coalesce_window_ms: u64,
    pub message_text_soft_cap: usize,
    pub caption_soft_cap: usize,
    pub status_detail_char_cap: usize,
    pub status_ttl_seconds: i64,
    pub typing_initial_delay_ms: u64,
    pub typing_heartbeat_interval_seconds: u64,
    pub delivery_retry_attempts: usize,
    pub delivery_retry_base_delay_ms: u64,
    pub chat_turn_fast_settle_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct SessionDefaultsConfig {
    pub working_memory_limit: usize,
    pub project_memory_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(default)]
pub struct WorkspaceConfig {
    pub default_root: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct ContextConfig {
    pub compaction_min_messages: usize,
    pub compaction_keep_tail_messages: usize,
    pub compaction_max_output_tokens: u32,
    pub compaction_max_summary_chars: usize,
    pub auto_compaction_trigger_ratio: f64,
    pub context_window_tokens_override: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct WebConfig {
    pub search_backend: WebSearchBackend,
    pub search_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct KnowledgeConfig {
    pub operator_timezone: String,
    pub silverbullet_space_dir: Option<PathBuf>,
    pub silverbullet_base_url: Option<String>,
    pub silverbullet_journal_context_enabled: bool,
    pub silverbullet_mirror_enabled: bool,
    pub silverbullet_session_area_path: PathBuf,
    pub silverbullet_text_artifact_extensions: Vec<String>,
    pub silverbullet_script_artifact_extensions: Vec<String>,
    pub source_files: Vec<KnowledgeSourcePathConfig>,
    pub source_dirs: Vec<KnowledgeSourcePathConfig>,
    pub allowed_extensions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct KnowledgeSourcePathConfig {
    pub path: PathBuf,
    pub root: KnowledgeRoot,
    pub kind: KnowledgeSourceKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct BrowserConfig {
    pub enabled: bool,
    pub command: String,
    pub provider: String,
    pub session_prefix: String,
    pub default_timeout_ms: u64,
    pub max_output_chars: usize,
    pub browserless: BrowserlessConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct BrowserlessConfig {
    pub api_url: String,
    pub cdp_url: Option<String>,
    pub api_key: Option<String>,
    pub browser_type: String,
    pub ttl_ms: u64,
    pub stealth: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Mem0Config {
    pub enabled: bool,
    pub api_base: String,
    pub api_key: Option<String>,
    pub default_user_id: String,
    pub request_timeout_ms: u64,
    pub default_limit: usize,
    pub max_limit: usize,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct MemoryCuratorConfig {
    pub enabled: bool,
    pub mode: String,
    pub min_confidence: f64,
    pub max_candidates: usize,
    pub max_output_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct MemoryRecallConfig {
    pub enabled: bool,
    pub scopes: Vec<String>,
    pub max_results: usize,
    pub max_query_chars: usize,
    pub max_memory_chars: usize,
}

impl BrowserConfig {
    pub fn to_tool_config(&self, session_name: String) -> BrowserToolConfig {
        BrowserToolConfig {
            enabled: self.enabled,
            command: self.command.clone(),
            provider: Some(self.provider.clone()).filter(|value| !value.trim().is_empty()),
            session_name,
            default_timeout_ms: self.default_timeout_ms,
            max_output_chars: self.max_output_chars,
            browserless_api_key: self.browserless.api_key.clone(),
            browserless_api_url: Some(self.browserless.api_url.clone())
                .filter(|value| !value.trim().is_empty()),
            browserless_cdp_url: self.browserless.cdp_url.clone(),
            browserless_browser_type: Some(self.browserless.browser_type.clone())
                .filter(|value| !value.trim().is_empty()),
            browserless_ttl_ms: Some(self.browserless.ttl_ms),
            browserless_stealth: Some(self.browserless.stealth),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct ObservabilityConfig {
    pub otlp_export_enabled: bool,
    pub otlp_endpoint: String,
    pub otlp_timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct RuntimeTimingConfig {
    #[serde(alias = "sqlite_lock_retry_delay_ms")]
    pub store_retry_delay_ms: u64,
    pub daemon_http_connect_timeout_ms: u64,
    pub daemon_http_request_timeout_ms: u64,
    pub a2a_http_connect_timeout_ms: u64,
    pub autospawn_status_poll_attempts: usize,
    pub autospawn_status_poll_interval_ms: u64,
    pub shutdown_wait_poll_attempts: usize,
    pub shutdown_wait_poll_interval_ms: u64,
    pub restart_stop_poll_attempts: usize,
    pub restart_stop_poll_interval_ms: u64,
    pub restart_stop_required_unavailable_probes: usize,
    pub http_server_request_poll_interval_ms: u64,
    pub daemon_test_startup_probe_attempts: usize,
    pub daemon_test_startup_probe_interval_ms: u64,
    pub daemon_background_worker_tick_interval_ms: u64,
    pub daemon_background_worker_lease_seconds: i64,
    pub tui_event_poll_interval_ms: u64,
    pub tui_active_run_heartbeat_notice_interval_seconds: u64,
    pub mcp_stdio_command_poll_interval_ms: u64,
    pub provider_loop_transient_retry_base_delay_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct RuntimeLimitsConfig {
    pub diagnostic_tail_lines: usize,
    #[serde(alias = "sqlite_lock_retry_attempts")]
    pub store_retry_attempts: usize,
    pub active_run_step_tail_limit: usize,
    pub active_process_output_tail_max_bytes: usize,
    pub active_process_output_tail_max_lines: usize,
    pub transcript_tail_run_limit: usize,
    pub agent_list_default_limit: usize,
    pub agent_list_max_limit: usize,
    pub schedule_list_default_limit: usize,
    pub schedule_list_max_limit: usize,
    pub mcp_search_default_limit: usize,
    pub mcp_search_max_limit: usize,
    pub session_search_default_limit: usize,
    pub session_search_max_limit: usize,
    pub session_read_default_max_items: usize,
    pub session_read_max_items: usize,
    pub session_read_default_max_bytes: usize,
    pub session_read_max_bytes: usize,
    pub knowledge_search_default_limit: usize,
    pub knowledge_search_max_limit: usize,
    pub knowledge_read_excerpt_default_max_lines: usize,
    pub knowledge_read_full_default_max_lines: usize,
    pub knowledge_read_max_lines: usize,
    pub knowledge_read_default_max_bytes: usize,
    pub knowledge_read_max_bytes: usize,
    pub operator_user_context_max_chars: usize,
    pub silverbullet_journal_context_max_chars_per_day: usize,
    pub silverbullet_mirror_text_artifact_max_chars: usize,
    pub silverbullet_mirror_script_max_chars: usize,
    pub session_warm_idle_seconds: u64,
    pub timeline_preview_chars: usize,
    pub fs_list_default_limit: usize,
    pub fs_list_max_limit: usize,
    pub process_output_read_default_max_bytes: usize,
    pub process_output_read_max_bytes: usize,
    pub process_output_read_default_max_lines: usize,
    pub process_output_read_max_lines: usize,
    pub process_wait_default_timeout_ms: u64,
    pub process_wait_max_timeout_ms: u64,
    pub process_wait_poll_interval_ms: u64,
    pub process_terminate_grace_ms: u64,
    pub process_reader_drain_grace_ms: u64,
    pub provider_loop_max_transient_retries: usize,
    pub provider_loop_max_identical_tool_call_repeats: usize,
    pub provider_loop_max_empty_response_recoveries: usize,
    pub tool_result_preview_char_limit: usize,
    pub offload_max_context_refs: usize,
    pub offload_inline_tool_output_token_limit: u32,
    pub offload_inline_find_in_files_preview_limit: usize,
    pub artifact_read_default_max_bytes: usize,
    pub artifact_read_max_bytes: usize,
    pub kv_list_default_limit: usize,
    pub kv_list_max_limit: usize,
    pub kv_key_max_bytes: usize,
    pub kv_value_max_bytes: usize,
    pub kv_metadata_max_bytes: usize,
    pub skill_list_default_limit: usize,
    pub skill_list_max_limit: usize,
    pub skill_read_default_max_bytes: usize,
    pub skill_read_max_bytes: usize,
    pub autonomy_state_default_max_items: usize,
    pub autonomy_state_max_items: usize,
    pub prompt_recent_filesystem_activity_limit: usize,
    pub prompt_recent_process_activity_limit: usize,
    pub prompt_workspace_tree_limit: usize,
    pub interagent_default_max_hops: u32,
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

#[derive(Debug, Clone, PartialEq)]
pub struct ConfigEnv {
    pub config_path: Option<PathBuf>,
    pub data_dir_override: Option<PathBuf>,
    pub database_url_override: Option<String>,
    pub database_connect_timeout_override: Option<u64>,
    pub database_application_name_override: Option<String>,
    pub daemon_bearer_token_override: Option<String>,
    pub daemon_bind_host_override: Option<String>,
    pub daemon_bind_port_override: Option<u16>,
    pub daemon_public_base_url_override: Option<String>,
    pub daemon_skills_dir_override: Option<PathBuf>,
    pub home_dir: Option<PathBuf>,
    pub telegram_bot_token_override: Option<String>,
    pub workspace_default_root_override: Option<PathBuf>,
    pub context_compaction_keep_tail_messages_override: Option<usize>,
    pub context_compaction_max_output_tokens_override: Option<u32>,
    pub context_compaction_max_summary_chars_override: Option<usize>,
    pub context_compaction_min_messages_override: Option<usize>,
    pub context_auto_compaction_trigger_ratio_override: Option<f64>,
    pub context_window_tokens_override: Option<u32>,
    pub web_search_backend_override: Option<String>,
    pub web_search_url_override: Option<String>,
    pub browser_enabled_override: Option<bool>,
    pub browser_command_override: Option<String>,
    pub browser_provider_override: Option<String>,
    pub browser_session_prefix_override: Option<String>,
    pub browser_default_timeout_ms_override: Option<u64>,
    pub browser_max_output_chars_override: Option<usize>,
    pub browserless_api_url_override: Option<String>,
    pub browserless_cdp_url_override: Option<String>,
    pub browserless_api_key_override: Option<String>,
    pub browserless_browser_type_override: Option<String>,
    pub browserless_ttl_ms_override: Option<u64>,
    pub browserless_stealth_override: Option<bool>,
    pub mem0_enabled_override: Option<bool>,
    pub mem0_api_base_override: Option<String>,
    pub mem0_api_key_override: Option<String>,
    pub mem0_default_user_id_override: Option<String>,
    pub mem0_request_timeout_ms_override: Option<u64>,
    pub mem0_default_limit_override: Option<usize>,
    pub mem0_max_limit_override: Option<usize>,
    pub memory_curator_enabled_override: Option<bool>,
    pub memory_curator_mode_override: Option<String>,
    pub memory_curator_min_confidence_override: Option<f64>,
    pub memory_curator_max_candidates_override: Option<usize>,
    pub memory_curator_max_output_tokens_override: Option<u32>,
    pub memory_recall_enabled_override: Option<bool>,
    pub memory_recall_scopes_override: Option<Vec<String>>,
    pub memory_recall_max_results_override: Option<usize>,
    pub memory_recall_max_query_chars_override: Option<usize>,
    pub memory_recall_max_memory_chars_override: Option<usize>,
    pub operator_timezone_override: Option<String>,
    pub silverbullet_space_dir_override: Option<PathBuf>,
    pub silverbullet_base_url_override: Option<String>,
    pub silverbullet_journal_context_enabled_override: Option<bool>,
    pub silverbullet_mirror_enabled_override: Option<bool>,
    pub otlp_export_enabled_override: Option<bool>,
    pub otlp_endpoint_override: Option<String>,
    pub otlp_timeout_ms_override: Option<u64>,
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
    database: Option<DatabaseConfig>,
    daemon: Option<DaemonConfig>,
    telegram: Option<TelegramConfig>,
    permissions: Option<PermissionConfig>,
    provider: Option<ConfiguredProvider>,
    session_defaults: Option<SessionDefaultsConfig>,
    workspace: Option<WorkspaceConfig>,
    context: Option<ContextConfig>,
    web: Option<WebConfig>,
    browser: Option<BrowserConfig>,
    mem0: Option<Mem0Config>,
    memory_curator: Option<MemoryCuratorConfig>,
    memory_recall: Option<MemoryRecallConfig>,
    knowledge: Option<KnowledgeConfig>,
    observability: Option<ObservabilityConfig>,
    runtime_timing: Option<RuntimeTimingConfig>,
    runtime_limits: Option<RuntimeLimitsConfig>,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            bind_host: DEFAULT_DAEMON_BIND_HOST.to_string(),
            bind_port: DEFAULT_DAEMON_BIND_PORT,
            bearer_token: None,
            skills_dir: PathBuf::from(DEFAULT_DAEMON_SKILLS_DIR),
            public_base_url: None,
            worker_lease_owner: "daemon".to_string(),
            a2a_peers: BTreeMap::new(),
            mcp_connectors: BTreeMap::new(),
        }
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: DEFAULT_DATABASE_URL.to_string(),
            connect_timeout_seconds: DEFAULT_DATABASE_CONNECT_TIMEOUT_SECONDS,
            application_name: DEFAULT_DATABASE_APPLICATION_NAME.to_string(),
        }
    }
}

impl Default for TelegramConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bot_token: None,
            poll_interval_ms: 1_000,
            poll_request_timeout_seconds: 50,
            progress_update_min_interval_ms: 1_250,
            global_send_min_interval_ms: 42,
            private_chat_send_min_interval_ms: 1_250,
            group_chat_send_min_interval_ms: 3_750,
            pairing_token_ttl_seconds: 15 * 60,
            max_upload_bytes: 16 * 1024 * 1024,
            max_download_bytes: 40 * 1024 * 1024,
            private_chat_auto_create_session: true,
            group_require_mention: true,
            default_autoapprove: true,
            inbound_queue_default_mode: "coalesce".to_string(),
            inbound_coalesce_window_ms: 5_000,
            inbound_min_coalesce_window_ms: 5_000,
            message_text_soft_cap: 3_276,
            caption_soft_cap: 819,
            status_detail_char_cap: 700,
            status_ttl_seconds: 30 * 60,
            typing_initial_delay_ms: 750,
            typing_heartbeat_interval_seconds: 4,
            delivery_retry_attempts: 3,
            delivery_retry_base_delay_ms: 250,
            chat_turn_fast_settle_ms: 50,
        }
    }
}

impl Default for KnowledgeConfig {
    fn default() -> Self {
        Self {
            operator_timezone: DEFAULT_OPERATOR_TIMEZONE.to_string(),
            silverbullet_space_dir: Some(PathBuf::from(DEFAULT_SILVERBULLET_SPACE_DIR)),
            silverbullet_base_url: None,
            silverbullet_journal_context_enabled: true,
            silverbullet_mirror_enabled: true,
            silverbullet_session_area_path: PathBuf::from("a/teamd-agents.md"),
            silverbullet_text_artifact_extensions: vec![
                "bash".to_string(),
                "css".to_string(),
                "csv".to_string(),
                "html".to_string(),
                "js".to_string(),
                "json".to_string(),
                "lua".to_string(),
                "md".to_string(),
                "py".to_string(),
                "rs".to_string(),
                "sh".to_string(),
                "sql".to_string(),
                "toml".to_string(),
                "ts".to_string(),
                "txt".to_string(),
                "xml".to_string(),
                "yaml".to_string(),
                "yml".to_string(),
            ],
            silverbullet_script_artifact_extensions: vec![
                "bash".to_string(),
                "js".to_string(),
                "lua".to_string(),
                "py".to_string(),
                "rs".to_string(),
                "sh".to_string(),
                "ts".to_string(),
            ],
            source_files: vec![
                KnowledgeSourcePathConfig {
                    path: PathBuf::from("README.md"),
                    root: KnowledgeRoot::RootDocs,
                    kind: KnowledgeSourceKind::RootDoc,
                },
                KnowledgeSourcePathConfig {
                    path: PathBuf::from("SYSTEM.md"),
                    root: KnowledgeRoot::RootDocs,
                    kind: KnowledgeSourceKind::RootDoc,
                },
                KnowledgeSourcePathConfig {
                    path: PathBuf::from("AGENTS.md"),
                    root: KnowledgeRoot::RootDocs,
                    kind: KnowledgeSourceKind::RootDoc,
                },
            ],
            source_dirs: vec![
                KnowledgeSourcePathConfig {
                    path: PathBuf::from("docs"),
                    root: KnowledgeRoot::Docs,
                    kind: KnowledgeSourceKind::ProjectDoc,
                },
                KnowledgeSourcePathConfig {
                    path: PathBuf::from("projects"),
                    root: KnowledgeRoot::Projects,
                    kind: KnowledgeSourceKind::ProjectDoc,
                },
                KnowledgeSourcePathConfig {
                    path: PathBuf::from("notes"),
                    root: KnowledgeRoot::Notes,
                    kind: KnowledgeSourceKind::ProjectNote,
                },
            ],
            allowed_extensions: vec![
                "md".to_string(),
                "txt".to_string(),
                "json".to_string(),
                "yaml".to_string(),
                "yml".to_string(),
                "toml".to_string(),
            ],
        }
    }
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            search_backend: WebSearchBackend::default(),
            search_url: DEFAULT_WEB_SEARCH_URL.to_string(),
        }
    }
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            command: DEFAULT_BROWSER_COMMAND.to_string(),
            provider: DEFAULT_BROWSER_PROVIDER.to_string(),
            session_prefix: DEFAULT_BROWSER_SESSION_PREFIX.to_string(),
            default_timeout_ms: DEFAULT_BROWSER_TIMEOUT_MS,
            max_output_chars: DEFAULT_BROWSER_MAX_OUTPUT_CHARS,
            browserless: BrowserlessConfig::default(),
        }
    }
}

impl Default for BrowserlessConfig {
    fn default() -> Self {
        Self {
            api_url: DEFAULT_BROWSERLESS_API_URL.to_string(),
            cdp_url: None,
            api_key: None,
            browser_type: DEFAULT_BROWSERLESS_BROWSER_TYPE.to_string(),
            ttl_ms: DEFAULT_BROWSERLESS_TTL_MS,
            stealth: true,
        }
    }
}

impl Default for Mem0Config {
    fn default() -> Self {
        Self {
            enabled: false,
            api_base: DEFAULT_MEM0_API_BASE.to_string(),
            api_key: None,
            default_user_id: DEFAULT_MEM0_DEFAULT_USER_ID.to_string(),
            request_timeout_ms: DEFAULT_MEM0_REQUEST_TIMEOUT_MS,
            default_limit: DEFAULT_MEM0_DEFAULT_LIMIT,
            max_limit: DEFAULT_MEM0_MAX_LIMIT,
        }
    }
}

impl Default for MemoryCuratorConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: DEFAULT_MEMORY_CURATOR_MODE.to_string(),
            min_confidence: DEFAULT_MEMORY_CURATOR_MIN_CONFIDENCE,
            max_candidates: DEFAULT_MEMORY_CURATOR_MAX_CANDIDATES,
            max_output_tokens: DEFAULT_MEMORY_CURATOR_MAX_OUTPUT_TOKENS,
        }
    }
}

impl Default for MemoryRecallConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            scopes: vec![
                "operator".to_string(),
                "workspace".to_string(),
                "agent_shared".to_string(),
            ],
            max_results: DEFAULT_MEMORY_RECALL_MAX_RESULTS,
            max_query_chars: DEFAULT_MEMORY_RECALL_MAX_QUERY_CHARS,
            max_memory_chars: DEFAULT_MEMORY_RECALL_MAX_MEMORY_CHARS,
        }
    }
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            otlp_export_enabled: false,
            otlp_endpoint: DEFAULT_OTLP_ENDPOINT.to_string(),
            otlp_timeout_ms: 2_000,
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
            auto_compaction_trigger_ratio: 0.7,
            context_window_tokens_override: None,
        }
    }
}

impl Default for RuntimeTimingConfig {
    fn default() -> Self {
        Self {
            store_retry_delay_ms: 250,
            daemon_http_connect_timeout_ms: 2_000,
            daemon_http_request_timeout_ms: 5_000,
            a2a_http_connect_timeout_ms: 2_000,
            autospawn_status_poll_attempts: 50,
            autospawn_status_poll_interval_ms: 100,
            shutdown_wait_poll_attempts: 50,
            shutdown_wait_poll_interval_ms: 50,
            restart_stop_poll_attempts: 60,
            restart_stop_poll_interval_ms: 50,
            restart_stop_required_unavailable_probes: 3,
            http_server_request_poll_interval_ms: 100,
            daemon_test_startup_probe_attempts: 50,
            daemon_test_startup_probe_interval_ms: 20,
            daemon_background_worker_tick_interval_ms: 100,
            daemon_background_worker_lease_seconds: 60,
            tui_event_poll_interval_ms: 100,
            tui_active_run_heartbeat_notice_interval_seconds: 30,
            mcp_stdio_command_poll_interval_ms: 100,
            provider_loop_transient_retry_base_delay_ms: 100,
        }
    }
}

impl RuntimeTimingConfig {
    pub fn store_retry_delay(&self) -> Duration {
        Duration::from_millis(self.store_retry_delay_ms)
    }

    pub fn daemon_http_connect_timeout(&self) -> Duration {
        Duration::from_millis(self.daemon_http_connect_timeout_ms)
    }

    pub fn daemon_http_request_timeout(&self) -> Duration {
        Duration::from_millis(self.daemon_http_request_timeout_ms)
    }

    pub fn a2a_http_connect_timeout(&self) -> Duration {
        Duration::from_millis(self.a2a_http_connect_timeout_ms)
    }

    pub fn autospawn_status_poll_interval(&self) -> Duration {
        Duration::from_millis(self.autospawn_status_poll_interval_ms)
    }

    pub fn shutdown_wait_poll_interval(&self) -> Duration {
        Duration::from_millis(self.shutdown_wait_poll_interval_ms)
    }

    pub fn restart_stop_poll_interval(&self) -> Duration {
        Duration::from_millis(self.restart_stop_poll_interval_ms)
    }

    pub fn http_server_request_poll_interval(&self) -> Duration {
        Duration::from_millis(self.http_server_request_poll_interval_ms)
    }

    pub fn daemon_test_startup_probe_interval(&self) -> Duration {
        Duration::from_millis(self.daemon_test_startup_probe_interval_ms)
    }

    pub fn daemon_background_worker_tick_interval(&self) -> Duration {
        Duration::from_millis(self.daemon_background_worker_tick_interval_ms)
    }

    pub fn tui_event_poll_interval(&self) -> Duration {
        Duration::from_millis(self.tui_event_poll_interval_ms)
    }

    pub fn mcp_stdio_command_poll_interval(&self) -> Duration {
        Duration::from_millis(self.mcp_stdio_command_poll_interval_ms)
    }

    pub fn provider_loop_transient_retry_base_delay(&self) -> Duration {
        Duration::from_millis(self.provider_loop_transient_retry_base_delay_ms)
    }

    pub fn provider_loop_transient_retry_delay(&self, attempt: usize) -> Duration {
        self.provider_loop_transient_retry_base_delay()
            .saturating_mul(attempt as u32)
    }
}

impl Default for RuntimeLimitsConfig {
    fn default() -> Self {
        Self {
            diagnostic_tail_lines: 80,
            store_retry_attempts: 4,
            active_run_step_tail_limit: 3,
            active_process_output_tail_max_bytes: 2 * 1024,
            active_process_output_tail_max_lines: 8,
            transcript_tail_run_limit: 32,
            agent_list_default_limit: 100,
            agent_list_max_limit: 1_000,
            schedule_list_default_limit: 100,
            schedule_list_max_limit: 1_000,
            mcp_search_default_limit: 20,
            mcp_search_max_limit: 100,
            session_search_default_limit: 20,
            session_search_max_limit: 100,
            session_read_default_max_items: 20,
            session_read_max_items: 200,
            session_read_default_max_bytes: 8 * 1024,
            session_read_max_bytes: 64 * 1024,
            knowledge_search_default_limit: 20,
            knowledge_search_max_limit: 100,
            knowledge_read_excerpt_default_max_lines: 40,
            knowledge_read_full_default_max_lines: 200,
            knowledge_read_max_lines: 400,
            knowledge_read_default_max_bytes: 8 * 1024,
            knowledge_read_max_bytes: 64 * 1024,
            operator_user_context_max_chars: 4 * 1024,
            silverbullet_journal_context_max_chars_per_day: 4 * 1024,
            silverbullet_mirror_text_artifact_max_chars: 24 * 1024,
            silverbullet_mirror_script_max_chars: 24 * 1024,
            session_warm_idle_seconds: 60 * 60,
            timeline_preview_chars: 160,
            fs_list_default_limit: agent_runtime::tool::DEFAULT_FS_LIST_LIMIT,
            fs_list_max_limit: agent_runtime::tool::MAX_FS_LIST_LIMIT,
            process_output_read_default_max_bytes:
                agent_runtime::tool::DEFAULT_PROCESS_OUTPUT_READ_MAX_BYTES,
            process_output_read_max_bytes: agent_runtime::tool::MAX_PROCESS_OUTPUT_READ_MAX_BYTES,
            process_output_read_default_max_lines:
                agent_runtime::tool::DEFAULT_PROCESS_OUTPUT_READ_MAX_LINES,
            process_output_read_max_lines: agent_runtime::tool::MAX_PROCESS_OUTPUT_READ_MAX_LINES,
            process_wait_default_timeout_ms: duration_millis_u64(
                agent_runtime::tool::DEFAULT_PROCESS_WAIT_TIMEOUT,
            ),
            process_wait_max_timeout_ms: duration_millis_u64(
                agent_runtime::tool::MAX_PROCESS_WAIT_TIMEOUT,
            ),
            process_wait_poll_interval_ms: duration_millis_u64(
                agent_runtime::tool::PROCESS_WAIT_POLL_INTERVAL,
            ),
            process_terminate_grace_ms: duration_millis_u64(
                agent_runtime::tool::PROCESS_TERMINATE_GRACE,
            ),
            process_reader_drain_grace_ms: duration_millis_u64(
                agent_runtime::tool::PROCESS_READER_DRAIN_GRACE,
            ),
            provider_loop_max_transient_retries: 3,
            provider_loop_max_identical_tool_call_repeats: 3,
            provider_loop_max_empty_response_recoveries: 1,
            tool_result_preview_char_limit: 16 * 1024,
            offload_max_context_refs: 16,
            offload_inline_tool_output_token_limit: 512,
            offload_inline_find_in_files_preview_limit: 6,
            artifact_read_default_max_bytes: 8 * 1024,
            artifact_read_max_bytes: 32 * 1024,
            kv_list_default_limit: 50,
            kv_list_max_limit: 500,
            kv_key_max_bytes: 512,
            kv_value_max_bytes: 64 * 1024,
            kv_metadata_max_bytes: 16 * 1024,
            skill_list_default_limit: 64,
            skill_list_max_limit: 256,
            skill_read_default_max_bytes: 16 * 1024,
            skill_read_max_bytes: 128 * 1024,
            autonomy_state_default_max_items: 8,
            autonomy_state_max_items: 50,
            prompt_recent_filesystem_activity_limit: 6,
            prompt_recent_process_activity_limit: 6,
            prompt_workspace_tree_limit: 12,
            interagent_default_max_hops: agent_runtime::interagent::DEFAULT_MAX_HOPS,
        }
    }
}

impl RuntimeLimitsConfig {
    pub fn to_tool_runtime_limits(&self) -> ToolRuntimeLimits {
        ToolRuntimeLimits {
            fs_list_default_limit: self.fs_list_default_limit,
            fs_list_max_limit: self.fs_list_max_limit,
            process_output_read_default_max_bytes: self.process_output_read_default_max_bytes,
            process_output_read_max_bytes: self.process_output_read_max_bytes,
            process_output_read_default_max_lines: self.process_output_read_default_max_lines,
            process_output_read_max_lines: self.process_output_read_max_lines,
            process_wait_default_timeout: Duration::from_millis(
                self.process_wait_default_timeout_ms,
            ),
            process_wait_max_timeout: Duration::from_millis(self.process_wait_max_timeout_ms),
            process_wait_poll_interval: Duration::from_millis(self.process_wait_poll_interval_ms),
            process_terminate_grace: Duration::from_millis(self.process_terminate_grace_ms),
            process_reader_drain_grace: Duration::from_millis(self.process_reader_drain_grace_ms),
        }
    }
}

fn duration_millis_u64(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
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
            database_url_override: read_string_var("TEAMD_DATABASE_URL", &dotenv),
            database_connect_timeout_override: read_u64_var(
                "TEAMD_DATABASE_CONNECT_TIMEOUT_SECONDS",
                &dotenv,
            )?,
            database_application_name_override: read_string_var(
                "TEAMD_DATABASE_APPLICATION_NAME",
                &dotenv,
            ),
            daemon_bearer_token_override: read_string_var("TEAMD_DAEMON_BEARER_TOKEN", &dotenv),
            daemon_bind_host_override: read_string_var("TEAMD_DAEMON_BIND_HOST", &dotenv),
            daemon_bind_port_override: read_u16_var("TEAMD_DAEMON_BIND_PORT", &dotenv)?,
            daemon_public_base_url_override: read_string_var(
                "TEAMD_DAEMON_PUBLIC_BASE_URL",
                &dotenv,
            ),
            daemon_skills_dir_override: read_path_var("TEAMD_DAEMON_SKILLS_DIR", &dotenv)?,
            home_dir: read_path_var("HOME", &dotenv)?,
            telegram_bot_token_override: read_string_var("TEAMD_TELEGRAM_BOT_TOKEN", &dotenv),
            workspace_default_root_override: read_path_var(
                "TEAMD_WORKSPACE_DEFAULT_ROOT",
                &dotenv,
            )?,
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
            context_auto_compaction_trigger_ratio_override: read_f64_var(
                "TEAMD_CONTEXT_AUTO_COMPACTION_TRIGGER_RATIO",
                &dotenv,
            )?,
            context_window_tokens_override: read_u32_var("TEAMD_CONTEXT_WINDOW_TOKENS", &dotenv)?,
            web_search_backend_override: read_string_var("TEAMD_WEB_SEARCH_BACKEND", &dotenv),
            web_search_url_override: read_string_var("TEAMD_WEB_SEARCH_URL", &dotenv),
            browser_enabled_override: read_bool_var("TEAMD_BROWSER_ENABLED", &dotenv)?,
            browser_command_override: read_string_var("TEAMD_BROWSER_COMMAND", &dotenv),
            browser_provider_override: read_string_var("TEAMD_BROWSER_PROVIDER", &dotenv),
            browser_session_prefix_override: read_string_var(
                "TEAMD_BROWSER_SESSION_PREFIX",
                &dotenv,
            ),
            browser_default_timeout_ms_override: read_u64_var(
                "TEAMD_BROWSER_DEFAULT_TIMEOUT_MS",
                &dotenv,
            )?,
            browser_max_output_chars_override: read_usize_var(
                "TEAMD_BROWSER_MAX_OUTPUT_CHARS",
                &dotenv,
            )?,
            browserless_api_url_override: read_string_var("TEAMD_BROWSERLESS_API_URL", &dotenv),
            browserless_cdp_url_override: read_string_var("TEAMD_BROWSERLESS_CDP_URL", &dotenv),
            browserless_api_key_override: read_string_var("TEAMD_BROWSERLESS_API_KEY", &dotenv),
            browserless_browser_type_override: read_string_var(
                "TEAMD_BROWSERLESS_BROWSER_TYPE",
                &dotenv,
            ),
            browserless_ttl_ms_override: read_u64_var("TEAMD_BROWSERLESS_TTL_MS", &dotenv)?,
            browserless_stealth_override: read_bool_var("TEAMD_BROWSERLESS_STEALTH", &dotenv)?,
            mem0_enabled_override: read_bool_var("TEAMD_MEM0_ENABLED", &dotenv)?,
            mem0_api_base_override: read_string_var("TEAMD_MEM0_API_BASE", &dotenv),
            mem0_api_key_override: read_string_var("TEAMD_MEM0_API_KEY", &dotenv),
            mem0_default_user_id_override: read_string_var("TEAMD_MEM0_DEFAULT_USER_ID", &dotenv),
            mem0_request_timeout_ms_override: read_u64_var(
                "TEAMD_MEM0_REQUEST_TIMEOUT_MS",
                &dotenv,
            )?,
            mem0_default_limit_override: read_usize_var("TEAMD_MEM0_DEFAULT_LIMIT", &dotenv)?,
            mem0_max_limit_override: read_usize_var("TEAMD_MEM0_MAX_LIMIT", &dotenv)?,
            memory_curator_enabled_override: read_bool_var(
                "TEAMD_MEMORY_CURATOR_ENABLED",
                &dotenv,
            )?,
            memory_curator_mode_override: read_string_var("TEAMD_MEMORY_CURATOR_MODE", &dotenv),
            memory_curator_min_confidence_override: read_f64_var(
                "TEAMD_MEMORY_CURATOR_MIN_CONFIDENCE",
                &dotenv,
            )?,
            memory_curator_max_candidates_override: read_usize_var(
                "TEAMD_MEMORY_CURATOR_MAX_CANDIDATES",
                &dotenv,
            )?,
            memory_curator_max_output_tokens_override: read_u32_var(
                "TEAMD_MEMORY_CURATOR_MAX_OUTPUT_TOKENS",
                &dotenv,
            )?,
            memory_recall_enabled_override: read_bool_var("TEAMD_MEMORY_RECALL_ENABLED", &dotenv)?,
            memory_recall_scopes_override: read_string_list_var(
                "TEAMD_MEMORY_RECALL_SCOPES",
                &dotenv,
            ),
            memory_recall_max_results_override: read_usize_var(
                "TEAMD_MEMORY_RECALL_MAX_RESULTS",
                &dotenv,
            )?,
            memory_recall_max_query_chars_override: read_usize_var(
                "TEAMD_MEMORY_RECALL_MAX_QUERY_CHARS",
                &dotenv,
            )?,
            memory_recall_max_memory_chars_override: read_usize_var(
                "TEAMD_MEMORY_RECALL_MAX_MEMORY_CHARS",
                &dotenv,
            )?,
            operator_timezone_override: read_string_var("TEAMD_OPERATOR_TIMEZONE", &dotenv),
            silverbullet_space_dir_override: read_path_var(
                "TEAMD_SILVERBULLET_SPACE_DIR",
                &dotenv,
            )?,
            silverbullet_base_url_override: read_string_var("TEAMD_SILVERBULLET_BASE_URL", &dotenv),
            silverbullet_journal_context_enabled_override: read_bool_var(
                "TEAMD_SILVERBULLET_JOURNAL_CONTEXT_ENABLED",
                &dotenv,
            )?,
            silverbullet_mirror_enabled_override: read_bool_var(
                "TEAMD_SILVERBULLET_MIRROR_ENABLED",
                &dotenv,
            )?,
            otlp_export_enabled_override: read_bool_var("TEAMD_OTLP_EXPORT_ENABLED", &dotenv)?,
            otlp_endpoint_override: read_string_var("TEAMD_OTLP_ENDPOINT", &dotenv),
            otlp_timeout_ms_override: read_u64_var("TEAMD_OTLP_TIMEOUT_MS", &dotenv)?,
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
        let mut database = file_config
            .as_ref()
            .and_then(|config| config.database.clone())
            .unwrap_or_default();
        let mut daemon = file_config
            .as_ref()
            .and_then(|config| config.daemon.clone())
            .unwrap_or_default();
        let mut telegram = file_config
            .as_ref()
            .and_then(|config| config.telegram.clone())
            .unwrap_or_default();
        let mut provider = file_config
            .as_ref()
            .and_then(|config| config.provider.clone())
            .unwrap_or_default();
        let mut session_defaults = file_config
            .as_ref()
            .and_then(|config| config.session_defaults.clone())
            .unwrap_or_default();
        let mut workspace = file_config
            .as_ref()
            .and_then(|config| config.workspace.clone())
            .unwrap_or_default();
        let mut context = file_config
            .as_ref()
            .and_then(|config| config.context.clone())
            .unwrap_or_default();
        let mut web = file_config
            .as_ref()
            .and_then(|config| config.web.clone())
            .unwrap_or_default();
        let mut browser = file_config
            .as_ref()
            .and_then(|config| config.browser.clone())
            .unwrap_or_default();
        let mut mem0 = file_config
            .as_ref()
            .and_then(|config| config.mem0.clone())
            .unwrap_or_default();
        let mut memory_curator = file_config
            .as_ref()
            .and_then(|config| config.memory_curator.clone())
            .unwrap_or_default();
        let mut memory_recall = file_config
            .as_ref()
            .and_then(|config| config.memory_recall.clone())
            .unwrap_or_default();
        let mut knowledge = file_config
            .as_ref()
            .and_then(|config| config.knowledge.clone())
            .unwrap_or_default();
        let mut observability = file_config
            .as_ref()
            .and_then(|config| config.observability.clone())
            .unwrap_or_default();
        let runtime_timing = file_config
            .as_ref()
            .and_then(|config| config.runtime_timing.clone())
            .unwrap_or_default();
        let runtime_limits = file_config
            .as_ref()
            .and_then(|config| config.runtime_limits.clone())
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
        if let Some(bot_token) = &env.telegram_bot_token_override {
            telegram.bot_token = Some(bot_token.clone());
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
        if let Some(url) = &env.database_url_override {
            database.url = url.clone();
        }
        if let Some(seconds) = env.database_connect_timeout_override {
            database.connect_timeout_seconds = seconds;
        }
        if let Some(application_name) = &env.database_application_name_override {
            database.application_name = application_name.clone();
        }
        if let Some(limit) = env.session_working_memory_limit_override {
            session_defaults.working_memory_limit = limit;
        }
        if let Some(enabled) = env.session_project_memory_enabled_override {
            session_defaults.project_memory_enabled = enabled;
        }
        if let Some(path) = &env.workspace_default_root_override {
            workspace.default_root = Some(path.clone());
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
        if let Some(value) = env.context_auto_compaction_trigger_ratio_override {
            context.auto_compaction_trigger_ratio = value;
        }
        if let Some(value) = env.context_window_tokens_override {
            context.context_window_tokens_override = Some(value);
        }
        if let Some(search_backend) = env.web_search_backend_override.as_deref() {
            web.search_backend = parse_web_search_backend(search_backend)?;
        }
        if let Some(search_url) = &env.web_search_url_override {
            web.search_url = search_url.clone();
        }
        if let Some(enabled) = env.browser_enabled_override {
            browser.enabled = enabled;
        }
        if let Some(command) = &env.browser_command_override {
            browser.command = command.clone();
        }
        if let Some(provider) = &env.browser_provider_override {
            browser.provider = provider.clone();
        }
        if let Some(session_prefix) = &env.browser_session_prefix_override {
            browser.session_prefix = session_prefix.clone();
        }
        if let Some(timeout_ms) = env.browser_default_timeout_ms_override {
            browser.default_timeout_ms = timeout_ms;
        }
        if let Some(max_output_chars) = env.browser_max_output_chars_override {
            browser.max_output_chars = max_output_chars;
        }
        if let Some(api_url) = &env.browserless_api_url_override {
            browser.browserless.api_url = api_url.clone();
        }
        if let Some(cdp_url) = &env.browserless_cdp_url_override {
            browser.browserless.cdp_url = Some(cdp_url.clone());
        }
        if let Some(api_key) = &env.browserless_api_key_override {
            browser.browserless.api_key = Some(api_key.clone());
        }
        if let Some(browser_type) = &env.browserless_browser_type_override {
            browser.browserless.browser_type = browser_type.clone();
        }
        if let Some(ttl_ms) = env.browserless_ttl_ms_override {
            browser.browserless.ttl_ms = ttl_ms;
        }
        if let Some(stealth) = env.browserless_stealth_override {
            browser.browserless.stealth = stealth;
        }
        if let Some(enabled) = env.mem0_enabled_override {
            mem0.enabled = enabled;
        }
        if let Some(api_base) = &env.mem0_api_base_override {
            mem0.api_base = api_base.clone();
        }
        if let Some(api_key) = &env.mem0_api_key_override {
            mem0.api_key = Some(api_key.clone());
        }
        if let Some(default_user_id) = &env.mem0_default_user_id_override {
            mem0.default_user_id = default_user_id.clone();
        }
        if let Some(timeout_ms) = env.mem0_request_timeout_ms_override {
            mem0.request_timeout_ms = timeout_ms;
        }
        if let Some(limit) = env.mem0_default_limit_override {
            mem0.default_limit = limit;
        }
        if let Some(limit) = env.mem0_max_limit_override {
            mem0.max_limit = limit;
        }
        if let Some(enabled) = env.memory_curator_enabled_override {
            memory_curator.enabled = enabled;
        }
        if let Some(mode) = &env.memory_curator_mode_override {
            memory_curator.mode = mode.clone();
        }
        if let Some(confidence) = env.memory_curator_min_confidence_override {
            memory_curator.min_confidence = confidence;
        }
        if let Some(limit) = env.memory_curator_max_candidates_override {
            memory_curator.max_candidates = limit;
        }
        if let Some(tokens) = env.memory_curator_max_output_tokens_override {
            memory_curator.max_output_tokens = tokens;
        }
        if let Some(enabled) = env.memory_recall_enabled_override {
            memory_recall.enabled = enabled;
        }
        if let Some(scopes) = &env.memory_recall_scopes_override {
            memory_recall.scopes = scopes.clone();
        }
        if let Some(limit) = env.memory_recall_max_results_override {
            memory_recall.max_results = limit;
        }
        if let Some(chars) = env.memory_recall_max_query_chars_override {
            memory_recall.max_query_chars = chars;
        }
        if let Some(chars) = env.memory_recall_max_memory_chars_override {
            memory_recall.max_memory_chars = chars;
        }
        if let Some(timezone) = &env.operator_timezone_override {
            knowledge.operator_timezone = timezone.clone();
        }
        if let Some(path) = &env.silverbullet_space_dir_override {
            knowledge.silverbullet_space_dir = Some(path.clone());
        }
        if let Some(base_url) = &env.silverbullet_base_url_override {
            knowledge.silverbullet_base_url = Some(base_url.clone());
        }
        if let Some(enabled) = env.silverbullet_journal_context_enabled_override {
            knowledge.silverbullet_journal_context_enabled = enabled;
        }
        if let Some(enabled) = env.silverbullet_mirror_enabled_override {
            knowledge.silverbullet_mirror_enabled = enabled;
        }
        if let Some(enabled) = env.otlp_export_enabled_override {
            observability.otlp_export_enabled = enabled;
        }
        if let Some(endpoint) = &env.otlp_endpoint_override {
            observability.otlp_endpoint = endpoint.clone();
        }
        if let Some(timeout_ms) = env.otlp_timeout_ms_override {
            observability.otlp_timeout_ms = timeout_ms;
        }
        if provider.kind == ProviderKind::ZaiChatCompletions && provider.api_base.is_none() {
            provider.api_base = Some(DEFAULT_ZAI_API_BASE.to_string());
        }
        if provider.kind == ProviderKind::ZaiChatCompletions && provider.default_model.is_none() {
            provider.default_model = Some(DEFAULT_ZAI_MODEL.to_string());
        }

        let config = Self {
            data_dir,
            database,
            daemon,
            telegram,
            permissions,
            provider,
            session_defaults,
            workspace,
            context,
            web,
            browser,
            mem0,
            memory_curator,
            memory_recall,
            knowledge,
            observability,
            runtime_timing,
            runtime_limits,
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

        if self.database.url.trim().is_empty() {
            return Err(ConfigError::InvalidProviderValue {
                name: "database.url",
                value: self.database.url.clone(),
                reason: "must not be empty",
            });
        }
        if self.database.application_name.trim().is_empty() {
            return Err(ConfigError::InvalidProviderValue {
                name: "database.application_name",
                value: self.database.application_name.clone(),
                reason: "must not be empty",
            });
        }
        validate_positive_u64_value(
            "database.connect_timeout_seconds",
            self.database.connect_timeout_seconds,
        )?;

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
        if self.daemon.worker_lease_owner.trim().is_empty() {
            return Err(ConfigError::InvalidProviderValue {
                name: "daemon.worker_lease_owner",
                value: self.daemon.worker_lease_owner.clone(),
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

        if self.telegram.enabled && self.telegram.bot_token.is_none() {
            return Err(ConfigError::InvalidProviderValue {
                name: "telegram.bot_token",
                value: String::new(),
                reason: "must be set when telegram.enabled is true",
            });
        }

        if let Some(bot_token) = &self.telegram.bot_token
            && bot_token.trim().is_empty()
        {
            return Err(ConfigError::InvalidProviderValue {
                name: "telegram.bot_token",
                value: bot_token.clone(),
                reason: "must not be empty",
            });
        }

        if self.web.search_url.trim().is_empty() {
            return Err(ConfigError::InvalidProviderValue {
                name: "web.search_url",
                value: self.web.search_url.clone(),
                reason: "must not be empty",
            });
        }
        if self.browser.enabled && self.browser.command.trim().is_empty() {
            return Err(ConfigError::InvalidProviderValue {
                name: "browser.command",
                value: self.browser.command.clone(),
                reason: "must not be empty when browser.enabled is true",
            });
        }
        if self.browser.enabled && self.browser.provider.trim().is_empty() {
            return Err(ConfigError::InvalidProviderValue {
                name: "browser.provider",
                value: self.browser.provider.clone(),
                reason: "must not be empty when browser.enabled is true",
            });
        }
        if self.browser.session_prefix.trim().is_empty() {
            return Err(ConfigError::InvalidProviderValue {
                name: "browser.session_prefix",
                value: self.browser.session_prefix.clone(),
                reason: "must not be empty",
            });
        }
        if self.browser.enabled && self.browser.browserless.api_url.trim().is_empty() {
            return Err(ConfigError::InvalidProviderValue {
                name: "browser.browserless.api_url",
                value: self.browser.browserless.api_url.clone(),
                reason: "must not be empty when browser.enabled is true",
            });
        }
        if self.browser.enabled
            && self.browser.provider == "cdp"
            && self
                .browser
                .browserless
                .cdp_url
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
        {
            return Err(ConfigError::InvalidProviderValue {
                name: "browser.browserless.cdp_url",
                value: self.browser.browserless.cdp_url.clone().unwrap_or_default(),
                reason: "must not be empty when browser.provider is cdp",
            });
        }
        if self.browser.enabled && self.browser.browserless.browser_type.trim().is_empty() {
            return Err(ConfigError::InvalidProviderValue {
                name: "browser.browserless.browser_type",
                value: self.browser.browserless.browser_type.clone(),
                reason: "must not be empty when browser.enabled is true",
            });
        }
        if self.mem0.enabled && self.mem0.api_base.trim().is_empty() {
            return Err(ConfigError::InvalidProviderValue {
                name: "mem0.api_base",
                value: self.mem0.api_base.clone(),
                reason: "must not be empty when mem0.enabled is true",
            });
        }
        if self.mem0.enabled && self.mem0.default_user_id.trim().is_empty() {
            return Err(ConfigError::InvalidProviderValue {
                name: "mem0.default_user_id",
                value: self.mem0.default_user_id.clone(),
                reason: "must not be empty when mem0.enabled is true",
            });
        }
        if let Some(api_key) = &self.mem0.api_key
            && api_key.trim().is_empty()
        {
            return Err(ConfigError::InvalidProviderValue {
                name: "mem0.api_key",
                value: api_key.clone(),
                reason: "must not be empty",
            });
        }
        validate_memory_curator_mode("memory_curator.mode", self.memory_curator.mode.as_str())?;
        validate_memory_recall_scopes(&self.memory_recall.scopes)?;
        if self.knowledge.operator_timezone.trim().is_empty() {
            return Err(ConfigError::InvalidProviderValue {
                name: "knowledge.operator_timezone",
                value: self.knowledge.operator_timezone.clone(),
                reason: "must not be empty",
            });
        }
        if let Some(path) = &self.knowledge.silverbullet_space_dir
            && (!path.is_absolute() || has_parent_component(path))
        {
            return Err(ConfigError::InvalidProviderValue {
                name: "knowledge.silverbullet_space_dir",
                value: path.display().to_string(),
                reason: "must be an absolute path without parent components",
            });
        }
        if let Some(base_url) = &self.knowledge.silverbullet_base_url
            && base_url.trim().is_empty()
        {
            return Err(ConfigError::InvalidProviderValue {
                name: "knowledge.silverbullet_base_url",
                value: base_url.clone(),
                reason: "must not be empty when configured",
            });
        }
        validate_relative_config_path(
            "knowledge.silverbullet_session_area_path",
            &self.knowledge.silverbullet_session_area_path,
        )?;
        validate_extension_list(
            "knowledge.silverbullet_text_artifact_extensions",
            &self.knowledge.silverbullet_text_artifact_extensions,
        )?;
        validate_extension_list(
            "knowledge.silverbullet_script_artifact_extensions",
            &self.knowledge.silverbullet_script_artifact_extensions,
        )?;
        validate_knowledge_source_paths("knowledge.source_files", &self.knowledge.source_files)?;
        validate_knowledge_source_paths("knowledge.source_dirs", &self.knowledge.source_dirs)?;
        validate_extension_list(
            "knowledge.allowed_extensions",
            &self.knowledge.allowed_extensions,
        )?;
        if self.observability.otlp_endpoint.trim().is_empty() {
            return Err(ConfigError::InvalidProviderValue {
                name: "observability.otlp_endpoint",
                value: self.observability.otlp_endpoint.clone(),
                reason: "must not be empty",
            });
        }

        if let Some(default_root) = &self.workspace.default_root {
            if default_root.as_os_str().is_empty() {
                return Err(ConfigError::InvalidProviderValue {
                    name: "workspace.default_root",
                    value: default_root.display().to_string(),
                    reason: "must not be empty",
                });
            }

            let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let normalized = normalize_absolute_path(default_root, &cwd);
            validate_workspace_root_path("workspace.default_root", &normalized, &self.data_dir)?;
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
        validate_positive_u64_value("telegram.poll_interval_ms", self.telegram.poll_interval_ms)?;
        validate_positive_u64_value(
            "telegram.poll_request_timeout_seconds",
            self.telegram.poll_request_timeout_seconds,
        )?;
        validate_positive_u64_value(
            "telegram.progress_update_min_interval_ms",
            self.telegram.progress_update_min_interval_ms,
        )?;
        validate_positive_u64_value(
            "telegram.global_send_min_interval_ms",
            self.telegram.global_send_min_interval_ms,
        )?;
        validate_positive_u64_value(
            "telegram.private_chat_send_min_interval_ms",
            self.telegram.private_chat_send_min_interval_ms,
        )?;
        validate_positive_u64_value(
            "telegram.group_chat_send_min_interval_ms",
            self.telegram.group_chat_send_min_interval_ms,
        )?;
        validate_positive_u64_value(
            "telegram.pairing_token_ttl_seconds",
            self.telegram.pairing_token_ttl_seconds,
        )?;
        validate_positive_usize_value("telegram.max_upload_bytes", self.telegram.max_upload_bytes)?;
        validate_positive_usize_value(
            "telegram.max_download_bytes",
            self.telegram.max_download_bytes,
        )?;
        validate_telegram_inbound_queue_mode(
            "telegram.inbound_queue_default_mode",
            self.telegram.inbound_queue_default_mode.as_str(),
        )?;
        validate_positive_u64_value(
            "telegram.inbound_min_coalesce_window_ms",
            self.telegram.inbound_min_coalesce_window_ms,
        )?;
        if self.telegram.inbound_coalesce_window_ms < self.telegram.inbound_min_coalesce_window_ms {
            return Err(ConfigError::InvalidProviderValue {
                name: "telegram.inbound_coalesce_window_ms",
                value: self.telegram.inbound_coalesce_window_ms.to_string(),
                reason: "must be at least telegram.inbound_min_coalesce_window_ms",
            });
        }
        validate_positive_usize_value(
            "telegram.message_text_soft_cap",
            self.telegram.message_text_soft_cap,
        )?;
        validate_positive_usize_value("telegram.caption_soft_cap", self.telegram.caption_soft_cap)?;
        validate_positive_usize_value(
            "telegram.status_detail_char_cap",
            self.telegram.status_detail_char_cap,
        )?;
        validate_positive_i64_value(
            "telegram.status_ttl_seconds",
            self.telegram.status_ttl_seconds,
        )?;
        validate_positive_u64_value(
            "telegram.typing_initial_delay_ms",
            self.telegram.typing_initial_delay_ms,
        )?;
        validate_positive_u64_value(
            "telegram.typing_heartbeat_interval_seconds",
            self.telegram.typing_heartbeat_interval_seconds,
        )?;
        validate_positive_usize_value(
            "telegram.delivery_retry_attempts",
            self.telegram.delivery_retry_attempts,
        )?;
        validate_positive_u64_value(
            "telegram.delivery_retry_base_delay_ms",
            self.telegram.delivery_retry_base_delay_ms,
        )?;
        validate_positive_u64_value(
            "telegram.chat_turn_fast_settle_ms",
            self.telegram.chat_turn_fast_settle_ms,
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
        validate_ratio_value(
            "context.auto_compaction_trigger_ratio",
            self.context.auto_compaction_trigger_ratio,
        )?;
        validate_positive_provider_value(
            "context.context_window_tokens_override",
            self.context.context_window_tokens_override,
        )?;
        validate_positive_u64_value(
            "observability.otlp_timeout_ms",
            self.observability.otlp_timeout_ms,
        )?;
        validate_positive_u64_value(
            "browser.default_timeout_ms",
            self.browser.default_timeout_ms,
        )?;
        validate_positive_usize_value("browser.max_output_chars", self.browser.max_output_chars)?;
        validate_positive_u64_value(
            "browser.browserless.ttl_ms",
            self.browser.browserless.ttl_ms,
        )?;
        validate_positive_u64_value("mem0.request_timeout_ms", self.mem0.request_timeout_ms)?;
        validate_positive_usize_value("mem0.default_limit", self.mem0.default_limit)?;
        validate_positive_usize_value("mem0.max_limit", self.mem0.max_limit)?;
        validate_limit_bounds(
            "mem0.default_limit",
            self.mem0.default_limit,
            "mem0.max_limit",
            self.mem0.max_limit,
        )?;
        validate_ratio_value(
            "memory_curator.min_confidence",
            self.memory_curator.min_confidence,
        )?;
        validate_positive_usize_value(
            "memory_curator.max_candidates",
            self.memory_curator.max_candidates,
        )?;
        validate_positive_u32_value(
            "memory_curator.max_output_tokens",
            self.memory_curator.max_output_tokens,
        )?;
        validate_positive_usize_value("memory_recall.max_results", self.memory_recall.max_results)?;
        validate_positive_usize_value(
            "memory_recall.max_query_chars",
            self.memory_recall.max_query_chars,
        )?;
        validate_positive_usize_value(
            "memory_recall.max_memory_chars",
            self.memory_recall.max_memory_chars,
        )?;
        validate_positive_u64_value(
            "runtime_timing.store_retry_delay_ms",
            self.runtime_timing.store_retry_delay_ms,
        )?;
        validate_positive_u64_value(
            "runtime_timing.daemon_http_connect_timeout_ms",
            self.runtime_timing.daemon_http_connect_timeout_ms,
        )?;
        validate_positive_u64_value(
            "runtime_timing.daemon_http_request_timeout_ms",
            self.runtime_timing.daemon_http_request_timeout_ms,
        )?;
        validate_positive_u64_value(
            "runtime_timing.a2a_http_connect_timeout_ms",
            self.runtime_timing.a2a_http_connect_timeout_ms,
        )?;
        validate_positive_usize_value(
            "runtime_timing.autospawn_status_poll_attempts",
            self.runtime_timing.autospawn_status_poll_attempts,
        )?;
        validate_positive_u64_value(
            "runtime_timing.autospawn_status_poll_interval_ms",
            self.runtime_timing.autospawn_status_poll_interval_ms,
        )?;
        validate_positive_usize_value(
            "runtime_timing.shutdown_wait_poll_attempts",
            self.runtime_timing.shutdown_wait_poll_attempts,
        )?;
        validate_positive_u64_value(
            "runtime_timing.shutdown_wait_poll_interval_ms",
            self.runtime_timing.shutdown_wait_poll_interval_ms,
        )?;
        validate_positive_usize_value(
            "runtime_timing.restart_stop_poll_attempts",
            self.runtime_timing.restart_stop_poll_attempts,
        )?;
        validate_positive_u64_value(
            "runtime_timing.restart_stop_poll_interval_ms",
            self.runtime_timing.restart_stop_poll_interval_ms,
        )?;
        validate_positive_usize_value(
            "runtime_timing.restart_stop_required_unavailable_probes",
            self.runtime_timing.restart_stop_required_unavailable_probes,
        )?;
        validate_positive_u64_value(
            "runtime_timing.http_server_request_poll_interval_ms",
            self.runtime_timing.http_server_request_poll_interval_ms,
        )?;
        validate_positive_usize_value(
            "runtime_timing.daemon_test_startup_probe_attempts",
            self.runtime_timing.daemon_test_startup_probe_attempts,
        )?;
        validate_positive_u64_value(
            "runtime_timing.daemon_test_startup_probe_interval_ms",
            self.runtime_timing.daemon_test_startup_probe_interval_ms,
        )?;
        validate_positive_u64_value(
            "runtime_timing.daemon_background_worker_tick_interval_ms",
            self.runtime_timing
                .daemon_background_worker_tick_interval_ms,
        )?;
        validate_positive_i64_value(
            "runtime_timing.daemon_background_worker_lease_seconds",
            self.runtime_timing.daemon_background_worker_lease_seconds,
        )?;
        validate_positive_u64_value(
            "runtime_timing.tui_event_poll_interval_ms",
            self.runtime_timing.tui_event_poll_interval_ms,
        )?;
        validate_positive_u64_value(
            "runtime_timing.tui_active_run_heartbeat_notice_interval_seconds",
            self.runtime_timing
                .tui_active_run_heartbeat_notice_interval_seconds,
        )?;
        validate_positive_u64_value(
            "runtime_timing.mcp_stdio_command_poll_interval_ms",
            self.runtime_timing.mcp_stdio_command_poll_interval_ms,
        )?;
        validate_positive_u64_value(
            "runtime_timing.provider_loop_transient_retry_base_delay_ms",
            self.runtime_timing
                .provider_loop_transient_retry_base_delay_ms,
        )?;
        validate_positive_usize_value(
            "runtime_limits.diagnostic_tail_lines",
            self.runtime_limits.diagnostic_tail_lines,
        )?;
        validate_positive_usize_value(
            "runtime_limits.store_retry_attempts",
            self.runtime_limits.store_retry_attempts,
        )?;
        validate_positive_usize_value(
            "runtime_limits.active_run_step_tail_limit",
            self.runtime_limits.active_run_step_tail_limit,
        )?;
        validate_positive_usize_value(
            "runtime_limits.active_process_output_tail_max_bytes",
            self.runtime_limits.active_process_output_tail_max_bytes,
        )?;
        validate_positive_usize_value(
            "runtime_limits.active_process_output_tail_max_lines",
            self.runtime_limits.active_process_output_tail_max_lines,
        )?;
        validate_positive_usize_value(
            "runtime_limits.transcript_tail_run_limit",
            self.runtime_limits.transcript_tail_run_limit,
        )?;
        validate_positive_usize_value(
            "runtime_limits.agent_list_default_limit",
            self.runtime_limits.agent_list_default_limit,
        )?;
        validate_positive_usize_value(
            "runtime_limits.agent_list_max_limit",
            self.runtime_limits.agent_list_max_limit,
        )?;
        validate_positive_usize_value(
            "runtime_limits.schedule_list_default_limit",
            self.runtime_limits.schedule_list_default_limit,
        )?;
        validate_positive_usize_value(
            "runtime_limits.schedule_list_max_limit",
            self.runtime_limits.schedule_list_max_limit,
        )?;
        validate_positive_usize_value(
            "runtime_limits.mcp_search_default_limit",
            self.runtime_limits.mcp_search_default_limit,
        )?;
        validate_positive_usize_value(
            "runtime_limits.mcp_search_max_limit",
            self.runtime_limits.mcp_search_max_limit,
        )?;
        validate_positive_usize_value(
            "runtime_limits.session_search_default_limit",
            self.runtime_limits.session_search_default_limit,
        )?;
        validate_positive_usize_value(
            "runtime_limits.session_search_max_limit",
            self.runtime_limits.session_search_max_limit,
        )?;
        validate_positive_usize_value(
            "runtime_limits.session_read_default_max_items",
            self.runtime_limits.session_read_default_max_items,
        )?;
        validate_positive_usize_value(
            "runtime_limits.session_read_max_items",
            self.runtime_limits.session_read_max_items,
        )?;
        validate_positive_usize_value(
            "runtime_limits.session_read_default_max_bytes",
            self.runtime_limits.session_read_default_max_bytes,
        )?;
        validate_positive_usize_value(
            "runtime_limits.session_read_max_bytes",
            self.runtime_limits.session_read_max_bytes,
        )?;
        validate_positive_usize_value(
            "runtime_limits.knowledge_search_default_limit",
            self.runtime_limits.knowledge_search_default_limit,
        )?;
        validate_positive_usize_value(
            "runtime_limits.knowledge_search_max_limit",
            self.runtime_limits.knowledge_search_max_limit,
        )?;
        validate_positive_usize_value(
            "runtime_limits.knowledge_read_excerpt_default_max_lines",
            self.runtime_limits.knowledge_read_excerpt_default_max_lines,
        )?;
        validate_positive_usize_value(
            "runtime_limits.knowledge_read_full_default_max_lines",
            self.runtime_limits.knowledge_read_full_default_max_lines,
        )?;
        validate_positive_usize_value(
            "runtime_limits.knowledge_read_max_lines",
            self.runtime_limits.knowledge_read_max_lines,
        )?;
        validate_positive_usize_value(
            "runtime_limits.knowledge_read_default_max_bytes",
            self.runtime_limits.knowledge_read_default_max_bytes,
        )?;
        validate_positive_usize_value(
            "runtime_limits.knowledge_read_max_bytes",
            self.runtime_limits.knowledge_read_max_bytes,
        )?;
        validate_positive_usize_value(
            "runtime_limits.operator_user_context_max_chars",
            self.runtime_limits.operator_user_context_max_chars,
        )?;
        validate_positive_usize_value(
            "runtime_limits.silverbullet_journal_context_max_chars_per_day",
            self.runtime_limits
                .silverbullet_journal_context_max_chars_per_day,
        )?;
        validate_positive_usize_value(
            "runtime_limits.silverbullet_mirror_text_artifact_max_chars",
            self.runtime_limits
                .silverbullet_mirror_text_artifact_max_chars,
        )?;
        validate_positive_usize_value(
            "runtime_limits.silverbullet_mirror_script_max_chars",
            self.runtime_limits.silverbullet_mirror_script_max_chars,
        )?;
        validate_positive_u64_value(
            "runtime_limits.session_warm_idle_seconds",
            self.runtime_limits.session_warm_idle_seconds,
        )?;
        validate_positive_usize_value(
            "runtime_limits.timeline_preview_chars",
            self.runtime_limits.timeline_preview_chars,
        )?;
        validate_positive_usize_value(
            "runtime_limits.fs_list_default_limit",
            self.runtime_limits.fs_list_default_limit,
        )?;
        validate_positive_usize_value(
            "runtime_limits.fs_list_max_limit",
            self.runtime_limits.fs_list_max_limit,
        )?;
        validate_positive_usize_value(
            "runtime_limits.process_output_read_default_max_bytes",
            self.runtime_limits.process_output_read_default_max_bytes,
        )?;
        validate_positive_usize_value(
            "runtime_limits.process_output_read_max_bytes",
            self.runtime_limits.process_output_read_max_bytes,
        )?;
        validate_positive_usize_value(
            "runtime_limits.process_output_read_default_max_lines",
            self.runtime_limits.process_output_read_default_max_lines,
        )?;
        validate_positive_usize_value(
            "runtime_limits.process_output_read_max_lines",
            self.runtime_limits.process_output_read_max_lines,
        )?;
        validate_positive_u64_value(
            "runtime_limits.process_wait_default_timeout_ms",
            self.runtime_limits.process_wait_default_timeout_ms,
        )?;
        validate_positive_u64_value(
            "runtime_limits.process_wait_max_timeout_ms",
            self.runtime_limits.process_wait_max_timeout_ms,
        )?;
        validate_positive_u64_value(
            "runtime_limits.process_wait_poll_interval_ms",
            self.runtime_limits.process_wait_poll_interval_ms,
        )?;
        validate_positive_u64_value(
            "runtime_limits.process_terminate_grace_ms",
            self.runtime_limits.process_terminate_grace_ms,
        )?;
        validate_positive_u64_value(
            "runtime_limits.process_reader_drain_grace_ms",
            self.runtime_limits.process_reader_drain_grace_ms,
        )?;
        validate_positive_usize_value(
            "runtime_limits.provider_loop_max_transient_retries",
            self.runtime_limits.provider_loop_max_transient_retries,
        )?;
        validate_positive_usize_value(
            "runtime_limits.provider_loop_max_identical_tool_call_repeats",
            self.runtime_limits
                .provider_loop_max_identical_tool_call_repeats,
        )?;
        validate_positive_usize_value(
            "runtime_limits.tool_result_preview_char_limit",
            self.runtime_limits.tool_result_preview_char_limit,
        )?;
        validate_positive_usize_value(
            "runtime_limits.offload_max_context_refs",
            self.runtime_limits.offload_max_context_refs,
        )?;
        validate_positive_u32_value(
            "runtime_limits.offload_inline_tool_output_token_limit",
            self.runtime_limits.offload_inline_tool_output_token_limit,
        )?;
        validate_positive_usize_value(
            "runtime_limits.offload_inline_find_in_files_preview_limit",
            self.runtime_limits
                .offload_inline_find_in_files_preview_limit,
        )?;
        validate_positive_usize_value(
            "runtime_limits.artifact_read_default_max_bytes",
            self.runtime_limits.artifact_read_default_max_bytes,
        )?;
        validate_positive_usize_value(
            "runtime_limits.artifact_read_max_bytes",
            self.runtime_limits.artifact_read_max_bytes,
        )?;
        validate_positive_usize_value(
            "runtime_limits.kv_list_default_limit",
            self.runtime_limits.kv_list_default_limit,
        )?;
        validate_positive_usize_value(
            "runtime_limits.kv_list_max_limit",
            self.runtime_limits.kv_list_max_limit,
        )?;
        validate_positive_usize_value(
            "runtime_limits.kv_key_max_bytes",
            self.runtime_limits.kv_key_max_bytes,
        )?;
        validate_positive_usize_value(
            "runtime_limits.kv_value_max_bytes",
            self.runtime_limits.kv_value_max_bytes,
        )?;
        validate_positive_usize_value(
            "runtime_limits.kv_metadata_max_bytes",
            self.runtime_limits.kv_metadata_max_bytes,
        )?;
        validate_positive_usize_value(
            "runtime_limits.skill_list_default_limit",
            self.runtime_limits.skill_list_default_limit,
        )?;
        validate_positive_usize_value(
            "runtime_limits.skill_list_max_limit",
            self.runtime_limits.skill_list_max_limit,
        )?;
        validate_positive_usize_value(
            "runtime_limits.skill_read_default_max_bytes",
            self.runtime_limits.skill_read_default_max_bytes,
        )?;
        validate_positive_usize_value(
            "runtime_limits.skill_read_max_bytes",
            self.runtime_limits.skill_read_max_bytes,
        )?;
        validate_positive_usize_value(
            "runtime_limits.autonomy_state_default_max_items",
            self.runtime_limits.autonomy_state_default_max_items,
        )?;
        validate_positive_usize_value(
            "runtime_limits.autonomy_state_max_items",
            self.runtime_limits.autonomy_state_max_items,
        )?;
        validate_positive_usize_value(
            "runtime_limits.prompt_recent_filesystem_activity_limit",
            self.runtime_limits.prompt_recent_filesystem_activity_limit,
        )?;
        validate_positive_usize_value(
            "runtime_limits.prompt_recent_process_activity_limit",
            self.runtime_limits.prompt_recent_process_activity_limit,
        )?;
        validate_positive_usize_value(
            "runtime_limits.prompt_workspace_tree_limit",
            self.runtime_limits.prompt_workspace_tree_limit,
        )?;
        validate_positive_u32_value(
            "runtime_limits.interagent_default_max_hops",
            self.runtime_limits.interagent_default_max_hops,
        )?;
        if self.context.compaction_keep_tail_messages > self.context.compaction_min_messages {
            return Err(ConfigError::InvalidProviderValue {
                name: "context.compaction_keep_tail_messages",
                value: self.context.compaction_keep_tail_messages.to_string(),
                reason: "must be less than or equal to compaction_min_messages",
            });
        }
        validate_limit_bounds(
            "runtime_limits.agent_list_default_limit",
            self.runtime_limits.agent_list_default_limit,
            "runtime_limits.agent_list_max_limit",
            self.runtime_limits.agent_list_max_limit,
        )?;
        validate_limit_bounds(
            "runtime_limits.schedule_list_default_limit",
            self.runtime_limits.schedule_list_default_limit,
            "runtime_limits.schedule_list_max_limit",
            self.runtime_limits.schedule_list_max_limit,
        )?;
        validate_limit_bounds(
            "runtime_limits.mcp_search_default_limit",
            self.runtime_limits.mcp_search_default_limit,
            "runtime_limits.mcp_search_max_limit",
            self.runtime_limits.mcp_search_max_limit,
        )?;
        validate_limit_bounds(
            "runtime_limits.session_search_default_limit",
            self.runtime_limits.session_search_default_limit,
            "runtime_limits.session_search_max_limit",
            self.runtime_limits.session_search_max_limit,
        )?;
        validate_limit_bounds(
            "runtime_limits.session_read_default_max_items",
            self.runtime_limits.session_read_default_max_items,
            "runtime_limits.session_read_max_items",
            self.runtime_limits.session_read_max_items,
        )?;
        validate_limit_bounds(
            "runtime_limits.session_read_default_max_bytes",
            self.runtime_limits.session_read_default_max_bytes,
            "runtime_limits.session_read_max_bytes",
            self.runtime_limits.session_read_max_bytes,
        )?;
        validate_limit_bounds(
            "runtime_limits.knowledge_search_default_limit",
            self.runtime_limits.knowledge_search_default_limit,
            "runtime_limits.knowledge_search_max_limit",
            self.runtime_limits.knowledge_search_max_limit,
        )?;
        validate_limit_bounds(
            "runtime_limits.knowledge_read_excerpt_default_max_lines",
            self.runtime_limits.knowledge_read_excerpt_default_max_lines,
            "runtime_limits.knowledge_read_max_lines",
            self.runtime_limits.knowledge_read_max_lines,
        )?;
        validate_limit_bounds(
            "runtime_limits.knowledge_read_full_default_max_lines",
            self.runtime_limits.knowledge_read_full_default_max_lines,
            "runtime_limits.knowledge_read_max_lines",
            self.runtime_limits.knowledge_read_max_lines,
        )?;
        validate_limit_bounds(
            "runtime_limits.knowledge_read_default_max_bytes",
            self.runtime_limits.knowledge_read_default_max_bytes,
            "runtime_limits.knowledge_read_max_bytes",
            self.runtime_limits.knowledge_read_max_bytes,
        )?;
        validate_limit_bounds(
            "runtime_limits.fs_list_default_limit",
            self.runtime_limits.fs_list_default_limit,
            "runtime_limits.fs_list_max_limit",
            self.runtime_limits.fs_list_max_limit,
        )?;
        validate_limit_bounds(
            "runtime_limits.process_output_read_default_max_bytes",
            self.runtime_limits.process_output_read_default_max_bytes,
            "runtime_limits.process_output_read_max_bytes",
            self.runtime_limits.process_output_read_max_bytes,
        )?;
        validate_limit_bounds(
            "runtime_limits.process_output_read_default_max_lines",
            self.runtime_limits.process_output_read_default_max_lines,
            "runtime_limits.process_output_read_max_lines",
            self.runtime_limits.process_output_read_max_lines,
        )?;
        validate_limit_bounds_u64(
            "runtime_limits.process_wait_default_timeout_ms",
            self.runtime_limits.process_wait_default_timeout_ms,
            "runtime_limits.process_wait_max_timeout_ms",
            self.runtime_limits.process_wait_max_timeout_ms,
        )?;
        validate_limit_bounds(
            "runtime_limits.artifact_read_default_max_bytes",
            self.runtime_limits.artifact_read_default_max_bytes,
            "runtime_limits.artifact_read_max_bytes",
            self.runtime_limits.artifact_read_max_bytes,
        )?;
        validate_limit_bounds(
            "runtime_limits.kv_list_default_limit",
            self.runtime_limits.kv_list_default_limit,
            "runtime_limits.kv_list_max_limit",
            self.runtime_limits.kv_list_max_limit,
        )?;
        validate_limit_bounds(
            "runtime_limits.skill_list_default_limit",
            self.runtime_limits.skill_list_default_limit,
            "runtime_limits.skill_list_max_limit",
            self.runtime_limits.skill_list_max_limit,
        )?;
        validate_limit_bounds(
            "runtime_limits.skill_read_default_max_bytes",
            self.runtime_limits.skill_read_default_max_bytes,
            "runtime_limits.skill_read_max_bytes",
            self.runtime_limits.skill_read_max_bytes,
        )?;
        validate_limit_bounds(
            "runtime_limits.autonomy_state_default_max_items",
            self.runtime_limits.autonomy_state_default_max_items,
            "runtime_limits.autonomy_state_max_items",
            self.runtime_limits.autonomy_state_max_items,
        )?;

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
                database: None,
                daemon: None,
                telegram: None,
                permissions: None,
                provider: None,
                session_defaults: None,
                workspace: None,
                context: None,
                web: None,
                browser: None,
                mem0: None,
                memory_curator: None,
                memory_recall: None,
                knowledge: None,
                observability: None,
                runtime_timing: None,
                runtime_limits: None,
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

fn read_string_list_var(
    name: &'static str,
    dotenv: &BTreeMap<String, String>,
) -> Option<Vec<String>> {
    read_string_var(name, dotenv).map(|value| {
        value
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect()
    })
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

fn read_f64_var(
    name: &'static str,
    dotenv: &BTreeMap<String, String>,
) -> Result<Option<f64>, ConfigError> {
    read_string_var(name, dotenv)
        .map(|value| parse_positive_ratio(name, &value))
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

fn parse_web_search_backend(value: &str) -> Result<WebSearchBackend, ConfigError> {
    match value {
        "duckduckgo_html" => Ok(WebSearchBackend::DuckDuckGoHtml),
        "searxng_json" => Ok(WebSearchBackend::SearxngJson),
        _ => Err(ConfigError::InvalidProviderValue {
            name: "web.search_backend",
            value: value.to_string(),
            reason: "supported values are duckduckgo_html and searxng_json",
        }),
    }
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

fn parse_positive_ratio(name: &'static str, value: &str) -> Result<f64, ConfigError> {
    let parsed = value
        .parse::<f64>()
        .map_err(|_| ConfigError::InvalidProviderValue {
            name,
            value: value.to_string(),
            reason: "must be a positive decimal ratio",
        })?;
    if !(parsed > 0.0 && parsed <= 1.0) {
        return Err(ConfigError::InvalidProviderValue {
            name,
            value: value.to_string(),
            reason: "must be greater than zero and less than or equal to one",
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

fn validate_positive_u64_value(name: &'static str, value: u64) -> Result<(), ConfigError> {
    if value == 0 {
        return Err(ConfigError::InvalidProviderValue {
            name,
            value: value.to_string(),
            reason: "must be greater than zero",
        });
    }
    Ok(())
}

fn validate_positive_i64_value(name: &'static str, value: i64) -> Result<(), ConfigError> {
    if value <= 0 {
        return Err(ConfigError::InvalidProviderValue {
            name,
            value: value.to_string(),
            reason: "must be greater than zero",
        });
    }
    Ok(())
}

fn validate_telegram_inbound_queue_mode(
    name: &'static str,
    value: &str,
) -> Result<(), ConfigError> {
    match value {
        "reject" | "queue" | "coalesce" | "restart" => Ok(()),
        other => Err(ConfigError::InvalidProviderValue {
            name,
            value: other.to_string(),
            reason: "must be one of reject, queue, coalesce, restart",
        }),
    }
}

fn validate_memory_curator_mode(name: &'static str, value: &str) -> Result<(), ConfigError> {
    match value {
        "auto" | "review" | "off" => Ok(()),
        other => Err(ConfigError::InvalidProviderValue {
            name,
            value: other.to_string(),
            reason: "must be one of auto, review, off",
        }),
    }
}

fn validate_memory_recall_scopes(scopes: &[String]) -> Result<(), ConfigError> {
    if scopes.is_empty() {
        return Err(ConfigError::InvalidProviderValue {
            name: "memory_recall.scopes",
            value: String::new(),
            reason: "must contain at least one scope",
        });
    }
    for scope in scopes {
        match scope.as_str() {
            "operator" | "agent" | "agent_shared" | "workspace" | "session" => {}
            other => {
                return Err(ConfigError::InvalidProviderValue {
                    name: "memory_recall.scopes",
                    value: other.to_string(),
                    reason: "must contain only operator, agent, agent_shared, workspace, or session",
                });
            }
        }
    }
    Ok(())
}

fn validate_knowledge_source_paths(
    name: &'static str,
    paths: &[KnowledgeSourcePathConfig],
) -> Result<(), ConfigError> {
    if paths.is_empty() {
        return Err(ConfigError::InvalidProviderValue {
            name,
            value: String::new(),
            reason: "must contain at least one path",
        });
    }
    for source in paths {
        validate_relative_config_path(name, &source.path)?;
    }
    Ok(())
}

fn validate_relative_config_path(name: &'static str, path: &Path) -> Result<(), ConfigError> {
    if path.as_os_str().is_empty() || path.is_absolute() || has_parent_component(path) {
        return Err(ConfigError::InvalidProviderValue {
            name,
            value: path.display().to_string(),
            reason: "must be a non-empty relative path without parent components",
        });
    }
    Ok(())
}

fn validate_extension_list(name: &'static str, extensions: &[String]) -> Result<(), ConfigError> {
    if extensions.is_empty() {
        return Err(ConfigError::InvalidProviderValue {
            name,
            value: String::new(),
            reason: "must contain at least one extension",
        });
    }
    for extension in extensions {
        if extension.trim().is_empty()
            || extension.contains('/')
            || extension.contains('\\')
            || extension.starts_with('.')
        {
            return Err(ConfigError::InvalidProviderValue {
                name,
                value: extension.clone(),
                reason: "must contain plain extension names without separators or leading dot",
            });
        }
    }
    Ok(())
}

fn validate_ratio_value(name: &'static str, value: f64) -> Result<(), ConfigError> {
    if !(value > 0.0 && value <= 1.0) {
        return Err(ConfigError::InvalidProviderValue {
            name,
            value: value.to_string(),
            reason: "must be greater than zero and less than or equal to one",
        });
    }
    Ok(())
}

fn validate_limit_bounds(
    name: &'static str,
    value: usize,
    max_name: &'static str,
    max_value: usize,
) -> Result<(), ConfigError> {
    if value > max_value {
        return Err(ConfigError::InvalidProviderValue {
            name,
            value: value.to_string(),
            reason: max_name,
        });
    }
    Ok(())
}

fn validate_limit_bounds_u64(
    name: &'static str,
    value: u64,
    max_name: &'static str,
    max_value: u64,
) -> Result<(), ConfigError> {
    if value > max_value {
        return Err(ConfigError::InvalidProviderValue {
            name,
            value: value.to_string(),
            reason: max_name,
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

pub fn normalize_absolute_path(path: &Path, cwd: &Path) -> PathBuf {
    let mut normalized = if path.is_absolute() {
        PathBuf::new()
    } else {
        cwd.to_path_buf()
    };

    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::Normal(part) => normalized.push(part),
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::RootDir => normalized.push(Path::new("/")),
            std::path::Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
        }
    }

    normalized
}

fn has_parent_component(path: &Path) -> bool {
    path.components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
}

pub fn validate_workspace_root_path(
    field_name: &'static str,
    workspace_root: &Path,
    data_dir: &Path,
) -> Result<(), ConfigError> {
    let artifacts_dir = data_dir.join("artifacts");
    let transcripts_dir = data_dir.join("transcripts");
    let runs_dir = data_dir.join("runs");
    let audit_dir = data_dir.join("audit");
    let reserved_roots = [
        data_dir,
        &artifacts_dir,
        &transcripts_dir,
        &runs_dir,
        &audit_dir,
    ];

    if reserved_roots
        .iter()
        .any(|reserved| workspace_root == *reserved || workspace_root.starts_with(reserved))
    {
        return Err(ConfigError::InvalidProviderValue {
            name: field_name,
            value: workspace_root.display().to_string(),
            reason: "must not point into teamd state directories",
        });
    }

    Ok(())
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            data_dir: default_data_dir(),
            database: DatabaseConfig::default(),
            daemon: DaemonConfig::default(),
            telegram: TelegramConfig::default(),
            permissions: PermissionConfig::default(),
            provider: ConfiguredProvider::default(),
            session_defaults: SessionDefaultsConfig::default(),
            workspace: WorkspaceConfig::default(),
            context: ContextConfig::default(),
            web: WebConfig::default(),
            browser: BrowserConfig::default(),
            mem0: Mem0Config::default(),
            memory_curator: MemoryCuratorConfig::default(),
            memory_recall: MemoryRecallConfig::default(),
            knowledge: KnowledgeConfig::default(),
            observability: ObservabilityConfig::default(),
            runtime_timing: RuntimeTimingConfig::default(),
            runtime_limits: RuntimeLimitsConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests;
