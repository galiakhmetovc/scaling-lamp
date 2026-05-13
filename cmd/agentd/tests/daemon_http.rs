use agent_persistence::{
    A2APeerConfig, AppConfig, ArtifactRecord, ArtifactRepository, JobRecord, JobRepository,
    MissionRecord, MissionRepository, PersistenceStore, RunRepository, SessionInboxRepository,
    SessionRecord, SessionRepository, TaskRegistryRecord, TaskRegistryRepository, ToolCallRecord,
    ToolCallRepository, TranscriptRecord, TranscriptRepository, audit::DiagnosticEvent,
};
use agent_runtime::mission::{
    JobExecutionInput, JobResult, JobSpec, JobStatus, MissionExecutionIntent, MissionSchedule,
    MissionStatus,
};
use agent_runtime::tool::{
    KnowledgeReadInput, KnowledgeReadMode, KnowledgeSearchInput, SessionReadInput, SessionReadMode,
    SessionSearchInput,
};
use agentd::bootstrap;
use agentd::daemon;
use agentd::http::types::{
    AgentFileReadResponse, AgentFileWriteResponse, AgentFilesResponse, CreateSessionRequest,
    DaemonStopResponse, DiagnosticsTailRequest, DiagnosticsTailResponse, ErrorResponse,
    McpConnectorCreateRequest, McpConnectorDetailResponse, McpConnectorUpdateRequest,
    MemoryRenderResponse, SessionArtifactFileResponse, SessionArtifactFilesResponse,
    SessionBackgroundJobResponse, SessionDebugResponse, SessionSummaryResponse,
    SessionWorkspaceFileResponse, SessionWorkspaceListResponse, SkillCommandRequest,
    StatusResponse, TaskControlResponse, TaskRenderResponse,
};
use reqwest::StatusCode;
use reqwest::blocking::Client;
use std::collections::BTreeMap;
use std::thread;
use std::time::Duration;

#[path = "daemon_http/support.rs"]
mod support;

use support::{free_port, spawn_json_server_sequence, test_app};

#[test]
fn daemon_http_status_is_public_when_no_token_is_configured() {
    let (_temp, app, base_url) = test_app(None);
    let handle = daemon::spawn_for_test(app).expect("spawn daemon");
    let client = Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .expect("http client");

    let response = client
        .get(format!("{base_url}/v1/status"))
        .send()
        .expect("status request");

    assert_eq!(response.status(), StatusCode::OK);
    let body: StatusResponse = response.json().expect("status json");
    assert!(body.ok);
    assert_eq!(body.bind_host, "127.0.0.1");
    assert_eq!(body.version.as_deref(), Some(env!("CARGO_PKG_VERSION")));
    assert_eq!(
        body.commit.as_deref(),
        Some(option_env!("AGENTD_GIT_COMMIT").unwrap_or("unknown"))
    );

    handle.stop().expect("stop daemon");
}

#[test]
fn daemon_http_requires_bearer_token_when_configured() {
    let (_temp, app, base_url) = test_app(Some("secret-token"));
    let handle = daemon::spawn_for_test(app).expect("spawn daemon");
    let client = Client::new();

    let unauthorized = client
        .get(format!("{base_url}/v1/status"))
        .send()
        .expect("unauthorized response");

    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
    let error: ErrorResponse = unauthorized.json().expect("error json");
    assert!(error.error.contains("authorization"));

    let authorized = client
        .get(format!("{base_url}/v1/status"))
        .bearer_auth("secret-token")
        .send()
        .expect("authorized response");

    assert_eq!(authorized.status(), StatusCode::OK);

    handle.stop().expect("stop daemon");
}

#[test]
fn daemon_http_can_manage_agent_profile_files() {
    let (_temp, app, base_url) = test_app(Some("secret-token"));
    let handle = daemon::spawn_for_test(app).expect("spawn daemon");
    let client = Client::new();

    let list_response = client
        .get(format!("{base_url}/v1/agents/default/files"))
        .bearer_auth("secret-token")
        .send()
        .expect("agent files list response");
    assert_eq!(list_response.status(), StatusCode::OK);
    let list: AgentFilesResponse = list_response.json().expect("agent files json");
    assert_eq!(list.agent_id, "default");
    assert!(list.files.iter().any(|file| file.path == "SYSTEM.md"));
    assert!(list.files.iter().any(|file| file.path == "AGENTS.md"));

    let read_response = client
        .get(format!(
            "{base_url}/v1/agents/default/files/read?path=SYSTEM.md"
        ))
        .bearer_auth("secret-token")
        .send()
        .expect("agent file read response");
    assert_eq!(read_response.status(), StatusCode::OK);
    let read: AgentFileReadResponse = read_response.json().expect("agent file read json");
    assert_eq!(read.path, "SYSTEM.md");
    assert!(!read.content.is_empty());

    let write_response = client
        .post(format!("{base_url}/v1/agents/default/files/write"))
        .bearer_auth("secret-token")
        .json(&serde_json::json!({
            "path": "SYSTEM.md",
            "content": "# Test system\n",
            "mode": "overwrite"
        }))
        .send()
        .expect("agent file write response");
    assert_eq!(write_response.status(), StatusCode::OK);
    let written: AgentFileWriteResponse = write_response.json().expect("agent write json");
    assert_eq!(written.path, "SYSTEM.md");
    assert!(written.overwritten);

    let reread_response = client
        .get(format!(
            "{base_url}/v1/agents/default/files/read?path=SYSTEM.md"
        ))
        .bearer_auth("secret-token")
        .send()
        .expect("agent file reread response");
    let reread: AgentFileReadResponse = reread_response.json().expect("agent file reread json");
    assert_eq!(reread.content, "# Test system\n");

    let skill_response = client
        .post(format!("{base_url}/v1/agents/default/files/write"))
        .bearer_auth("secret-token")
        .json(&serde_json::json!({
            "path": "skills/web-test/SKILL.md",
            "content": "---\nname: web-test\ndescription: Test skill\n---\n\n# Web test\n",
            "mode": "create"
        }))
        .send()
        .expect("agent skill write response");
    assert_eq!(skill_response.status(), StatusCode::OK);

    let invalid_response = client
        .post(format!("{base_url}/v1/agents/default/files/write"))
        .bearer_auth("secret-token")
        .json(&serde_json::json!({
            "path": "../escape.md",
            "content": "no\n",
            "mode": "upsert"
        }))
        .send()
        .expect("invalid agent file write response");
    assert_eq!(invalid_response.status(), StatusCode::BAD_REQUEST);

    handle.stop().expect("stop daemon");
}

