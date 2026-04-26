use super::{
    AppConfig, ConfigEnv, ConfigError, DEFAULT_ZAI_API_BASE, DEFAULT_ZAI_MODEL,
    load_dotenv_from_locations, parse_dotenv,
};
use agent_runtime::permission::{PermissionAction, PermissionMode};
use agent_runtime::provider::ProviderKind;
use agent_runtime::tool::WebSearchBackend;
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
        daemon_public_base_url_override: None,
        daemon_skills_dir_override: None,
        home_dir: Some(root.join("home")),
        telegram_bot_token_override: None,
        context_compaction_keep_tail_messages_override: None,
        context_compaction_max_output_tokens_override: None,
        context_compaction_max_summary_chars_override: None,
        context_compaction_min_messages_override: None,
        context_auto_compaction_trigger_ratio_override: None,
        context_window_tokens_override: None,
        web_search_backend_override: None,
        web_search_url_override: None,
        provider_api_base_override: None,
        provider_api_key_override: None,
        provider_connect_timeout_override: None,
        provider_kind_override: None,
        provider_max_tool_rounds_override: None,
        provider_max_output_tokens_override: None,
        provider_model_override: None,
        provider_request_timeout_override: None,
        provider_stream_idle_timeout_override: None,
        permission_mode_override: None,
        session_project_memory_enabled_override: None,
        session_working_memory_limit_override: None,
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
        telegram: Default::default(),
        permissions: Default::default(),
        provider: Default::default(),
        session_defaults: Default::default(),
        context: Default::default(),
        web: Default::default(),
        runtime_timing: Default::default(),
        runtime_limits: Default::default(),
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
fn load_merges_session_defaults_and_context_policy_from_file() {
    let temp = tempfile::tempdir().expect("tempdir");
    let config_path = temp.path().join("teamd.toml");

    fs::write(
        &config_path,
        r#"
data_dir = "/tmp/teamd-config"

[session_defaults]
working_memory_limit = 96
project_memory_enabled = false

[context]
compaction_min_messages = 12
compaction_keep_tail_messages = 4
compaction_max_output_tokens = 2048
compaction_max_summary_chars = 8192
auto_compaction_trigger_ratio = 0.7
context_window_tokens_override = 200000
"#,
    )
    .expect("write config");

    let mut env = base_env(temp.path());
    env.config_path = Some(config_path);

    let config = AppConfig::load_from_env(&env).expect("load config");

    assert_eq!(config.session_defaults.working_memory_limit, 96);
    assert!(!config.session_defaults.project_memory_enabled);
    assert_eq!(config.context.compaction_min_messages, 12);
    assert_eq!(config.context.compaction_keep_tail_messages, 4);
    assert_eq!(config.context.compaction_max_output_tokens, 2048);
    assert_eq!(config.context.compaction_max_summary_chars, 8192);
    assert_eq!(config.context.auto_compaction_trigger_ratio, 0.7);
    assert_eq!(config.context.context_window_tokens_override, Some(200_000));
}

#[test]
fn load_merges_runtime_timing_and_limits_from_file() {
    let temp = tempfile::tempdir().expect("tempdir");
    let config_path = temp.path().join("teamd.toml");

    fs::write(
        &config_path,
        r#"
data_dir = "/tmp/teamd-config"

[runtime_timing]
daemon_http_connect_timeout_ms = 2500
daemon_http_request_timeout_ms = 12000
autospawn_status_poll_attempts = 75
autospawn_status_poll_interval_ms = 150
provider_loop_transient_retry_base_delay_ms = 220
tui_active_run_heartbeat_notice_interval_seconds = 45

[runtime_limits]
diagnostic_tail_lines = 120
transcript_tail_run_limit = 48
agent_list_default_limit = 150
agent_list_max_limit = 1500
session_search_default_limit = 30
session_search_max_limit = 130
knowledge_read_default_max_bytes = 12000
knowledge_read_max_bytes = 128000
"#,
    )
    .expect("write config");

    let mut env = base_env(temp.path());
    env.config_path = Some(config_path);

    let config = AppConfig::load_from_env(&env).expect("load config");

    assert_eq!(config.runtime_timing.daemon_http_connect_timeout_ms, 2500);
    assert_eq!(config.runtime_timing.daemon_http_request_timeout_ms, 12000);
    assert_eq!(config.runtime_timing.autospawn_status_poll_attempts, 75);
    assert_eq!(config.runtime_timing.autospawn_status_poll_interval_ms, 150);
    assert_eq!(
        config
            .runtime_timing
            .provider_loop_transient_retry_base_delay_ms,
        220
    );
    assert_eq!(
        config
            .runtime_timing
            .tui_active_run_heartbeat_notice_interval_seconds,
        45
    );

    assert_eq!(config.runtime_limits.diagnostic_tail_lines, 120);
    assert_eq!(config.runtime_limits.transcript_tail_run_limit, 48);
    assert_eq!(config.runtime_limits.agent_list_default_limit, 150);
    assert_eq!(config.runtime_limits.agent_list_max_limit, 1500);
    assert_eq!(config.runtime_limits.session_search_default_limit, 30);
    assert_eq!(config.runtime_limits.session_search_max_limit, 130);
    assert_eq!(
        config.runtime_limits.knowledge_read_default_max_bytes,
        12000
    );
    assert_eq!(config.runtime_limits.knowledge_read_max_bytes, 128000);
}

