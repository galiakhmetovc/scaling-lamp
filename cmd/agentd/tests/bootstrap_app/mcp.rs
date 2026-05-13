use super::support::*;
use agent_persistence::McpConnectorSeedConfig;
use agent_runtime::mcp::McpConnectorTransport;
use agentd::bootstrap::{McpConnectorCreateOptions, McpConnectorUpdatePatch, build_from_config};
use agentd::mcp::{
    McpConnectorState, McpDiscoveredTool, McpWorkerControl, MockMcpConnectorRuntime,
    SharedMcpRegistry,
};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn build_from_config_seeds_mcp_connectors_and_reports_stopped_runtime_status() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut config = AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    };
    config.daemon.mcp_connectors.insert(
        "filesystem".to_string(),
        McpConnectorSeedConfig {
            transport: McpConnectorTransport::Stdio,
            command: "npx".to_string(),
            args: vec![
                "-y".to_string(),
                "@modelcontextprotocol/server-filesystem".to_string(),
                "/workspace".to_string(),
            ],
            env: BTreeMap::from([(String::from("DEBUG"), String::from("1"))]),
            cwd: Some(PathBuf::from("/srv/mcp")),
            enabled: true,
        },
    );

    let app = build_from_config(config).expect("build app");
    let connectors = app.list_mcp_connectors().expect("list connectors");

    assert_eq!(connectors.len(), 1);
    let connector = &connectors[0];
    assert_eq!(connector.id, "filesystem");
    assert_eq!(connector.transport, McpConnectorTransport::Stdio);
    assert_eq!(connector.command, "npx");
    assert_eq!(
        connector.args,
        vec![
            "-y".to_string(),
            "@modelcontextprotocol/server-filesystem".to_string(),
            "/workspace".to_string()
        ]
    );
    assert_eq!(
        connector.env,
        BTreeMap::from([(String::from("DEBUG"), String::from("1"))])
    );
    assert_eq!(connector.cwd.as_deref(), Some("/srv/mcp"));
    assert!(connector.enabled);
    assert_eq!(connector.runtime.state, McpConnectorState::Stopped);
    assert_eq!(connector.runtime.pid, None);
    assert_eq!(connector.runtime.restart_count, 0);
}

#[test]
fn mcp_connector_lifecycle_can_create_update_and_delete_persisted_configs() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");

    let created = app
        .create_mcp_connector(
            "git",
            McpConnectorCreateOptions {
                transport: McpConnectorTransport::Stdio,
                command: "uvx".to_string(),
                args: vec!["mcp-server-git".to_string()],
                env: BTreeMap::new(),
                cwd: Some("/repos".to_string()),
                enabled: true,
            },
        )
        .expect("create connector");

    assert_eq!(created.id, "git");
    assert_eq!(created.command, "uvx");
    assert_eq!(created.runtime.state, McpConnectorState::Stopped);

    let updated = app
        .update_mcp_connector(
            "git",
            McpConnectorUpdatePatch {
                command: Some("npx".to_string()),
                args: Some(vec![
                    "-y".to_string(),
                    "@modelcontextprotocol/server-git".to_string(),
                ]),
                env: Some(BTreeMap::from([(String::from("TRACE"), String::from("1"))])),
                cwd: Some(None),
                enabled: Some(false),
            },
        )
        .expect("update connector");

    assert_eq!(updated.command, "npx");
    assert_eq!(
        updated.args,
        vec![
            "-y".to_string(),
            "@modelcontextprotocol/server-git".to_string()
        ]
    );
    assert_eq!(
        updated.env,
        BTreeMap::from([(String::from("TRACE"), String::from("1"))])
    );
    assert_eq!(updated.cwd, None);
    assert!(!updated.enabled);

    assert_eq!(
        app.list_mcp_connectors().expect("list after update").len(),
        1
    );
    assert!(app.delete_mcp_connector("git").expect("delete connector"));
    assert!(
        app.list_mcp_connectors()
            .expect("list after delete")
            .is_empty()
    );
}

