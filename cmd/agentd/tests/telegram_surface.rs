use agent_persistence::{
    AppConfig, ArtifactRecord, ArtifactRepository, DeliveryRepository, FileDeliveryRepository,
    FileDeliveryRequestRecord, SessionInboxRepository, TelegramRepository,
    TelegramUserPairingRecord, TranscriptRecord, TranscriptRepository,
};
use agent_runtime::provider::ProviderKind;
use agentd::bootstrap;
use agentd::bootstrap::{BootstrapError, SessionPreferencesPatch, SessionSummary};
use agentd::daemon;
use agentd::execution::{ChatExecutionEvent, ChatTurnExecutionReport};
use agentd::http::client::{DaemonClient, DaemonConnectOptions};
use agentd::telegram::backend::{DaemonTelegramBackend, TelegramAgentSummary, TelegramBackend};
use agentd::telegram::client::{TelegramClient, TelegramClientConfig, TelegramCommandSpec};
use agentd::telegram::render::{
    TELEGRAM_MESSAGE_TEXT_SOFT_CAP, chunk_message_text, render_session_list, truncate_caption,
};
use agentd::telegram::router::TelegramWorker;
use std::collections::{BTreeMap, VecDeque};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
struct CapturedRequest {
    method: String,
    path: String,
    body: String,
}

#[derive(Debug)]
struct FakeResponse {
    content_type: &'static str,
    body: Vec<u8>,
}

#[derive(Debug, Clone)]
struct RecordingTelegramBackend {
    state: Arc<Mutex<RecordingTelegramBackendState>>,
}

#[derive(Debug, Default)]
struct RecordingTelegramBackendState {
    listed_sessions: Vec<SessionSummary>,
    session_lookup: BTreeMap<String, SessionSummary>,
    agent_summaries: Vec<TelegramAgentSummary>,
    create_session_results: Vec<SessionSummary>,
    created_titles: Vec<Option<String>>,
    created_agent_identifiers: Vec<Option<String>>,
    updated_preferences: Vec<(String, SessionPreferencesPatch)>,
    executed_turns: Vec<(String, String)>,
    agent_messages: Vec<(String, String, String)>,
    active_run_status: String,
    background_jobs_status: String,
    session_tasks_status: String,
    plan_status: String,
    session_skills_status: String,
    cancelled_active_runs: Vec<String>,
    cancelled_all_work: Vec<String>,
    enabled_skills: Vec<(String, String)>,
    disabled_skills: Vec<(String, String)>,
    compacted_sessions: Vec<String>,
    next_agent_message_response: String,
    next_chat_output: String,
    response_delay_ms: u64,
    execution_events: Vec<(u64, ChatExecutionEvent)>,
    chat_turn_blocker: Option<ChatTurnBlocker>,
}

#[derive(Debug, Clone)]
struct ChatTurnBlocker {
    started: mpsc::Sender<()>,
    release: Arc<(Mutex<bool>, Condvar)>,
}

impl RecordingTelegramBackend {
    fn with_state(state: RecordingTelegramBackendState) -> Self {
        Self {
            state: Arc::new(Mutex::new(state)),
        }
    }

    fn state(&self) -> Arc<Mutex<RecordingTelegramBackendState>> {
        self.state.clone()
    }
}

impl Default for RecordingTelegramBackend {
    fn default() -> Self {
        Self::with_state(RecordingTelegramBackendState::default())
    }
}

impl TelegramBackend for RecordingTelegramBackend {
    fn list_agents(&self) -> Result<Vec<TelegramAgentSummary>, BootstrapError> {
        Ok(self
            .state
            .lock()
            .expect("backend state")
            .agent_summaries
            .clone())
    }

    fn resolve_agent(&self, identifier: &str) -> Result<TelegramAgentSummary, BootstrapError> {
        let state = self.state.lock().expect("backend state");
        state
            .agent_summaries
            .iter()
            .find(|agent| agent.id == identifier || agent.name.eq_ignore_ascii_case(identifier))
            .cloned()
            .ok_or_else(|| BootstrapError::MissingRecord {
                kind: "agent",
                id: identifier.to_string(),
            })
    }

    fn list_session_summaries(&self) -> Result<Vec<SessionSummary>, BootstrapError> {
        Ok(self
            .state
            .lock()
            .expect("backend state")
            .listed_sessions
            .clone())
    }

    fn create_session_for_agent(
        &self,
        title: Option<&str>,
        agent_identifier: Option<&str>,
    ) -> Result<SessionSummary, BootstrapError> {
        let mut state = self.state.lock().expect("backend state");
        state.created_titles.push(title.map(str::to_string));
        state
            .created_agent_identifiers
            .push(agent_identifier.map(str::to_string));
        let summary = state
            .create_session_results
            .first()
            .cloned()
            .ok_or_else(|| BootstrapError::Usage {
                reason: "missing fake create session result".to_string(),
            })?;
        state.create_session_results.remove(0);
        let mut summary = summary;
        if let Some(agent_identifier) = agent_identifier
            && let Some(agent) = state.agent_summaries.iter().find(|agent| {
                agent.id == agent_identifier || agent.name.eq_ignore_ascii_case(agent_identifier)
            })
        {
            summary.agent_profile_id = agent.id.clone();
            summary.agent_name = agent.name.clone();
        }
        state
            .session_lookup
            .insert(summary.id.clone(), summary.clone());
        state.listed_sessions.push(summary.clone());
        Ok(summary)
    }

    fn update_session_preferences(
        &self,
        session_id: &str,
        patch: SessionPreferencesPatch,
    ) -> Result<SessionSummary, BootstrapError> {
        let mut state = self.state.lock().expect("backend state");
        state
            .updated_preferences
            .push((session_id.to_string(), patch.clone()));
        let auto_approve = patch.auto_approve;
        let model = patch.model.clone();
        let reasoning_visible = patch.reasoning_visible;
        let think_level = patch.think_level.clone();
        let title = patch.title.clone();
        {
            let summary = state.session_lookup.get_mut(session_id).ok_or_else(|| {
                BootstrapError::MissingRecord {
                    kind: "session",
                    id: session_id.to_string(),
                }
            })?;
            if let Some(auto_approve) = auto_approve {
                summary.auto_approve = auto_approve;
            }
            if let Some(model) = model.clone() {
                summary.model = model;
            }
            if let Some(reasoning_visible) = reasoning_visible {
                summary.reasoning_visible = reasoning_visible;
            }
            if let Some(think_level) = think_level.clone() {
                summary.think_level = think_level;
            }
            if let Some(title) = title.clone() {
                summary.title = title;
            }
        }
        if let Some(listed) = state
            .listed_sessions
            .iter_mut()
            .find(|candidate| candidate.id == session_id)
        {
            if let Some(auto_approve) = auto_approve {
                listed.auto_approve = auto_approve;
            }
            if let Some(model) = model {
                listed.model = model;
            }
            if let Some(reasoning_visible) = reasoning_visible {
                listed.reasoning_visible = reasoning_visible;
            }
            if let Some(think_level) = think_level {
                listed.think_level = think_level;
            }
            if let Some(title) = title {
                listed.title = title;
            }
        }
        Ok(state
            .session_lookup
            .get(session_id)
            .cloned()
            .expect("updated session"))
    }

    fn session_summary(&self, session_id: &str) -> Result<SessionSummary, BootstrapError> {
        self.state
            .lock()
            .expect("backend state")
            .session_lookup
            .get(session_id)
            .cloned()
            .ok_or_else(|| BootstrapError::MissingRecord {
                kind: "session",
                id: session_id.to_string(),
            })
    }

    fn send_agent_message(
        &self,
        session_id: &str,
        target_agent_id: &str,
        message: &str,
    ) -> Result<String, BootstrapError> {
        let mut state = self.state.lock().expect("backend state");
        state.agent_messages.push((
            session_id.to_string(),
            target_agent_id.to_string(),
            message.to_string(),
        ));
        Ok(state.next_agent_message_response.clone())
    }

    fn render_active_run(&self, session_id: &str) -> Result<String, BootstrapError> {
        let state = self.state.lock().expect("backend state");
        if !state.session_lookup.contains_key(session_id) {
            return Err(BootstrapError::MissingRecord {
                kind: "session",
                id: session_id.to_string(),
            });
        }
        Ok(if state.active_run_status.is_empty() {
            "Ход: активного выполнения нет".to_string()
        } else {
            state.active_run_status.clone()
        })
    }

    fn cancel_active_run(&self, session_id: &str) -> Result<String, BootstrapError> {
        let mut state = self.state.lock().expect("backend state");
        state.cancelled_active_runs.push(session_id.to_string());
        Ok(format!("stopped {session_id}"))
    }

    fn cancel_all_session_work(&self, session_id: &str) -> Result<String, BootstrapError> {
        let mut state = self.state.lock().expect("backend state");
        state.cancelled_all_work.push(session_id.to_string());
        Ok(format!("cancelled {session_id}"))
    }

    fn render_session_background_jobs(&self, session_id: &str) -> Result<String, BootstrapError> {
        let state = self.state.lock().expect("backend state");
        if !state.session_lookup.contains_key(session_id) {
            return Err(BootstrapError::MissingRecord {
                kind: "session",
                id: session_id.to_string(),
            });
        }
        Ok(if state.background_jobs_status.is_empty() {
            "Задачи: активных нет".to_string()
        } else {
            state.background_jobs_status.clone()
        })
    }

    fn render_session_tasks(&self, session_id: &str) -> Result<String, BootstrapError> {
        let state = self.state.lock().expect("backend state");
        if !state.session_lookup.contains_key(session_id) {
            return Err(BootstrapError::MissingRecord {
                kind: "session",
                id: session_id.to_string(),
            });
        }
        Ok(if state.session_tasks_status.is_empty() {
            "Делегированные задачи: нет".to_string()
        } else {
            state.session_tasks_status.clone()
        })
    }

    fn render_plan(&self, session_id: &str) -> Result<String, BootstrapError> {
        let state = self.state.lock().expect("backend state");
        if !state.session_lookup.contains_key(session_id) {
            return Err(BootstrapError::MissingRecord {
                kind: "session",
                id: session_id.to_string(),
            });
        }
        Ok(if state.plan_status.is_empty() {
            "План: нет активного плана".to_string()
        } else {
            state.plan_status.clone()
        })
    }

    fn render_session_skills(&self, session_id: &str) -> Result<String, BootstrapError> {
        let state = self.state.lock().expect("backend state");
        if !state.session_lookup.contains_key(session_id) {
            return Err(BootstrapError::MissingRecord {
                kind: "session",
                id: session_id.to_string(),
            });
        }
        Ok(if state.session_skills_status.is_empty() {
            "Скиллы: ничего не найдено".to_string()
        } else {
            state.session_skills_status.clone()
        })
    }

    fn enable_session_skill(
        &self,
        session_id: &str,
        skill_name: &str,
    ) -> Result<String, BootstrapError> {
        let mut state = self.state.lock().expect("backend state");
        state
            .enabled_skills
            .push((session_id.to_string(), skill_name.to_string()));
        Ok(format!("enabled {skill_name}"))
    }

    fn disable_session_skill(
        &self,
        session_id: &str,
        skill_name: &str,
    ) -> Result<String, BootstrapError> {
        let mut state = self.state.lock().expect("backend state");
        state
            .disabled_skills
            .push((session_id.to_string(), skill_name.to_string()));
        Ok(format!("disabled {skill_name}"))
    }

    fn compact_session(&self, session_id: &str) -> Result<SessionSummary, BootstrapError> {
        let mut state = self.state.lock().expect("backend state");
        state.compacted_sessions.push(session_id.to_string());
        let summary = state.session_lookup.get_mut(session_id).ok_or_else(|| {
            BootstrapError::MissingRecord {
                kind: "session",
                id: session_id.to_string(),
            }
        })?;
        summary.compactifications += 1;
        Ok(summary.clone())
    }

    fn execute_chat_turn(
        &self,
        session_id: &str,
        message: &str,
        _now: i64,
        observer: &mut dyn FnMut(ChatExecutionEvent),
    ) -> Result<ChatTurnExecutionReport, BootstrapError> {
        let mut state = self.state.lock().expect("backend state");
        state
            .executed_turns
            .push((session_id.to_string(), message.to_string()));
        let response_delay_ms = state.response_delay_ms;
        let execution_events = state.execution_events.clone();
        let output_text = state.next_chat_output.clone();
        let chat_turn_blocker = state.chat_turn_blocker.clone();
        drop(state);
        if let Some(blocker) = chat_turn_blocker {
            let _ = blocker.started.send(());
            let (released, condvar) = &*blocker.release;
            let mut released = released.lock().expect("chat turn release lock");
            while !*released {
                released = condvar.wait(released).expect("chat turn release wait");
            }
        }
        for (delay_ms, event) in execution_events {
            thread::sleep(Duration::from_millis(delay_ms));
            observer(event);
        }
        if response_delay_ms > 0 {
            thread::sleep(Duration::from_millis(response_delay_ms));
        }
        Ok(ChatTurnExecutionReport {
            session_id: session_id.to_string(),
            run_id: "run-telegram-1".to_string(),
            response_id: "response-telegram-1".to_string(),
            output_text,
        })
    }
}

#[test]
fn telegram_client_polls_updates_from_custom_api_base() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (api_url, requests, handle) = spawn_fake_telegram_api(vec![json_response(
        r#"{"ok":true,"result":[{"update_id":11,"message":{"message_id":7,"date":0,"chat":{"id":42,"type":"private"},"text":"hello from telegram"}}]}"#,
    )]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");

    let updates = runtime
        .block_on(client.poll_updates(Some(11), 50, 30))
        .expect("poll updates");

    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].id.0, 11);
    let request = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured getUpdates");
    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/bottest-token/GetUpdates");
    assert!(request.body.contains("\"offset\":11"));
    assert!(request.body.contains("\"limit\":50"));
    assert!(request.body.contains("\"timeout\":30"));

    handle.join().expect("join fake api");
}