#[test]
fn load_merges_telegram_config_and_env_override() {
    let temp = tempfile::tempdir().expect("tempdir");
    let config_path = temp.path().join("teamd.toml");

    fs::write(
        &config_path,
        r#"
data_dir = "/tmp/teamd-config"

[telegram]
enabled = true
poll_interval_ms = 1500
poll_request_timeout_seconds = 40
progress_update_min_interval_ms = 1250
pairing_token_ttl_seconds = 900
max_upload_bytes = 16777216
max_download_bytes = 41943040
private_chat_auto_create_session = true
group_require_mention = true
default_autoapprove = true
"#,
    )
    .expect("write config");

    let mut env = base_env(temp.path());
    env.config_path = Some(config_path);
    env.telegram_bot_token_override = Some("telegram-secret-token".to_string());

    let config = AppConfig::load_from_env(&env).expect("load config");

    assert!(config.telegram.enabled);
    assert_eq!(
        config.telegram.bot_token.as_deref(),
        Some("telegram-secret-token")
    );
    assert_eq!(config.telegram.poll_interval_ms, 1500);
    assert_eq!(config.telegram.poll_request_timeout_seconds, 40);
    assert_eq!(config.telegram.progress_update_min_interval_ms, 1250);
    assert_eq!(config.telegram.pairing_token_ttl_seconds, 900);
    assert_eq!(config.telegram.max_upload_bytes, 16 * 1024 * 1024);
    assert_eq!(config.telegram.max_download_bytes, 40 * 1024 * 1024);
    assert!(config.telegram.private_chat_auto_create_session);
    assert!(config.telegram.group_require_mention);
    assert!(config.telegram.default_autoapprove);
}

#[test]
fn config_example_toml_loads() {
    let temp = tempfile::tempdir().expect("tempdir");
    let config_path = temp.path().join("config.example.toml");
    fs::write(
        &config_path,
        include_str!("../../../../config.example.toml"),
    )
    .expect("write config example");

    let mut env = base_env(temp.path());
    env.config_path = Some(config_path);

    let config = AppConfig::load_from_env(&env).expect("load config example");

    assert_eq!(config.daemon.bind_port, 5140);
    assert_eq!(config.web.search_backend, WebSearchBackend::DuckDuckGoHtml);
    assert_eq!(config.runtime_timing.daemon_http_request_timeout_ms, 5000);
    assert_eq!(config.runtime_limits.diagnostic_tail_lines, 80);
}

#[test]
fn load_merges_web_search_backend_from_file_and_env() {
    let temp = tempfile::tempdir().expect("tempdir");
    let config_path = temp.path().join("teamd.toml");
    fs::write(
        &config_path,
        r#"
data_dir = "/tmp/teamd"

[web]
search_backend = "searxng_json"
search_url = "http://127.0.0.1:8888/search"
"#,
    )
    .expect("write config");

    let mut env = base_env(temp.path());
    env.config_path = Some(config_path);
    env.web_search_backend_override = Some("duckduckgo_html".to_string());
    env.web_search_url_override = Some("https://duckduckgo.com/html/".to_string());

    let config = AppConfig::load_from_env(&env).expect("load config");

    assert_eq!(config.web.search_backend, WebSearchBackend::DuckDuckGoHtml);
    assert_eq!(config.web.search_url, "https://duckduckgo.com/html/");
}