#[test]
fn background_worker_starts_enabled_mcp_connectors_and_skips_disabled_ones() {
    let (api_base, _requests, _handle) = spawn_json_server(
        r#"{
            "id":"resp_background_mcp",
            "model":"gpt-5.4",
            "output":[{"id":"msg","type":"message","status":"completed","role":"assistant","content":[{"type":"output_text","text":"noop"}]}],
            "usage":{"input_tokens":1,"output_tokens":1,"total_tokens":2}
        }"#,
    );
    let temp = tempfile::tempdir().expect("tempdir");
    let mut app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(format!("{api_base}/v1")),
            api_key: Some("test-key".to_string()),
            default_model: Some("gpt-5.4".to_string()),
            ..ConfiguredProvider::default()
        },
        ..AppConfig::default()
    })
    .expect("build app");

    let starts = Arc::new(AtomicUsize::new(0));
    let starts_clone = starts.clone();
    app.mcp = SharedMcpRegistry::with_starter(move |connector, registry, now| {
        starts_clone.fetch_add(1, Ordering::Relaxed);
        registry.mark_running(&connector.id, now, Some(4242));
        let status = registry.status(&connector.id);
        assert_eq!(status.state, McpConnectorState::Running);
        Ok(McpWorkerControl::noop())
    });

    app.create_mcp_connector(
        "enabled",
        McpConnectorCreateOptions {
            transport: McpConnectorTransport::Stdio,
            command: "npx".to_string(),
            args: vec!["server-enabled".to_string()],
            env: BTreeMap::new(),
            cwd: None,
            enabled: true,
        },
    )
    .expect("create enabled connector");
    app.create_mcp_connector(
        "disabled",
        McpConnectorCreateOptions {
            transport: McpConnectorTransport::Stdio,
            command: "npx".to_string(),
            args: vec!["server-disabled".to_string()],
            env: BTreeMap::new(),
            cwd: None,
            enabled: false,
        },
    )
    .expect("create disabled connector");

    app.background_worker_tick(100)
        .expect("background worker tick");

    assert_eq!(starts.load(Ordering::Relaxed), 1);
    assert_eq!(
        app.mcp_connector("enabled")
            .expect("enabled connector")
            .runtime
            .state,
        McpConnectorState::Running
    );
    assert_eq!(
        app.mcp_connector("disabled")
            .expect("disabled connector")
            .runtime
            .state,
        McpConnectorState::Stopped
    );
}

#[test]
fn restart_mcp_connector_starts_enabled_connector_and_leaves_disabled_stopped() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");

    let starts = Arc::new(AtomicUsize::new(0));
    let starts_clone = starts.clone();
    app.mcp = SharedMcpRegistry::with_starter(move |connector, registry, now| {
        starts_clone.fetch_add(1, Ordering::Relaxed);
        registry.mark_running(&connector.id, now, Some(31337));
        Ok(McpWorkerControl::noop())
    });

    app.create_mcp_connector(
        "enabled",
        McpConnectorCreateOptions {
            transport: McpConnectorTransport::Stdio,
            command: "npx".to_string(),
            args: vec!["server-enabled".to_string()],
            env: BTreeMap::new(),
            cwd: None,
            enabled: true,
        },
    )
    .expect("create enabled connector");
    app.create_mcp_connector(
        "disabled",
        McpConnectorCreateOptions {
            transport: McpConnectorTransport::Stdio,
            command: "npx".to_string(),
            args: vec!["server-disabled".to_string()],
            env: BTreeMap::new(),
            cwd: None,
            enabled: false,
        },
    )
    .expect("create disabled connector");

    let enabled = app
        .restart_mcp_connector("enabled")
        .expect("restart enabled connector");
    let disabled = app
        .restart_mcp_connector("disabled")
        .expect("restart disabled connector");

    assert_eq!(starts.load(Ordering::Relaxed), 1);
    assert_eq!(enabled.runtime.state, McpConnectorState::Running);
    assert_eq!(enabled.runtime.pid, Some(31337));
    assert_eq!(disabled.runtime.state, McpConnectorState::Stopped);
}

#[test]
fn tool_catalog_includes_builtin_and_discovered_mcp_tools() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    app.mcp = SharedMcpRegistry::with_mock_connectors(vec![MockMcpConnectorRuntime {
        id: "docs".to_string(),
        tools: vec![McpDiscoveredTool {
            exposed_name: "mcp__docs__search_code".to_string(),
            remote_name: "search_code".to_string(),
            title: Some("Search code".to_string()),
            description: Some("Search code through MCP".to_string()),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": { "query": { "type": "string" } },
                "required": ["query"],
                "additionalProperties": false
            }),
            read_only: true,
            destructive: false,
        }],
        resources: Vec::new(),
        prompts: Vec::new(),
        tool_results: BTreeMap::new(),
        resource_reads: BTreeMap::new(),
        prompt_gets: BTreeMap::new(),
    }]);

    let catalog = app.tool_catalog().expect("tool catalog");
    let fs_read = catalog
        .tools
        .iter()
        .find(|tool| tool.id == "fs_read_text")
        .expect("fs_read_text");
    let mcp_search = catalog
        .tools
        .iter()
        .find(|tool| tool.id == "mcp__docs__search_code")
        .expect("mcp search tool");

    assert_eq!(fs_read.origin, "built_in");
    assert_eq!(fs_read.family, "fs");
    assert!(fs_read.read_only);
    assert!(fs_read.automatic);
    assert_eq!(mcp_search.origin, "mcp");
    assert_eq!(mcp_search.family, "mcp");
    assert_eq!(mcp_search.connector_id.as_deref(), Some("docs"));
    assert_eq!(mcp_search.remote_name.as_deref(), Some("search_code"));
    assert!(mcp_search.available);
}