#[test]
fn daemon_http_serves_read_only_web_console_without_exposing_data() {
    let (_temp, app, base_url) = test_app(Some("secret-token"));
    let handle = daemon::spawn_for_test(app).expect("spawn daemon");
    let client = Client::new();

    let page = client
        .get(format!("{base_url}/web"))
        .send()
        .expect("web console response");

    assert_eq!(page.status(), StatusCode::OK);
    let content_type = page
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(content_type.contains("text/html"));
    let body = page.text().expect("web console html");
    assert!(body.contains("teamD Read-only Console"));
    assert!(body.contains("/v1/web/snapshot"));

    let unauthorized_snapshot = client
        .get(format!("{base_url}/v1/web/snapshot"))
        .send()
        .expect("unauthorized snapshot response");
    assert_eq!(unauthorized_snapshot.status(), StatusCode::UNAUTHORIZED);

    handle.stop().expect("stop daemon");
}

#[test]
fn daemon_http_web_snapshot_reads_runtime_data() {
    let (_temp, app, base_url) = test_app(Some("secret-token"));
    let store = app.store().expect("open store");
    store
        .put_session(&SessionRecord {
            id: "session-web-old".to_string(),
            title: "Old Web Session".to_string(),
            prompt_override: None,
            settings_json: "{}".to_string(),
            workspace_root: app.runtime.workspace.root.display().to_string(),
            agent_profile_id: "default".to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: None,
            created_at: 10,
            updated_at: 20,
        })
        .expect("put old session");
    store
        .put_session(&SessionRecord {
            id: "session-web-new".to_string(),
            title: "New Web Session".to_string(),
            prompt_override: None,
            settings_json: "{}".to_string(),
            workspace_root: app.runtime.workspace.root.display().to_string(),
            agent_profile_id: "default".to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: None,
            created_at: 11,
            updated_at: 30,
        })
        .expect("put new session");
    store
        .put_run(&agent_persistence::RunRecord {
            id: "run-web-new".to_string(),
            session_id: "session-web-new".to_string(),
            mission_id: None,
            status: "running".to_string(),
            error: None,
            result: None,
            provider_usage_json: "{\"input_tokens\":12,\"output_tokens\":3,\"total_tokens\":15}"
                .to_string(),
            active_processes_json: "[]".to_string(),
            recent_steps_json: "[]".to_string(),
            evidence_refs_json: "[]".to_string(),
            pending_approvals_json: "[]".to_string(),
            provider_loop_json: "{}".to_string(),
            delegate_runs_json: "[]".to_string(),
            started_at: 32,
            updated_at: 33,
            finished_at: None,
        })
        .expect("put run");
    store
        .put_transcript(&TranscriptRecord {
            id: "transcript-web-new-user".to_string(),
            session_id: "session-web-new".to_string(),
            run_id: Some("run-web-new".to_string()),
            kind: "user".to_string(),
            content: "show web snapshot".to_string(),
            created_at: 31,
        })
        .expect("put transcript");
    store
        .put_tool_call(&ToolCallRecord {
            id: "toolcall-web-new".to_string(),
            session_id: "session-web-new".to_string(),
            run_id: "run-web-new".to_string(),
            provider_tool_call_id: "call_web_new".to_string(),
            tool_name: "web_fetch".to_string(),
            arguments_json:
                "{\"url\":\"https://example.com\",\"api_key\":\"zai-secret-token\"}".to_string(),
            summary:
                "web_fetch url=https://user:SW86Awtsx7CW@example.com Authorization: Bearer zai-secret-token"
                    .to_string(),
            status: "completed".to_string(),
            error: Some("provider rejected password=SW86Awtsx7CW".to_string()),
            result_summary: Some("status=200 token=zai-secret-token".to_string()),
            result_preview: Some("Example Domain".to_string()),
            result_artifact_id: None,
            result_truncated: false,
            result_byte_len: Some(14),
            requested_at: 34,
            updated_at: 35,
        })
        .expect("put tool call");
    store
        .put_task_registry(&TaskRegistryRecord {
            task_id: "task-web-new".to_string(),
            kind: "agent_task".to_string(),
            source_session_id: Some("session-web-new".to_string()),
            owner_agent_id: Some("default".to_string()),
            executor_agent_id: Some("judge".to_string()),
            parent_task_id: None,
            status: "running".to_string(),
            dependency_json: "{}".to_string(),
            context_ref_json: "{\"session_id\":\"session-web-new\"}".to_string(),
            result_ref_json: None,
            retry_policy_json: "{}".to_string(),
            attempt_count: 1,
            max_attempts: 3,
            timeout_at: None,
            chain_id: Some("chain-web-new".to_string()),
            hop_count: Some(1),
            max_hops: Some(3),
            trace_id: Some("trace-web-new".to_string()),
            created_at: 36,
            updated_at: 37,
            started_at: Some(36),
            finished_at: None,
            error: None,
        })
        .expect("put task");

    let handle = daemon::spawn_for_test(app).expect("spawn daemon");
    let client = Client::new();
    let response = client
        .get(format!("{base_url}/v1/web/snapshot"))
        .bearer_auth("secret-token")
        .send()
        .expect("snapshot response");

    assert_eq!(response.status(), StatusCode::OK);
    let snapshot: serde_json::Value = response.json().expect("snapshot json");
    assert_eq!(snapshot["status"]["ok"], true);
    assert_eq!(snapshot["sessions"][0]["id"], "session-web-new");
    assert_eq!(snapshot["recent_runs"][0]["id"], "run-web-new");
    assert_eq!(snapshot["recent_tasks"][0]["id"], "task-web-new");
    assert_eq!(snapshot["recent_tasks"][0]["executor_agent_id"], "judge");
    assert_eq!(snapshot["recent_tool_calls"][0]["tool_name"], "web_fetch");
    let rendered_tool_calls =
        serde_json::to_string(&snapshot["recent_tool_calls"]).expect("tool calls json");
    assert!(!rendered_tool_calls.contains("SW86Awtsx7CW"));
    assert!(!rendered_tool_calls.contains("zai-secret-token"));
    assert!(rendered_tool_calls.contains("<redacted>"));
    assert_eq!(snapshot["event_bus"]["backend"], "nats_jetstream");

    handle.stop().expect("stop daemon");
}

