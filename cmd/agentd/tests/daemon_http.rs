use agent_persistence::{
    A2APeerConfig, AppConfig, JobRecord, JobRepository, MissionRecord, MissionRepository,
    PersistenceStore, SessionInboxRepository, SessionRecord, SessionRepository, TranscriptRecord,
    TranscriptRepository, audit::DiagnosticEvent,
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
    A2ACallbackTargetRequest, A2ADelegationAcceptedResponse, A2ADelegationCompletionOutcomeRequest,
    A2ADelegationCompletionRequest, A2ADelegationCreateRequest, CreateSessionRequest,
    DaemonStopResponse, DiagnosticsTailRequest, DiagnosticsTailResponse, ErrorResponse,
    McpConnectorCreateRequest, McpConnectorDetailResponse, McpConnectorUpdateRequest,
    MemoryRenderResponse, SessionBackgroundJobResponse, SessionDebugResponse,
    SessionSummaryResponse, SkillCommandRequest, StatusResponse,
};
use reqwest::StatusCode;
use reqwest::blocking::Client;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind ephemeral port")
        .local_addr()
        .expect("local addr")
        .port()
}

fn test_app(token: Option<&str>) -> (tempfile::TempDir, bootstrap::App, String) {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut config = AppConfig {
        data_dir: temp.path().join("teamd-state"),
        ..AppConfig::default()
    };
    config.daemon.bind_host = "127.0.0.1".to_string();
    config.daemon.bind_port = free_port();
    config.daemon.bearer_token = token.map(str::to_string);
    let base_url = format!(
        "http://{}:{}",
        config.daemon.bind_host, config.daemon.bind_port
    );
    let app = bootstrap::build_from_config(config).expect("build app");
    (temp, app, base_url)
}

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
fn daemon_http_can_render_session_debug_view() {
    let (_temp, app, base_url) = test_app(Some("secret-token"));
    let store = app.store().expect("open store");
    store
        .put_session(&SessionRecord {
            id: "session-debug".to_string(),
            title: "Debug".to_string(),
            prompt_override: None,
            settings_json: "{}".to_string(),
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
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].mode, "inactive");

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
    assert_eq!(enabled[0].mode, "manual");

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
    assert_eq!(disabled[0].mode, "disabled");

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
fn daemon_http_a2a_accepts_remote_delegation_and_creates_child_session_and_job() {
    let (_temp, app, base_url) = test_app(Some("secret-token"));
    let handle = daemon::spawn_for_test(app.clone()).expect("spawn daemon");
    let client = Client::new();

    let response = client
        .post(format!("{base_url}/v1/a2a/delegations"))
        .bearer_auth("secret-token")
        .json(&A2ADelegationCreateRequest {
            parent_session_id: "session-parent".to_string(),
            parent_job_id: "job-parent".to_string(),
            label: "judge".to_string(),
            goal: "Review the artifacts and return a verdict.".to_string(),
            bounded_context: vec!["reports/judge.md".to_string()],
            write_scope: agent_runtime::delegation::DelegateWriteScope::new(vec![
                "reports".to_string(),
            ])
            .expect("write scope"),
            expected_output: "Short verdict".to_string(),
            owner: "a2a:judge".to_string(),
            callback: A2ACallbackTargetRequest {
                url: "https://daemon-a.example/v1/a2a/delegations/job-parent/complete".to_string(),
                bearer_token: Some("callback-token".to_string()),
            },
            now: 10,
        })
        .send()
        .expect("create a2a delegation");

    assert_eq!(response.status(), StatusCode::CREATED);
    let accepted: A2ADelegationAcceptedResponse = response.json().expect("accepted json");
    assert_eq!(accepted.remote_session_id, "session-a2a-job-parent");
    assert_eq!(accepted.remote_job_id, "job-a2a-job-parent");

    let store = PersistenceStore::open(&app.persistence).expect("open store");
    let session = store
        .get_session("session-a2a-job-parent")
        .expect("get remote session")
        .expect("remote session exists");
    assert_eq!(session.parent_session_id.as_deref(), Some("session-parent"));
    assert_eq!(session.parent_job_id.as_deref(), Some("job-parent"));

    let job = JobSpec::try_from(
        store
            .get_job("job-a2a-job-parent")
            .expect("get remote job")
            .expect("remote job exists"),
    )
    .expect("restore remote job");
    assert_eq!(job.status, JobStatus::Running);
    assert!(job.callback.is_some());

    handle.stop().expect("stop daemon");
}

#[test]
fn daemon_http_a2a_completion_callback_updates_parent_job_and_queues_inbox_event() {
    let (_temp, app, base_url) = test_app(Some("secret-token"));
    let session = app
        .create_session("session-parent", "Parent")
        .expect("create parent");
    let store = PersistenceStore::open(&app.persistence).expect("open store");
    let mut job = JobSpec::delegate(
        "job-parent",
        &session.id,
        None,
        None,
        "judge",
        "Review the artifacts and return a verdict.",
        vec!["reports/judge.md".to_string()],
        agent_runtime::delegation::DelegateWriteScope::new(vec!["reports".to_string()])
            .expect("write scope"),
        "Short verdict",
        "a2a:judge",
        5,
    );
    job.status = JobStatus::WaitingExternal;
    store
        .put_job(&JobRecord::try_from(&job).expect("job record"))
        .expect("put parent job");

    let handle = daemon::spawn_for_test(app.clone()).expect("spawn daemon");
    let client = Client::new();
    let response = client
        .post(format!("{base_url}/v1/a2a/delegations/{}/complete", job.id))
        .bearer_auth("secret-token")
        .json(&A2ADelegationCompletionRequest {
            outcome: A2ADelegationCompletionOutcomeRequest::Completed {
                remote_session_id: "session-a2a-job-parent".to_string(),
                remote_job_id: "job-a2a-job-parent".to_string(),
                package: agent_runtime::delegation::DelegateResultPackage::new(
                    "Judge complete",
                    Vec::new(),
                    vec!["artifact-1".to_string()],
                    Vec::new(),
                )
                .expect("package"),
            },
            now: 20,
        })
        .send()
        .expect("complete remote delegation");

    assert_eq!(response.status(), StatusCode::OK);

    let job = JobSpec::try_from(
        store
            .get_job("job-parent")
            .expect("get updated job")
            .expect("updated job exists"),
    )
    .expect("restore updated job");
    assert_eq!(job.status, JobStatus::Completed);

    let inbox = store
        .list_session_inbox_events_for_session(&session.id)
        .expect("list inbox");
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].kind, "delegation_result_ready");

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
    for _ in 0..60 {
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

fn spawn_json_server_sequence(
    responses: Vec<String>,
) -> (String, Receiver<String>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let port = listener.local_addr().expect("local addr").port();
    let (sender, receiver) = mpsc::channel();

    let handle = thread::spawn(move || {
        for body in responses {
            let (mut stream, _) = listener.accept().expect("accept connection");
            let mut request = Vec::new();
            let mut buffer = [0_u8; 1024];

            loop {
                let bytes_read = stream.read(&mut buffer).expect("read request");
                if bytes_read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..bytes_read]);
                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }

            sender
                .send(String::from_utf8_lossy(&request).into_owned())
                .expect("send request");

            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("write response");
        }
    });

    (format!("http://127.0.0.1:{port}"), receiver, handle)
}