#[test]
fn validate_rejects_invalid_runtime_limit_bounds() {
    let config = AppConfig {
        data_dir: PathBuf::from("/tmp/teamd"),
        daemon: Default::default(),
        telegram: Default::default(),
        permissions: Default::default(),
        provider: Default::default(),
        session_defaults: Default::default(),
        context: Default::default(),
        web: Default::default(),
        runtime_timing: Default::default(),
        runtime_limits: super::RuntimeLimitsConfig {
            agent_list_default_limit: 200,
            agent_list_max_limit: 100,
            ..Default::default()
        },
    };

    let error = config.validate().expect_err("invalid bounds must fail");
    assert!(matches!(error, ConfigError::InvalidProviderValue { .. }));
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
    assert_eq!(config.provider.max_tool_rounds, Some(24));
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
    env.provider_max_tool_rounds_override = Some(32);
    env.provider_max_output_tokens_override = Some(8192);

    let config = AppConfig::load_from_env(&env).expect("load config");

    assert_eq!(config.provider.connect_timeout_seconds, Some(20));
    assert_eq!(config.provider.request_timeout_seconds, Some(3600));
    assert_eq!(config.provider.stream_idle_timeout_seconds, Some(1800));
    assert_eq!(config.provider.max_tool_rounds, Some(32));
    assert_eq!(config.provider.max_output_tokens, Some(8192));
}

#[test]
fn load_applies_context_auto_compaction_env_overrides() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut env = base_env(temp.path());
    env.xdg_config_home = None;
    env.context_auto_compaction_trigger_ratio_override = Some(0.8);
    env.context_window_tokens_override = Some(140_000);

    let config = AppConfig::load_from_env(&env).expect("load config");

    assert_eq!(config.context.auto_compaction_trigger_ratio, 0.8);
    assert_eq!(config.context.context_window_tokens_override, Some(140_000));
}

#[test]
fn validate_rejects_invalid_auto_compaction_ratio() {
    let config = AppConfig {
        data_dir: PathBuf::from("/tmp/teamd"),
        daemon: Default::default(),
        telegram: Default::default(),
        permissions: Default::default(),
        provider: Default::default(),
        session_defaults: Default::default(),
        context: super::ContextConfig {
            auto_compaction_trigger_ratio: 1.5,
            ..Default::default()
        },
        web: Default::default(),
        runtime_timing: Default::default(),
        runtime_limits: Default::default(),
    };

    let error = config
        .validate()
        .expect_err("invalid ratio must fail validation");
    assert!(matches!(error, ConfigError::InvalidProviderValue { .. }));
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
    fs::write(exe_dir.join(".env"), "TEAMD_PROVIDER_MODEL=glm-5.1\n").expect("write exe dotenv");

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
    assert_eq!(config.daemon.public_base_url, None);
    assert!(config.daemon.a2a_peers.is_empty());
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
public_base_url = "https://daemon-a.example"

[daemon.a2a_peers.judge]
base_url = "https://daemon-b.example"
bearer_token = "peer-token"
"#,
    )
    .expect("write config");

    let mut env = base_env(temp.path());
    env.config_path = Some(config_path);
    env.daemon_bind_host_override = Some("10.6.5.3".to_string());
    env.daemon_bind_port_override = Some(6140);
    env.daemon_bearer_token_override = Some("env-token".to_string());
    env.daemon_skills_dir_override = Some(temp.path().join("runtime-skills"));
    env.daemon_public_base_url_override = Some("https://override.example".to_string());

    let config = AppConfig::load_from_env(&env).expect("load config");

    assert_eq!(config.daemon.bind_host, "10.6.5.3");
    assert_eq!(config.daemon.bind_port, 6140);
    assert_eq!(config.daemon.bearer_token.as_deref(), Some("env-token"));
    assert_eq!(config.daemon.skills_dir, temp.path().join("runtime-skills"));
    assert_eq!(
        config.daemon.public_base_url.as_deref(),
        Some("https://override.example")
    );
    assert_eq!(config.daemon.a2a_peers.len(), 1);
    let peer = config.daemon.a2a_peers.get("judge").expect("judge peer");
    assert_eq!(peer.base_url, "https://daemon-b.example");
    assert_eq!(peer.bearer_token.as_deref(), Some("peer-token"));
}