#[test]
fn daemon_http_sessions_can_filter_by_agent_profile() {
    let (_temp, app, base_url) = test_app(Some("secret-token"));
    let store = app.store().expect("open store");
    for (id, agent_profile_id, updated_at) in [
        ("session-default-old", "default", 20),
        ("session-judge", "judge", 30),
        ("session-default-new", "default", 40),
    ] {
        store
            .put_session(&SessionRecord {
                id: id.to_string(),
                title: id.to_string(),
                prompt_override: None,
                settings_json: "{}".to_string(),
                workspace_root: app.runtime.workspace.root.display().to_string(),
                agent_profile_id: agent_profile_id.to_string(),
                active_mission_id: None,
                parent_session_id: None,
                parent_job_id: None,
                delegation_label: None,
                created_at: updated_at - 10,
                updated_at,
            })
            .expect("put session");
    }

    let handle = daemon::spawn_for_test(app).expect("spawn daemon");
    let client = Client::new();
    let response = client
        .get(format!(
            "{base_url}/v1/sessions?agent_profile_id=default&limit=1&offset=1"
        ))
        .bearer_auth("secret-token")
        .send()
        .expect("sessions response");

    assert_eq!(response.status(), StatusCode::OK);
    let sessions: Vec<SessionSummaryResponse> = response.json().expect("sessions json");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, "session-default-old");
    assert_eq!(sessions[0].agent_profile_id, "default");

    handle.stop().expect("stop daemon");
}

#[test]
fn daemon_http_can_create_a_session_over_json() {
    let (_temp, app, base_url) = test_app(Some("secret-token"));
    let handle = daemon::spawn_for_test(app).expect("spawn daemon");
    let client = Client::new();

    let response = client
        .post(format!("{base_url}/v1/sessions"))
        .bearer_auth("secret-token")
        .json(&CreateSessionRequest {
            id: None,
            title: Some("Daemon Session".to_string()),
            agent_identifier: None,
        })
        .send()
        .expect("create session");

    assert_eq!(response.status(), StatusCode::CREATED);
    let session: SessionSummaryResponse = response.json().expect("session json");
    assert_eq!(session.title, "Daemon Session");
    assert_eq!(session.message_count, 0);
    assert!(!session.id.is_empty());

    handle.stop().expect("stop daemon");
}

#[test]
fn daemon_http_can_create_a_session_for_agent_profile() {
    let (_temp, app, base_url) = test_app(Some("secret-token"));
    let handle = daemon::spawn_for_test(app).expect("spawn daemon");
    let client = Client::new();

    let response = client
        .post(format!("{base_url}/v1/sessions"))
        .bearer_auth("secret-token")
        .json(&CreateSessionRequest {
            id: None,
            title: Some("Judge Session".to_string()),
            agent_identifier: Some("judge".to_string()),
        })
        .send()
        .expect("create judge session");

    assert_eq!(response.status(), StatusCode::CREATED);
    let session: SessionSummaryResponse = response.json().expect("session json");
    assert_eq!(session.title, "Judge Session");
    assert_eq!(session.agent_profile_id, "judge");
    assert_eq!(session.agent_name, "Judge");

    handle.stop().expect("stop daemon");
}

#[test]
fn daemon_http_can_page_sessions_with_query_params() {
    let (_temp, app, base_url) = test_app(Some("secret-token"));
    let store = app.store().expect("open store");
    for (index, updated_at) in [10_i64, 30, 20].into_iter().enumerate() {
        store
            .put_session(&SessionRecord {
                id: format!("session-page-{index}"),
                title: format!("Paged {index}"),
                prompt_override: None,
                settings_json: "{}".to_string(),
                workspace_root: app.runtime.workspace.root.display().to_string(),
                agent_profile_id: "default".to_string(),
                active_mission_id: None,
                parent_session_id: None,
                parent_job_id: None,
                delegation_label: None,
                created_at: updated_at,
                updated_at,
            })
            .expect("put session");
    }

    let handle = daemon::spawn_for_test(app).expect("spawn daemon");
    let client = Client::new();
    let response = client
        .get(format!("{base_url}/v1/sessions?limit=2&offset=1"))
        .bearer_auth("secret-token")
        .send()
        .expect("list sessions");

    assert_eq!(response.status(), StatusCode::OK);
    let sessions: Vec<SessionSummaryResponse> = response.json().expect("sessions json");
    assert_eq!(
        sessions
            .iter()
            .map(|session| session.id.as_str())
            .collect::<Vec<_>>(),
        vec!["session-page-2", "session-page-0"]
    );

    handle.stop().expect("stop daemon");
}

#[test]
fn daemon_http_can_render_session_debug_view() {
    let (_temp, app, base_url) = test_app(Some("secret-token"));
    let store = app.store().expect("open store");
    store
        .put_session(&SessionRecord {
            id: "session-debug".to_string(),
            title: "Debug".to_string(),
            prompt_override: None,
            settings_json: "{}".to_string(),
            workspace_root: app.runtime.workspace.root.display().to_string(),
            agent_profile_id: "default".to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: None,
            created_at: 1,
            updated_at: 2,
        })
        .expect("put session");
    store
        .put_transcript(&TranscriptRecord {
            id: "transcript-debug-1".to_string(),
            session_id: "session-debug".to_string(),
            run_id: None,
            kind: "user".to_string(),
            content: "debug me".to_string(),
            created_at: 2,
        })
        .expect("put transcript");

    let handle = daemon::spawn_for_test(app).expect("spawn daemon");
    let client = Client::new();
    let response = client
        .get(format!("{base_url}/v1/sessions/session-debug/debug"))
        .bearer_auth("secret-token")
        .send()
        .expect("debug view response");

    assert_eq!(response.status(), StatusCode::OK);
    let debug: SessionDebugResponse = response.json().expect("debug json");
    assert_eq!(debug.session_id, "session-debug");
    assert_eq!(debug.entries.len(), 1);
    assert_eq!(debug.entries[0].kind, "message");
    assert!(debug.entries[0].detail.contains("debug me"));

    handle.stop().expect("stop daemon");
}

