#[test]
fn decode_repl_line_bytes_uses_cp1251_locale_hint() {
    let bytes = "привет\n".as_bytes();
    let encoded = encoding_rs::WINDOWS_1251.encode("привет\n").0;

    let decoded = super::decode_repl_line_bytes(&encoded, Some("cp1251"))
        .expect("cp1251 input should decode");

    assert_eq!(decoded, String::from_utf8(bytes.to_vec()).expect("utf8"));
}

#[test]
fn process_cli_accepts_russian_version_and_update_commands() {
    assert!(super::ProcessInvocation::parse(["версия"]).is_ok());
    assert!(super::ProcessInvocation::parse(["обновить", "v1.0.1"]).is_ok());
    assert!(super::ProcessInvocation::parse(["логи"]).is_ok());
    assert!(super::ProcessInvocation::parse(["logs", "25"]).is_ok());
}

#[test]
fn process_cli_accepts_telegram_commands() {
    let run = super::ProcessInvocation::parse(["telegram", "run"]).expect("parse telegram run");
    let pair =
        super::ProcessInvocation::parse(["telegram", "pair", "pair-123"]).expect("parse pair");
    let pairings =
        super::ProcessInvocation::parse(["telegram", "pairings"]).expect("parse pairings");

    assert!(matches!(run.command, super::Command::TelegramRun));
    assert!(matches!(
        pair.command,
        super::Command::TelegramPair { ref key } if key == "pair-123"
    ));
    assert!(matches!(pairings.command, super::Command::TelegramPairings));
}

#[test]
fn process_cli_accepts_session_list_commands() {
    let session_list =
        super::ProcessInvocation::parse(["session", "list"]).expect("parse session list");
    let sessions = super::ProcessInvocation::parse(["sessions"]).expect("parse sessions alias");
    let raw_sessions =
        super::ProcessInvocation::parse(["sessions", "--raw"]).expect("parse raw sessions alias");
    let russian_sessions =
        super::ProcessInvocation::parse(["сессии"]).expect("parse russian sessions alias");
    let russian_session_list =
        super::ProcessInvocation::parse(["сессия", "список"]).expect("parse russian session list");

    assert!(matches!(
        session_list.command,
        super::Command::SessionList {
            format: super::SessionListFormat::Human
        }
    ));
    assert!(matches!(
        sessions.command,
        super::Command::SessionList {
            format: super::SessionListFormat::Human
        }
    ));
    assert!(matches!(
        raw_sessions.command,
        super::Command::SessionList {
            format: super::SessionListFormat::Raw
        }
    ));
    assert!(matches!(
        russian_sessions.command,
        super::Command::SessionList {
            format: super::SessionListFormat::Human
        }
    ));
    assert!(matches!(
        russian_session_list.command,
        super::Command::SessionList {
            format: super::SessionListFormat::Human
        }
    ));
}

#[test]
fn process_cli_accepts_agent_profile_commands() {
    assert!(super::ProcessInvocation::parse(["agents"]).is_ok());
    assert!(super::ProcessInvocation::parse(["agent", "list"]).is_ok());
    assert!(super::ProcessInvocation::parse(["агенты"]).is_ok());
    assert!(super::ProcessInvocation::parse(["агент", "список"]).is_ok());
    assert!(super::ProcessInvocation::parse(["agent", "show", "judge"]).is_ok());
    assert!(super::ProcessInvocation::parse(["agent", "select", "judge"]).is_ok());
    assert!(super::ProcessInvocation::parse(["agent", "open", "judge"]).is_ok());
    assert!(
        super::ProcessInvocation::parse(["agent", "create", "Reviewer", "from", "judge"]).is_ok()
    );
    assert!(
        super::ProcessInvocation::parse(["агент", "создать", "Ревьюер", "из", "judge"]).is_ok()
    );
}

