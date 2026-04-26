use agent_persistence::AppConfig;
use std::time::Duration;

#[test]
fn runtime_timing_policy_is_explicit_and_centralized() {
    let config = AppConfig::default();

    assert_eq!(
        config.runtime_timing.sqlite_busy_timeout(),
        Duration::from_secs(15)
    );
    assert_eq!(
        config.runtime_timing.daemon_http_connect_timeout(),
        Duration::from_secs(2)
    );
    assert_eq!(
        config.runtime_timing.daemon_http_request_timeout(),
        Duration::from_secs(5)
    );
    assert_eq!(
        config.runtime_timing.a2a_http_connect_timeout(),
        Duration::from_secs(2)
    );

    assert_eq!(config.runtime_timing.autospawn_status_poll_attempts, 50);
    assert_eq!(
        config.runtime_timing.autospawn_status_poll_interval(),
        Duration::from_millis(100)
    );
    assert_eq!(config.runtime_timing.shutdown_wait_poll_attempts, 50);
    assert_eq!(
        config.runtime_timing.shutdown_wait_poll_interval(),
        Duration::from_millis(50)
    );
    assert_eq!(config.runtime_timing.restart_stop_poll_attempts, 60);
    assert_eq!(
        config.runtime_timing.restart_stop_poll_interval(),
        Duration::from_millis(50)
    );
    assert_eq!(
        config
            .runtime_timing
            .restart_stop_required_unavailable_probes,
        3
    );

    assert_eq!(
        config.runtime_timing.http_server_request_poll_interval(),
        Duration::from_millis(100)
    );
    assert_eq!(config.runtime_timing.daemon_test_startup_probe_attempts, 50);
    assert_eq!(
        config.runtime_timing.daemon_test_startup_probe_interval(),
        Duration::from_millis(20)
    );
    assert_eq!(
        config
            .runtime_timing
            .daemon_background_worker_tick_interval(),
        Duration::from_millis(100)
    );
    assert_eq!(
        config.runtime_timing.tui_event_poll_interval(),
        Duration::from_millis(100)
    );
    assert_eq!(
        config
            .runtime_timing
            .tui_active_run_heartbeat_notice_interval_seconds,
        30
    );
    assert_eq!(
        config.runtime_timing.mcp_stdio_command_poll_interval(),
        Duration::from_millis(100)
    );
    assert_eq!(
        config
            .runtime_timing
            .provider_loop_transient_retry_base_delay(),
        Duration::from_millis(100)
    );
}

#[test]
fn runtime_limits_defaults_are_explicit_and_centralized() {
    let config = AppConfig::default();

    assert_eq!(config.runtime_limits.diagnostic_tail_lines, 80);
    assert_eq!(config.runtime_limits.transcript_tail_run_limit, 32);
    assert_eq!(config.runtime_limits.agent_list_default_limit, 100);
    assert_eq!(config.runtime_limits.agent_list_max_limit, 1_000);
    assert_eq!(config.runtime_limits.schedule_list_default_limit, 100);
    assert_eq!(config.runtime_limits.schedule_list_max_limit, 1_000);
    assert_eq!(config.runtime_limits.mcp_search_default_limit, 20);
    assert_eq!(config.runtime_limits.mcp_search_max_limit, 100);
    assert_eq!(config.runtime_limits.session_search_default_limit, 20);
    assert_eq!(config.runtime_limits.session_search_max_limit, 100);
    assert_eq!(config.runtime_limits.session_read_default_max_items, 20);
    assert_eq!(config.runtime_limits.session_read_max_items, 200);
    assert_eq!(
        config.runtime_limits.session_read_default_max_bytes,
        8 * 1024
    );
    assert_eq!(config.runtime_limits.session_read_max_bytes, 64 * 1024);
    assert_eq!(config.runtime_limits.knowledge_search_default_limit, 20);
    assert_eq!(config.runtime_limits.knowledge_search_max_limit, 100);
    assert_eq!(
        config
            .runtime_limits
            .knowledge_read_excerpt_default_max_lines,
        40
    );
    assert_eq!(
        config.runtime_limits.knowledge_read_full_default_max_lines,
        200
    );
    assert_eq!(config.runtime_limits.knowledge_read_max_lines, 400);
    assert_eq!(
        config.runtime_limits.knowledge_read_default_max_bytes,
        8 * 1024
    );
    assert_eq!(config.runtime_limits.knowledge_read_max_bytes, 64 * 1024);
}