#[test]
fn daemon_http_exposes_session_workspace_and_artifact_files() {
    let (temp, app, base_url) = test_app(Some("secret-token"));
    let workspace_root = temp.path().join("session-workspace");
    std::fs::create_dir_all(workspace_root.join("docs")).expect("create docs");
    std::fs::write(workspace_root.join("docs/report.txt"), "workspace report\n")
        .expect("write workspace file");
    std::fs::write(workspace_root.join("binary.bin"), [0, 159, 146, 150])
        .expect("write binary file");

    let store = app.store().expect("open store");
    store
        .put_session(&SessionRecord {
            id: "session-files".to_string(),
            title: "Files".to_string(),
            prompt_override: None,
            settings_json: "{}".to_string(),
            workspace_root: workspace_root.display().to_string(),
            agent_profile_id: "default".to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: None,
            created_at: 1,
            updated_at: 2,
        })
        .expect("put session");
    store
        .put_artifact(&ArtifactRecord {
            id: "artifact-http-1".to_string(),
            session_id: "session-files".to_string(),
            kind: "workspace_file".to_string(),
            metadata_json: r#"{"workspace_path":"docs/report.txt"}"#.to_string(),
            path: std::path::PathBuf::from("artifacts/artifact-http-1.bin"),
            bytes: b"artifact bytes\n".to_vec(),
            created_at: 3,
        })
        .expect("put artifact");

    let handle = daemon::spawn_for_test(app).expect("spawn daemon");
    let client = Client::new();

    let list_response = client
        .get(format!(
            "{base_url}/v1/sessions/session-files/workspace/list?path=docs"
        ))
        .bearer_auth("secret-token")
        .send()
        .expect("workspace list response");
    assert_eq!(list_response.status(), StatusCode::OK);
    let list: SessionWorkspaceListResponse = list_response.json().expect("workspace list json");
    assert_eq!(list.entries.len(), 1);
    assert_eq!(list.entries[0].path, "docs/report.txt");

    let read_response = client
        .get(format!(
            "{base_url}/v1/sessions/session-files/workspace/read?path=docs/report.txt"
        ))
        .bearer_auth("secret-token")
        .send()
        .expect("workspace read response");
    assert_eq!(read_response.status(), StatusCode::OK);
    let read: SessionWorkspaceFileResponse = read_response.json().expect("workspace read json");
    assert_eq!(read.content.as_deref(), Some("workspace report\n"));
    assert!(read.text);

    let download = client
        .get(format!(
            "{base_url}/v1/sessions/session-files/workspace/download?path=docs/report.txt"
        ))
        .bearer_auth("secret-token")
        .send()
        .expect("workspace download response");
    assert_eq!(download.status(), StatusCode::OK);
    assert_eq!(
        download.bytes().expect("download bytes").as_ref(),
        b"workspace report\n"
    );

    let artifacts_response = client
        .get(format!(
            "{base_url}/v1/sessions/session-files/artifact-files"
        ))
        .bearer_auth("secret-token")
        .send()
        .expect("artifact list response");
    assert_eq!(artifacts_response.status(), StatusCode::OK);
    let artifacts: SessionArtifactFilesResponse =
        artifacts_response.json().expect("artifact list json");
    assert_eq!(artifacts.artifacts.len(), 1);
    assert_eq!(artifacts.artifacts[0].id, "artifact-http-1");

    let artifact_response = client
        .get(format!(
            "{base_url}/v1/sessions/session-files/artifact-files/artifact-http-1"
        ))
        .bearer_auth("secret-token")
        .send()
        .expect("artifact read response");
    assert_eq!(artifact_response.status(), StatusCode::OK);
    let artifact: SessionArtifactFileResponse = artifact_response.json().expect("artifact json");
    assert_eq!(artifact.content.as_deref(), Some("artifact bytes\n"));

    let artifact_download = client
        .get(format!(
            "{base_url}/v1/sessions/session-files/artifact-files/artifact-http-1/download"
        ))
        .bearer_auth("secret-token")
        .send()
        .expect("artifact download response");
    assert_eq!(artifact_download.status(), StatusCode::OK);
    assert_eq!(
        artifact_download.bytes().expect("artifact bytes").as_ref(),
        b"artifact bytes\n"
    );

    handle.stop().expect("stop daemon");
}

#[test]
fn daemon_http_can_mutate_session_workspace_files() {
    let (temp, app, base_url) = test_app(Some("secret-token"));
    let workspace_root = temp.path().join("mutable-workspace");
    std::fs::create_dir_all(&workspace_root).expect("create workspace");

    let store = app.store().expect("open store");
    store
        .put_session(&SessionRecord {
            id: "session-mutable-files".to_string(),
            title: "Mutable Files".to_string(),
            prompt_override: None,
            settings_json: "{}".to_string(),
            workspace_root: workspace_root.display().to_string(),
            agent_profile_id: "default".to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: None,
            created_at: 1,
            updated_at: 2,
        })
        .expect("put session");

    let handle = daemon::spawn_for_test(app).expect("spawn daemon");
    let client = Client::new();

    let create_response = client
        .post(format!(
            "{base_url}/v1/sessions/session-mutable-files/workspace/write"
        ))
        .bearer_auth("secret-token")
        .json(&serde_json::json!({
            "path": "notes/new.md",
            "content": "hello\n",
            "mode": "create"
        }))
        .send()
        .expect("workspace write response");
    assert_eq!(create_response.status(), StatusCode::OK);
    assert_eq!(
        std::fs::read_to_string(workspace_root.join("notes/new.md")).expect("read created file"),
        "hello\n"
    );

    let overwrite_response = client
        .post(format!(
            "{base_url}/v1/sessions/session-mutable-files/workspace/write"
        ))
        .bearer_auth("secret-token")
        .json(&serde_json::json!({
            "path": "notes/new.md",
            "content": "updated\n",
            "mode": "overwrite"
        }))
        .send()
        .expect("workspace overwrite response");
    assert_eq!(overwrite_response.status(), StatusCode::OK);
    assert_eq!(
        std::fs::read_to_string(workspace_root.join("notes/new.md"))
            .expect("read overwritten file"),
        "updated\n"
    );

    let mkdir_response = client
        .post(format!(
            "{base_url}/v1/sessions/session-mutable-files/workspace/mkdir"
        ))
        .bearer_auth("secret-token")
        .json(&serde_json::json!({ "path": "drafts" }))
        .send()
        .expect("workspace mkdir response");
    assert_eq!(mkdir_response.status(), StatusCode::OK);
    assert!(workspace_root.join("drafts").is_dir());

    let trash_response = client
        .post(format!(
            "{base_url}/v1/sessions/session-mutable-files/workspace/trash"
        ))
        .bearer_auth("secret-token")
        .json(&serde_json::json!({ "path": "notes/new.md" }))
        .send()
        .expect("workspace trash response");
    assert_eq!(trash_response.status(), StatusCode::OK);
    assert!(!workspace_root.join("notes/new.md").exists());
    assert!(workspace_root.join(".trash").is_dir());

    handle.stop().expect("stop daemon");
}