#[test]
fn execute_renders_agent_profile_commands() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = crate::bootstrap::build_from_config(agent_persistence::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..agent_persistence::AppConfig::default()
    })
    .expect("build app");

    let agents = super::execute(&app, ["agent", "list"]).expect("list agents");
    assert!(agents.contains("default"));
    assert!(agents.contains("judge"));

    let created =
        super::execute(&app, ["agent", "create", "Reviewer", "from", "judge"]).expect("create");
    assert!(created.contains("Reviewer"));
    assert!(created.contains("reviewer"));

    let shown = super::execute(&app, ["agent", "show", "Reviewer"]).expect("show reviewer");
    assert!(shown.contains("id=reviewer"));
    assert!(shown.contains("name=Reviewer"));
    assert!(shown.contains("default_workspace_root="));

    let selected = super::execute(&app, ["agent", "select", "Reviewer"]).expect("select");
    assert!(selected.contains("Reviewer"));

    let open = super::execute(&app, ["agent", "open", "Reviewer"]).expect("open");
    assert!(open.contains("SYSTEM.md"));
    assert!(open.contains("AGENTS.md"));
}

#[test]
fn process_cli_accepts_session_transcript_and_tool_commands() {
    let transcript = super::ProcessInvocation::parse(["session", "transcript", "session-1"])
        .expect("parse transcript");
    let tools =
        super::ProcessInvocation::parse(["session", "tools", "session-1"]).expect("parse tools");
    let paged_tools = super::ProcessInvocation::parse([
        "session",
        "tools",
        "session-1",
        "--limit",
        "25",
        "--offset",
        "50",
    ])
    .expect("parse paged tools");
    let raw_tools = super::ProcessInvocation::parse(["session", "tools", "session-1", "--raw"])
        .expect("parse raw tools");
    let russian_transcript = super::ProcessInvocation::parse(["сессия", "транскрипт", "session-1"])
        .expect("parse russian transcript");
    let russian_tools = super::ProcessInvocation::parse(["сессия", "инструменты", "session-1"])
        .expect("parse russian tools");

    assert!(matches!(
        transcript.command,
        super::Command::SessionTranscript { ref id } if id == "session-1"
    ));
    assert!(matches!(
        tools.command,
        super::Command::SessionTools { ref id, limit: None, offset: 0, format: super::SessionToolsFormat::Human, include_results: false } if id == "session-1"
    ));
    assert!(matches!(
        paged_tools.command,
        super::Command::SessionTools { ref id, limit: Some(25), offset: 50, format: super::SessionToolsFormat::Human, include_results: false } if id == "session-1"
    ));
    assert!(matches!(
        raw_tools.command,
        super::Command::SessionTools { ref id, limit: None, offset: 0, format: super::SessionToolsFormat::Raw, include_results: false } if id == "session-1"
    ));
    assert!(matches!(
        russian_transcript.command,
        super::Command::SessionTranscript { ref id } if id == "session-1"
    ));
    assert!(matches!(
        russian_tools.command,
        super::Command::SessionTools { ref id, limit: None, offset: 0, format: super::SessionToolsFormat::Human, include_results: false } if id == "session-1"
    ));
}

#[test]
fn execute_renders_session_list() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = crate::bootstrap::build_from_config(agent_persistence::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..agent_persistence::AppConfig::default()
    })
    .expect("build app");

    super::execute(&app, ["session", "create", "session-1", "Alpha"]).expect("create alpha");
    super::execute(&app, ["session", "create", "session-2", "Beta"]).expect("create beta");

    let rendered = super::execute(&app, ["session", "list"]).expect("render session list");

    assert!(rendered.contains("Sessions"));
    assert!(rendered.contains("total: 2"));
    assert!(rendered.contains("1. Alpha"));
    assert!(rendered.contains("id: session-1"));
    assert!(rendered.contains("2. Beta"));
    assert!(rendered.contains("id: session-2"));
    assert!(rendered.contains("agent: Ассистент (default)"));
    assert!(rendered.contains("messages: 0"));
    assert!(rendered.contains("pending approval: no"));
    assert!(rendered.contains("background jobs: 0 total, 0 running, 0 queued"));
    assert!(rendered.contains("preview: <none>"));
    assert!(!rendered.contains("session id="));
}