#[test]
fn telegram_client_sends_edits_and_registers_commands() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (api_url, requests, handle) = spawn_fake_telegram_api(vec![
        json_response(
            r#"{"ok":true,"result":{"message_id":100,"date":0,"chat":{"id":42,"type":"private"},"text":"hello from bot"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":100,"date":0,"chat":{"id":42,"type":"private"},"text":"edited text"}}"#,
        ),
        json_response(r#"{"ok":true,"result":true}"#),
    ]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");

    let sent = runtime
        .block_on(client.send_text(42, "hello from bot"))
        .expect("send text");
    assert_eq!(sent.id.0, 100);
    let edited = runtime
        .block_on(client.edit_text(42, 100, "edited text"))
        .expect("edit text");
    assert_eq!(edited.id.0, 100);
    runtime
        .block_on(client.register_commands(&[
            TelegramCommandSpec::new("help", "Show help"),
            TelegramCommandSpec::new("new", "Create session"),
        ]))
        .expect("register commands");

    let send_request = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured sendMessage");
    assert_eq!(send_request.path, "/bottest-token/SendMessage");
    assert!(send_request.body.contains("\"chat_id\":42"));
    assert!(send_request.body.contains("\"text\":\"hello from bot\""));

    let edit_request = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured editMessageText");
    assert_eq!(edit_request.path, "/bottest-token/EditMessageText");
    assert!(edit_request.body.contains("\"message_id\":100"));
    assert!(edit_request.body.contains("\"text\":\"edited text\""));

    let commands_request = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured setMyCommands");
    assert_eq!(commands_request.path, "/bottest-token/SetMyCommands");
    assert!(commands_request.body.contains("\"command\":\"help\""));
    assert!(
        commands_request
            .body
            .contains("\"description\":\"Show help\"")
    );
    assert!(commands_request.body.contains("\"command\":\"new\""));

    handle.join().expect("join fake api");
}

#[test]
fn telegram_client_sends_typing_and_deletes_messages() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (api_url, requests, handle) = spawn_fake_telegram_api(vec![
        json_response(r#"{"ok":true,"result":true}"#),
        json_response(r#"{"ok":true,"result":true}"#),
    ]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");

    runtime
        .block_on(client.send_typing(42))
        .expect("send typing");
    runtime
        .block_on(client.delete_message(42, 100))
        .expect("delete message");

    let typing_request = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured sendChatAction");
    assert_eq!(typing_request.path, "/bottest-token/SendChatAction");
    assert!(typing_request.body.contains("\"chat_id\":42"));
    assert!(typing_request.body.contains("\"action\":\"typing\""));

    let delete_request = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured deleteMessage");
    assert_eq!(delete_request.path, "/bottest-token/DeleteMessage");
    assert!(delete_request.body.contains("\"chat_id\":42"));
    assert!(delete_request.body.contains("\"message_id\":100"));

    handle.join().expect("join fake api");
}

#[test]
fn telegram_client_fetches_file_metadata_and_downloads_bytes() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (api_url, requests, handle) = spawn_fake_telegram_api(vec![
        json_response(
            r#"{"ok":true,"result":{"file_id":"file-1","file_unique_id":"uniq-1","file_path":"docs/file.txt"}}"#,
        ),
        FakeResponse {
            content_type: "application/octet-stream",
            body: b"fixture-bytes".to_vec(),
        },
    ]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");

    let file = runtime
        .block_on(client.get_file("file-1"))
        .expect("get file metadata");
    assert_eq!(file.meta.id.0, "file-1");
    assert_eq!(file.path, "docs/file.txt");

    let bytes = runtime
        .block_on(client.download_file("docs/file.txt"))
        .expect("download file bytes");
    assert_eq!(bytes, b"fixture-bytes");

    let metadata_request = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured getFile");
    assert_eq!(metadata_request.path, "/bottest-token/GetFile");
    assert!(metadata_request.body.contains("\"file_id\":\"file-1\""));

    let download_request = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured file download");
    assert_eq!(download_request.method, "GET");
    assert_eq!(download_request.path, "/file/bottest-token/docs%2Ffile.txt");

    handle.join().expect("join fake api");
}

#[test]
fn telegram_client_sends_document_from_memory_with_filename_and_caption() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (api_url, requests, handle) = spawn_fake_telegram_api(vec![json_response(
        r#"{"ok":true,"result":{"message_id":101,"date":0,"chat":{"id":42,"type":"private"},"document":{"file_id":"sent-file","file_unique_id":"sent-unique","file_size":12,"file_name":"report.txt","mime_type":"text/plain"}}}"#,
    )]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");

    let sent = runtime
        .block_on(client.send_document(42, b"report body".to_vec(), "report.txt", Some("caption")))
        .expect("send document");
    assert_eq!(sent.id.0, 101);

    let request = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured sendDocument");
    assert_eq!(request.path, "/bottest-token/SendDocument");
    assert!(request.body.contains("report.txt"));
    assert!(request.body.contains("caption"));

    handle.join().expect("join fake api");
}

#[test]
fn telegram_renderer_respects_text_and_caption_soft_caps() {
    let chunks = chunk_message_text(&"x".repeat(7_000), 3_276);
    assert_eq!(chunks.len(), 3);
    assert!(chunks.iter().all(|chunk| chunk.len() <= 3_276));

    let caption = truncate_caption(&"y".repeat(2_000), 819);
    assert_eq!(caption.len(), 819);
}

#[test]
fn telegram_worker_registers_default_commands() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app();
    let backend = RecordingTelegramBackend::default();
    let (api_url, requests, handle) =
        spawn_fake_telegram_api(vec![json_response(r#"{"ok":true,"result":true}"#)]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker = TelegramWorker::with_consumer(app, backend, client, "telegram-test");

    runtime
        .block_on(worker.register_commands())
        .expect("register commands");

    let request = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured setMyCommands");
    assert_eq!(request.path, "/bottest-token/SetMyCommands");
    assert!(request.body.contains("\"command\":\"start\""));
    assert!(request.body.contains("\"command\":\"help\""));
    assert!(request.body.contains("\"command\":\"new\""));
    assert!(request.body.contains("\"command\":\"sessions\""));
    assert!(request.body.contains("\"command\":\"use\""));
    assert!(request.body.contains("\"command\":\"status\""));
    assert!(request.body.contains("\"command\":\"stop\""));
    assert!(request.body.contains("\"command\":\"cancel\""));
    assert!(request.body.contains("\"command\":\"plan\""));
    assert!(request.body.contains("\"command\":\"model\""));
    assert!(request.body.contains("\"command\":\"think\""));
    assert!(request.body.contains("\"command\":\"autoapprove\""));
    assert!(
        request.body.contains("\"command\":\"skills\""),
        "setMyCommands body missing skills: {}",
        request.body
    );
    assert!(request.body.contains("\"command\":\"agents\""));
    assert!(request.body.contains("\"command\":\"agentuse\""));
    assert!(request.body.contains("\"command\":\"newagent\""));
    assert!(request.body.contains("\"command\":\"files\""));
    assert!(request.body.contains("\"command\":\"file\""));

    handle.join().expect("join fake api");
}

#[test]
fn telegram_worker_updates_session_preferences_from_operator_commands() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app();
    seed_activated_pairing(&app, "pair-operator-prefs", 777, 42);
    seed_binding(&app, 42, 777, Some("session-operator"));
    let backend = RecordingTelegramBackend::with_state(RecordingTelegramBackendState {
        session_lookup: BTreeMap::from([(
            "session-operator".to_string(),
            session_summary("session-operator", "Operator", true),
        )]),
        listed_sessions: vec![session_summary("session-operator", "Operator", true)],
        ..RecordingTelegramBackendState::default()
    });
    let backend_state = backend.state();
    let (api_url, requests, handle) = spawn_fake_telegram_api(vec![
        json_response(
            r#"{"ok":true,"result":[
                {"update_id":136,"message":{"message_id":101,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/model gpt-5.5"}},
                {"update_id":137,"message":{"message_id":102,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/think high"}},
                {"update_id":138,"message":{"message_id":103,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/reasoning off"}},
                {"update_id":139,"message":{"message_id":104,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/autoapprove off"}}
            ]}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":105,"date":0,"chat":{"id":42,"type":"private"},"text":"model"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":106,"date":0,"chat":{"id":42,"type":"private"},"text":"think"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":107,"date":0,"chat":{"id":42,"type":"private"},"text":"reasoning"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":108,"date":0,"chat":{"id":42,"type":"private"},"text":"autoapprove"}}"#,
        ),
    ]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker =
        TelegramWorker::with_consumer(app.clone(), backend.clone(), client, "telegram-test");

    let processed = runtime.block_on(worker.poll_once()).expect("poll once");
    assert_eq!(processed, 4);

    let state = backend_state.lock().expect("backend state");
    assert_eq!(
        state.updated_preferences,
        vec![
            (
                "session-operator".to_string(),
                SessionPreferencesPatch {
                    model: Some(Some("gpt-5.5".to_string())),
                    ..SessionPreferencesPatch::default()
                }
            ),
            (
                "session-operator".to_string(),
                SessionPreferencesPatch {
                    think_level: Some(Some("high".to_string())),
                    ..SessionPreferencesPatch::default()
                }
            ),
            (
                "session-operator".to_string(),
                SessionPreferencesPatch {
                    reasoning_visible: Some(false),
                    ..SessionPreferencesPatch::default()
                }
            ),
            (
                "session-operator".to_string(),
                SessionPreferencesPatch {
                    auto_approve: Some(false),
                    ..SessionPreferencesPatch::default()
                }
            ),
        ]
    );
    assert!(state.executed_turns.is_empty());
    drop(state);

    let _ = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured getUpdates");
    let model_response = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured model response");
    assert!(model_response.body.contains("Model: gpt-5.5"));
    let think_response = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured think response");
    assert!(think_response.body.contains("Think level: high"));
    let reasoning_response = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured reasoning response");
    assert!(reasoning_response.body.contains("Reasoning visible: false"));
    let autoapprove_response = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured autoapprove response");
    assert!(autoapprove_response.body.contains("Auto-approve: false"));

    handle.join().expect("join fake api");
}

#[test]
fn telegram_worker_routes_session_operator_commands_to_backend() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app();
    seed_activated_pairing(&app, "pair-operator", 777, 42);
    seed_binding(&app, 42, 777, Some("session-operator"));
    let mut summary = session_summary("session-operator", "Operator", true);
    summary.model = Some("gpt-5.5".to_string());
    summary.think_level = Some("off".to_string());
    summary.message_count = 7;
    let backend = RecordingTelegramBackend::with_state(RecordingTelegramBackendState {
        session_lookup: BTreeMap::from([("session-operator".to_string(), summary)]),
        active_run_status: "Ход:\n- статус: running".to_string(),
        background_jobs_status: "Задачи:\n- [running] job-1 (chat_turn)".to_string(),
        session_tasks_status: "Делегированные задачи:\n- [queued] task-agent-1 (agent_task)"
            .to_string(),
        plan_status: "План:\n- [in_progress] task-1: check".to_string(),
        session_skills_status: "Скиллы:\n- [manual] silverbullet-space: Notes".to_string(),
        ..RecordingTelegramBackendState::default()
    });
    let backend_state = backend.state();
    let (api_url, requests, handle) = spawn_fake_telegram_api(vec![
        json_response(
            r#"{"ok":true,"result":[
                {"update_id":140,"message":{"message_id":109,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/status"}},
                {"update_id":141,"message":{"message_id":110,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/lifecycle"}},
                {"update_id":142,"message":{"message_id":111,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/rename Renamed Operator"}},
                {"update_id":143,"message":{"message_id":112,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/jobs"}},
                {"update_id":144,"message":{"message_id":113,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/tasks"}},
                {"update_id":145,"message":{"message_id":114,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/plan"}},
                {"update_id":146,"message":{"message_id":115,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/stop"}},
                {"update_id":147,"message":{"message_id":116,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/cancel"}},
                {"update_id":148,"message":{"message_id":117,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/skills"}},
                {"update_id":149,"message":{"message_id":118,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/enable silverbullet-space"}},
                {"update_id":150,"message":{"message_id":119,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/disable silverbullet-space"}},
                {"update_id":151,"message":{"message_id":120,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/compact"}}
            ]}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":120,"date":0,"chat":{"id":42,"type":"private"},"text":"status"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":121,"date":0,"chat":{"id":42,"type":"private"},"text":"lifecycle"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":122,"date":0,"chat":{"id":42,"type":"private"},"text":"rename"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":123,"date":0,"chat":{"id":42,"type":"private"},"text":"jobs"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":124,"date":0,"chat":{"id":42,"type":"private"},"text":"tasks"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":125,"date":0,"chat":{"id":42,"type":"private"},"text":"plan"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":126,"date":0,"chat":{"id":42,"type":"private"},"text":"stop"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":127,"date":0,"chat":{"id":42,"type":"private"},"text":"cancel"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":128,"date":0,"chat":{"id":42,"type":"private"},"text":"skills"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":129,"date":0,"chat":{"id":42,"type":"private"},"text":"enable"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":130,"date":0,"chat":{"id":42,"type":"private"},"text":"disable"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":131,"date":0,"chat":{"id":42,"type":"private"},"text":"compact"}}"#,
        ),
    ]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker =
        TelegramWorker::with_consumer(app.clone(), backend.clone(), client, "telegram-test");

    let processed = runtime.block_on(worker.poll_once()).expect("poll once");
    assert_eq!(processed, 12);

    let state = backend_state.lock().expect("backend state");
    assert_eq!(state.cancelled_active_runs, vec!["session-operator"]);
    assert_eq!(state.cancelled_all_work, vec!["session-operator"]);
    assert_eq!(
        state.enabled_skills,
        vec![(
            "session-operator".to_string(),
            "silverbullet-space".to_string()
        )]
    );
    assert_eq!(
        state.disabled_skills,
        vec![(
            "session-operator".to_string(),
            "silverbullet-space".to_string()
        )]
    );
    assert_eq!(state.compacted_sessions, vec!["session-operator"]);
    assert_eq!(
        state.updated_preferences,
        vec![(
            "session-operator".to_string(),
            SessionPreferencesPatch {
                title: Some("Renamed Operator".to_string()),
                ..SessionPreferencesPatch::default()
            }
        )]
    );
    assert!(state.executed_turns.is_empty());
    drop(state);

    let _ = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured getUpdates");
    let status = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured status response");
    assert!(status.body.contains("Current session"));
    assert!(status.body.contains("Ход"));
    assert!(status.body.contains("lifecycle"));
    let lifecycle = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured lifecycle response");
    assert!(lifecycle.body.contains("Session lifecycle"));
    assert!(
        lifecycle
            .body
            .contains("no silent cleanup of project directories")
    );
    let rename = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured rename response");
    assert!(rename.body.contains("Renamed session"));
    assert!(rename.body.contains("Renamed Operator"));
    let jobs = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured jobs response");
    assert!(jobs.body.contains("job-1"));
    let tasks = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured tasks response");
    assert!(tasks.body.contains("task-agent-1"));
    let plan = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured plan response");
    assert!(plan.body.contains("task-1"));
    let stop = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured stop response");
    assert!(stop.body.contains("stopped session-operator"));
    let cancel = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured cancel response");
    assert!(cancel.body.contains("cancelled session-operator"));
    let skills = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured skills response");
    assert!(skills.body.contains("silverbullet-space"));
    let enable = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured enable response");
    assert!(enable.body.contains("enabled silverbullet-space"));
    let disable = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured disable response");
    assert!(disable.body.contains("disabled silverbullet-space"));
    let compact = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured compact response");
    assert!(compact.body.contains("compactifications=1"));

    handle.join().expect("join fake api");
}

#[test]
fn telegram_worker_configures_queue_mode_and_reports_it_in_status() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app();
    seed_activated_pairing(&app, "pair-queue-config", 777, 42);
    app.create_session("session-queue", "Queue")
        .expect("create session");
    seed_binding(&app, 42, 777, Some("session-queue"));
    let backend = RecordingTelegramBackend::with_state(RecordingTelegramBackendState {
        session_lookup: BTreeMap::from([(
            "session-queue".to_string(),
            session_summary("session-queue", "Queue", true),
        )]),
        ..RecordingTelegramBackendState::default()
    });
    let (api_url, requests, handle) = spawn_fake_telegram_api(vec![
        json_response(
            r#"{"ok":true,"result":[
                {"update_id":148,"message":{"message_id":117,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/queue"}},
                {"update_id":149,"message":{"message_id":118,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/queue reject"}},
                {"update_id":150,"message":{"message_id":119,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/status"}}
            ]}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":120,"date":0,"chat":{"id":42,"type":"private"},"text":"queue"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":121,"date":0,"chat":{"id":42,"type":"private"},"text":"queue reject"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":122,"date":0,"chat":{"id":42,"type":"private"},"text":"status"}}"#,
        ),
    ]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker =
        TelegramWorker::with_consumer(app.clone(), backend.clone(), client, "telegram-test");

    let processed = runtime.block_on(worker.poll_once()).expect("poll once");
    assert_eq!(processed, 3);

    let binding = app
        .store()
        .expect("open store")
        .get_telegram_chat_binding(42)
        .expect("get binding")
        .expect("binding exists");
    assert_eq!(binding.inbound_queue_mode, "reject");

    let _ = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured getUpdates");
    let queue_status = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured queue status");
    assert!(queue_status.body.contains("Telegram queue"));
    assert!(queue_status.body.contains("mode: coalesce"));
    assert!(queue_status.body.contains("coalesce_window_ms: 5000"));
    let queue_reject = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured queue reject");
    assert!(queue_reject.body.contains("mode: reject"));
    let status = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured status");
    assert!(status.body.contains("Telegram queue"));
    assert!(status.body.contains("mode: reject"));

    handle.join().expect("join fake api");
}

#[test]
#[ignore = "active-turn Telegram polling still needs a full async Postgres store path"]
fn telegram_worker_processes_status_and_stop_while_chat_turn_is_running() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app();
    seed_activated_pairing(&app, "pair-running-operator", 777, 42);
    seed_binding(&app, 42, 777, Some("session-running"));

    let (started_sender, started_receiver) = mpsc::channel();
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let backend = RecordingTelegramBackend::with_state(RecordingTelegramBackendState {
        session_lookup: BTreeMap::from([(
            "session-running".to_string(),
            session_summary("session-running", "Running", true),
        )]),
        active_run_status: "Ход:\n- статус: running".to_string(),
        next_chat_output: "Long turn completed.".to_string(),
        chat_turn_blocker: Some(ChatTurnBlocker {
            started: started_sender,
            release: release.clone(),
        }),
        ..RecordingTelegramBackendState::default()
    });
    let backend_state = backend.state();
    let mut update_responses = VecDeque::from([
        r#"{"ok":true,"result":[{"update_id":150,"message":{"message_id":125,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"start a long task"}}]}"#,
        r#"{"ok":true,"result":[
            {"update_id":151,"message":{"message_id":127,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/status"}},
            {"update_id":152,"message":{"message_id":128,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/stop"}},
            {"update_id":153,"message":{"message_id":129,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/cancel"}}
        ]}"#,
    ]);
    let (api_url, requests, handle) = spawn_routed_telegram_api(move |request| {
        if request.path.ends_with("/GetUpdates") {
            return json_response(
                update_responses
                    .pop_front()
                    .unwrap_or(r#"{"ok":true,"result":[]}"#),
            );
        }
        if request.path.ends_with("/DeleteMessage") || request.path.ends_with("/SendChatAction") {
            return json_response(r#"{"ok":true,"result":true}"#);
        }
        if request.path.ends_with("/SendMessage") || request.path.ends_with("/EditMessageText") {
            return json_response(
                r#"{"ok":true,"result":{"message_id":130,"date":0,"chat":{"id":42,"type":"private"},"text":"ok"}}"#,
            );
        }
        json_response(r#"{"ok":true,"result":true}"#)
    });
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker =
        TelegramWorker::with_consumer(app.clone(), backend.clone(), client, "telegram-test");

    let first_poll = runtime
        .block_on(async { tokio::time::timeout(Duration::from_secs(2), worker.poll_once()).await });
    if first_poll.is_err() {
        let (released, condvar) = &*release;
        *released.lock().expect("release lock") = true;
        condvar.notify_all();
    }
    let processed = first_poll
        .expect("poll_once should not wait for the full chat turn")
        .expect("first poll");
    assert_eq!(processed, 1);
    started_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("chat turn started");

    let second_poll = runtime
        .block_on(async { tokio::time::timeout(Duration::from_secs(2), worker.poll_once()).await });
    let processed = second_poll
        .expect("operator commands should be processed while the chat turn runs")
        .expect("second poll");
    assert_eq!(processed, 3);

    let state = backend_state.lock().expect("backend state");
    assert_eq!(state.executed_turns.len(), 1);
    assert_eq!(state.cancelled_active_runs, vec!["session-running"]);
    assert_eq!(state.cancelled_all_work, vec!["session-running"]);
    drop(state);

    let (released, condvar) = &*release;
    *released.lock().expect("release lock") = true;
    condvar.notify_all();
    drive_runtime(&runtime, Duration::from_secs(1));

    let _ = requests
        .recv_timeout(Duration::from_secs(10))
        .expect("captured first getUpdates");
    let _ = requests
        .recv_timeout(Duration::from_secs(10))
        .expect("captured temporary status message");
    let _ = requests
        .recv_timeout(Duration::from_secs(10))
        .expect("captured second getUpdates");
    let delete_status = requests
        .recv_timeout(Duration::from_secs(10))
        .expect("captured temporary status delete");
    assert_eq!(delete_status.path, "/bottest-token/DeleteMessage");
    let status = requests
        .recv_timeout(Duration::from_secs(10))
        .expect("captured status response");
    assert!(status.body.contains("Current session"));
    let stop = requests
        .recv_timeout(Duration::from_secs(10))
        .expect("captured stop response");
    assert!(stop.body.contains("stopped session-running"));
    let cancel = requests
        .recv_timeout(Duration::from_secs(10))
        .expect("captured cancel response");
    assert!(cancel.body.contains("cancelled session-running"));
    let final_message = requests
        .recv_timeout(Duration::from_secs(10))
        .expect("captured final response");
    assert_eq!(final_message.path, "/bottest-token/SendMessage");
    assert_eq!(
        app.store()
            .expect("open store")
            .get_telegram_chat_status(42)
            .expect("status lookup"),
        None
    );

    handle.stop().expect("join fake api");
}

#[test]
#[ignore = "active-turn Telegram polling still needs a full async Postgres store path"]
fn telegram_worker_coalesces_inbound_messages_while_turn_is_running() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app();
    seed_activated_pairing(&app, "pair-running-coalesce", 777, 42);
    app.create_session("session-coalesce", "Coalesce")
        .expect("create session");
    seed_binding(&app, 42, 777, Some("session-coalesce"));

    let (started_sender, started_receiver) = mpsc::channel();
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let backend = RecordingTelegramBackend::with_state(RecordingTelegramBackendState {
        session_lookup: BTreeMap::from([(
            "session-coalesce".to_string(),
            session_summary("session-coalesce", "Coalesce", true),
        )]),
        next_chat_output: "Long turn completed.".to_string(),
        chat_turn_blocker: Some(ChatTurnBlocker {
            started: started_sender,
            release: release.clone(),
        }),
        ..RecordingTelegramBackendState::default()
    });
    let backend_state = backend.state();
    let mut update_responses = VecDeque::from([
        r#"{"ok":true,"result":[{"update_id":160,"message":{"message_id":140,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"start a long task"}}]}"#,
        r#"{"ok":true,"result":[
            {"update_id":161,"message":{"message_id":142,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"add this"}},
            {"update_id":162,"message":{"message_id":143,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"and this too"}}
        ]}"#,
    ]);
    let (api_url, requests, handle) = spawn_routed_telegram_api(move |request| {
        if request.path.ends_with("/GetUpdates") {
            return json_response(
                update_responses
                    .pop_front()
                    .unwrap_or(r#"{"ok":true,"result":[]}"#),
            );
        }
        if request.path.ends_with("/SendMessage") || request.path.ends_with("/EditMessageText") {
            return json_response(
                r#"{"ok":true,"result":{"message_id":141,"date":0,"chat":{"id":42,"type":"private"},"text":"ok"}}"#,
            );
        }
        json_response(r#"{"ok":true,"result":true}"#)
    });
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker =
        TelegramWorker::with_consumer(app.clone(), backend.clone(), client, "telegram-test");

    runtime
        .block_on(async { tokio::time::timeout(Duration::from_secs(2), worker.poll_once()).await })
        .expect("first poll should not block")
        .expect("first poll");

    let processed = runtime
        .block_on(async { tokio::time::timeout(Duration::from_secs(2), worker.poll_once()).await })
        .expect("queued inputs should not block")
        .expect("second poll");
    assert_eq!(processed, 2);
    started_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("chat turn started");

    let events = app
        .store()
        .expect("open store")
        .list_queued_session_inbox_events_for_session("session-coalesce")
        .expect("list queued inbox");
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].available_at, events[1].available_at);
    assert!(events[0].available_at >= events[0].created_at + 5);
    assert!(events[0].payload_json.contains("add this"));
    assert!(events[1].payload_json.contains("and this too"));

    let state = backend_state.lock().expect("backend state");
    assert_eq!(state.executed_turns.len(), 1);
    drop(state);

    let (released, condvar) = &*release;
    *released.lock().expect("release lock") = true;
    condvar.notify_all();
    drive_runtime(&runtime, Duration::from_secs(1));

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut captured = Vec::new();
    while Instant::now() < deadline {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .unwrap_or_else(|| Duration::from_millis(0));
        match requests.recv_timeout(remaining.min(Duration::from_millis(100))) {
            Ok(request) => captured.push(request),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        let has_document = captured
            .iter()
            .any(|request| request.path.ends_with("/SendDocument"));
        let has_failure_notice = captured.iter().any(|request| {
            request.path.ends_with("/SendMessage")
                && request.body.contains("Не удалось отправить файл")
        });
        if has_document && has_failure_notice {
            break;
        }
    }
    assert!(
        captured
            .iter()
            .filter(|request| request.path.ends_with("/GetUpdates"))
            .count()
            >= 2,
        "expected both polling requests, captured: {captured:?}"
    );
    assert!(
        captured
            .iter()
            .any(|request| request.path.ends_with("/DeleteMessage")),
        "expected temporary status cleanup, captured: {captured:?}"
    );
    let queued_acks = captured
        .iter()
        .filter(|request| {
            request.path.ends_with("/SendMessage")
                && request.body.contains("Queued inbound message")
        })
        .count();
    assert_eq!(queued_acks, 2, "captured: {captured:?}");
    assert!(
        captured.iter().any(|request| {
            request.path.ends_with("/SendMessage") && request.body.contains("Long turn completed")
        }),
        "expected final response, captured: {captured:?}"
    );

    handle.stop().expect("stop fake api");
}

#[test]
fn telegram_worker_start_creates_pending_pairing_and_returns_cli_hint() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app();
    let backend = RecordingTelegramBackend::default();
    let (api_url, requests, handle) = spawn_fake_telegram_api(vec![
        json_response(
            r#"{"ok":true,"result":[{"update_id":100,"message":{"message_id":7,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/start"}}]}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":8,"date":0,"chat":{"id":42,"type":"private"},"text":"pairing reply"}}"#,
        ),
    ]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker =
        TelegramWorker::with_consumer(app.clone(), backend.clone(), client, "telegram-test");

    let processed = runtime.block_on(worker.poll_once()).expect("poll once");
    assert_eq!(processed, 1);

    let store = app.store().expect("open store");
    let pairing = store
        .get_telegram_user_pairing_by_user_id(777)
        .expect("get pairing")
        .expect("pairing exists");
    assert_eq!(pairing.status, "pending");
    let cursor = store
        .get_telegram_update_cursor("telegram-test")
        .expect("get cursor")
        .expect("cursor exists");
    assert_eq!(cursor.update_id, 101);

    let get_updates = requests
        .recv_timeout(Duration::from_secs(10))
        .expect("captured getUpdates");
    assert_eq!(get_updates.path, "/bottest-token/GetUpdates");

    let send_message = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured sendMessage");
    assert_eq!(send_message.path, "/bottest-token/SendMessage");
    assert!(send_message.body.contains("\"chat_id\":42"));
    assert!(
        send_message
            .body
            .contains(&format!("agentd telegram pair {}", pairing.token))
    );

    handle.join().expect("join fake api");
}

#[test]
fn telegram_worker_start_for_activated_user_keeps_pairing_and_shows_status_hint() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app();
    seed_activated_pairing(&app, "pair-activated", 777, 42);
    seed_binding(&app, 42, 777, Some("session-1"));
    let backend = RecordingTelegramBackend::default();
    let (api_url, requests, handle) = spawn_fake_telegram_api(vec![
        json_response(
            r#"{"ok":true,"result":[{"update_id":101,"message":{"message_id":9,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/start"}}]}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":10,"date":0,"chat":{"id":42,"type":"private"},"text":"already connected"}}"#,
        ),
    ]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker =
        TelegramWorker::with_consumer(app.clone(), backend.clone(), client, "telegram-test");

    let processed = runtime.block_on(worker.poll_once()).expect("poll once");
    assert_eq!(processed, 1);

    let store = app.store().expect("open store");
    let pairing = store
        .get_telegram_user_pairing_by_user_id(777)
        .expect("get pairing")
        .expect("pairing exists");
    assert_eq!(pairing.status, "activated");
    assert_eq!(pairing.token, "pair-activated");

    let _ = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured getUpdates");
    let send_message = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured sendMessage");
    assert_eq!(send_message.path, "/bottest-token/SendMessage");
    assert!(send_message.body.contains("Already connected"));
    assert!(send_message.body.contains("session-1"));
    assert!(!send_message.body.contains("agentd telegram pair"));

    handle.join().expect("join fake api");
}

#[test]
fn telegram_render_session_list_is_recent_and_actionable() {
    let mut sessions = (1..=6)
        .map(|index| {
            let mut summary = session_summary(
                &format!("session-177700000000{index}"),
                &format!("Session {index}"),
                true,
            );
            summary.updated_at = 1_777_000_000 + index as i64;
            summary.message_count = index;
            summary
        })
        .collect::<Vec<_>>();
    sessions[0].id = "session-current".to_string();

    let output = render_session_list(&sessions, Some("session-current"));

    assert!(output.contains("Recent sessions:"));
    assert!(output.contains("> Session 1 [current]"));
    assert!(output.contains("updated=2026-04-24"));
    assert!(output.contains("messages=1"));
    assert!(output.contains("use=/use session-current"));
    assert!(output.contains("use=/use session-1777000000005"));
    assert!(!output.contains("use=/use session-1777000000006"));
    assert!(output.contains("1 older sessions hidden"));
}

#[test]
fn telegram_worker_ingests_private_document_as_session_artifact_and_turn_input() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app();
    seed_activated_pairing(&app, "pair-file", 777, 42);
    app.create_session("session-file-1", "Telegram Chat")
        .expect("create session");
    let backend = RecordingTelegramBackend::with_state(RecordingTelegramBackendState {
        create_session_results: vec![session_summary("session-file-1", "Telegram Chat", false)],
        next_chat_output: "I received the file.".to_string(),
        ..RecordingTelegramBackendState::default()
    });
    let backend_state = backend.state();
    let (api_url, requests, handle) = spawn_fake_telegram_api(vec![
        json_response(
            r#"{"ok":true,"result":[{"update_id":130,"message":{"message_id":70,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"caption":"Please inspect it","document":{"file_id":"doc-file-1","file_unique_id":"doc-unique-1","file_size":12,"file_name":"report.txt","mime_type":"text/plain"}}}]}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"file_id":"doc-file-1","file_unique_id":"doc-unique-1","file_size":12,"file_path":"documents/report.txt"}}"#,
        ),
        FakeResponse {
            content_type: "application/octet-stream",
            body: b"report bytes".to_vec(),
        },
        json_response(
            r#"{"ok":true,"result":{"message_id":71,"date":0,"chat":{"id":42,"type":"private"},"text":"file attached"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":72,"date":0,"chat":{"id":42,"type":"private"},"text":"working"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":73,"date":0,"chat":{"id":42,"type":"private"},"text":"final"}}"#,
        ),
    ]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker =
        TelegramWorker::with_consumer(app.clone(), backend.clone(), client, "telegram-test");

    runtime.block_on(worker.poll_once()).expect("poll once");

    let artifacts = app
        .store()
        .expect("open store")
        .list_artifacts_for_session("session-file-1")
        .expect("list artifacts");
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].kind, "telegram_file");
    assert_eq!(artifacts[0].bytes, b"report bytes");
    assert!(
        artifacts[0]
            .metadata_json
            .contains("\"file_name\":\"report.txt\"")
    );
    assert!(
        artifacts[0]
            .metadata_json
            .contains("\"telegram_content_kind\":\"document\"")
    );

    let state = backend_state.lock().expect("backend state");
    assert_eq!(state.executed_turns.len(), 1);
    assert_eq!(state.executed_turns[0].0, "session-file-1");
    assert!(
        state.executed_turns[0]
            .1
            .contains("Пользователь загрузил файл.")
    );
    assert!(state.executed_turns[0].1.contains("name=report.txt"));
    assert!(state.executed_turns[0].1.contains(&artifacts[0].id));
    assert!(
        state.executed_turns[0]
            .1
            .contains("caption=Please inspect it")
    );
    drop(state);

    let get_updates = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured getUpdates");
    assert_eq!(get_updates.path, "/bottest-token/GetUpdates");
    let get_file = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured getFile");
    assert_eq!(get_file.path, "/bottest-token/GetFile");
    let download = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured file download");
    assert_eq!(download.method, "GET");
    assert_eq!(download.path, "/file/bottest-token/documents%2Freport.txt");
    let attached = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured file attached");
    assert_eq!(attached.path, "/bottest-token/SendMessage");
    assert!(attached.body.contains("File attached to session"));
    assert!(attached.body.contains(&artifacts[0].id));
    let status = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured status");
    assert_eq!(status.path, "/bottest-token/SendMessage");
    let final_message = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured final");
    assert_eq!(final_message.path, "/bottest-token/SendMessage");

    handle.join().expect("join fake api");
}

#[test]
fn telegram_worker_rejects_oversized_document_with_operator_message() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app_with_max_download_bytes(8);
    seed_activated_pairing(&app, "pair-file-large", 777, 42);
    app.create_session("session-file-large", "Telegram Chat")
        .expect("create session");
    let backend = RecordingTelegramBackend::with_state(RecordingTelegramBackendState {
        create_session_results: vec![session_summary(
            "session-file-large",
            "Telegram Chat",
            false,
        )],
        next_chat_output: "should not run".to_string(),
        ..RecordingTelegramBackendState::default()
    });
    let backend_state = backend.state();
    let (api_url, requests, handle) = spawn_fake_telegram_api(vec![
        json_response(
            r#"{"ok":true,"result":[{"update_id":131,"message":{"message_id":71,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"document":{"file_id":"doc-file-big","file_unique_id":"doc-unique-big","file_size":12,"file_name":"big.txt","mime_type":"text/plain"}}}]}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":72,"date":0,"chat":{"id":42,"type":"private"},"text":"too large"}}"#,
        ),
    ]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker =
        TelegramWorker::with_consumer(app.clone(), backend.clone(), client, "telegram-test");

    runtime.block_on(worker.poll_once()).expect("poll once");

    let artifacts = app
        .store()
        .expect("open store")
        .list_artifacts_for_session("session-file-large")
        .expect("list artifacts");
    assert!(artifacts.is_empty());
    let state = backend_state.lock().expect("backend state");
    assert!(state.executed_turns.is_empty());
    drop(state);

    let _ = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured getUpdates");
    let send_message = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured oversized message");
    assert_eq!(send_message.path, "/bottest-token/SendMessage");
    assert!(send_message.body.contains("File intake failed"));
    assert!(send_message.body.contains("too large"));

    handle.join().expect("join fake api");
}

#[test]
fn telegram_worker_lists_and_sends_session_artifacts_as_files() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app();
    seed_activated_pairing(&app, "pair-file-command", 777, 42);
    app.create_session("session-files", "Files")
        .expect("create session");
    seed_binding(&app, 42, 777, Some("session-files"));
    app.store()
        .expect("open store")
        .put_artifact(&ArtifactRecord {
            id: "artifact-report-1".to_string(),
            session_id: "session-files".to_string(),
            kind: "telegram_file".to_string(),
            metadata_json: r#"{"source":"telegram","file_name":"report.txt","mime_type":"text/plain","file_size":11}"#.to_string(),
            path: PathBuf::from("artifacts/artifact-report-1.bin"),
            bytes: b"report body".to_vec(),
            created_at: 20,
        })
        .expect("put artifact");
    let backend = RecordingTelegramBackend::with_state(RecordingTelegramBackendState {
        session_lookup: BTreeMap::from([(
            "session-files".to_string(),
            session_summary("session-files", "Files", true),
        )]),
        ..RecordingTelegramBackendState::default()
    });
    let (api_url, requests, handle) = spawn_fake_telegram_api(vec![
        json_response(
            r#"{"ok":true,"result":[
                {"update_id":131,"message":{"message_id":72,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/files"}},
                {"update_id":132,"message":{"message_id":73,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/file artifact-report-1"}}
            ]}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":74,"date":0,"chat":{"id":42,"type":"private"},"text":"files"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":75,"date":0,"chat":{"id":42,"type":"private"},"document":{"file_id":"sent","file_unique_id":"sent-unique","file_size":11,"file_name":"report.txt","mime_type":"text/plain"}}}"#,
        ),
    ]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker = TelegramWorker::with_consumer(app, backend, client, "telegram-test");

    let processed = runtime.block_on(worker.poll_once()).expect("poll once");
    assert_eq!(processed, 2);

    let _ = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured getUpdates");
    let files_message = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured files response");
    assert_eq!(files_message.path, "/bottest-token/SendMessage");
    assert!(files_message.body.contains("Files in current session"));
    assert!(files_message.body.contains("report.txt"));
    assert!(files_message.body.contains("artifact-report-1"));
    let send_document = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured sendDocument");
    assert_eq!(send_document.path, "/bottest-token/SendDocument");
    assert!(send_document.body.contains("report.txt"));
    assert!(send_document.body.contains("artifact-report-1"));

    handle.join().expect("join fake api");
}

#[test]
fn telegram_worker_registers_current_chat_target_and_attaches_session_output() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app();
    seed_activated_pairing(&app, "pair-targets", 777, 42);
    let session = app
        .create_session_auto(Some("Server Watcher"))
        .expect("create session");
    seed_binding(&app, 42, 777, Some(session.id.as_str()));
    let backend = RecordingTelegramBackend::default();
    let (api_url, requests, handle) = spawn_fake_telegram_api(vec![
        json_response(
            r#"{"ok":true,"result":[
                {"update_id":151,"message":{"message_id":119,"date":0,"chat":{"id":-9000,"type":"group","title":"Ops"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/target"}},
                {"update_id":152,"message":{"message_id":120,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/outputs attach telegram-n9000 summary"}}
            ]}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":121,"date":0,"chat":{"id":-9000,"type":"group"},"text":"target"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":122,"date":0,"chat":{"id":42,"type":"private"},"text":"outputs"}}"#,
        ),
    ]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker = TelegramWorker::with_consumer(app.clone(), backend, client, "telegram-test");

    let processed = runtime.block_on(worker.poll_once()).expect("poll once");
    assert_eq!(processed, 2);

    let store = app.store().expect("open store");
    let target = store
        .get_delivery_target("telegram-n9000")
        .expect("get target")
        .expect("target exists");
    assert_eq!(target.kind, "telegram");
    assert_eq!(target.address, "-9000");
    assert_eq!(target.scope, "group");

    let routes = store
        .list_enabled_session_output_routes(&session.id)
        .expect("list routes");
    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0].target_id, "telegram-n9000");
    assert_eq!(routes[0].format_policy, "summary");

    let _ = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured getUpdates");
    let target_response = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured target response");
    assert!(target_response.body.contains("telegram-n9000"));
    let outputs_response = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured outputs response");
    assert!(outputs_response.body.contains("attached"));

    handle.join().expect("join fake api");
}

#[test]
fn telegram_worker_delivers_assistant_transcripts_to_session_output_routes_once() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app();
    let session = app
        .create_session_auto(Some("Server Watcher"))
        .expect("create session");
    app.create_delivery_target(
        "telegram-n9000",
        agentd::bootstrap::DeliveryTargetCreateOptions {
            kind: "telegram".to_string(),
            address: "-9000".to_string(),
            scope: "group".to_string(),
            owner_user_id: Some("telegram:777".to_string()),
            allowed_agent_ids: Vec::new(),
            allowed_session_ids: Vec::new(),
            send_policy_json: "null".to_string(),
            format_policy: "full_text".to_string(),
        },
    )
    .expect("create target");
    app.attach_session_output_route(
        &session.id,
        "telegram-n9000",
        agentd::bootstrap::SessionOutputRouteCreateOptions {
            route_id: Some("route-server-watcher-ops".to_string()),
            filter_json: r#"{"kind":"assistant"}"#.to_string(),
            format_policy: "summary".to_string(),
            enabled: true,
        },
    )
    .expect("attach route");
    app.store()
        .expect("open store")
        .put_transcript(&TranscriptRecord {
            id: "transcript-route-1".to_string(),
            session_id: session.id.clone(),
            run_id: None,
            kind: "assistant".to_string(),
            content: "Server status: OK".to_string(),
            created_at: 200,
        })
        .expect("put assistant transcript");

    let backend = RecordingTelegramBackend::default();
    let (api_url, requests, handle) = spawn_routed_telegram_api(|request| {
        if request.path.ends_with("/GetUpdates") {
            json_response(r#"{"ok":true,"result":[]}"#)
        } else {
            json_response(
                r#"{"ok":true,"result":{"message_id":220,"date":0,"chat":{"id":-9000,"type":"group"},"text":"delivered"}}"#,
            )
        }
    });
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker = TelegramWorker::with_consumer(app.clone(), backend, client, "telegram-test");

    let processed = runtime.block_on(worker.poll_once()).expect("poll once");
    assert_eq!(processed, 0);

    let route = app
        .store()
        .expect("open store")
        .get_session_output_route("route-server-watcher-ops")
        .expect("get route")
        .expect("route exists");
    assert_eq!(route.last_delivered_transcript_created_at, Some(200));
    assert_eq!(
        route.last_delivered_transcript_id.as_deref(),
        Some("transcript-route-1")
    );

    let captured = drain_requests_for(&requests, Duration::from_secs(2));
    let sends = captured
        .iter()
        .filter(|request| request.path.ends_with("/SendMessage"))
        .collect::<Vec<_>>();
    assert_eq!(sends.len(), 1, "captured requests: {captured:?}");
    assert!(sends[0].body.contains("\"chat_id\":-9000"));
    assert!(sends[0].body.contains("Server status"));

    runtime.block_on(worker.poll_once()).expect("second poll");
    let duplicate_check = drain_requests_for(&requests, Duration::from_millis(500));
    assert!(
        duplicate_check
            .iter()
            .all(|request| !request.path.ends_with("/SendMessage")),
        "unexpected duplicate delivery: {duplicate_check:?}"
    );

    handle.stop().expect("stop fake api");
}

#[test]
fn telegram_worker_sends_queued_file_delivery_requests_after_chat_turn() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app();
    seed_activated_pairing(&app, "pair-file-delivery", 777, 42);
    app.create_session("session-delivery", "Delivery")
        .expect("create session");
    seed_binding(&app, 42, 777, Some("session-delivery"));
    {
        let store = app.store().expect("open store");
        store
            .put_artifact(&ArtifactRecord {
                id: "artifact-report-queued".to_string(),
                session_id: "session-delivery".to_string(),
                kind: "workspace_file".to_string(),
                metadata_json: r#"{"file_name":"report.txt","mime_type":"text/plain"}"#.to_string(),
                path: PathBuf::from("artifacts/artifact-report-queued.bin"),
                bytes: b"report body".to_vec(),
                created_at: 20,
            })
            .expect("put artifact");
        store
            .put_file_delivery_request(&FileDeliveryRequestRecord {
                id: "delivery-queued-1".to_string(),
                session_id: "session-delivery".to_string(),
                run_id: None,
                artifact_id: "artifact-report-queued".to_string(),
                target: "current_chat".to_string(),
                file_name: "report.txt".to_string(),
                caption: Some("Отправляю файл".to_string()),
                status: "queued".to_string(),
                created_at: 21,
                updated_at: 21,
                delivered_at: None,
                error: None,
            })
            .expect("queue delivery");
    }
    let backend = RecordingTelegramBackend::with_state(RecordingTelegramBackendState {
        session_lookup: BTreeMap::from([(
            "session-delivery".to_string(),
            session_summary("session-delivery", "Delivery", true),
        )]),
        listed_sessions: vec![session_summary("session-delivery", "Delivery", true)],
        next_chat_output: "Файл отправляю отдельным документом.".to_string(),
        ..RecordingTelegramBackendState::default()
    });
    let (api_url, requests, handle) = spawn_fake_telegram_api(vec![
        json_response(
            r#"{"ok":true,"result":[{"update_id":133,"message":{"message_id":76,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"пришли файл"}}]}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":77,"date":0,"chat":{"id":42,"type":"private"},"text":"working"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":78,"date":0,"chat":{"id":42,"type":"private"},"text":"final"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":79,"date":0,"chat":{"id":42,"type":"private"},"document":{"file_id":"sent","file_unique_id":"sent-unique","file_size":11,"file_name":"report.txt","mime_type":"text/plain"}}}"#,
        ),
    ]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker = TelegramWorker::with_consumer(app.clone(), backend, client, "telegram-test");

    runtime.block_on(worker.poll_once()).expect("poll once");
    drive_runtime(&runtime, Duration::from_secs(1));

    let _ = requests
        .recv_timeout(Duration::from_secs(10))
        .expect("captured getUpdates");
    let _status = requests
        .recv_timeout(Duration::from_secs(10))
        .expect("captured status");
    let final_message = requests
        .recv_timeout(Duration::from_secs(10))
        .expect("captured final");
    assert_eq!(final_message.path, "/bottest-token/SendMessage");
    let send_document = requests
        .recv_timeout(Duration::from_secs(10))
        .expect("captured queued sendDocument");
    assert_eq!(send_document.path, "/bottest-token/SendDocument");
    assert!(send_document.body.contains("report.txt"));
    assert!(send_document.body.contains("Отправляю файл"));

    let delivered = app
        .store()
        .expect("open store")
        .get_file_delivery_request("delivery-queued-1")
        .expect("get delivery")
        .expect("delivery exists");
    assert_eq!(delivered.status, "delivered");
    assert!(delivered.delivered_at.is_some());
    assert!(delivered.error.is_none());

    handle.join().expect("join fake api");
}

#[test]
fn telegram_worker_reports_failed_queued_file_delivery_to_chat() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app();
    seed_activated_pairing(&app, "pair-file-delivery-failed", 777, 42);
    app.create_session("session-delivery-failed", "Delivery")
        .expect("create session");
    seed_binding(&app, 42, 777, Some("session-delivery-failed"));
    {
        let store = app.store().expect("open store");
        store
            .put_artifact(&ArtifactRecord {
                id: "artifact-report-failed".to_string(),
                session_id: "session-delivery-failed".to_string(),
                kind: "workspace_file".to_string(),
                metadata_json: r#"{"file_name":"report.txt","mime_type":"text/plain"}"#.to_string(),
                path: PathBuf::from("artifacts/artifact-report-failed.bin"),
                bytes: b"report body".to_vec(),
                created_at: 20,
            })
            .expect("put artifact");
        store
            .put_file_delivery_request(&FileDeliveryRequestRecord {
                id: "delivery-failed-1".to_string(),
                session_id: "session-delivery-failed".to_string(),
                run_id: None,
                artifact_id: "artifact-report-failed".to_string(),
                target: "current_chat".to_string(),
                file_name: "report.txt".to_string(),
                caption: Some("Отправляю файл".to_string()),
                status: "queued".to_string(),
                created_at: 21,
                updated_at: 21,
                delivered_at: None,
                error: None,
            })
            .expect("queue delivery");
    }
    let backend = RecordingTelegramBackend::with_state(RecordingTelegramBackendState {
        session_lookup: BTreeMap::from([(
            "session-delivery-failed".to_string(),
            session_summary("session-delivery-failed", "Delivery", true),
        )]),
        listed_sessions: vec![session_summary("session-delivery-failed", "Delivery", true)],
        next_chat_output: "Файл отправляю отдельным документом.".to_string(),
        ..RecordingTelegramBackendState::default()
    });
    let mut update_responses = VecDeque::from([
        r#"{"ok":true,"result":[{"update_id":134,"message":{"message_id":76,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"пришли файл"}}]}"#,
    ]);
    let (api_url, requests, handle) = spawn_routed_telegram_api(move |request| {
        if request.path.ends_with("/GetUpdates") {
            return json_response(
                update_responses
                    .pop_front()
                    .unwrap_or(r#"{"ok":true,"result":[]}"#),
            );
        }
        if request.path.ends_with("/SendDocument") {
            return json_response(
                r#"{"ok":false,"error_code":400,"description":"Bad Request: document file is invalid"}"#,
            );
        }
        if request.path.ends_with("/SendMessage") || request.path.ends_with("/EditMessageText") {
            return json_response(
                r#"{"ok":true,"result":{"message_id":80,"date":0,"chat":{"id":42,"type":"private"},"text":"ok"}}"#,
            );
        }
        json_response(r#"{"ok":true,"result":true}"#)
    });
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker = TelegramWorker::with_consumer(app.clone(), backend, client, "telegram-test");

    runtime.block_on(worker.poll_once()).expect("poll once");

    let captured = drive_runtime_collect_requests_until(
        &runtime,
        &requests,
        Duration::from_secs(5),
        |captured| {
            let has_document = captured
                .iter()
                .any(|request| request.path.ends_with("/SendDocument"));
            let has_failure_notice = captured.iter().any(|request| {
                request.path.ends_with("/SendMessage")
                    && request.body.contains("Не удалось отправить файл")
            });
            has_document && has_failure_notice
        },
    );
    assert!(
        captured
            .iter()
            .any(|request| request.path.ends_with("/GetUpdates")),
        "expected getUpdates, captured: {captured:?}"
    );
    assert!(
        captured
            .iter()
            .any(|request| request.path.ends_with("/SendDocument")),
        "expected queued sendDocument, captured: {captured:?}"
    );
    assert!(
        captured.iter().any(|request| {
            request.path.ends_with("/SendMessage")
                && request.body.contains("Не удалось отправить файл")
        }),
        "expected delivery failure notice, captured: {captured:?}"
    );
    assert!(
        captured
            .iter()
            .any(|request| request.body.contains("report.txt")),
        "expected failed file name in Telegram requests, captured: {captured:?}"
    );

    let failed = app
        .store()
        .expect("open store")
        .get_file_delivery_request("delivery-failed-1")
        .expect("get delivery")
        .expect("delivery exists");
    assert_eq!(failed.status, "failed");
    assert!(
        failed
            .error
            .as_deref()
            .is_some_and(|error| error.contains("document file is invalid"))
    );

    handle.stop().expect("stop fake api");
}

#[test]
fn telegram_worker_retries_transient_send_message_failures() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app_with_progress_interval_ms(10);
    seed_activated_pairing(&app, "pair-retry", 777, 42);
    let backend = RecordingTelegramBackend::with_state(RecordingTelegramBackendState {
        create_session_results: vec![session_summary("session-retry", "Telegram Chat", false)],
        next_chat_output: "Retried reply.".to_string(),
        ..RecordingTelegramBackendState::default()
    });
    let (api_url, requests, handle) = spawn_fake_telegram_api(vec![
        json_response(
            r#"{"ok":true,"result":[{"update_id":133,"message":{"message_id":76,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"retry please"}}]}"#,
        ),
        json_response(r#"{"ok":false,"error_code":502,"description":"Bad Gateway"}"#),
        json_response(
            r#"{"ok":true,"result":{"message_id":77,"date":0,"chat":{"id":42,"type":"private"},"text":"working"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":78,"date":0,"chat":{"id":42,"type":"private"},"text":"final"}}"#,
        ),
    ]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker = TelegramWorker::with_consumer(app, backend, client, "telegram-test");

    runtime.block_on(worker.poll_once()).expect("poll once");
    drive_runtime(&runtime, Duration::from_secs(1));

    let _ = requests
        .recv_timeout(Duration::from_secs(10))
        .expect("captured getUpdates");
    let first_status_attempt = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured failed status attempt");
    assert_eq!(first_status_attempt.path, "/bottest-token/SendMessage");
    let second_status_attempt = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured retried status attempt");
    assert_eq!(second_status_attempt.path, "/bottest-token/SendMessage");
    let final_message = requests
        .recv_timeout(Duration::from_secs(10))
        .expect("captured final message");
    assert_eq!(final_message.path, "/bottest-token/SendMessage");

    handle.join().expect("join fake api");
}

#[test]
fn telegram_worker_rejects_unpaired_private_text_until_start() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app();
    let backend = RecordingTelegramBackend::default();
    let (api_url, requests, handle) = spawn_fake_telegram_api(vec![
        json_response(
            r#"{"ok":true,"result":[{"update_id":101,"message":{"message_id":9,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"hello"}}]}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":10,"date":0,"chat":{"id":42,"type":"private"},"text":"pairing required"}}"#,
        ),
    ]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker = TelegramWorker::with_consumer(app, backend, client, "telegram-test");

    runtime.block_on(worker.poll_once()).expect("poll once");

    let _ = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured getUpdates");
    let send_message = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured sendMessage");
    assert!(send_message.body.contains("Pairing required"));
    assert!(send_message.body.contains("/start"));

    handle.join().expect("join fake api");
}

#[test]
fn telegram_worker_judge_command_queues_interagent_message() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app();
    seed_activated_pairing(&app, "pair-judge", 777, 42);
    let backend = RecordingTelegramBackend::with_state(RecordingTelegramBackendState {
        create_session_results: vec![session_summary("session-judge", "Telegram Chat", false)],
        next_agent_message_response:
            "message_agent queued: target=judge recipient_session=session-agentmsg-1 recipient_job=job-agentmsg-1 chain_id=chain-judge hop_count=1"
                .to_string(),
        ..RecordingTelegramBackendState::default()
    });
    let backend_state = backend.state();
    let (api_url, requests, handle) = spawn_fake_telegram_api(vec![
        json_response(
            r#"{"ok":true,"result":[{"update_id":117,"message":{"message_id":46,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/judge who are you?"}}]}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":47,"date":0,"chat":{"id":42,"type":"private"},"text":"judge queued"}}"#,
        ),
    ]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker =
        TelegramWorker::with_consumer(app.clone(), backend.clone(), client, "telegram-test");

    runtime.block_on(worker.poll_once()).expect("poll once");
    drive_runtime(&runtime, Duration::from_millis(100));

    let binding = app
        .store()
        .expect("open store")
        .get_telegram_chat_binding(42)
        .expect("get binding")
        .expect("binding exists");
    assert_eq!(
        binding.selected_session_id.as_deref(),
        Some("session-judge")
    );

    let state = backend_state.lock().expect("backend state");
    assert_eq!(
        state.agent_messages,
        vec![(
            "session-judge".to_string(),
            "judge".to_string(),
            "who are you?".to_string()
        )]
    );
    drop(state);

    let _ = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured getUpdates");
    let response = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured judge response");
    assert!(
        response
            .body
            .contains("recipient_session=session-agentmsg-1")
    );
    assert!(response.body.contains("chain_id=chain-judge"));

    handle.join().expect("join fake api");
}

#[test]
fn telegram_worker_agent_command_queues_interagent_message() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app();
    seed_activated_pairing(&app, "pair-agent", 777, 42);
    let backend = RecordingTelegramBackend::with_state(RecordingTelegramBackendState {
        create_session_results: vec![session_summary("session-agent", "Telegram Chat", false)],
        next_agent_message_response:
            "message_agent queued: target=reviewer recipient_session=session-agentmsg-2 recipient_job=job-agentmsg-2 chain_id=chain-agent hop_count=1"
                .to_string(),
        ..RecordingTelegramBackendState::default()
    });
    let backend_state = backend.state();
    let (api_url, requests, handle) = spawn_fake_telegram_api(vec![
        json_response(
            r#"{"ok":true,"result":[{"update_id":118,"message":{"message_id":48,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/agent reviewer check status"}}]}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":49,"date":0,"chat":{"id":42,"type":"private"},"text":"agent queued"}}"#,
        ),
    ]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker =
        TelegramWorker::with_consumer(app.clone(), backend.clone(), client, "telegram-test");

    runtime.block_on(worker.poll_once()).expect("poll once");
    drive_runtime(&runtime, Duration::from_millis(100));

    let binding = app
        .store()
        .expect("open store")
        .get_telegram_chat_binding(42)
        .expect("get binding")
        .expect("binding exists");
    assert_eq!(
        binding.selected_session_id.as_deref(),
        Some("session-agent")
    );

    let state = backend_state.lock().expect("backend state");
    assert_eq!(
        state.agent_messages,
        vec![(
            "session-agent".to_string(),
            "reviewer".to_string(),
            "check status".to_string()
        )]
    );
    drop(state);

    let _ = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured getUpdates");
    let response = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured agent response");
    assert!(response.body.contains("target=reviewer"));
    assert!(response.body.contains("chain_id=chain-agent"));

    handle.join().expect("join fake api");
}

#[test]
fn telegram_worker_auto_creates_private_session_and_routes_text_turn() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app();
    seed_activated_pairing(&app, "pair-activated", 777, 42);
    let backend = RecordingTelegramBackend::with_state(RecordingTelegramBackendState {
        create_session_results: vec![session_summary("session-1", "Telegram Chat", false)],
        next_chat_output: "Hello from agent.".to_string(),
        ..RecordingTelegramBackendState::default()
    });
    let backend_state = backend.state();
    let (api_url, requests, handle) = spawn_fake_telegram_api(vec![
        json_response(
            r#"{"ok":true,"result":[{"update_id":102,"message":{"message_id":11,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"hello from telegram"}}]}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":12,"date":0,"chat":{"id":42,"type":"private"},"text":"working"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":12,"date":0,"chat":{"id":42,"type":"private"},"text":"Hello from agent."}}"#,
        ),
    ]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker =
        TelegramWorker::with_consumer(app.clone(), backend.clone(), client, "telegram-test");

    runtime.block_on(worker.poll_once()).expect("poll once");
    drive_runtime(&runtime, Duration::from_millis(100));

    let binding = app
        .store()
        .expect("open store")
        .get_telegram_chat_binding(42)
        .expect("get binding")
        .expect("binding exists");
    assert_eq!(binding.selected_session_id.as_deref(), Some("session-1"));

    let state = backend_state.lock().expect("backend state");
    assert_eq!(state.created_titles, vec![None]);
    assert_eq!(
        state.updated_preferences,
        vec![(
            "session-1".to_string(),
            SessionPreferencesPatch {
                auto_approve: Some(true),
                reasoning_visible: Some(false),
                think_level: Some(Some("off".to_string())),
                ..SessionPreferencesPatch::default()
            }
        )]
    );
    assert_eq!(
        state.executed_turns,
        vec![("session-1".to_string(), "hello from telegram".to_string())]
    );
    drop(state);

    let _ = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured getUpdates");
    let send_message = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured start ack");
    assert_eq!(send_message.path, "/bottest-token/SendMessage");
    assert!(send_message.body.contains("\"parse_mode\":\"HTML\""));
    assert!(send_message.body.contains("Стадия: запуск"));
    let final_message = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured final message");
    assert_eq!(final_message.path, "/bottest-token/SendMessage");
    assert!(final_message.body.contains("Hello from agent."));

    handle.join().expect("join fake api");
}

#[test]
fn telegram_worker_group_mention_creates_shared_session_and_routes_text_turn() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app();
    seed_activated_pairing(&app, "pair-group", 777, 42);
    let backend = RecordingTelegramBackend::with_state(RecordingTelegramBackendState {
        create_session_results: vec![session_summary("session-group-1", "Group Chat", false)],
        next_chat_output: "Hello from group agent.".to_string(),
        ..RecordingTelegramBackendState::default()
    });
    let backend_state = backend.state();
    let (api_url, requests, handle) = spawn_fake_telegram_api(vec![
        json_response(
            r#"{"ok":true,"result":[{"update_id":109,"message":{"message_id":31,"date":0,"chat":{"id":9000,"type":"group","title":"Team Chat"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"@teamd_agent_bot hello group"}}]}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"id":9001,"is_bot":true,"first_name":"teamd","username":"teamd_agent_bot","can_join_groups":true,"can_read_all_group_messages":false,"supports_inline_queries":false,"has_main_web_app":false}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":32,"date":0,"chat":{"id":9000,"type":"group","title":"Team Chat"},"text":"working"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":32,"date":0,"chat":{"id":9000,"type":"group","title":"Team Chat"},"text":"Hello from group agent."}}"#,
        ),
    ]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker =
        TelegramWorker::with_consumer(app.clone(), backend.clone(), client, "telegram-test");

    runtime.block_on(worker.poll_once()).expect("poll once");

    let binding = app
        .store()
        .expect("open store")
        .get_telegram_chat_binding(9000)
        .expect("get binding")
        .expect("binding exists");
    assert_eq!(binding.scope, "group");
    assert_eq!(binding.owner_telegram_user_id, None);
    assert_eq!(
        binding.selected_session_id.as_deref(),
        Some("session-group-1")
    );

    let state = backend_state.lock().expect("backend state");
    assert_eq!(state.created_titles, vec![None]);
    assert_eq!(
        state.updated_preferences,
        vec![(
            "session-group-1".to_string(),
            SessionPreferencesPatch {
                auto_approve: Some(true),
                reasoning_visible: Some(false),
                think_level: Some(Some("off".to_string())),
                ..SessionPreferencesPatch::default()
            }
        )]
    );
    assert_eq!(
        state.executed_turns,
        vec![("session-group-1".to_string(), "hello group".to_string())]
    );
    drop(state);

    let _ = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured getUpdates");
    let get_me = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured getMe");
    assert_eq!(get_me.path, "/bottest-token/GetMe");
    let send_message = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured start ack");
    assert_eq!(send_message.path, "/bottest-token/SendMessage");
    assert!(send_message.body.contains("\"parse_mode\":\"HTML\""));
    assert!(send_message.body.contains("Стадия: запуск"));
    let final_message = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured final message");
    assert_eq!(final_message.path, "/bottest-token/SendMessage");
    assert!(final_message.body.contains("Hello from group agent."));

    handle.join().expect("join fake api");
}

#[test]
fn telegram_worker_routes_group_text_without_bot_mention_from_paired_user() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app();
    seed_activated_pairing(&app, "pair-group-no-mention", 777, 42);
    let backend = RecordingTelegramBackend::with_state(RecordingTelegramBackendState {
        create_session_results: vec![session_summary("session-group-plain", "Group Chat", false)],
        next_chat_output: "Hello without mention.".to_string(),
        ..RecordingTelegramBackendState::default()
    });
    let backend_state = backend.state();
    let (api_url, requests, handle) = spawn_fake_telegram_api(vec![
        json_response(
            r#"{"ok":true,"result":[{"update_id":110,"message":{"message_id":33,"date":0,"chat":{"id":9000,"type":"group","title":"Team Chat"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"hello everyone"}}]}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":34,"date":0,"chat":{"id":9000,"type":"group","title":"Team Chat"},"text":"working"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":35,"date":0,"chat":{"id":9000,"type":"group","title":"Team Chat"},"text":"Hello without mention."}}"#,
        ),
    ]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker = TelegramWorker::with_consumer(app.clone(), backend, client, "telegram-test");

    runtime.block_on(worker.poll_once()).expect("poll once");
    drive_runtime(&runtime, Duration::from_millis(100));

    let binding = app
        .store()
        .expect("open store")
        .get_telegram_chat_binding(9000)
        .expect("get binding")
        .expect("binding exists");
    assert_eq!(
        binding.selected_session_id.as_deref(),
        Some("session-group-plain")
    );

    let state = backend_state.lock().expect("backend state");
    assert_eq!(state.created_titles, vec![None]);
    assert_eq!(
        state.executed_turns,
        vec![(
            "session-group-plain".to_string(),
            "hello everyone".to_string()
        )]
    );
    drop(state);

    let _ = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured getUpdates");
    let start_message = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured start message");
    assert_eq!(start_message.path, "/bottest-token/SendMessage");
    let final_message = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured final message");
    assert_eq!(final_message.path, "/bottest-token/SendMessage");
    assert!(final_message.body.contains("Hello without mention."));

    handle.join().expect("join fake api");
}

#[test]
fn telegram_worker_ignores_unpaired_group_text_without_bot_mention() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app();
    let backend = RecordingTelegramBackend::default();
    let backend_state = backend.state();
    let (api_url, requests, handle) = spawn_fake_telegram_api(vec![
        json_response(
            r#"{"ok":true,"result":[{"update_id":110,"message":{"message_id":33,"date":0,"chat":{"id":9000,"type":"group","title":"Team Chat"},"from":{"id":778,"is_bot":false,"first_name":"Bob","username":"bob"},"text":"hello everyone"}}]}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"id":9001,"is_bot":true,"first_name":"teamd","username":"teamd_agent_bot","can_join_groups":true,"can_read_all_group_messages":false,"supports_inline_queries":false,"has_main_web_app":false}}"#,
        ),
    ]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker = TelegramWorker::with_consumer(app.clone(), backend, client, "telegram-test");

    runtime.block_on(worker.poll_once()).expect("poll once");

    let binding = app
        .store()
        .expect("open store")
        .get_telegram_chat_binding(9000)
        .expect("get binding");
    assert!(binding.is_none());

    let state = backend_state.lock().expect("backend state");
    assert!(state.created_titles.is_empty());
    assert!(state.updated_preferences.is_empty());
    assert!(state.executed_turns.is_empty());
    drop(state);

    let get_updates = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured getUpdates");
    assert_eq!(get_updates.path, "/bottest-token/GetUpdates");
    let get_me = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured getMe");
    assert_eq!(get_me.path, "/bottest-token/GetMe");

    handle.join().expect("join fake api");
}

#[test]
fn telegram_worker_supports_group_sessions_new_and_use_commands() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app();
    seed_activated_pairing(&app, "pair-group-commands", 777, 42);
    app.store()
        .expect("open store")
        .put_telegram_chat_binding(&agent_persistence::TelegramChatBindingRecord {
            telegram_chat_id: 9000,
            scope: "group".to_string(),
            owner_telegram_user_id: None,
            selected_session_id: Some("session-1".to_string()),
            default_agent_profile_id: None,
            last_delivered_transcript_created_at: Some(0),
            last_delivered_transcript_id: Some(String::new()),
            inbound_queue_mode: "coalesce".to_string(),
            inbound_coalesce_window_ms: None,
            created_at: 10,
            updated_at: 10,
        })
        .expect("seed group binding");
    let backend = RecordingTelegramBackend::with_state(RecordingTelegramBackendState {
        listed_sessions: vec![
            session_summary("session-1", "Alpha", true),
            session_summary("session-2", "Beta", true),
        ],
        session_lookup: BTreeMap::from([
            (
                "session-1".to_string(),
                session_summary("session-1", "Alpha", true),
            ),
            (
                "session-2".to_string(),
                session_summary("session-2", "Beta", true),
            ),
            (
                "session-3".to_string(),
                session_summary("session-3", "Group War Room", false),
            ),
        ]),
        create_session_results: vec![session_summary("session-3", "Group War Room", false)],
        ..RecordingTelegramBackendState::default()
    });
    let backend_state = backend.state();
    let (api_url, requests, handle) = spawn_fake_telegram_api(vec![
        json_response(
            r#"{"ok":true,"result":[
                {"update_id":111,"message":{"message_id":34,"date":0,"chat":{"id":9000,"type":"group","title":"Team Chat"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/sessions"}},
                {"update_id":112,"message":{"message_id":35,"date":0,"chat":{"id":9000,"type":"group","title":"Team Chat"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/new Group War Room"}},
                {"update_id":113,"message":{"message_id":36,"date":0,"chat":{"id":9000,"type":"group","title":"Team Chat"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/use session-2"}}
            ]}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":37,"date":0,"chat":{"id":9000,"type":"group","title":"Team Chat"},"text":"sessions"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":38,"date":0,"chat":{"id":9000,"type":"group","title":"Team Chat"},"text":"new"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":39,"date":0,"chat":{"id":9000,"type":"group","title":"Team Chat"},"text":"use"}}"#,
        ),
    ]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker =
        TelegramWorker::with_consumer(app.clone(), backend.clone(), client, "telegram-test");

    let processed = runtime.block_on(worker.poll_once()).expect("poll once");
    assert_eq!(processed, 3);

    let binding = app
        .store()
        .expect("open store")
        .get_telegram_chat_binding(9000)
        .expect("get binding")
        .expect("binding exists");
    assert_eq!(binding.scope, "group");
    assert_eq!(binding.owner_telegram_user_id, None);
    assert_eq!(binding.selected_session_id.as_deref(), Some("session-2"));

    let state = backend_state.lock().expect("backend state");
    assert_eq!(
        state.created_titles,
        vec![Some("Group War Room".to_string())]
    );
    assert_eq!(
        state.updated_preferences,
        vec![
            (
                "session-3".to_string(),
                SessionPreferencesPatch {
                    auto_approve: Some(true),
                    reasoning_visible: Some(false),
                    think_level: Some(Some("off".to_string())),
                    ..SessionPreferencesPatch::default()
                }
            ),
            (
                "session-2".to_string(),
                SessionPreferencesPatch {
                    reasoning_visible: Some(false),
                    think_level: Some(Some("off".to_string())),
                    ..SessionPreferencesPatch::default()
                }
            ),
        ]
    );
    drop(state);

    let _ = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured getUpdates");
    let sessions_message = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured sessions response");
    assert!(sessions_message.body.contains("Recent sessions"));
    assert!(sessions_message.body.contains("> Alpha [1]"));
    assert!(sessions_message.body.contains("use=/use session-1"));
    assert!(sessions_message.body.contains("updated=1970-01-01"));
    let new_message = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured new response");
    assert!(new_message.body.contains("session-3"));
    let use_message = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured use response");
    assert!(use_message.body.contains("session-2"));

    handle.join().expect("join fake api");
}

#[test]
#[ignore = "same-poll group mention execution depends on active-turn fast-settle timing"]
fn telegram_worker_caches_bot_identity_across_group_mentions() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app();
    seed_activated_pairing(&app, "pair-group-cache", 777, 42);
    let backend = RecordingTelegramBackend::with_state(RecordingTelegramBackendState {
        create_session_results: vec![session_summary("session-group-1", "Group Chat", false)],
        next_chat_output: "Cached group reply.".to_string(),
        ..RecordingTelegramBackendState::default()
    });
    let backend_state = backend.state();
    let (api_url, requests, handle) = spawn_fake_telegram_api(vec![
        json_response(
            r#"{"ok":true,"result":[
                {"update_id":114,"message":{"message_id":40,"date":0,"chat":{"id":9000,"type":"group","title":"Team Chat"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"@teamd_agent_bot first"}},
                {"update_id":115,"message":{"message_id":41,"date":0,"chat":{"id":9000,"type":"group","title":"Team Chat"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"@teamd_agent_bot second"}}
            ]}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"id":9001,"is_bot":true,"first_name":"teamd","username":"teamd_agent_bot","can_join_groups":true,"can_read_all_group_messages":false,"supports_inline_queries":false,"has_main_web_app":false}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":42,"date":0,"chat":{"id":9000,"type":"group","title":"Team Chat"},"text":"working"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":42,"date":0,"chat":{"id":9000,"type":"group","title":"Team Chat"},"text":"Cached group reply."}}"#,
        ),
        json_response(r#"{"ok":true,"result":true}"#),
        json_response(
            r#"{"ok":true,"result":{"message_id":43,"date":0,"chat":{"id":9000,"type":"group","title":"Team Chat"},"text":"working"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":43,"date":0,"chat":{"id":9000,"type":"group","title":"Team Chat"},"text":"Cached group reply."}}"#,
        ),
    ]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker =
        TelegramWorker::with_consumer(app.clone(), backend.clone(), client, "telegram-test");

    let processed = runtime.block_on(worker.poll_once()).expect("poll once");
    assert_eq!(processed, 2);

    let binding = app
        .store()
        .expect("open store")
        .get_telegram_chat_binding(9000)
        .expect("get binding")
        .expect("binding exists");
    assert_eq!(
        binding.selected_session_id.as_deref(),
        Some("session-group-1")
    );

    let state = backend_state.lock().expect("backend state");
    assert_eq!(state.created_titles, vec![None]);
    assert_eq!(
        state.executed_turns,
        vec![
            ("session-group-1".to_string(), "first".to_string()),
            ("session-group-1".to_string(), "second".to_string()),
        ]
    );
    drop(state);

    let get_updates = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured getUpdates");
    assert_eq!(get_updates.path, "/bottest-token/GetUpdates");
    let get_me = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured single getMe");
    assert_eq!(get_me.path, "/bottest-token/GetMe");
    let ack_first = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured first ack");
    assert_eq!(ack_first.path, "/bottest-token/SendMessage");
    assert!(ack_first.body.contains("\"parse_mode\":\"HTML\""));
    assert!(ack_first.body.contains("Стадия: запуск"));
    let final_first = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured first final message");
    assert_eq!(final_first.path, "/bottest-token/SendMessage");
    assert!(final_first.body.contains("Cached group reply."));
    let delete_stale = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured stale status cleanup");
    assert_eq!(delete_stale.path, "/bottest-token/DeleteMessage");
    assert!(delete_stale.body.contains("\"message_id\":42"));
    let ack_second = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured second ack");
    assert_eq!(ack_second.path, "/bottest-token/SendMessage");
    assert!(ack_second.body.contains("\"parse_mode\":\"HTML\""));
    assert!(ack_second.body.contains("Стадия: запуск"));
    let final_second = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured second final message");
    assert_eq!(final_second.path, "/bottest-token/SendMessage");
    assert!(final_second.body.contains("Cached group reply."));

    handle.join().expect("join fake api");
}

#[test]
fn telegram_worker_preserves_existing_private_session_preferences_before_turn() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app();
    seed_activated_pairing(&app, "pair-activated", 777, 42);
    seed_binding(&app, 42, 777, Some("session-1"));
    let backend = RecordingTelegramBackend::with_state(RecordingTelegramBackendState {
        listed_sessions: vec![session_summary(
            "session-1",
            "Existing Telegram Chat",
            false,
        )],
        session_lookup: BTreeMap::from([(
            "session-1".to_string(),
            session_summary("session-1", "Existing Telegram Chat", false),
        )]),
        next_chat_output: "Hello from existing session.".to_string(),
        ..RecordingTelegramBackendState::default()
    });
    let backend_state = backend.state();
    let (api_url, requests, handle) = spawn_fake_telegram_api(vec![
        json_response(
            r#"{"ok":true,"result":[{"update_id":108,"message":{"message_id":23,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"reuse the selected session"}}]}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":24,"date":0,"chat":{"id":42,"type":"private"},"text":"working"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":24,"date":0,"chat":{"id":42,"type":"private"},"text":"Hello from existing session."}}"#,
        ),
    ]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker = TelegramWorker::with_consumer(app, backend, client, "telegram-test");

    runtime.block_on(worker.poll_once()).expect("poll once");

    let state = backend_state.lock().expect("backend state");
    assert!(state.updated_preferences.is_empty());
    assert_eq!(
        state.executed_turns,
        vec![(
            "session-1".to_string(),
            "reuse the selected session".to_string()
        )]
    );
    drop(state);

    let _ = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured getUpdates");
    let _ = requests
        .recv_timeout(Duration::from_secs(10))
        .expect("captured ack");
    let final_message = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured final message");
    assert_eq!(final_message.path, "/bottest-token/SendMessage");
    assert!(final_message.body.contains("Hello from existing session."));

    handle.join().expect("join fake api");
}

#[test]
fn telegram_worker_supports_new_sessions_listing_and_use_command() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app();
    seed_activated_pairing(&app, "pair-activated", 777, 42);
    seed_binding(&app, 42, 777, Some("session-1"));
    let backend = RecordingTelegramBackend::with_state(RecordingTelegramBackendState {
        listed_sessions: vec![
            session_summary("session-1", "Alpha", true),
            session_summary("session-2", "Beta", true),
        ],
        session_lookup: BTreeMap::from([
            (
                "session-1".to_string(),
                session_summary("session-1", "Alpha", true),
            ),
            (
                "session-2".to_string(),
                session_summary("session-2", "Beta", true),
            ),
            (
                "session-3".to_string(),
                session_summary("session-3", "Created from /new", false),
            ),
        ]),
        create_session_results: vec![session_summary("session-3", "Created from /new", false)],
        ..RecordingTelegramBackendState::default()
    });
    let backend_state = backend.state();
    let (api_url, requests, handle) = spawn_fake_telegram_api(vec![
        json_response(
            r#"{"ok":true,"result":[
                {"update_id":103,"message":{"message_id":13,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/sessions"}},
                {"update_id":104,"message":{"message_id":14,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/new Created from /new"}},
                {"update_id":105,"message":{"message_id":15,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/use session-2"}}
            ]}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":16,"date":0,"chat":{"id":42,"type":"private"},"text":"sessions"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":17,"date":0,"chat":{"id":42,"type":"private"},"text":"new"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":18,"date":0,"chat":{"id":42,"type":"private"},"text":"use"}}"#,
        ),
    ]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker =
        TelegramWorker::with_consumer(app.clone(), backend.clone(), client, "telegram-test");

    let processed = runtime.block_on(worker.poll_once()).expect("poll once");
    assert_eq!(processed, 3);

    let binding = app
        .store()
        .expect("open store")
        .get_telegram_chat_binding(42)
        .expect("get binding")
        .expect("binding exists");
    assert_eq!(binding.selected_session_id.as_deref(), Some("session-2"));

    let state = backend_state.lock().expect("backend state");
    assert_eq!(
        state.created_titles,
        vec![Some("Created from /new".to_string())]
    );
    assert_eq!(
        state.updated_preferences,
        vec![
            (
                "session-3".to_string(),
                SessionPreferencesPatch {
                    auto_approve: Some(true),
                    reasoning_visible: Some(false),
                    think_level: Some(Some("off".to_string())),
                    ..SessionPreferencesPatch::default()
                }
            ),
            (
                "session-2".to_string(),
                SessionPreferencesPatch {
                    reasoning_visible: Some(false),
                    think_level: Some(Some("off".to_string())),
                    ..SessionPreferencesPatch::default()
                }
            ),
        ]
    );
    drop(state);

    let _ = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured getUpdates");
    let sessions_message = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured sessions response");
    assert!(sessions_message.body.contains("Recent sessions"));
    assert!(sessions_message.body.contains("> Alpha [1]"));
    assert!(sessions_message.body.contains("use=/use session-1"));
    let new_message = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured new response");
    assert!(new_message.body.contains("session-3"));
    let use_message = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured use response");
    assert!(use_message.body.contains("session-2"));

    handle.join().expect("join fake api");
}

#[test]
fn telegram_worker_switches_chat_agent_and_creates_agent_sessions() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app();
    seed_activated_pairing(&app, "pair-agent-switch", 777, 42);
    let backend = RecordingTelegramBackend::with_state(RecordingTelegramBackendState {
        agent_summaries: vec![
            agent_summary("default", "Assistant", "default"),
            agent_summary("judge", "Judge", "judge"),
        ],
        create_session_results: vec![
            session_summary("session-judge", "Review room", false),
            session_summary("session-default", "Default room", false),
        ],
        ..RecordingTelegramBackendState::default()
    });
    let backend_state = backend.state();
    let (api_url, requests, handle) = spawn_fake_telegram_api(vec![
        json_response(
            r#"{"ok":true,"result":[
                {"update_id":301,"message":{"message_id":201,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/agents"}},
                {"update_id":302,"message":{"message_id":202,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/agentuse judge"}},
                {"update_id":303,"message":{"message_id":203,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/new Review room"}},
                {"update_id":304,"message":{"message_id":204,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/newagent default Default room"}},
                {"update_id":305,"message":{"message_id":205,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/status"}}
            ]}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":205,"date":0,"chat":{"id":42,"type":"private"},"text":"agents"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":206,"date":0,"chat":{"id":42,"type":"private"},"text":"agentuse"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":207,"date":0,"chat":{"id":42,"type":"private"},"text":"new"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":208,"date":0,"chat":{"id":42,"type":"private"},"text":"newagent"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":209,"date":0,"chat":{"id":42,"type":"private"},"text":"status"}}"#,
        ),
    ]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker =
        TelegramWorker::with_consumer(app.clone(), backend.clone(), client, "telegram-test");

    let processed = runtime.block_on(worker.poll_once()).expect("poll once");
    assert_eq!(processed, 5);

    let binding = app
        .store()
        .expect("open store")
        .get_telegram_chat_binding(42)
        .expect("get binding")
        .expect("binding exists");
    assert_eq!(
        binding.selected_session_id.as_deref(),
        Some("session-default")
    );
    assert_eq!(binding.default_agent_profile_id.as_deref(), Some("default"));

    let state = backend_state.lock().expect("backend state");
    assert_eq!(
        state.created_titles,
        vec![
            Some("Review room".to_string()),
            Some("Default room".to_string())
        ]
    );
    assert_eq!(
        state.created_agent_identifiers,
        vec![Some("judge".to_string()), Some("default".to_string())]
    );
    drop(state);

    let _ = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured getUpdates");
    let agents_message = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured agents response");
    assert!(agents_message.body.contains("Agents"));
    assert!(agents_message.body.contains("Judge (judge)"));
    let agentuse_message = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured agentuse response");
    assert!(agentuse_message.body.contains("Chat default agent"));
    assert!(agentuse_message.body.contains("Judge (judge)"));
    let new_message = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured new response");
    assert!(new_message.body.contains("session-judge"));
    let newagent_message = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured newagent response");
    assert!(newagent_message.body.contains("session-default"));
    let status_message = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured status response");
    assert!(status_message.body.contains("chat_default_agent: default"));

    handle.join().expect("join fake api");
}

#[test]
fn telegram_worker_advances_cursor_after_single_update_handler_error() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app();
    seed_activated_pairing(&app, "pair-activated", 777, 42);
    seed_binding(&app, 42, 777, Some("session-1"));
    let backend = RecordingTelegramBackend::with_state(RecordingTelegramBackendState {
        listed_sessions: vec![session_summary("session-1", "Alpha", true)],
        session_lookup: BTreeMap::from([(
            "session-1".to_string(),
            session_summary("session-1", "Alpha", true),
        )]),
        ..RecordingTelegramBackendState::default()
    });
    let (api_url, requests, handle) = spawn_fake_telegram_api(vec![
        json_response(
            r#"{"ok":true,"result":[
                {"update_id":200,"message":{"message_id":30,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/use session-missing"}},
                {"update_id":201,"message":{"message_id":31,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"/sessions"}}
            ]}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":32,"date":0,"chat":{"id":42,"type":"private"},"text":"sessions"}}"#,
        ),
    ]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker =
        TelegramWorker::with_consumer(app.clone(), backend.clone(), client, "telegram-test");

    let processed = runtime.block_on(worker.poll_once()).expect("poll once");
    assert_eq!(processed, 2);

    let cursor = app
        .store()
        .expect("open store")
        .get_telegram_update_cursor("telegram-test")
        .expect("get cursor")
        .expect("cursor exists");
    assert_eq!(cursor.update_id, 202);

    let _ = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured getUpdates");
    let sessions_message = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured sessions response");
    assert!(sessions_message.body.contains("Recent sessions"));
    assert!(sessions_message.body.contains("> Alpha [1]"));
    assert!(sessions_message.body.contains("use=/use session-1"));

    handle.join().expect("join fake api");
}

#[test]
fn telegram_worker_rate_limits_progress_edits_on_the_status_message() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app_with_progress_interval_ms(10);
    seed_activated_pairing(&app, "pair-activated", 777, 42);
    let backend = RecordingTelegramBackend::with_state(RecordingTelegramBackendState {
        create_session_results: vec![session_summary("session-1", "Telegram Chat", false)],
        next_chat_output: "Final reply.".to_string(),
        execution_events: vec![(
            5,
            ChatExecutionEvent::ToolStatus {
                tool_call_id: "call_web_search_1".to_string(),
                tool_name: "web_search".to_string(),
                summary: "Fetching results".to_string(),
                status: agentd::execution::ToolExecutionStatus::Running,
            },
        )],
        response_delay_ms: 20,
        ..RecordingTelegramBackendState::default()
    });
    let (api_url, requests, handle) = spawn_fake_telegram_api(vec![
        json_response(
            r#"{"ok":true,"result":[{"update_id":107,"message":{"message_id":21,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"slow progress please"}}]}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":22,"date":0,"chat":{"id":42,"type":"private"},"text":"working"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":22,"date":0,"chat":{"id":42,"type":"private"},"text":"progress"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":22,"date":0,"chat":{"id":42,"type":"private"},"text":"final"}}"#,
        ),
    ]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker = TelegramWorker::with_consumer(app, backend, client, "telegram-test");

    runtime.block_on(worker.poll_once()).expect("poll once");
    drive_runtime(&runtime, Duration::from_millis(100));

    let _ = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured getUpdates");
    let send_message = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured ack");
    assert_eq!(send_message.path, "/bottest-token/SendMessage");
    assert!(send_message.body.contains("Стадия: запуск"));
    let progress_edit = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured progress edit");
    assert_eq!(progress_edit.path, "/bottest-token/EditMessageText");
    assert!(progress_edit.body.contains("Работаю с вебом"));
    assert!(
        progress_edit
            .body
            .contains("Инструмент: <code>web_search</code>")
    );
    assert!(progress_edit.body.contains("Статус: выполняется"));
    assert!(progress_edit.body.contains("Fetching results"));
    assert!(progress_edit.body.contains("Вызовы: 1"));
    let final_message = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured final message");
    assert_eq!(final_message.path, "/bottest-token/SendMessage");
    assert!(final_message.body.contains("Final reply."));

    handle.join().expect("join fake api");
}

#[test]
fn telegram_worker_bounds_long_progress_status_edits() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app_with_progress_interval_ms(10);
    seed_activated_pairing(&app, "pair-long-progress", 777, 42);
    let long_summary = format!("exec_wait process_id=exec-10 stdout={}", "x".repeat(8_000));
    let backend = RecordingTelegramBackend::with_state(RecordingTelegramBackendState {
        create_session_results: vec![session_summary("session-1", "Telegram Chat", false)],
        next_chat_output: "Final reply after long tool output.".to_string(),
        execution_events: vec![(
            5,
            ChatExecutionEvent::ToolStatus {
                tool_call_id: "call_exec_wait_10".to_string(),
                tool_name: "exec_wait".to_string(),
                summary: long_summary,
                status: agentd::execution::ToolExecutionStatus::Running,
            },
        )],
        response_delay_ms: 20,
        ..RecordingTelegramBackendState::default()
    });
    let (api_url, requests, handle) = spawn_fake_telegram_api(vec![
        json_response(
            r#"{"ok":true,"result":[{"update_id":134,"message":{"message_id":80,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"long progress please"}}]}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":81,"date":0,"chat":{"id":42,"type":"private"},"text":"working"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":81,"date":0,"chat":{"id":42,"type":"private"},"text":"progress"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":82,"date":0,"chat":{"id":42,"type":"private"},"text":"final"}}"#,
        ),
    ]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker = TelegramWorker::with_consumer(app, backend, client, "telegram-test");

    runtime.block_on(worker.poll_once()).expect("poll once");
    drive_runtime(&runtime, Duration::from_millis(100));

    let _ = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured getUpdates");
    let _ = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured ack");
    let progress_edit = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured progress edit");
    assert_eq!(progress_edit.path, "/bottest-token/EditMessageText");
    assert!(progress_edit.body.contains("exec_wait"));
    assert!(
        progress_edit.body.len() <= TELEGRAM_MESSAGE_TEXT_SOFT_CAP + 512,
        "progress edit body should stay below Telegram message limits, got {} bytes",
        progress_edit.body.len()
    );
    assert!(!progress_edit.body.contains(&"x".repeat(4_000)));
    let final_message = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured final message");
    assert_eq!(final_message.path, "/bottest-token/SendMessage");
    assert!(
        final_message
            .body
            .contains("Final reply after long tool output.")
    );

    handle.join().expect("join fake api");
}

#[test]
fn telegram_worker_keeps_turn_alive_when_progress_edit_is_too_long() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app_with_progress_interval_ms(10);
    seed_activated_pairing(&app, "pair-progress-too-long", 777, 42);
    let backend = RecordingTelegramBackend::with_state(RecordingTelegramBackendState {
        create_session_results: vec![session_summary("session-1", "Telegram Chat", false)],
        next_chat_output: "Final reply after rejected status edit.".to_string(),
        execution_events: vec![(
            5,
            ChatExecutionEvent::ToolStatus {
                tool_call_id: "call_exec_wait_10".to_string(),
                tool_name: "exec_wait".to_string(),
                summary: "exec_wait process_id=exec-10".to_string(),
                status: agentd::execution::ToolExecutionStatus::Running,
            },
        )],
        response_delay_ms: 20,
        ..RecordingTelegramBackendState::default()
    });
    let (api_url, requests, handle) = spawn_fake_telegram_api(vec![
        json_response(
            r#"{"ok":true,"result":[{"update_id":135,"message":{"message_id":90,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"status too long please"}}]}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":91,"date":0,"chat":{"id":42,"type":"private"},"text":"working"}}"#,
        ),
        json_response(
            r#"{"ok":false,"error_code":400,"description":"Bad Request: MESSAGE_TOO_LONG"}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":92,"date":0,"chat":{"id":42,"type":"private"},"text":"final"}}"#,
        ),
    ]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker = TelegramWorker::with_consumer(app, backend, client, "telegram-test");

    runtime.block_on(worker.poll_once()).expect("poll once");
    drive_runtime(&runtime, Duration::from_secs(1));

    let _ = requests
        .recv_timeout(Duration::from_secs(10))
        .expect("captured getUpdates");
    let _ = requests
        .recv_timeout(Duration::from_secs(10))
        .expect("captured ack");
    let failed_edit = requests
        .recv_timeout(Duration::from_secs(10))
        .expect("captured rejected progress edit");
    assert_eq!(failed_edit.path, "/bottest-token/EditMessageText");
    let final_message = requests
        .recv_timeout(Duration::from_secs(10))
        .expect("captured final message");
    assert_eq!(final_message.path, "/bottest-token/SendMessage");
    assert!(
        final_message
            .body
            .contains("Final reply after rejected status edit.")
    );

    handle.join().expect("join fake api");
}

#[test]
fn telegram_worker_reports_drafting_phase_before_final_reply() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app_with_progress_interval_ms(10);
    seed_activated_pairing(&app, "pair-drafting", 777, 42);
    let backend = RecordingTelegramBackend::with_state(RecordingTelegramBackendState {
        create_session_results: vec![session_summary("session-1", "Telegram Chat", false)],
        next_chat_output: "Final reply.".to_string(),
        execution_events: vec![(
            5,
            ChatExecutionEvent::AssistantTextDelta("Hello".to_string()),
        )],
        response_delay_ms: 20,
        ..RecordingTelegramBackendState::default()
    });
    let mut update_responses = VecDeque::from([
        r#"{"ok":true,"result":[{"update_id":108,"message":{"message_id":21,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"draft please"}}]}"#,
    ]);
    let (api_url, requests, handle) = spawn_routed_telegram_api(move |request| {
        if request.path.ends_with("/GetUpdates") {
            return json_response(
                update_responses
                    .pop_front()
                    .unwrap_or(r#"{"ok":true,"result":[]}"#),
            );
        }
        if request.path.ends_with("/SendMessage") || request.path.ends_with("/EditMessageText") {
            return json_response(
                r#"{"ok":true,"result":{"message_id":22,"date":0,"chat":{"id":42,"type":"private"},"text":"ok"}}"#,
            );
        }
        json_response(r#"{"ok":true,"result":true}"#)
    });
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker = TelegramWorker::with_consumer(app, backend, client, "telegram-test");

    runtime.block_on(worker.poll_once()).expect("poll once");
    drive_runtime(&runtime, Duration::from_secs(1));

    let captured = drain_requests_for(&requests, Duration::from_secs(10));
    assert!(
        captured
            .iter()
            .any(|request| request.path.ends_with("/GetUpdates")),
        "expected getUpdates, captured: {captured:?}"
    );
    assert!(
        captured.iter().any(|request| {
            request.path.ends_with("/EditMessageText") && request.body.contains("Пишу ответ")
        }),
        "expected drafting progress edit, captured: {captured:?}"
    );
    assert!(
        captured.iter().any(|request| {
            request.path.ends_with("/SendMessage") && request.body.contains("Final reply.")
        }),
        "expected final message, captured: {captured:?}"
    );

    handle.stop().expect("stop fake api");
}

#[test]
fn telegram_worker_sends_final_reply_as_a_new_message_after_temporary_status() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app_with_progress_interval_ms(10);
    seed_activated_pairing(&app, "pair-separate-final", 777, 42);
    let backend = RecordingTelegramBackend::with_state(RecordingTelegramBackendState {
        create_session_results: vec![session_summary("session-1", "Telegram Chat", false)],
        next_chat_output: "Final reply.".to_string(),
        ..RecordingTelegramBackendState::default()
    });
    let (api_url, requests, handle) = spawn_fake_telegram_api(vec![
        json_response(
            r#"{"ok":true,"result":[{"update_id":109,"message":{"message_id":21,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"separate final please"}}]}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":22,"date":0,"chat":{"id":42,"type":"private"},"text":"working"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":23,"date":0,"chat":{"id":42,"type":"private"},"text":"final"}}"#,
        ),
    ]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker = TelegramWorker::with_consumer(app, backend, client, "telegram-test");

    runtime.block_on(worker.poll_once()).expect("poll once");

    let _ = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured getUpdates");
    let status_message = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured temporary status message");
    assert_eq!(status_message.path, "/bottest-token/SendMessage");
    let final_message = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured final response");
    assert_eq!(final_message.path, "/bottest-token/SendMessage");
    assert!(final_message.body.contains("\"text\":\"Final reply.\""));

    handle.join().expect("join fake api");
}

#[test]
fn telegram_worker_deletes_previous_temporary_status_on_next_user_message() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app_with_progress_interval_ms(10);
    seed_activated_pairing(&app, "pair-delete-stale", 777, 42);
    let backend = RecordingTelegramBackend::with_state(RecordingTelegramBackendState {
        create_session_results: vec![session_summary("session-1", "Telegram Chat", false)],
        next_chat_output: "Final reply.".to_string(),
        ..RecordingTelegramBackendState::default()
    });
    let (api_url, requests, handle) = spawn_fake_telegram_api(vec![
        json_response(
            r#"{"ok":true,"result":[{"update_id":120,"message":{"message_id":21,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"first turn"}}]}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":22,"date":0,"chat":{"id":42,"type":"private"},"text":"status 1"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":23,"date":0,"chat":{"id":42,"type":"private"},"text":"final 1"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":[{"update_id":121,"message":{"message_id":24,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"second turn"}}]}"#,
        ),
        json_response(r#"{"ok":true,"result":true}"#),
        json_response(
            r#"{"ok":true,"result":{"message_id":25,"date":0,"chat":{"id":42,"type":"private"},"text":"status 2"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":26,"date":0,"chat":{"id":42,"type":"private"},"text":"final 2"}}"#,
        ),
    ]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker = TelegramWorker::with_consumer(app, backend, client, "telegram-test");

    runtime.block_on(worker.poll_once()).expect("first poll");
    runtime.block_on(worker.poll_once()).expect("second poll");

    let _ = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured first getUpdates");
    let _ = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured first status message");
    let _ = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured first final message");
    let _ = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured second getUpdates");
    let delete_request = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured stale status delete");
    assert_eq!(delete_request.path, "/bottest-token/DeleteMessage");
    assert!(delete_request.body.contains("\"message_id\":22"));
    let second_status = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured second status message");
    assert_eq!(second_status.path, "/bottest-token/SendMessage");
    let second_final = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured second final message");
    assert_eq!(second_final.path, "/bottest-token/SendMessage");

    handle.join().expect("join fake api");
}

#[test]
fn telegram_worker_deletes_expired_stale_status_during_polling() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app_with_progress_interval_ms(10);
    app.store()
        .expect("open store")
        .put_telegram_chat_status(&agent_persistence::TelegramChatStatusRecord {
            telegram_chat_id: 42,
            message_id: 333,
            state: "stale".to_string(),
            expires_at: Some(1),
            created_at: 1,
            updated_at: 1,
        })
        .expect("seed expired status");
    let backend = RecordingTelegramBackend::default();
    let (api_url, requests, handle) = spawn_fake_telegram_api(vec![
        json_response(r#"{"ok":true,"result":true}"#),
        json_response(r#"{"ok":true,"result":[]}"#),
    ]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker = TelegramWorker::with_consumer(app.clone(), backend, client, "telegram-test");

    runtime.block_on(worker.poll_once()).expect("poll once");

    let delete_request = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured deleteMessage");
    assert_eq!(delete_request.path, "/bottest-token/DeleteMessage");
    assert!(delete_request.body.contains("\"message_id\":333"));
    let get_updates = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured getUpdates");
    assert_eq!(get_updates.path, "/bottest-token/GetUpdates");
    assert_eq!(
        app.store()
            .expect("open store")
            .get_telegram_chat_status(42)
            .expect("status lookup"),
        None
    );

    handle.join().expect("join fake api");
}

#[test]
fn telegram_worker_sends_typing_while_turn_is_running() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app_with_progress_interval_ms(10);
    seed_activated_pairing(&app, "pair-typing", 777, 42);
    let backend = RecordingTelegramBackend::with_state(RecordingTelegramBackendState {
        create_session_results: vec![session_summary("session-1", "Telegram Chat", false)],
        next_chat_output: "Final reply.".to_string(),
        response_delay_ms: 900,
        ..RecordingTelegramBackendState::default()
    });
    let (api_url, requests, handle) = spawn_fake_telegram_api(vec![
        json_response(
            r#"{"ok":true,"result":[{"update_id":122,"message":{"message_id":21,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"show typing"}}]}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":22,"date":0,"chat":{"id":42,"type":"private"},"text":"status"}}"#,
        ),
        json_response(r#"{"ok":true,"result":true}"#),
        json_response(
            r#"{"ok":true,"result":{"message_id":23,"date":0,"chat":{"id":42,"type":"private"},"text":"final"}}"#,
        ),
    ]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker = TelegramWorker::with_consumer(app, backend, client, "telegram-test");

    runtime.block_on(worker.poll_once()).expect("poll once");
    drive_runtime(&runtime, Duration::from_millis(1_000));

    let _ = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured getUpdates");
    let status_message = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured status message");
    assert_eq!(status_message.path, "/bottest-token/SendMessage");
    let typing_request = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured sendChatAction");
    assert_eq!(typing_request.path, "/bottest-token/SendChatAction");
    assert!(typing_request.body.contains("\"action\":\"typing\""));
    let final_message = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured final message");
    assert_eq!(final_message.path, "/bottest-token/SendMessage");

    handle.join().expect("join fake api");
}

#[test]
fn telegram_worker_formats_markdown_reply_as_html() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app();
    seed_activated_pairing(&app, "pair-markdown", 777, 42);
    let backend = RecordingTelegramBackend::with_state(RecordingTelegramBackendState {
        create_session_results: vec![session_summary("session-1", "Telegram Chat", false)],
        next_chat_output: "# Title\n\n**bold** and `code` and [link](https://example.com)"
            .to_string(),
        ..RecordingTelegramBackendState::default()
    });
    let (api_url, requests, handle) = spawn_fake_telegram_api(vec![
        json_response(
            r#"{"ok":true,"result":[{"update_id":116,"message":{"message_id":44,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"format markdown please"}}]}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":45,"date":0,"chat":{"id":42,"type":"private"},"text":"working"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":45,"date":0,"chat":{"id":42,"type":"private"},"text":"formatted"}}"#,
        ),
    ]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker = TelegramWorker::with_consumer(app, backend, client, "telegram-test");

    runtime.block_on(worker.poll_once()).expect("poll once");

    let _ = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured getUpdates");
    let _ = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured ack");
    let final_message = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured final message");
    assert_eq!(final_message.path, "/bottest-token/SendMessage");
    assert!(final_message.body.contains("\"parse_mode\":\"HTML\""));
    assert!(final_message.body.contains("<b>Title</b>"));
    assert!(final_message.body.contains("<b>bold</b>"));
    assert!(final_message.body.contains("<code>code</code>"));
    assert!(
        final_message
            .body
            .contains("<a href=\\\"https://example.com\\\">")
    );
    assert!(final_message.body.contains(">link</a>"));
    assert!(!final_message.body.contains("**bold**"));

    handle.join().expect("join fake api");
}

#[test]
fn telegram_worker_renders_markdown_tables_as_preformatted_html() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app();
    seed_activated_pairing(&app, "pair-markdown-table", 777, 42);
    let backend = RecordingTelegramBackend::with_state(RecordingTelegramBackendState {
        create_session_results: vec![session_summary("session-1", "Telegram Chat", false)],
        next_chat_output: [
            "Готово!",
            "",
            "| Заметка | Что внутри |",
            "| --- | --- |",
            "| Дом.md | Главная зона |",
            "| Гарвардская тарелка.md | Чеклист на 2 недели |",
        ]
        .join("\n"),
        ..RecordingTelegramBackendState::default()
    });
    let (api_url, requests, handle) = spawn_fake_telegram_api(vec![
        json_response(
            r#"{"ok":true,"result":[{"update_id":117,"message":{"message_id":46,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"format markdown table please"}}]}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":47,"date":0,"chat":{"id":42,"type":"private"},"text":"working"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":47,"date":0,"chat":{"id":42,"type":"private"},"text":"formatted table"}}"#,
        ),
    ]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker = TelegramWorker::with_consumer(app, backend, client, "telegram-test");

    runtime.block_on(worker.poll_once()).expect("poll once");

    let _ = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured getUpdates");
    let _ = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured ack");
    let final_message = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured final message");
    assert_eq!(final_message.path, "/bottest-token/SendMessage");
    assert!(final_message.body.contains("\"parse_mode\":\"HTML\""));
    assert!(final_message.body.contains("<pre><code>| Заметка"));
    assert!(final_message.body.contains("| Дом.md"));
    assert!(final_message.body.contains("| Гарвардская тарелка.md"));
    assert!(final_message.body.contains("</code></pre>"));
    assert!(!final_message.body.contains("ЗаметкаЧто внутри"));

    handle.join().expect("join fake api");
}

#[test]
fn telegram_worker_delivers_new_assistant_transcript_for_bound_session() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app();
    seed_activated_pairing(&app, "pair-reminder", 777, 42);
    app.create_session("session-reminder", "Reminder")
        .expect("create session");
    app.store()
        .expect("open store")
        .put_telegram_chat_binding(&agent_persistence::TelegramChatBindingRecord {
            telegram_chat_id: 42,
            scope: "private".to_string(),
            owner_telegram_user_id: Some(777),
            selected_session_id: Some("session-reminder".to_string()),
            default_agent_profile_id: None,
            last_delivered_transcript_created_at: Some(10),
            last_delivered_transcript_id: Some("transcript-old".to_string()),
            inbound_queue_mode: "coalesce".to_string(),
            inbound_coalesce_window_ms: None,
            created_at: 10,
            updated_at: 10,
        })
        .expect("seed binding");
    app.store()
        .expect("open store")
        .put_transcript(&TranscriptRecord {
            id: "transcript-reminder-assistant".to_string(),
            session_id: "session-reminder".to_string(),
            run_id: None,
            kind: "assistant".to_string(),
            content: "Reminder: **stand up** now.".to_string(),
            created_at: 20,
        })
        .expect("seed assistant transcript");

    let backend = RecordingTelegramBackend::default();
    let (api_url, requests, handle) = spawn_fake_telegram_api(vec![
        json_response(
            r#"{"ok":true,"result":{"message_id":51,"date":0,"chat":{"id":42,"type":"private"},"text":"reminder"}}"#,
        ),
        json_response(r#"{"ok":true,"result":[]}"#),
    ]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker = TelegramWorker::with_consumer(app.clone(), backend, client, "telegram-test");

    runtime.block_on(worker.poll_once()).expect("poll once");

    let sent = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured scheduled assistant message");
    assert_eq!(sent.path, "/bottest-token/SendMessage");
    assert!(sent.body.contains("\"parse_mode\":\"HTML\""));
    assert!(sent.body.contains("Reminder:"));
    assert!(sent.body.contains("<b>stand up</b>"));
    let get_updates = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("captured getUpdates");
    assert_eq!(get_updates.path, "/bottest-token/GetUpdates");

    let binding = app
        .store()
        .expect("open store")
        .get_telegram_chat_binding(42)
        .expect("get binding")
        .expect("binding exists");
    assert_eq!(
        binding.last_delivered_transcript_id.as_deref(),
        Some("transcript-reminder-assistant")
    );

    handle.join().expect("join fake api");
}

#[test]
fn telegram_worker_flushes_pending_transcripts_before_new_inbound_turn() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (_temp, app) = telegram_test_app();
    seed_activated_pairing(&app, "pair-reminder-inbound", 777, 42);
    app.create_session("session-reminder", "Reminder")
        .expect("create session");
    app.store()
        .expect("open store")
        .put_telegram_chat_binding(&agent_persistence::TelegramChatBindingRecord {
            telegram_chat_id: 42,
            scope: "private".to_string(),
            owner_telegram_user_id: Some(777),
            selected_session_id: Some("session-reminder".to_string()),
            default_agent_profile_id: None,
            last_delivered_transcript_created_at: Some(10),
            last_delivered_transcript_id: Some("transcript-old".to_string()),
            inbound_queue_mode: "coalesce".to_string(),
            inbound_coalesce_window_ms: None,
            created_at: 10,
            updated_at: 10,
        })
        .expect("seed binding");
    app.store()
        .expect("open store")
        .put_transcript(&TranscriptRecord {
            id: "transcript-scheduled-assistant".to_string(),
            session_id: "session-reminder".to_string(),
            run_id: None,
            kind: "assistant".to_string(),
            content: "Scheduled hello from the past.".to_string(),
            created_at: 20,
        })
        .expect("seed scheduled assistant transcript");

    let backend = RecordingTelegramBackend::with_state(RecordingTelegramBackendState {
        session_lookup: BTreeMap::from([(
            "session-reminder".to_string(),
            session_summary("session-reminder", "Reminder", false),
        )]),
        next_chat_output: "Direct reply after inbound message.".to_string(),
        ..RecordingTelegramBackendState::default()
    });
    let (api_url, requests, handle) = spawn_fake_telegram_api(vec![
        json_response(
            r#"{"ok":true,"result":{"message_id":50,"date":0,"chat":{"id":42,"type":"private"},"text":"scheduled"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":[{"update_id":117,"message":{"message_id":44,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"new message after reminder"}}]}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":51,"date":0,"chat":{"id":42,"type":"private"},"text":"working"}}"#,
        ),
        json_response(
            r#"{"ok":true,"result":{"message_id":51,"date":0,"chat":{"id":42,"type":"private"},"text":"direct"}}"#,
        ),
    ]);
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker = TelegramWorker::with_consumer(app.clone(), backend, client, "telegram-test");

    runtime.block_on(worker.poll_once()).expect("poll once");
    drive_runtime(&runtime, Duration::from_secs(1));

    let scheduled = requests
        .recv_timeout(Duration::from_secs(10))
        .expect("captured scheduled delivery");
    assert_eq!(scheduled.path, "/bottest-token/SendMessage");
    assert!(scheduled.body.contains("Scheduled hello from the past."));
    let get_updates = requests
        .recv_timeout(Duration::from_secs(10))
        .expect("captured getUpdates");
    assert_eq!(get_updates.path, "/bottest-token/GetUpdates");
    let working = requests
        .recv_timeout(Duration::from_secs(10))
        .expect("captured working ack");
    assert_eq!(working.path, "/bottest-token/SendMessage");
    assert!(working.body.contains("Стадия: запуск"));
    let direct = requests
        .recv_timeout(Duration::from_secs(10))
        .expect("captured direct final message");
    assert_eq!(direct.path, "/bottest-token/SendMessage");
    assert!(direct.body.contains("Direct reply after inbound message."));

    let binding = app
        .store()
        .expect("open store")
        .get_telegram_chat_binding(42)
        .expect("get binding")
        .expect("binding exists");
    assert_eq!(
        binding.last_delivered_transcript_id.as_deref(),
        Some("transcript-scheduled-assistant")
    );

    handle.join().expect("join fake api");
}

#[test]
fn telegram_worker_real_daemon_backend_uses_canonical_chat_path() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");
    let (provider_base, provider_requests, provider_handle) =
        spawn_provider_sse_server_with_response_delay(vec![
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_tg_worker\",\"model\":\"gpt-5.4\",\"output\":[{\"id\":\"msg_tg_worker\",\"type\":\"message\",\"status\":\"completed\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"Provider-backed telegram reply.\"}]}],\"usage\":{\"input_tokens\":12,\"output_tokens\":5,\"total_tokens\":17}}}\n\n".to_string(),
        ], Duration::from_millis(900));
    let (_temp, app) = telegram_daemon_test_app(&provider_base);
    let handle = daemon::spawn_for_test(app.clone()).expect("spawn daemon");
    seed_activated_pairing(&app, "pair-real", 777, 42);
    let backend = DaemonTelegramBackend::new(DaemonClient::new(
        &app.config,
        &DaemonConnectOptions::default(),
    ));
    let (api_url, requests, telegram_handle) = spawn_routed_telegram_api(|request| {
        if request.path.ends_with("/GetUpdates") {
            return json_response(
                r#"{"ok":true,"result":[{"update_id":106,"message":{"message_id":19,"date":0,"chat":{"id":42,"type":"private"},"from":{"id":777,"is_bot":false,"first_name":"Alice","username":"alice"},"text":"hello from real backend"}}]}"#,
            );
        }
        if request.path.ends_with("/EditMessageText")
            || request.path.ends_with("/SendChatAction")
            || request.path.ends_with("/DeleteMessage")
        {
            return json_response(r#"{"ok":true,"result":true}"#);
        }
        if request.path.ends_with("/SendMessage") {
            return json_response(
                r#"{"ok":true,"result":{"message_id":20,"date":0,"chat":{"id":42,"type":"private"},"text":"telegram message"}}"#,
            );
        }
        json_response(r#"{"ok":true,"result":true}"#)
    });
    let client = TelegramClient::new(TelegramClientConfig {
        token: "test-token".to_string(),
        api_url: Some(api_url),
        poll_request_timeout_seconds: 40,
    })
    .expect("telegram client");
    let worker =
        TelegramWorker::with_consumer(app.clone(), backend.clone(), client, "telegram-test");

    runtime.block_on(worker.poll_once()).expect("poll once");
    drive_runtime(&runtime, Duration::from_secs(5));

    let store = app.store().expect("open store");
    let binding = store
        .get_telegram_chat_binding(42)
        .expect("get binding")
        .expect("binding exists");
    let session_id = binding
        .selected_session_id
        .clone()
        .expect("selected session id");
    let transcripts = store
        .list_transcripts_for_session(&session_id)
        .expect("list transcripts");
    assert!(
        transcripts
            .iter()
            .any(|entry| { entry.kind == "user" && entry.content == "hello from real backend" })
    );
    let transcript_view = backend
        .client()
        .session_transcript(&session_id)
        .expect("session transcript view");
    assert!(
        transcript_view.entries.iter().any(|entry| {
            entry.role == "assistant" && entry.content == "Provider-backed telegram reply."
        }),
        "unexpected session transcript view: {transcript_view:#?}"
    );
    let summary = backend
        .client()
        .session_summary(&session_id)
        .expect("session summary");
    assert!(!summary.reasoning_visible);
    assert_eq!(summary.think_level.as_deref(), Some("off"));
    let provider_request = provider_requests.recv().expect("provider request");
    let normalized_provider_request = provider_request.to_ascii_lowercase();
    assert!(
        !normalized_provider_request.contains("\"reasoning\":"),
        "unexpected provider request: {provider_request}"
    );

    let captured = drive_runtime_collect_requests_until(
        &runtime,
        &requests,
        Duration::from_secs(10),
        |requests| {
            requests.iter().any(|request| {
                request.path == "/bottest-token/SendMessage"
                    && request.body.contains("Provider-backed telegram reply.")
            })
        },
    );
    assert!(
        captured
            .iter()
            .any(|request| request.path == "/bottest-token/GetUpdates")
    );
    assert!(captured.iter().any(|request| {
        request.path == "/bottest-token/SendMessage"
            && request.body.contains("\"parse_mode\":\"HTML\"")
            && request.body.contains("Стадия: запуск")
    }));
    assert!(captured.iter().any(|request| {
        request.path == "/bottest-token/SendMessage"
            && request.body.contains("Provider-backed telegram reply.")
    }));
    assert!(captured.iter().any(|request| {
        request.path == "/bottest-token/EditMessageText"
            && (request.body.contains("Собираю контекст") || request.body.contains("Контекст:"))
    }));

    handle.stop().expect("stop daemon");
    telegram_handle.stop().expect("join telegram api");
    provider_handle.join().expect("join provider api");
}

fn spawn_fake_telegram_api(
    responses: Vec<FakeResponse>,
) -> (String, Receiver<CapturedRequest>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake telegram api");
    let port = listener.local_addr().expect("local addr").port();
    let (sender, receiver) = mpsc::channel();

    let handle = thread::spawn(move || {
        for response in responses {
            let (mut stream, _) = listener.accept().expect("accept connection");
            let request = read_http_request(&mut stream);
            sender.send(request).expect("send captured request");
            let response_bytes = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                response.content_type,
                response.body.len()
            )
            .into_bytes();
            stream
                .write_all(&response_bytes)
                .expect("write response headers");
            stream
                .write_all(&response.body)
                .expect("write response body");
        }
    });

    (format!("http://127.0.0.1:{port}"), receiver, handle)
}

struct RoutedTelegramApiHandle {
    stop: Arc<AtomicBool>,
    address: SocketAddr,
    join: thread::JoinHandle<()>,
}

impl RoutedTelegramApiHandle {
    fn stop(self) -> thread::Result<()> {
        self.stop.store(true, Ordering::SeqCst);
        let _ = std::net::TcpStream::connect(self.address);
        self.join.join()
    }
}

fn spawn_routed_telegram_api<F>(
    mut handler: F,
) -> (String, Receiver<CapturedRequest>, RoutedTelegramApiHandle)
where
    F: FnMut(&CapturedRequest) -> FakeResponse + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind routed telegram api");
    listener
        .set_nonblocking(true)
        .expect("set routed telegram api nonblocking");
    let address = listener.local_addr().expect("local addr");
    let port = address.port();
    let (sender, receiver) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = stop.clone();

    let join = thread::spawn(move || {
        while !worker_stop.load(Ordering::SeqCst) {
            let (mut stream, _) = match listener.accept() {
                Ok(accepted) => accepted,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                    continue;
                }
                Err(error) => panic!("accept routed telegram api connection: {error}"),
            };
            if worker_stop.load(Ordering::SeqCst) {
                break;
            }
            let request = read_http_request(&mut stream);
            let response = handler(&request);
            sender.send(request).expect("send captured request");
            let response_bytes = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                response.content_type,
                response.body.len()
            )
            .into_bytes();
            stream
                .write_all(&response_bytes)
                .expect("write response headers");
            stream
                .write_all(&response.body)
                .expect("write response body");
        }
    });

    (
        format!("http://127.0.0.1:{port}"),
        receiver,
        RoutedTelegramApiHandle {
            stop,
            address,
            join,
        },
    )
}

fn drain_requests_for(
    requests: &Receiver<CapturedRequest>,
    timeout: Duration,
) -> Vec<CapturedRequest> {
    let deadline = Instant::now() + timeout;
    let mut captured = Vec::new();
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        match requests.recv_timeout(remaining.min(Duration::from_millis(100))) {
            Ok(request) => captured.push(request),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if Instant::now() >= deadline {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    captured
}

fn read_http_request(stream: &mut std::net::TcpStream) -> CapturedRequest {
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

    let header_end = request
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4)
        .expect("http header terminator");
    let headers = String::from_utf8_lossy(&request[..header_end]).into_owned();
    let mut body = request[header_end..].to_vec();
    let content_length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);

    while body.len() < content_length {
        let bytes_read = stream.read(&mut buffer).expect("read request body");
        if bytes_read == 0 {
            break;
        }
        body.extend_from_slice(&buffer[..bytes_read]);
    }

    let request_line = headers.lines().next().expect("request line");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().expect("http method").to_string();
    let path = parts.next().expect("http path").to_string();

    CapturedRequest {
        method,
        path,
        body: String::from_utf8_lossy(&body).into_owned(),
    }
}

fn json_response(body: &str) -> FakeResponse {
    FakeResponse {
        content_type: "application/json",
        body: body.as_bytes().to_vec(),
    }
}

fn telegram_test_app() -> (tempfile::TempDir, bootstrap::App) {
    telegram_test_app_with_progress_interval_ms(1_250)
}

fn telegram_test_app_with_max_download_bytes(
    max_download_bytes: usize,
) -> (tempfile::TempDir, bootstrap::App) {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut config = AppConfig {
        data_dir: temp.path().join("teamd-state"),
        ..AppConfig::default()
    };
    config.telegram.enabled = true;
    config.telegram.bot_token = Some("test-token".to_string());
    config.telegram.max_download_bytes = max_download_bytes;
    config.telegram.global_send_min_interval_ms = 1;
    config.telegram.private_chat_send_min_interval_ms = 1;
    config.telegram.group_chat_send_min_interval_ms = 1;
    config.telegram.chat_turn_fast_settle_ms = 250;
    let app = bootstrap::build_from_config(config).expect("build app");
    (temp, app)
}

fn drive_runtime(runtime: &tokio::runtime::Runtime, duration: Duration) {
    runtime.block_on(async { tokio::time::sleep(duration).await });
}

fn drive_runtime_collect_requests_until<F>(
    runtime: &tokio::runtime::Runtime,
    requests: &Receiver<CapturedRequest>,
    timeout: Duration,
    mut done: F,
) -> Vec<CapturedRequest>
where
    F: FnMut(&[CapturedRequest]) -> bool,
{
    runtime.block_on(async {
        let deadline = Instant::now() + timeout;
        let mut captured = Vec::new();
        loop {
            while let Ok(request) = requests.try_recv() {
                captured.push(request);
            }
            if done(&captured) || Instant::now() >= deadline {
                return captured;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
}

fn telegram_test_app_with_progress_interval_ms(
    progress_interval_ms: u64,
) -> (tempfile::TempDir, bootstrap::App) {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut config = AppConfig {
        data_dir: temp.path().join("teamd-state"),
        ..AppConfig::default()
    };
    config.telegram.enabled = true;
    config.telegram.bot_token = Some("test-token".to_string());
    config.telegram.progress_update_min_interval_ms = progress_interval_ms;
    config.telegram.global_send_min_interval_ms = 1;
    config.telegram.private_chat_send_min_interval_ms = 1;
    config.telegram.group_chat_send_min_interval_ms = 1;
    let app = bootstrap::build_from_config(config).expect("build app");
    (temp, app)
}

fn session_summary(id: &str, title: &str, auto_approve: bool) -> SessionSummary {
    SessionSummary {
        id: id.to_string(),
        title: title.to_string(),
        agent_profile_id: "default".to_string(),
        agent_name: "Assistant".to_string(),
        scheduled_by: None,
        schedule: None,
        model: None,
        reasoning_visible: true,
        think_level: None,
        compactifications: 0,
        completion_nudges: None,
        auto_approve,
        context_tokens: 0,
        usage_input_tokens: None,
        usage_output_tokens: None,
        usage_total_tokens: None,
        has_pending_approval: false,
        last_message_preview: None,
        message_count: 0,
        background_job_count: 0,
        running_background_job_count: 0,
        queued_background_job_count: 0,
        created_at: 0,
        updated_at: 0,
    }
}

fn agent_summary(id: &str, name: &str, template_kind: &str) -> TelegramAgentSummary {
    TelegramAgentSummary {
        id: id.to_string(),
        name: name.to_string(),
        template_kind: template_kind.to_string(),
    }
}

fn seed_activated_pairing(app: &bootstrap::App, token: &str, user_id: i64, chat_id: i64) {
    app.store()
        .expect("open store")
        .put_telegram_user_pairing(&TelegramUserPairingRecord {
            token: token.to_string(),
            telegram_user_id: user_id,
            telegram_chat_id: chat_id,
            telegram_username: Some("alice".to_string()),
            telegram_display_name: "Alice".to_string(),
            status: "activated".to_string(),
            created_at: 10,
            expires_at: 10_000,
            activated_at: Some(20),
        })
        .expect("seed pairing");
}

fn seed_binding(
    app: &bootstrap::App,
    chat_id: i64,
    owner_user_id: i64,
    selected_session_id: Option<&str>,
) {
    app.store()
        .expect("open store")
        .put_telegram_chat_binding(&agent_persistence::TelegramChatBindingRecord {
            telegram_chat_id: chat_id,
            scope: "private".to_string(),
            owner_telegram_user_id: Some(owner_user_id),
            selected_session_id: selected_session_id.map(str::to_string),
            default_agent_profile_id: None,
            last_delivered_transcript_created_at: Some(0),
            last_delivered_transcript_id: Some(String::new()),
            inbound_queue_mode: "coalesce".to_string(),
            inbound_coalesce_window_ms: None,
            created_at: 10,
            updated_at: 10,
        })
        .expect("seed binding");
}

fn telegram_daemon_test_app(provider_base: &str) -> (tempfile::TempDir, bootstrap::App) {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut config = AppConfig {
        data_dir: temp.path().join("teamd-state"),
        ..AppConfig::default()
    };
    config.daemon.bind_host = "127.0.0.1".to_string();
    config.daemon.bind_port = free_port();
    config.telegram.enabled = true;
    config.telegram.bot_token = Some("test-token".to_string());
    config.provider.kind = ProviderKind::OpenAiResponses;
    config.provider.api_base = Some(format!("{provider_base}/v1"));
    config.provider.api_key = Some("test-key".to_string());
    config.provider.default_model = Some("gpt-5.4".to_string());
    let app = bootstrap::build_from_config(config).expect("build app");
    (temp, app)
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind ephemeral port")
        .local_addr()
        .expect("local addr")
        .port()
}

fn spawn_provider_sse_server_with_response_delay(
    responses: Vec<String>,
    response_delay: Duration,
) -> (String, Receiver<String>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind provider server");
    let port = listener.local_addr().expect("local addr").port();
    let (sender, receiver) = mpsc::channel();

    let handle = thread::spawn(move || {
        for body in responses {
            let (mut stream, _) = listener.accept().expect("accept provider connection");
            let request = read_http_request(&mut stream);
            sender
                .send(request.body.clone())
                .expect("send provider request");
            if !response_delay.is_zero() {
                thread::sleep(response_delay);
            }
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("write provider response");
            stream.flush().expect("flush provider response");
        }
    });

    (format!("http://127.0.0.1:{port}"), receiver, handle)
}