#[test]
fn daemon_http_can_render_and_cancel_task_registry_entries() {
    let (_temp, app, base_url) = test_app(Some("secret-token"));
    let session = app
        .create_session_auto(Some("Task HTTP Session"))
        .expect("create session");
    let store = app.store().expect("open store");
    store
        .put_task_registry(&TaskRegistryRecord {
            task_id: "task-http-1".to_string(),
            kind: "agent_task".to_string(),
            status: "queued".to_string(),
            source_session_id: Some(session.id),
            owner_agent_id: Some("default".to_string()),
            executor_agent_id: Some("judge".to_string()),
            parent_task_id: None,
            dependency_json: "[]".to_string(),
            context_ref_json: r#"{"goal":"http task"}"#.to_string(),
            result_ref_json: None,
            retry_policy_json: r#"{"max_attempts":1}"#.to_string(),
            attempt_count: 0,
            max_attempts: 1,
            timeout_at: None,
            chain_id: None,
            hop_count: None,
            max_hops: None,
            trace_id: None,
            created_at: 10,
            updated_at: 20,
            started_at: None,
            finished_at: None,
            error: None,
        })
        .expect("put task");

    let handle = daemon::spawn_for_test(app).expect("spawn daemon");
    let client = Client::new();
    let rendered = client
        .get(format!("{base_url}/v1/tasks/task-http-1"))
        .bearer_auth("secret-token")
        .send()
        .expect("task response");

    assert_eq!(rendered.status(), StatusCode::OK);
    let rendered: TaskRenderResponse = rendered.json().expect("task json");
    assert!(rendered.task.contains("executor_agent_id: judge"));

    let cancelled = client
        .post(format!("{base_url}/v1/tasks/task-http-1/cancel"))
        .bearer_auth("secret-token")
        .json(&serde_json::json!({}))
        .send()
        .expect("cancel task response");

    assert_eq!(cancelled.status(), StatusCode::OK);
    let cancelled: TaskControlResponse = cancelled.json().expect("cancel json");
    assert_eq!(cancelled.message, "cancelled task-http-1");

    handle.stop().expect("stop daemon");
}

#[test]
fn daemon_http_can_manage_mcp_connector_lifecycle() {
    let (_temp, app, base_url) = test_app(Some("secret-token"));
    let handle = daemon::spawn_for_test(app).expect("spawn daemon");
    let client = Client::new();

    let created = client
        .post(format!("{base_url}/v1/mcp/connectors"))
        .bearer_auth("secret-token")
        .json(&McpConnectorCreateRequest {
            id: "filesystem".to_string(),
            options: bootstrap::McpConnectorCreateOptions {
                transport: agent_runtime::mcp::McpConnectorTransport::Stdio,
                command: "npx".to_string(),
                args: vec![
                    "-y".to_string(),
                    "@modelcontextprotocol/server-filesystem".to_string(),
                    "/workspace".to_string(),
                ],
                env: BTreeMap::new(),
                cwd: None,
                enabled: false,
            },
        })
        .send()
        .expect("create mcp connector");

    assert_eq!(created.status(), StatusCode::CREATED);
    let created: McpConnectorDetailResponse = created.json().expect("created json");
    assert_eq!(created.connector.id, "filesystem");
    assert!(!created.connector.enabled);
    assert_eq!(created.connector.runtime.state.as_str(), "stopped");

    let list = client
        .get(format!("{base_url}/v1/mcp/connectors"))
        .bearer_auth("secret-token")
        .send()
        .expect("list mcp connectors");
    assert_eq!(list.status(), StatusCode::OK);
    let list: Vec<bootstrap::McpConnectorView> = list.json().expect("list json");
    assert_eq!(list.len(), 1);

    let resolved = client
        .get(format!("{base_url}/v1/mcp/connectors/filesystem"))
        .bearer_auth("secret-token")
        .send()
        .expect("get mcp connector");
    assert_eq!(resolved.status(), StatusCode::OK);
    let resolved: McpConnectorDetailResponse = resolved.json().expect("resolved json");
    assert_eq!(resolved.connector.command, "npx");

    let updated = client
        .patch(format!("{base_url}/v1/mcp/connectors/filesystem"))
        .bearer_auth("secret-token")
        .json(&McpConnectorUpdateRequest {
            patch: bootstrap::McpConnectorUpdatePatch {
                command: Some("uvx".to_string()),
                args: Some(vec!["mcp-server-filesystem".to_string()]),
                env: None,
                cwd: Some(Some("/srv/mcp".to_string())),
                enabled: Some(false),
            },
        })
        .send()
        .expect("update mcp connector");
    assert_eq!(updated.status(), StatusCode::OK);
    let updated: McpConnectorDetailResponse = updated.json().expect("updated json");
    assert_eq!(updated.connector.command, "uvx");
    assert_eq!(updated.connector.cwd.as_deref(), Some("/srv/mcp"));
    assert_eq!(updated.connector.runtime.state.as_str(), "stopped");

    let restarted = client
        .post(format!("{base_url}/v1/mcp/connectors/filesystem/restart"))
        .bearer_auth("secret-token")
        .json(&())
        .send()
        .expect("restart mcp connector");
    assert_eq!(restarted.status(), StatusCode::OK);
    let restarted: McpConnectorDetailResponse = restarted.json().expect("restart json");
    assert_eq!(restarted.connector.runtime.state.as_str(), "stopped");

    let deleted = client
        .delete(format!("{base_url}/v1/mcp/connectors/filesystem"))
        .bearer_auth("secret-token")
        .send()
        .expect("delete mcp connector");
    assert_eq!(deleted.status(), StatusCode::OK);

    let missing = client
        .get(format!("{base_url}/v1/mcp/connectors/filesystem"))
        .bearer_auth("secret-token")
        .send()
        .expect("get missing mcp connector");
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);

    handle.stop().expect("stop daemon");
}

