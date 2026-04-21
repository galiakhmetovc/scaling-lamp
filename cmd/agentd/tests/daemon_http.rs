use agent_persistence::{
    AppConfig, JobRecord, JobRepository, MissionRecord, MissionRepository, PersistenceStore,
    SessionInboxRepository, TranscriptRepository,
};
use agent_runtime::mission::{
    JobExecutionInput, MissionExecutionIntent, MissionSchedule, MissionStatus,
};
use agentd::bootstrap;
use agentd::daemon;
use agentd::http::types::{
    CreateSessionRequest, DaemonStopResponse, ErrorResponse, SessionBackgroundJobResponse,
    SessionSummaryResponse, SkillCommandRequest, StatusResponse,
};
use reqwest::StatusCode;
use reqwest::blocking::Client;
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