#[test]
fn execute_renders_session_list_raw_format() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = crate::bootstrap::build_from_config(agent_persistence::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..agent_persistence::AppConfig::default()
    })
    .expect("build app");

    super::execute(&app, ["session", "create", "session-1", "Alpha"]).expect("create alpha");

    let rendered =
        super::execute(&app, ["session", "list", "--raw"]).expect("render raw session list");

    assert!(rendered.contains("sessions total=1"));
    assert!(rendered.contains("session id=session-1 title=Alpha"));
    assert!(rendered.contains("agent=Ассистент (default)"));
    assert!(rendered.contains("messages=0"));
    assert!(rendered.contains("updated_at="));
}

#[test]
fn execute_renders_session_transcript() {
    use agent_persistence::{
        SessionRecord, SessionRepository, TranscriptRecord, TranscriptRepository,
    };

    let temp = tempfile::tempdir().expect("tempdir");
    let app = crate::bootstrap::build_from_config(agent_persistence::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..agent_persistence::AppConfig::default()
    })
    .expect("build app");
    let store = app.store().expect("open store");
    store
        .put_session(&SessionRecord {
            id: "session-1".to_string(),
            title: "Transcript".to_string(),
            prompt_override: None,
            settings_json: "{}".to_string(),
            workspace_root: app.runtime.workspace.root.display().to_string(),
            agent_profile_id: "default".to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");
    store
        .put_transcript(&TranscriptRecord {
            id: "transcript-1".to_string(),
            session_id: "session-1".to_string(),
            run_id: None,
            kind: "user".to_string(),
            content: "hello from transcript".to_string(),
            created_at: 2,
        })
        .expect("put transcript");

    let rendered =
        super::execute(&app, ["session", "transcript", "session-1"]).expect("render transcript");

    assert!(rendered.contains("hello from transcript"));
}

#[test]
fn execute_renders_session_tool_calls() {
    use agent_persistence::{
        RunRecord, RunRepository, SessionRecord, SessionRepository, ToolCallRecord,
        ToolCallRepository,
    };

    let temp = tempfile::tempdir().expect("tempdir");
    let app = crate::bootstrap::build_from_config(agent_persistence::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..agent_persistence::AppConfig::default()
    })
    .expect("build app");
    let store = app.store().expect("open store");
    store
        .put_session(&SessionRecord {
            id: "session-1".to_string(),
            title: "Tools".to_string(),
            prompt_override: None,
            settings_json: "{}".to_string(),
            workspace_root: app.runtime.workspace.root.display().to_string(),
            agent_profile_id: "default".to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");
    store
        .put_run(&RunRecord {
            id: "run-1".to_string(),
            session_id: "session-1".to_string(),
            mission_id: None,
            status: "running".to_string(),
            error: None,
            result: None,
            provider_usage_json: "null".to_string(),
            active_processes_json: "[]".to_string(),
            recent_steps_json: "[]".to_string(),
            evidence_refs_json: "[]".to_string(),
            pending_approvals_json: "[]".to_string(),
            provider_loop_json: "null".to_string(),
            delegate_runs_json: "[]".to_string(),
            started_at: 2,
            updated_at: 2,
            finished_at: None,
        })
        .expect("put run");
    store
        .put_tool_call(&ToolCallRecord {
            id: "tool-call-1".to_string(),
            session_id: "session-1".to_string(),
            run_id: "run-1".to_string(),
            provider_tool_call_id: "provider-call-1".to_string(),
            tool_name: "fs_read_text".to_string(),
            arguments_json: "{\"path\":\"README.md\"}".to_string(),
            summary: "fs_read_text path=README.md".to_string(),
            status: "completed".to_string(),
            error: None,
            result_summary: None,
            result_preview: None,
            result_artifact_id: None,
            result_truncated: false,
            result_byte_len: None,
            requested_at: 3,
            updated_at: 4,
        })
        .expect("put tool call");
    store
        .put_tool_call(&ToolCallRecord {
            id: "tool-call-2".to_string(),
            session_id: "session-1".to_string(),
            run_id: "run-1".to_string(),
            provider_tool_call_id: "provider-call-2".to_string(),
            tool_name: "exec_wait".to_string(),
            arguments_json: "{\"process_id\":\"exec-1\"}".to_string(),
            summary: "exec_wait process_id=exec-1".to_string(),
            status: "failed".to_string(),
            error: Some("process not found".to_string()),
            result_summary: None,
            result_preview: None,
            result_artifact_id: None,
            result_truncated: false,
            result_byte_len: None,
            requested_at: 5,
            updated_at: 6,
        })
        .expect("put second tool call");

    let rendered = super::execute(&app, ["session", "tools", "session-1"]).expect("render tools");

    assert!(rendered.contains("Session tools"));
    assert!(rendered.contains("session: session-1"));
    assert!(
        rendered.contains("total: 2 | showing: 1-2 | limit: 50 | offset: 0 | next_offset: <none>")
    );
    assert!(rendered.contains("Run run-1"));
    assert!(rendered.contains("1. fs_read_text [completed]"));
    assert!(rendered.contains("requested: 1970-01-01T00:00:03Z (3)"));
    assert!(rendered.contains("updated: 1970-01-01T00:00:04Z (4)"));
    assert!(rendered.contains("summary: fs_read_text path=README.md"));
    assert!(rendered.contains("args:\n       {"));
    assert!(rendered.contains("\"path\": \"README.md\""));
    assert!(rendered.contains("2. exec_wait [failed]"));
    assert!(rendered.contains("requested: 1970-01-01T00:00:05Z (5)"));
    assert!(rendered.contains("updated: 1970-01-01T00:00:06Z (6)"));
    assert!(rendered.contains("error: process not found"));
    assert!(!rendered.contains("tool_call id="));
}

#[test]
fn execute_renders_session_tool_calls_raw_format() {
    use agent_persistence::{
        RunRecord, RunRepository, SessionRecord, SessionRepository, ToolCallRecord,
        ToolCallRepository,
    };

    let temp = tempfile::tempdir().expect("tempdir");
    let app = crate::bootstrap::build_from_config(agent_persistence::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..agent_persistence::AppConfig::default()
    })
    .expect("build app");
    let store = app.store().expect("open store");
    store
        .put_session(&SessionRecord {
            id: "session-1".to_string(),
            title: "Tools".to_string(),
            prompt_override: None,
            settings_json: "{}".to_string(),
            workspace_root: app.runtime.workspace.root.display().to_string(),
            agent_profile_id: "default".to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");
    store
        .put_run(&RunRecord {
            id: "run-1".to_string(),
            session_id: "session-1".to_string(),
            mission_id: None,
            status: "running".to_string(),
            error: None,
            result: None,
            provider_usage_json: "null".to_string(),
            active_processes_json: "[]".to_string(),
            recent_steps_json: "[]".to_string(),
            evidence_refs_json: "[]".to_string(),
            pending_approvals_json: "[]".to_string(),
            provider_loop_json: "null".to_string(),
            delegate_runs_json: "[]".to_string(),
            started_at: 2,
            updated_at: 2,
            finished_at: None,
        })
        .expect("put run");
    store
        .put_tool_call(&ToolCallRecord {
            id: "tool-call-1".to_string(),
            session_id: "session-1".to_string(),
            run_id: "run-1".to_string(),
            provider_tool_call_id: "provider-call-1".to_string(),
            tool_name: "fs_read_text".to_string(),
            arguments_json: "{\"path\":\"README.md\"}".to_string(),
            summary: "fs_read_text path=README.md".to_string(),
            status: "completed".to_string(),
            error: None,
            result_summary: None,
            result_preview: None,
            result_artifact_id: None,
            result_truncated: false,
            result_byte_len: None,
            requested_at: 3,
            updated_at: 4,
        })
        .expect("put tool call");

    let rendered =
        super::execute(&app, ["session", "tools", "session-1", "--raw"]).expect("render tools");

    assert!(rendered.contains("session tools session_id=session-1 total=1 showing=1-1"));
    assert!(rendered.contains("tool_call id=tool-call-1"));
    assert!(rendered.contains("tool=fs_read_text"));
    assert!(rendered.contains("args={\"path\":\"README.md\"}"));
}

#[test]
fn execute_renders_session_tool_calls_with_result_previews() {
    use agent_persistence::{
        RunRecord, RunRepository, SessionRecord, SessionRepository, ToolCallRecord,
        ToolCallRepository,
    };

    let temp = tempfile::tempdir().expect("tempdir");
    let app = crate::bootstrap::build_from_config(agent_persistence::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..agent_persistence::AppConfig::default()
    })
    .expect("build app");
    let store = app.store().expect("open store");
    store
        .put_session(&SessionRecord {
            id: "session-1".to_string(),
            title: "Tools".to_string(),
            prompt_override: None,
            settings_json: "{}".to_string(),
            workspace_root: app.runtime.workspace.root.display().to_string(),
            agent_profile_id: "default".to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");
    store
        .put_run(&RunRecord {
            id: "run-1".to_string(),
            session_id: "session-1".to_string(),
            mission_id: None,
            status: "running".to_string(),
            error: None,
            result: None,
            provider_usage_json: "null".to_string(),
            active_processes_json: "[]".to_string(),
            recent_steps_json: "[]".to_string(),
            evidence_refs_json: "[]".to_string(),
            pending_approvals_json: "[]".to_string(),
            provider_loop_json: "null".to_string(),
            delegate_runs_json: "[]".to_string(),
            started_at: 2,
            updated_at: 2,
            finished_at: None,
        })
        .expect("put run");
    store
        .put_tool_call(&ToolCallRecord {
            id: "tool-call-1".to_string(),
            session_id: "session-1".to_string(),
            run_id: "run-1".to_string(),
            provider_tool_call_id: "provider-call-1".to_string(),
            tool_name: "exec_wait".to_string(),
            arguments_json: "{\"process_id\":\"exec-1\"}".to_string(),
            summary: "exec_wait process_id=exec-1".to_string(),
            status: "completed".to_string(),
            error: None,
            result_summary: Some("exec_wait process_id=exec-1 exit_code=Some(0)".to_string()),
            result_preview: Some(
                "{\"tool\":\"process_result\",\"stdout\":\"hello\\n\",\"stderr\":\"\"}".to_string(),
            ),
            result_artifact_id: None,
            result_truncated: false,
            result_byte_len: Some(58),
            requested_at: 3,
            updated_at: 4,
        })
        .expect("put tool call");

    let rendered =
        super::execute(&app, ["session", "tools", "session-1", "--results"]).expect("render tools");

    assert!(rendered.contains("result_summary: exec_wait process_id=exec-1 exit_code=Some(0)"));
    assert!(rendered.contains("result_byte_len: 58"));
    assert!(rendered.contains("result_truncated: false"));
    assert!(rendered.contains("result_artifact_id: <none>"));
    assert!(rendered.contains("\"stdout\": \"hello\\n\""));
}

#[test]
fn execute_renders_full_session_tool_result_from_artifact() {
    use agent_persistence::{
        ArtifactRecord, ArtifactRepository, RunRecord, RunRepository, SessionRecord,
        SessionRepository, ToolCallRecord, ToolCallRepository,
    };

    let temp = tempfile::tempdir().expect("tempdir");
    let app = crate::bootstrap::build_from_config(agent_persistence::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..agent_persistence::AppConfig::default()
    })
    .expect("build app");
    let store = app.store().expect("open store");
    store
        .put_session(&SessionRecord {
            id: "session-1".to_string(),
            title: "Tools".to_string(),
            prompt_override: None,
            settings_json: "{}".to_string(),
            workspace_root: app.runtime.workspace.root.display().to_string(),
            agent_profile_id: "default".to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");
    store
        .put_run(&RunRecord {
            id: "run-1".to_string(),
            session_id: "session-1".to_string(),
            mission_id: None,
            status: "running".to_string(),
            error: None,
            result: None,
            provider_usage_json: "null".to_string(),
            active_processes_json: "[]".to_string(),
            recent_steps_json: "[]".to_string(),
            evidence_refs_json: "[]".to_string(),
            pending_approvals_json: "[]".to_string(),
            provider_loop_json: "null".to_string(),
            delegate_runs_json: "[]".to_string(),
            started_at: 2,
            updated_at: 2,
            finished_at: None,
        })
        .expect("put run");
    store
        .put_artifact(&ArtifactRecord {
            id: "artifact-tool-result-1".to_string(),
            session_id: "session-1".to_string(),
            kind: "tool_output".to_string(),
            metadata_json: "{}".to_string(),
            path: std::path::PathBuf::from("artifacts").join("artifact-tool-result-1.bin"),
            bytes: b"{\"tool\":\"process_result\",\"stdout\":\"full stdout\",\"stderr\":\"full stderr\"}"
                .to_vec(),
            created_at: 4,
        })
        .expect("put artifact");
    store
        .put_tool_call(&ToolCallRecord {
            id: "tool-call-1".to_string(),
            session_id: "session-1".to_string(),
            run_id: "run-1".to_string(),
            provider_tool_call_id: "provider-call-1".to_string(),
            tool_name: "exec_wait".to_string(),
            arguments_json: "{\"process_id\":\"exec-1\"}".to_string(),
            summary: "exec_wait process_id=exec-1".to_string(),
            status: "completed".to_string(),
            error: None,
            result_summary: Some("exec_wait process_id=exec-1 exit_code=Some(0)".to_string()),
            result_preview: Some(
                "{\"tool\":\"process_result\",\"stdout\":\"full st...".to_string(),
            ),
            result_artifact_id: Some("artifact-tool-result-1".to_string()),
            result_truncated: true,
            result_byte_len: Some(72),
            requested_at: 3,
            updated_at: 4,
        })
        .expect("put tool call");

    let rendered =
        super::execute(&app, ["session", "tool-result", "tool-call-1"]).expect("render result");

    assert!(rendered.contains("Session tool result"));
    assert!(rendered.contains("tool_call_id: tool-call-1"));
    assert!(rendered.contains("result_artifact_id: artifact-tool-result-1"));
    assert!(rendered.contains("\"stdout\": \"full stdout\""));
    assert!(rendered.contains("\"stderr\": \"full stderr\""));
}

#[test]
fn execute_renders_empty_session_tool_calls_raw_format_compatibly() {
    use agent_persistence::{SessionRecord, SessionRepository};

    let temp = tempfile::tempdir().expect("tempdir");
    let app = crate::bootstrap::build_from_config(agent_persistence::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..agent_persistence::AppConfig::default()
    })
    .expect("build app");
    let store = app.store().expect("open store");
    store
        .put_session(&SessionRecord {
            id: "session-1".to_string(),
            title: "Tools".to_string(),
            prompt_override: None,
            settings_json: "{}".to_string(),
            workspace_root: app.runtime.workspace.root.display().to_string(),
            agent_profile_id: "default".to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");

    let rendered =
        super::execute(&app, ["session", "tools", "session-1", "--raw"]).expect("render tools");

    assert_eq!(
        rendered,
        "session tools session_id=session-1 total=0 showing=0-0 next_offset=<none>\n<empty>"
    );
}

#[test]
fn execute_renders_session_tool_calls_page() {
    use agent_persistence::{
        RunRecord, RunRepository, SessionRecord, SessionRepository, ToolCallRecord,
        ToolCallRepository,
    };

    let temp = tempfile::tempdir().expect("tempdir");
    let app = crate::bootstrap::build_from_config(agent_persistence::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..agent_persistence::AppConfig::default()
    })
    .expect("build app");
    let store = app.store().expect("open store");
    store
        .put_session(&SessionRecord {
            id: "session-1".to_string(),
            title: "Tools".to_string(),
            prompt_override: None,
            settings_json: "{}".to_string(),
            workspace_root: app.runtime.workspace.root.display().to_string(),
            agent_profile_id: "default".to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");
    store
        .put_run(&RunRecord {
            id: "run-1".to_string(),
            session_id: "session-1".to_string(),
            mission_id: None,
            status: "running".to_string(),
            error: None,
            result: None,
            provider_usage_json: "null".to_string(),
            active_processes_json: "[]".to_string(),
            recent_steps_json: "[]".to_string(),
            evidence_refs_json: "[]".to_string(),
            pending_approvals_json: "[]".to_string(),
            provider_loop_json: "null".to_string(),
            delegate_runs_json: "[]".to_string(),
            started_at: 2,
            updated_at: 2,
            finished_at: None,
        })
        .expect("put run");
    for index in 1..=3 {
        store
            .put_tool_call(&ToolCallRecord {
                id: format!("tool-call-{index}"),
                session_id: "session-1".to_string(),
                run_id: "run-1".to_string(),
                provider_tool_call_id: format!("provider-call-{index}"),
                tool_name: format!("tool_{index}"),
                arguments_json: "{}".to_string(),
                summary: format!("tool_{index}"),
                status: "completed".to_string(),
                error: None,
                result_summary: None,
                result_preview: None,
                result_artifact_id: None,
                result_truncated: false,
                result_byte_len: None,
                requested_at: index,
                updated_at: index,
            })
            .expect("put tool call");
    }

    let rendered = super::execute(
        &app,
        [
            "session",
            "tools",
            "session-1",
            "--limit",
            "1",
            "--offset",
            "1",
        ],
    )
    .expect("render tools page");

    assert!(rendered.contains("Session tools"));
    assert!(rendered.contains("total: 3 | showing: 2-2 | limit: 1 | offset: 1 | next_offset: 2"));
    assert!(rendered.contains("2. tool_2 [completed]"));
    assert!(rendered.contains("summary: tool_2"));
    assert!(!rendered.contains("tool_1 [completed]"));
    assert!(!rendered.contains("tool_3 [completed]"));
}

#[test]
fn execute_process_with_io_renders_version_for_russian_alias() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = crate::bootstrap::build_from_config(agent_persistence::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..agent_persistence::AppConfig::default()
    })
    .expect("build app");
    let mut input = std::io::Cursor::new(Vec::<u8>::new());
    let mut output = Vec::new();

    super::execute_process_with_io(&app, ["версия"], &mut input, &mut output)
        .expect("render version");

    let rendered = String::from_utf8(output).expect("utf8");
    assert!(rendered.contains("версия="));
    assert!(rendered.contains("commit="));
    assert!(rendered.contains("tree="));
    assert!(rendered.contains("build_id="));
    assert!(rendered.contains(&format!(
        "data_dir={}",
        temp.path().join("state-root").display()
    )));
}

#[test]
fn execute_process_with_io_renders_diagnostics_tail() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = crate::bootstrap::build_from_config(agent_persistence::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..agent_persistence::AppConfig::default()
    })
    .expect("build app");
    let event = agent_persistence::audit::DiagnosticEvent::new(
        "info",
        "test",
        "logs.command",
        "diagnostic test line",
        app.config.data_dir.display().to_string(),
    );
    app.persistence
        .audit
        .append_event(&event)
        .expect("append diagnostic event");
    let mut input = std::io::Cursor::new(Vec::<u8>::new());
    let mut output = Vec::new();

    super::execute_process_with_io(&app, ["logs", "1"], &mut input, &mut output)
        .expect("render logs");

    let rendered = String::from_utf8(output).expect("utf8");
    assert!(rendered.contains("diagnostic test line"));
    assert!(!rendered.contains("версия="));
}

#[test]
fn execute_process_with_io_activates_telegram_pairing() {
    use agent_persistence::{TelegramRepository, TelegramUserPairingRecord};

    let temp = tempfile::tempdir().expect("tempdir");
    let app = crate::bootstrap::build_from_config(agent_persistence::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..agent_persistence::AppConfig::default()
    })
    .expect("build app");
    let store = app.store().expect("open store");
    store
        .put_telegram_user_pairing(&TelegramUserPairingRecord {
            token: "pair-123".to_string(),
            telegram_user_id: 42,
            telegram_chat_id: 42,
            telegram_username: Some("alice".to_string()),
            telegram_display_name: "Alice".to_string(),
            status: "pending".to_string(),
            created_at: 100,
            expires_at: i64::MAX,
            activated_at: None,
        })
        .expect("store pending pairing");
    let mut input = std::io::Cursor::new(Vec::<u8>::new());
    let mut output = Vec::new();

    super::execute_process_with_io(
        &app,
        ["telegram", "pair", "pair-123"],
        &mut input,
        &mut output,
    )
    .expect("activate pairing");

    let rendered = String::from_utf8(output).expect("utf8");
    assert!(rendered.contains("telegram pairing activated"));
    assert!(rendered.contains("token=pair-123"));
    assert!(rendered.contains("user_id=42"));

    let updated = app
        .store()
        .expect("reopen store")
        .get_telegram_user_pairing_by_token("pair-123")
        .expect("load pairing")
        .expect("pairing exists");
    assert_eq!(updated.status, "activated");
    assert!(updated.activated_at.is_some());
}

#[test]
fn execute_process_with_io_lists_telegram_pairings() {
    use agent_persistence::{TelegramRepository, TelegramUserPairingRecord};

    let temp = tempfile::tempdir().expect("tempdir");
    let app = crate::bootstrap::build_from_config(agent_persistence::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..agent_persistence::AppConfig::default()
    })
    .expect("build app");
    let store = app.store().expect("open store");
    store
        .put_telegram_user_pairing(&TelegramUserPairingRecord {
            token: "pair-aaa".to_string(),
            telegram_user_id: 1,
            telegram_chat_id: 1,
            telegram_username: Some("alice".to_string()),
            telegram_display_name: "Alice".to_string(),
            status: "activated".to_string(),
            created_at: 10,
            expires_at: 1000,
            activated_at: Some(20),
        })
        .expect("store first pairing");
    store
        .put_telegram_user_pairing(&TelegramUserPairingRecord {
            token: "pair-bbb".to_string(),
            telegram_user_id: 2,
            telegram_chat_id: 2,
            telegram_username: None,
            telegram_display_name: "Bob".to_string(),
            status: "pending".to_string(),
            created_at: 30,
            expires_at: 1000,
            activated_at: None,
        })
        .expect("store second pairing");
    let mut input = std::io::Cursor::new(Vec::<u8>::new());
    let mut output = Vec::new();

    super::execute_process_with_io(&app, ["telegram", "pairings"], &mut input, &mut output)
        .expect("list pairings");

    let rendered = String::from_utf8(output).expect("utf8");
    assert!(rendered.contains("pair-aaa"));
    assert!(rendered.contains("status=activated"));
    assert!(rendered.contains("pair-bbb"));
    assert!(rendered.contains("status=pending"));
}