#[test]
fn daemon_http_exposes_memory_render_routes() {
    let (_temp, app, base_url) = test_app(Some("secret-token"));
    let knowledge_path = "docs/http-memory-fixture.md";
    let knowledge_absolute = app.runtime.workspace.root.join(knowledge_path);
    std::fs::create_dir_all(
        knowledge_absolute
            .parent()
            .expect("fixture parent directory"),
    )
    .expect("create fixture dir");
    std::fs::write(
        &knowledge_absolute,
        "# HTTP memory fixture\nmemory foundation fixture\n",
    )
    .expect("write knowledge fixture");
    let session = app
        .create_session_auto(Some("Memory Search Session"))
        .expect("create session");
    let store = PersistenceStore::open(&app.persistence).expect("open store");
    store
        .put_transcript(&agent_persistence::TranscriptRecord {
            id: "memory-msg-1".to_string(),
            session_id: session.id.clone(),
            run_id: None,
            kind: "user".to_string(),
            content: "offline adet install notes".to_string(),
            created_at: 2,
        })
        .expect("put transcript");
    let handle = daemon::spawn_for_test(app).expect("spawn daemon");
    let client = Client::new();

    let session_search = client
        .post(format!("{base_url}/v1/memory/session-search"))
        .bearer_auth("secret-token")
        .json(&SessionSearchInput {
            query: "offline adet".to_string(),
            limit: None,
            offset: Some(0),
            tiers: None,
            agent_identifier: None,
            updated_after: None,
            updated_before: None,
        })
        .send()
        .expect("session search");
    assert_eq!(session_search.status(), StatusCode::OK);
    let session_search: MemoryRenderResponse = session_search.json().expect("memory json");
    assert!(session_search.memory.contains("Память сессий:"));
    assert!(session_search.memory.contains(&session.id));

    let session_read = client
        .post(format!("{base_url}/v1/memory/session-read"))
        .bearer_auth("secret-token")
        .json(&SessionReadInput {
            session_id: session.id.clone(),
            mode: Some(SessionReadMode::Transcript),
            cursor: None,
            max_items: None,
            max_bytes: None,
            include_tools: Some(true),
        })
        .send()
        .expect("session read");
    assert_eq!(session_read.status(), StatusCode::OK);
    let session_read: MemoryRenderResponse = session_read.json().expect("memory json");
    assert!(session_read.memory.contains("Память сессии:"));
    assert!(session_read.memory.contains("offline adet install notes"));

    let knowledge_search = client
        .post(format!("{base_url}/v1/memory/knowledge-search"))
        .bearer_auth("secret-token")
        .json(&KnowledgeSearchInput {
            query: "memory foundation fixture".to_string(),
            limit: None,
            offset: Some(0),
            kinds: None,
            roots: None,
        })
        .send()
        .expect("knowledge search");
    assert_eq!(knowledge_search.status(), StatusCode::OK);
    let knowledge_search: MemoryRenderResponse = knowledge_search.json().expect("memory json");
    assert!(knowledge_search.memory.contains("Память знаний:"));
    assert!(knowledge_search.memory.contains(knowledge_path));

    let knowledge_read = client
        .post(format!("{base_url}/v1/memory/knowledge-read"))
        .bearer_auth("secret-token")
        .json(&KnowledgeReadInput {
            path: knowledge_path.to_string(),
            mode: Some(KnowledgeReadMode::Excerpt),
            cursor: None,
            max_bytes: None,
            max_lines: None,
        })
        .send()
        .expect("knowledge read");
    assert_eq!(knowledge_read.status(), StatusCode::OK);
    let knowledge_read: MemoryRenderResponse = knowledge_read.json().expect("memory json");
    assert!(knowledge_read.memory.contains("Файл знаний:"));
    assert!(
        knowledge_read
            .memory
            .contains(&format!("path={knowledge_path}"))
    );

    handle.stop().expect("stop daemon");
    let _ = std::fs::remove_file(knowledge_absolute);
}

#[test]
fn daemon_http_exposes_diagnostics_tail_route() {
    let (_temp, app, base_url) = test_app(Some("secret-token"));
    app.persistence
        .audit
        .append_event(&DiagnosticEvent::new(
            "info",
            "test",
            "daemon_http.diagnostics",
            "diagnostics tail fixture",
            app.config.data_dir.display().to_string(),
        ))
        .expect("append diagnostic event");
    let handle = daemon::spawn_for_test(app).expect("spawn daemon");
    let client = Client::new();

    let response = client
        .post(format!("{base_url}/v1/diagnostics/tail"))
        .bearer_auth("secret-token")
        .json(&DiagnosticsTailRequest { max_lines: Some(1) })
        .send()
        .expect("diagnostics tail request");

    assert_eq!(response.status(), StatusCode::OK);
    let body: DiagnosticsTailResponse = response.json().expect("diagnostics tail json");
    assert!(body.diagnostics.contains("diagnostics tail fixture"));

    handle.stop().expect("stop daemon");
}

#[test]
fn daemon_http_lists_and_updates_session_skills() {
    let temp = tempfile::tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(skills_dir.join("rust-debug")).expect("rust skill dir");
    std::fs::write(
        skills_dir.join("rust-debug").join("SKILL.md"),
        "---\nname: rust-debug\ndescription: Debug Rust compiler errors and cargo regressions.\n---\n\n# rust-debug\n",
    )
    .expect("write skill");

    let mut config = AppConfig {
        data_dir: temp.path().join("teamd-state"),
        ..AppConfig::default()
    };
    config.daemon.bind_host = "127.0.0.1".to_string();
    config.daemon.bind_port = free_port();
    config.daemon.bearer_token = Some("secret-token".to_string());
    config.daemon.skills_dir = skills_dir;
    let base_url = format!(
        "http://{}:{}",
        config.daemon.bind_host, config.daemon.bind_port
    );
    let app = bootstrap::build_from_config(config).expect("build app");
    let session = app
        .create_session_auto(Some("Daemon Skill Session"))
        .expect("create session");
    let handle = daemon::spawn_for_test(app).expect("spawn daemon");
    let client = Client::new();

    let listed = client
        .get(format!("{base_url}/v1/sessions/{}/skills", session.id))
        .bearer_auth("secret-token")
        .send()
        .expect("list skills");
    assert_eq!(listed.status(), StatusCode::OK);
    let listed: Vec<bootstrap::SessionSkillStatus> = listed.json().expect("skills json");
    let rust_debug = listed
        .iter()
        .find(|skill| skill.name == "rust-debug")
        .expect("rust-debug listed");
    assert_eq!(rust_debug.mode, "inactive");

    let enabled = client
        .post(format!(
            "{base_url}/v1/sessions/{}/skills/enable",
            session.id
        ))
        .bearer_auth("secret-token")
        .json(&SkillCommandRequest {
            name: "rust-debug".to_string(),
        })
        .send()
        .expect("enable skill");
    assert_eq!(enabled.status(), StatusCode::OK);
    let enabled: Vec<bootstrap::SessionSkillStatus> = enabled.json().expect("enabled json");
    let rust_debug = enabled
        .iter()
        .find(|skill| skill.name == "rust-debug")
        .expect("rust-debug enabled");
    assert_eq!(rust_debug.mode, "manual");

    let disabled = client
        .post(format!(
            "{base_url}/v1/sessions/{}/skills/disable",
            session.id
        ))
        .bearer_auth("secret-token")
        .json(&SkillCommandRequest {
            name: "rust-debug".to_string(),
        })
        .send()
        .expect("disable skill");
    assert_eq!(disabled.status(), StatusCode::OK);
    let disabled: Vec<bootstrap::SessionSkillStatus> = disabled.json().expect("disabled json");
    let rust_debug = disabled
        .iter()
        .find(|skill| skill.name == "rust-debug")
        .expect("rust-debug disabled");
    assert_eq!(rust_debug.mode, "disabled");

    handle.stop().expect("stop daemon");
}

#[test]
fn daemon_http_stop_shuts_down_a_running_server() {
    let (_temp, app, base_url) = test_app(Some("secret-token"));
    let handle = daemon::spawn_for_test(app).expect("spawn daemon");
    let client = Client::new();

    let response = client
        .post(format!("{base_url}/v1/daemon/stop"))
        .bearer_auth("secret-token")
        .json(&serde_json::json!({}))
        .send()
        .expect("stop request");

    assert_eq!(response.status(), StatusCode::OK);
    let body: DaemonStopResponse = response.json().expect("stop json");
    assert!(body.stopping);

    thread::sleep(Duration::from_millis(250));

    let result = client
        .get(format!("{base_url}/v1/status"))
        .bearer_auth("secret-token")
        .send();
    assert!(result.is_err(), "daemon should stop answering status");

    handle.stop().expect("join stopped daemon");
}

#[test]
fn daemon_http_exposes_current_session_background_jobs_and_counts() {
    let (_temp, app, base_url) = test_app(Some("secret-token"));
    let session = app
        .create_session_auto(Some("Daemon Jobs Session"))
        .expect("create session");
    let store = PersistenceStore::open(&app.persistence).expect("open store");
    store
        .put_mission(&MissionRecord {
            id: "mission-daemon-jobs".to_string(),
            session_id: session.id.clone(),
            objective: "Watch jobs".to_string(),
            status: MissionStatus::Running.as_str().to_string(),
            execution_intent: MissionExecutionIntent::Autonomous.as_str().to_string(),
            schedule_json: serde_json::to_string(&MissionSchedule::once()).expect("schedule"),
            acceptance_json: "[]".to_string(),
            created_at: 1,
            updated_at: 1,
            completed_at: None,
        })
        .expect("put mission");
    store
        .put_job(&JobRecord {
            id: "job-daemon-queued".to_string(),
            session_id: session.id.clone(),
            mission_id: Some("mission-daemon-jobs".to_string()),
            run_id: None,
            parent_job_id: None,
            kind: "maintenance".to_string(),
            status: "queued".to_string(),
            input_json: Some(
                serde_json::to_string(&JobExecutionInput::Maintenance {
                    summary: "daemon queue".to_string(),
                })
                .expect("serialize input"),
            ),
            result_json: None,
            error: None,
            created_at: 2,
            updated_at: 2,
            started_at: None,
            finished_at: None,
            attempt_count: 0,
            max_attempts: 1,
            lease_owner: None,
            lease_expires_at: None,
            heartbeat_at: None,
            cancel_requested_at: None,
            last_progress_message: Some("queued via daemon".to_string()),
            callback_json: None,
            callback_sent_at: None,
        })
        .expect("put job");
    let handle = daemon::spawn_for_test(app).expect("spawn daemon");
    let client = Client::new();

    let summary = client
        .get(format!("{base_url}/v1/sessions/{}", session.id))
        .bearer_auth("secret-token")
        .send()
        .expect("summary request");
    assert_eq!(summary.status(), StatusCode::OK);
    let summary: SessionSummaryResponse = summary.json().expect("summary json");
    assert_eq!(summary.background_job_count, 1);
    assert_eq!(summary.queued_background_job_count, 1);

    let jobs = client
        .get(format!("{base_url}/v1/sessions/{}/jobs", session.id))
        .bearer_auth("secret-token")
        .send()
        .expect("jobs request");
    assert_eq!(jobs.status(), StatusCode::OK);
    let jobs: Vec<SessionBackgroundJobResponse> = jobs.json().expect("jobs json");
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].id, "job-daemon-queued");
    assert_eq!(jobs[0].status, "queued");
    assert_eq!(
        jobs[0].last_progress_message.as_deref(),
        Some("queued via daemon")
    );

    handle.stop().expect("stop daemon");
}

#[test]
fn daemon_a2a_remote_delegate_round_trip_wakes_parent_session() {
    let (provider_a_base, provider_a_requests, provider_a_handle) = spawn_json_server_sequence(vec![
        r#"{
                "id":"resp_parent_wake",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_parent_wake",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Родительская сессия получила удалённый вердикт и продолжила работу."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":20,"output_tokens":12,"total_tokens":32}
            }"#
        .to_string(),
    ]);
    let (provider_b_base, provider_b_requests, provider_b_handle) =
        spawn_json_server_sequence(vec![
            r#"{
                "id":"resp_remote_child",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_remote_child",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Судья проверил артефакты и замечаний не нашёл."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":18,"output_tokens":11,"total_tokens":29}
            }"#
            .to_string(),
        ]);

    let temp_a = tempfile::tempdir().expect("tempdir a");
    let temp_b = tempfile::tempdir().expect("tempdir b");
    let port_a = free_port();
    let port_b = free_port();

    let mut config_a = AppConfig {
        data_dir: temp_a.path().join("teamd-a"),
        ..AppConfig::default()
    };
    config_a.daemon.bind_host = "127.0.0.1".to_string();
    config_a.daemon.bind_port = port_a;
    config_a.daemon.bearer_token = Some("token-a".to_string());
    config_a.daemon.public_base_url = Some(format!("http://127.0.0.1:{port_a}"));
    config_a.daemon.a2a_peers = BTreeMap::from([(
        "judge".to_string(),
        A2APeerConfig {
            base_url: format!("http://127.0.0.1:{port_b}"),
            bearer_token: Some("token-b".to_string()),
        },
    )]);
    config_a.provider.kind = agent_runtime::provider::ProviderKind::OpenAiResponses;
    config_a.provider.api_base = Some(format!("{provider_a_base}/v1"));
    config_a.provider.api_key = Some("test-key-a".to_string());
    config_a.provider.default_model = Some("gpt-5.4".to_string());

    let mut config_b = AppConfig {
        data_dir: temp_b.path().join("teamd-b"),
        ..AppConfig::default()
    };
    config_b.daemon.bind_host = "127.0.0.1".to_string();
    config_b.daemon.bind_port = port_b;
    config_b.daemon.bearer_token = Some("token-b".to_string());
    config_b.provider.kind = agent_runtime::provider::ProviderKind::OpenAiResponses;
    config_b.provider.api_base = Some(format!("{provider_b_base}/v1"));
    config_b.provider.api_key = Some("test-key-b".to_string());
    config_b.provider.default_model = Some("gpt-5.4".to_string());

    let app_a = bootstrap::build_from_config(config_a).expect("build app a");
    let app_b = bootstrap::build_from_config(config_b).expect("build app b");

    let store_a = PersistenceStore::open(&app_a.persistence).expect("open store a");
    store_a
        .put_session(&agent_persistence::SessionRecord {
            id: "session-a2a-parent".to_string(),
            title: "A2A Parent".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(
                &agent_runtime::session::SessionSettings::default(),
            )
            .expect("serialize settings"),
            workspace_root: app_a.runtime.workspace.root.display().to_string(),
            agent_profile_id: "default".to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put parent session");
    let mut delegate_job = JobSpec::delegate(
        "job-a2a-parent",
        "session-a2a-parent",
        None,
        None,
        "judge-helper",
        "Проверь артефакты и верни короткий вердикт.",
        vec!["reports/judge.md".to_string()],
        agent_runtime::delegation::DelegateWriteScope::new(vec!["reports".to_string()])
            .expect("write scope"),
        "Короткий вердикт",
        "a2a:judge",
        2,
    );
    delegate_job.status = JobStatus::Running;
    delegate_job.updated_at = 2;
    store_a
        .put_job(&JobRecord::try_from(&delegate_job).expect("delegate job"))
        .expect("put delegate job");

    let handle_b = daemon::spawn_for_test(app_b.clone()).expect("spawn daemon b");
    let handle_a = daemon::spawn_for_test(app_a.clone()).expect("spawn daemon a");

    let mut completed = false;
    for _ in 0..80 {
        let poll_store_a = PersistenceStore::open(&app_a.persistence).expect("poll store a");
        let job = JobSpec::try_from(
            poll_store_a
                .get_job("job-a2a-parent")
                .expect("get parent job")
                .expect("parent job exists"),
        )
        .expect("restore parent job");
        let transcripts = poll_store_a
            .list_transcripts_for_session("session-a2a-parent")
            .expect("list parent transcripts");
        if job.status == JobStatus::Completed
            && transcripts.iter().any(|record| {
                record
                    .content
                    .contains("Родительская сессия получила удалённый вердикт")
            })
        {
            completed = true;
            if let Some(JobResult::Delegation {
                child_session_id,
                package,
            }) = job.result
            {
                assert_eq!(child_session_id, "session-a2a-job-a2a-parent");
                assert_eq!(
                    package.summary,
                    "Судья проверил артефакты и замечаний не нашёл."
                );
            } else {
                panic!("expected delegation result package");
            }
            let inbox = poll_store_a
                .list_session_inbox_events_for_session("session-a2a-parent")
                .expect("list parent inbox");
            assert_eq!(inbox.len(), 1);
            assert_eq!(inbox[0].kind, "delegation_result_ready");
            assert_eq!(inbox[0].status, "processed");
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }

    assert!(
        completed,
        "remote a2a delegation should round-trip and wake the parent session"
    );

    let store_b = PersistenceStore::open(&app_b.persistence).expect("open store b");
    let remote_session = store_b
        .get_session("session-a2a-job-a2a-parent")
        .expect("get remote session")
        .expect("remote session exists");
    assert_eq!(
        remote_session.parent_session_id.as_deref(),
        Some("session-a2a-parent")
    );
    assert_eq!(
        remote_session.parent_job_id.as_deref(),
        Some("job-a2a-parent")
    );

    let remote_job = JobSpec::try_from(
        store_b
            .get_job("job-a2a-job-a2a-parent")
            .expect("get remote job")
            .expect("remote job exists"),
    )
    .expect("restore remote job");
    assert_eq!(remote_job.status, JobStatus::Completed);
    assert!(remote_job.callback_sent_at.is_some());

    let parent_request = provider_a_requests
        .recv_timeout(Duration::from_secs(2))
        .expect("parent wake request");
    let remote_request = provider_b_requests
        .recv_timeout(Duration::from_secs(2))
        .expect("remote child request");
    assert!(parent_request.contains("POST /v1/responses HTTP/1.1"));
    assert!(remote_request.contains("POST /v1/responses HTTP/1.1"));

    handle_a.stop().expect("stop daemon a");
    handle_b.stop().expect("stop daemon b");
    provider_a_handle.join().expect("join provider a");
    provider_b_handle.join().expect("join provider b");
}

#[test]
fn daemon_background_worker_processes_queued_chat_jobs_and_wakes_session() {
    let (api_base, _requests, provider_handle) = spawn_json_server_sequence(vec![
        r#"{
                "id":"resp_daemon_bg_job",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_daemon_bg_job",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Background daemon job finished."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":12,"output_tokens":5,"total_tokens":17}
            }"#
        .to_string(),
        r#"{
                "id":"resp_daemon_wakeup",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_daemon_wakeup",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Daemon wake-up turn handled the background result."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":14,"output_tokens":7,"total_tokens":21}
            }"#
        .to_string(),
    ]);

    let temp = tempfile::tempdir().expect("tempdir");
    let mut config = AppConfig {
        data_dir: temp.path().join("teamd-state"),
        ..AppConfig::default()
    };
    config.daemon.bind_host = "127.0.0.1".to_string();
    config.daemon.bind_port = free_port();
    config.daemon.bearer_token = Some("secret-token".to_string());
    config.provider.kind = agent_runtime::provider::ProviderKind::OpenAiResponses;
    config.provider.api_base = Some(format!("{api_base}/v1"));
    config.provider.api_key = Some("test-key".to_string());
    config.provider.default_model = Some("gpt-5.4".to_string());
    let app = bootstrap::build_from_config(config).expect("build app");
    let scaffold = app.persistence.clone();
    let session = app
        .create_session_auto(Some("Daemon Background Worker"))
        .expect("create session");
    let store = PersistenceStore::open(&scaffold).expect("open store");
    store
        .put_job(
            &JobRecord::try_from(&agent_runtime::mission::JobSpec::chat_turn(
                "job-daemon-bg-chat",
                session.id.as_str(),
                None,
                None,
                "Complete this in the daemon background",
                2,
            ))
            .expect("job record"),
        )
        .expect("put job");

    let handle = daemon::spawn_for_test(app).expect("spawn daemon");

    let mut completed = false;
    for _ in 0..180 {
        let poll_store = PersistenceStore::open(&scaffold).expect("reopen store");
        let job = poll_store
            .get_job("job-daemon-bg-chat")
            .expect("get job")
            .expect("job exists");
        let transcripts = poll_store
            .list_transcripts_for_session(&session.id)
            .expect("list transcripts");
        if job.status == "completed" && transcripts.len() >= 4 {
            completed = true;
            assert_eq!(transcripts[2].kind, "system");
            assert!(transcripts[2].content.contains("background job completed"));
            assert_eq!(
                transcripts[3].content,
                "Daemon wake-up turn handled the background result."
            );
            let inbox = poll_store
                .list_session_inbox_events_for_session(&session.id)
                .expect("list inbox");
            assert_eq!(inbox.len(), 1);
            assert_eq!(inbox[0].status, "processed");
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }

    assert!(completed, "daemon should process the queued background job");

    handle.stop().expect("stop daemon");
    provider_handle.join().expect("join provider");
}
