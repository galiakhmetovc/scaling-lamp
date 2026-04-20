use agent_persistence::{
    AppConfig, PersistenceStore, RunRecord, RunRepository, TranscriptRepository,
};
use agent_runtime::provider::{ConfiguredProvider, ProviderKind};
use agent_runtime::run::{ApprovalRequest, RunEngine, RunSnapshot, RunStatus};
use agentd::bootstrap::{SessionPreferencesPatch, SessionSummary, build_from_config};
use agentd::tui::app::{DialogState, TuiAppState, TuiScreen};
use agentd::tui::events::TuiAction;
use agentd::tui::timeline::{Timeline, TimelineEntryKind};
use agentd::tui::{dispatch_action, pump_background};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::{Duration, Instant};

fn summary(id: &str, title: &str) -> SessionSummary {
    SessionSummary {
        id: id.to_string(),
        title: title.to_string(),
        model: Some("glm-5-turbo".to_string()),
        reasoning_visible: true,
        think_level: Some("medium".to_string()),
        compactifications: 0,
        context_tokens: 0,
        has_pending_approval: false,
        last_message_preview: None,
        message_count: 0,
        created_at: 10,
        updated_at: 20,
    }
}

#[test]
fn tui_shell_navigation_starts_in_session_screen_without_current_session() {
    let app = TuiAppState::new(vec![summary("session-a", "Session A")], None);

    assert_eq!(app.active_screen(), TuiScreen::Sessions);
}

#[test]
fn tui_shell_navigation_opens_chat_from_selected_session() {
    let mut app = TuiAppState::new(
        vec![
            summary("session-a", "Session A"),
            summary("session-b", "Session B"),
        ],
        None,
    );

    app.select_next_session();
    app.activate_selected_session().expect("activate session");

    assert_eq!(app.active_screen(), TuiScreen::Chat);
    assert_eq!(app.current_session_id(), Some("session-b"));
}

#[test]
fn tui_shell_navigation_returns_to_previous_chat_on_escape() {
    let mut app = TuiAppState::new(
        vec![summary("session-a", "Session A")],
        Some("session-a".to_string()),
    );

    app.open_session_screen();
    assert_eq!(app.active_screen(), TuiScreen::Sessions);

    app.handle_escape();

    assert_eq!(app.active_screen(), TuiScreen::Chat);
    assert_eq!(app.current_session_id(), Some("session-a"));
}

#[test]
fn tui_shell_navigation_opens_expected_dialogs() {
    let mut app = TuiAppState::new(
        vec![summary("session-a", "Session A")],
        Some("session-a".to_string()),
    );

    app.open_new_session_dialog();
    assert!(matches!(
        app.dialog_state(),
        Some(DialogState::CreateSession { .. })
    ));
    app.close_dialog();

    app.open_session_screen();
    app.open_delete_dialog().expect("delete dialog");
    assert_eq!(
        app.dialog_state(),
        Some(DialogState::ConfirmDelete {
            session_id: "session-a".to_string(),
        })
    );
}

#[test]
fn tui_chat_commands_and_timeline_new_creates_and_switches_immediately() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let original = app
        .create_session_auto(Some("Original Session"))
        .expect("create original");
    let mut state = TuiAppState::new(
        app.list_session_summaries().expect("list sessions"),
        Some(original.id.clone()),
    );
    let mut render = |_state: &TuiAppState| Ok::<_, agentd::bootstrap::BootstrapError>(());

    dispatch_action(
        &app,
        &mut state,
        TuiAction::SubmitChatInput("/new".to_string()),
        &mut render,
    )
    .expect("dispatch /new");

    assert_eq!(state.active_screen(), TuiScreen::Chat);
    assert_ne!(state.current_session_id(), Some(original.id.as_str()));
    assert_eq!(
        app.list_session_summaries().expect("list refreshed").len(),
        2
    );
    assert!(state.timeline().entries(true).is_empty());
}

#[test]
fn tui_chat_commands_and_timeline_rename_clear_and_preferences_use_the_app_layer() {
    let (api_base, _provider_handle) = spawn_json_server_sequence(vec![
        r#"{
                "id":"resp_tui_compact",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_tui_compact",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"TUI compact summary."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":40,"output_tokens":7,"total_tokens":47}
            }"#
        .to_string(),
    ]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
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
    let session = app
        .create_session_auto(Some("Original Session"))
        .expect("create session");
    let store = PersistenceStore::open(&app.persistence).expect("open store");
    let mut state = TuiAppState::new(
        app.list_session_summaries().expect("list sessions"),
        Some(session.id.clone()),
    );
    let mut render = |_state: &TuiAppState| Ok::<_, agentd::bootstrap::BootstrapError>(());

    for (index, (kind, content)) in [
        ("user", "covered user one"),
        ("assistant", "covered assistant one"),
        ("user", "recent user one"),
        ("assistant", "recent assistant one"),
        ("user", "recent user two"),
        ("assistant", "recent assistant two"),
        ("user", "recent user three"),
        ("assistant", "recent assistant three"),
    ]
    .into_iter()
    .enumerate()
    {
        store
            .put_transcript(&agent_persistence::TranscriptRecord {
                id: format!("tui-compact-transcript-{index}"),
                session_id: session.id.clone(),
                run_id: None,
                kind: kind.to_string(),
                content: content.to_string(),
                created_at: 50 + index as i64,
            })
            .expect("put tui transcript");
    }

    dispatch_action(
        &app,
        &mut state,
        TuiAction::SubmitChatInput("/rename".to_string()),
        &mut render,
    )
    .expect("open rename dialog");
    assert!(matches!(
        state.dialog_state(),
        Some(DialogState::RenameSession { .. })
    ));
    state.set_dialog_input("Renamed Session".to_string());
    dispatch_action(&app, &mut state, TuiAction::ConfirmDialog, &mut render)
        .expect("confirm rename");

    dispatch_action(
        &app,
        &mut state,
        TuiAction::SubmitChatInput("/model glm-5-air".to_string()),
        &mut render,
    )
    .expect("set model");
    dispatch_action(
        &app,
        &mut state,
        TuiAction::SubmitChatInput("/reasoning off".to_string()),
        &mut render,
    )
    .expect("hide reasoning");
    dispatch_action(
        &app,
        &mut state,
        TuiAction::SubmitChatInput("/think high".to_string()),
        &mut render,
    )
    .expect("set think");
    dispatch_action(
        &app,
        &mut state,
        TuiAction::SubmitChatInput("/compact".to_string()),
        &mut render,
    )
    .expect("compact placeholder");

    let current_id = state
        .current_session_id()
        .expect("current session")
        .to_string();
    let updated = app
        .list_session_summaries()
        .expect("list updated")
        .into_iter()
        .find(|item| item.id == current_id)
        .expect("updated session summary");
    assert_eq!(updated.title, "Renamed Session");
    assert_eq!(updated.model.as_deref(), Some("glm-5-air"));
    assert!(!updated.reasoning_visible);
    assert_eq!(updated.think_level.as_deref(), Some("high"));
    assert_eq!(updated.compactifications, 1);
    let context_summary = app
        .context_summary(&current_id)
        .expect("load context summary")
        .expect("persisted compact summary");
    assert_eq!(context_summary.summary_text, "TUI compact summary.");
    assert_eq!(context_summary.covered_message_count, 2);

    dispatch_action(
        &app,
        &mut state,
        TuiAction::SubmitChatInput("/clear".to_string()),
        &mut render,
    )
    .expect("open clear dialog");
    assert!(matches!(
        state.dialog_state(),
        Some(DialogState::ConfirmClear { .. })
    ));
    dispatch_action(&app, &mut state, TuiAction::ConfirmDialog, &mut render)
        .expect("confirm clear");

    assert_eq!(state.active_screen(), TuiScreen::Chat);
    assert_eq!(
        app.list_session_summaries()
            .expect("list after clear")
            .len(),
        1
    );
    assert_ne!(state.current_session_id(), Some(session.id.as_str()));
}

#[test]
fn tui_chat_commands_and_timeline_approve_targets_latest_or_explicit_pending_approval() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let session = app
        .create_session_auto(Some("Approval Session"))
        .expect("create session");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    let mut older = RunEngine::new("run-old", &session.id, None, 10);
    older.start(10).expect("start older");
    older
        .wait_for_approval(
            ApprovalRequest::new("approval-old", "tool-call-old", "approve old", 11),
            11,
        )
        .expect("older approval");
    store
        .put_run(&RunRecord::try_from(older.snapshot()).expect("old run record"))
        .expect("put older");

    let mut newer = RunEngine::new("run-new", &session.id, None, 20);
    newer.start(20).expect("start newer");
    newer
        .wait_for_approval(
            ApprovalRequest::new("approval-new", "tool-call-new", "approve new", 21),
            21,
        )
        .expect("newer approval");
    store
        .put_run(&RunRecord::try_from(newer.snapshot()).expect("new run record"))
        .expect("put newer");

    let mut state = TuiAppState::new(
        app.list_session_summaries().expect("list sessions"),
        Some(session.id.clone()),
    );
    let mut render = |_state: &TuiAppState| Ok::<_, agentd::bootstrap::BootstrapError>(());

    dispatch_action(
        &app,
        &mut state,
        TuiAction::SubmitChatInput("/approve".to_string()),
        &mut render,
    )
    .expect("approve latest");
    wait_for_tui_idle(&app, &mut state, &mut render);

    let latest = RunSnapshot::try_from(
        store
            .get_run("run-new")
            .expect("load latest run")
            .expect("latest run"),
    )
    .expect("latest snapshot");
    assert_eq!(latest.status, RunStatus::Resuming);

    dispatch_action(
        &app,
        &mut state,
        TuiAction::SubmitChatInput("/approve approval-old".to_string()),
        &mut render,
    )
    .expect("approve explicit");
    wait_for_tui_idle(&app, &mut state, &mut render);

    let explicit = RunSnapshot::try_from(
        store
            .get_run("run-old")
            .expect("load explicit run")
            .expect("explicit run"),
    )
    .expect("explicit snapshot");
    assert_eq!(explicit.status, RunStatus::Resuming);
}

#[test]
fn tui_chat_commands_and_timeline_assigns_timestamps_and_updates_tool_rows_in_place() {
    let mut timeline = Timeline::default();

    timeline.push_user("hello", 10);
    timeline.push_reasoning_delta("reasoning one", 11);
    timeline.push_assistant_delta("hello ", 12);
    timeline.push_assistant_delta("world", 12);
    timeline.update_tool_status(
        "web_fetch",
        agentd::execution::ToolExecutionStatus::Requested,
        13,
    );
    timeline.update_tool_status(
        "web_fetch",
        agentd::execution::ToolExecutionStatus::WaitingApproval,
        14,
    );
    timeline.update_tool_status(
        "web_fetch",
        agentd::execution::ToolExecutionStatus::Completed,
        15,
    );

    let rendered = timeline.entries(true);
    assert!(rendered.iter().all(|entry| entry.timestamp > 0));
    assert_eq!(
        rendered
            .iter()
            .filter(|entry| matches!(entry.kind, TimelineEntryKind::Tool { .. }))
            .count(),
        1
    );
    assert!(matches!(
        rendered
            .iter()
            .find(|entry| matches!(entry.kind, TimelineEntryKind::Tool { .. }))
            .expect("tool entry")
            .kind,
        TimelineEntryKind::Tool { ref status, .. } if status == "completed"
    ));
    assert_eq!(
        rendered
            .iter()
            .filter(|entry| matches!(entry.kind, TimelineEntryKind::Reasoning))
            .count(),
        1
    );
    assert_eq!(
        rendered
            .iter()
            .filter(|entry| matches!(entry.kind, TimelineEntryKind::Assistant))
            .count(),
        1
    );
}

#[test]
fn tui_end_to_end_streams_assistant_text_and_reasoning_into_the_timeline() {
    let stream = "data: {\"type\":\"response.reasoning_summary_text.delta\",\"item_id\":\"rs_1\",\"output_index\":0,\"summary_index\":0,\"delta\":\"compare context \"}\n\n\
data: {\"type\":\"response.output_text.delta\",\"item_id\":\"msg_1\",\"output_index\":1,\"content_index\":0,\"delta\":\"hello \"}\n\n\
data: {\"type\":\"response.output_text.delta\",\"item_id\":\"msg_1\",\"output_index\":1,\"content_index\":0,\"delta\":\"from tui\"}\n\n\
data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_tui_stream\",\"model\":\"gpt-5.4\",\"output\":[{\"id\":\"rs_1\",\"type\":\"reasoning\",\"summary\":[{\"type\":\"summary_text\",\"text\":\"compare context \"}]},{\"id\":\"msg_1\",\"type\":\"message\",\"status\":\"completed\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"hello from tui\",\"annotations\":[]}]}],\"usage\":{\"input_tokens\":11,\"output_tokens\":7,\"total_tokens\":18}}}\n\n".to_string();
    let (api_base, handle) = spawn_sse_server_sequence(vec![stream]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
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
    let session = app
        .create_session_auto(Some("Stream Session"))
        .expect("create session");
    let mut state = TuiAppState::new(
        app.list_session_summaries().expect("list sessions"),
        Some(session.id.clone()),
    );
    let mut redraw = |_state: &TuiAppState| Ok::<_, agentd::bootstrap::BootstrapError>(());

    dispatch_action(
        &app,
        &mut state,
        TuiAction::SubmitChatInput("hello streaming tui".to_string()),
        &mut redraw,
    )
    .expect("dispatch chat");
    wait_for_tui_idle(&app, &mut state, &mut redraw);
    handle.join().expect("join sse");

    let entries = state.timeline().entries(true);
    assert!(
        entries
            .iter()
            .any(|entry| matches!(entry.kind, TimelineEntryKind::User))
    );
    assert!(
        entries
            .iter()
            .any(|entry| matches!(entry.kind, TimelineEntryKind::Reasoning))
    );
    assert!(entries.iter().any(|entry| {
        matches!(entry.kind, TimelineEntryKind::Assistant) && entry.content == "hello from tui"
    }));
}

#[test]
fn tui_end_to_end_reasoning_toggle_hides_reasoning_lines_from_the_chat_view() {
    let stream = "data: {\"type\":\"response.reasoning_summary_text.delta\",\"item_id\":\"rs_2\",\"output_index\":0,\"summary_index\":0,\"delta\":\"quiet reasoning \"}\n\n\
data: {\"type\":\"response.output_text.delta\",\"item_id\":\"msg_2\",\"output_index\":1,\"content_index\":0,\"delta\":\"visible answer\"}\n\n\
data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_tui_hidden_reasoning\",\"model\":\"gpt-5.4\",\"output\":[{\"id\":\"rs_2\",\"type\":\"reasoning\",\"summary\":[{\"type\":\"summary_text\",\"text\":\"quiet reasoning \"}]},{\"id\":\"msg_2\",\"type\":\"message\",\"status\":\"completed\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"visible answer\",\"annotations\":[]}]}],\"usage\":{\"input_tokens\":9,\"output_tokens\":5,\"total_tokens\":14}}}\n\n".to_string();
    let (api_base, handle) = spawn_sse_server_sequence(vec![stream]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
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
    let session = app
        .create_session_auto(Some("Reasoning Toggle Session"))
        .expect("create session");
    app.update_session_preferences(
        &session.id,
        SessionPreferencesPatch {
            reasoning_visible: Some(false),
            ..SessionPreferencesPatch::default()
        },
    )
    .expect("hide reasoning");
    let mut state = TuiAppState::new(
        app.list_session_summaries().expect("list sessions"),
        Some(session.id.clone()),
    );
    let mut redraw = |_state: &TuiAppState| Ok::<_, agentd::bootstrap::BootstrapError>(());

    dispatch_action(
        &app,
        &mut state,
        TuiAction::SubmitChatInput("hello hidden reasoning".to_string()),
        &mut redraw,
    )
    .expect("dispatch chat");
    wait_for_tui_idle(&app, &mut state, &mut redraw);
    handle.join().expect("join sse");

    assert!(
        !state
            .timeline()
            .entries(false)
            .iter()
            .any(|entry| matches!(entry.kind, TimelineEntryKind::Reasoning))
    );
    assert!(
        state
            .timeline()
            .entries(true)
            .iter()
            .any(|entry| matches!(entry.kind, TimelineEntryKind::Reasoning))
    );
}

#[test]
fn tui_chat_send_provider_failure_stays_inside_timeline_instead_of_exiting() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind port probe");
    let address = listener.local_addr().expect("probe local addr");
    drop(listener);

    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        provider: ConfiguredProvider {
            kind: ProviderKind::ZaiChatCompletions,
            api_base: Some(format!("http://{address}")),
            api_key: Some("test-key".to_string()),
            default_model: Some("glm-5-turbo".to_string()),
            ..ConfiguredProvider::default()
        },
        ..AppConfig::default()
    })
    .expect("build app");
    let session = app
        .create_session_auto(Some("Failure Session"))
        .expect("create session");
    let mut state = TuiAppState::new(
        app.list_session_summaries().expect("list sessions"),
        Some(session.id.clone()),
    );
    let mut redraw = |_state: &TuiAppState| Ok::<_, agentd::bootstrap::BootstrapError>(());

    let result = dispatch_action(
        &app,
        &mut state,
        TuiAction::SubmitChatInput("hello timeout path".to_string()),
        &mut redraw,
    );
    wait_for_tui_idle(&app, &mut state, &mut redraw);

    assert!(result.is_ok(), "provider failure should stay in the TUI");
    assert!(state.timeline().entries(true).iter().any(|entry| {
        matches!(entry.kind, TimelineEntryKind::System) && entry.content.starts_with("chat failed:")
    }));
}

#[test]
fn tui_chat_submit_returns_without_blocking_on_provider_response() {
    let stream = "data: {\"type\":\"response.output_text.delta\",\"item_id\":\"msg_nonblocking\",\"output_index\":0,\"content_index\":0,\"delta\":\"hello later\"}\n\n\
data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_nonblocking\",\"model\":\"gpt-5.4\",\"output\":[{\"id\":\"msg_nonblocking\",\"type\":\"message\",\"status\":\"completed\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"hello later\",\"annotations\":[]}]}],\"usage\":{\"input_tokens\":9,\"output_tokens\":4,\"total_tokens\":13}}}\n\n".to_string();
    let (api_base, handle) =
        spawn_delayed_sse_server_sequence(vec![(Duration::from_millis(400), stream)]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
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
    let session = app
        .create_session_auto(Some("Non Blocking Session"))
        .expect("create session");
    let mut state = TuiAppState::new(
        app.list_session_summaries().expect("list sessions"),
        Some(session.id.clone()),
    );
    let mut redraw = |_state: &TuiAppState| Ok::<_, agentd::bootstrap::BootstrapError>(());

    let started = Instant::now();
    dispatch_action(
        &app,
        &mut state,
        TuiAction::SubmitChatInput("hello nonblocking".to_string()),
        &mut redraw,
    )
    .expect("dispatch chat");
    let elapsed = started.elapsed();

    handle.join().expect("join delayed sse");

    assert!(
        elapsed < Duration::from_millis(250),
        "expected TUI submit to return quickly, got {:?}",
        elapsed
    );
}

#[test]
fn tui_composer_remains_editable_while_a_background_run_is_active() {
    let stream = "data: {\"type\":\"response.output_text.delta\",\"item_id\":\"msg_background_edit\",\"output_index\":0,\"content_index\":0,\"delta\":\"hello later\"}\n\n\
data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_background_edit\",\"model\":\"gpt-5.4\",\"output\":[{\"id\":\"msg_background_edit\",\"type\":\"message\",\"status\":\"completed\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"hello later\",\"annotations\":[]}]}],\"usage\":{\"input_tokens\":9,\"output_tokens\":4,\"total_tokens\":13}}}\n\n".to_string();
    let (api_base, handle) =
        spawn_delayed_sse_server_sequence(vec![(Duration::from_millis(400), stream)]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
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
    let session = app
        .create_session_auto(Some("Editable Composer Session"))
        .expect("create session");
    let mut state = TuiAppState::new(
        app.list_session_summaries().expect("list sessions"),
        Some(session.id.clone()),
    );
    let mut redraw = |_state: &TuiAppState| Ok::<_, agentd::bootstrap::BootstrapError>(());

    dispatch_action(
        &app,
        &mut state,
        TuiAction::SubmitChatInput("hello nonblocking".to_string()),
        &mut redraw,
    )
    .expect("dispatch chat");
    state.push_input_char('n');
    state.push_input_char('e');
    state.push_input_char('x');
    state.push_input_char('t');

    assert!(state.has_active_run());
    assert_eq!(state.input_buffer(), "next");

    wait_for_tui_idle(&app, &mut state, &mut redraw);
    handle.join().expect("join delayed sse");
}

#[test]
fn tui_priority_submit_interrupts_after_a_completed_tool_step() {
    let (api_base, handle) = spawn_openai_priority_server();
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
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
    let session = app
        .create_session_auto(Some("Priority Session"))
        .expect("create session");
    let mut state = TuiAppState::new(
        app.list_session_summaries().expect("list sessions"),
        Some(session.id.clone()),
    );
    let mut redraw = |_state: &TuiAppState| Ok::<_, agentd::bootstrap::BootstrapError>(());

    dispatch_action(
        &app,
        &mut state,
        TuiAction::SubmitChatInput("plan first".to_string()),
        &mut redraw,
    )
    .expect("dispatch initial chat");
    dispatch_action(
        &app,
        &mut state,
        TuiAction::SubmitChatInput("urgent follow-up".to_string()),
        &mut redraw,
    )
    .expect("dispatch queued priority chat");

    wait_for_tui_idle(&app, &mut state, &mut redraw);
    handle.join().expect("join sse");

    let entries = state.timeline().entries(true);
    assert!(entries.iter().any(|entry| {
        matches!(entry.kind, TimelineEntryKind::Assistant) && entry.content == "fresh answer"
    }));
    assert!(!entries.iter().any(|entry| {
        matches!(entry.kind, TimelineEntryKind::Assistant) && entry.content == "stale answer"
    }));
}

#[test]
fn tui_deferred_queue_sends_after_the_current_run_completes() {
    let first = openai_stream_message_response("resp_tui_deferred_first", "first answer");
    let second = openai_stream_message_response("resp_tui_deferred_second", "second answer");
    let (api_base, handle) = spawn_delayed_sse_server_sequence(vec![
        (Duration::from_millis(200), first),
        (Duration::from_millis(10), second),
    ]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
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
    let session = app
        .create_session_auto(Some("Deferred Queue Session"))
        .expect("create session");
    let mut state = TuiAppState::new(
        app.list_session_summaries().expect("list sessions"),
        Some(session.id.clone()),
    );
    let mut redraw = |_state: &TuiAppState| Ok::<_, agentd::bootstrap::BootstrapError>(());

    dispatch_action(
        &app,
        &mut state,
        TuiAction::SubmitChatInput("first request".to_string()),
        &mut redraw,
    )
    .expect("dispatch first request");
    dispatch_action(
        &app,
        &mut state,
        TuiAction::QueueChatInput("second request".to_string()),
        &mut redraw,
    )
    .expect("queue second request");

    wait_for_tui_idle(&app, &mut state, &mut redraw);
    handle.join().expect("join delayed sequence");

    let assistant_messages = state
        .timeline()
        .entries(true)
        .into_iter()
        .filter(|entry| matches!(entry.kind, TimelineEntryKind::Assistant))
        .map(|entry| entry.content.clone())
        .collect::<Vec<_>>();
    assert_eq!(assistant_messages, vec!["first answer", "second answer"]);
}

#[test]
fn tui_shift_tab_cycles_slash_commands_in_place() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let mut state = TuiAppState::new(
        vec![summary("session-a", "Session A")],
        Some("session-a".to_string()),
    );
    state.replace_input_buffer("/r");

    dispatch_action(
        &app,
        &mut state,
        TuiAction::CyclePreviousCommand,
        &mut |_state| Ok::<_, agentd::bootstrap::BootstrapError>(()),
    )
    .expect("cycle commands first");
    assert_eq!(state.input_buffer(), "/rename");

    dispatch_action(
        &app,
        &mut state,
        TuiAction::CyclePreviousCommand,
        &mut |_state| Ok::<_, agentd::bootstrap::BootstrapError>(()),
    )
    .expect("cycle commands second");
    assert_eq!(state.input_buffer(), "/reasoning");
}

#[test]
fn tui_end_to_end_session_create_and_delete_wait_for_confirmation() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let first = app
        .create_session_auto(Some("First Session"))
        .expect("create first");
    let second = app
        .create_session_auto(Some("Second Session"))
        .expect("create second");
    let mut state = TuiAppState::new(app.list_session_summaries().expect("list sessions"), None);
    let mut redraw = |_state: &TuiAppState| Ok::<_, agentd::bootstrap::BootstrapError>(());

    dispatch_action(
        &app,
        &mut state,
        TuiAction::OpenNewSessionDialog,
        &mut redraw,
    )
    .expect("open new dialog");
    state.set_dialog_input("Created From Session Screen".to_string());
    assert_eq!(
        app.list_session_summaries().expect("before create").len(),
        2
    );
    dispatch_action(&app, &mut state, TuiAction::ConfirmDialog, &mut redraw)
        .expect("confirm create");
    assert_eq!(app.list_session_summaries().expect("after create").len(), 3);

    state.open_session_screen();
    while state.selected_session().map(|session| session.id.as_str()) != Some(first.id.as_str()) {
        state.select_next_session();
    }
    dispatch_action(&app, &mut state, TuiAction::OpenDeleteDialog, &mut redraw)
        .expect("open delete dialog");
    assert_eq!(
        app.list_session_summaries().expect("before delete").len(),
        3
    );
    dispatch_action(&app, &mut state, TuiAction::ConfirmDialog, &mut redraw)
        .expect("confirm delete");

    let remaining = app.list_session_summaries().expect("after delete");
    assert_eq!(remaining.len(), 2);
    assert!(remaining.iter().all(|session| session.id != first.id));
    assert!(remaining.iter().any(|session| session.id == second.id));
}

fn spawn_sse_server_sequence(responses: Vec<String>) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let address = listener.local_addr().expect("local addr");

    let handle = thread::spawn(move || {
        for body in responses {
            let (mut stream, _) = listener.accept().expect("accept connection");
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                .expect("set read timeout");
            let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
            let mut raw_request = String::new();
            loop {
                let mut line = String::new();
                let read = reader.read_line(&mut line).expect("read request line");
                if read == 0 || line == "\r\n" {
                    break;
                }
                raw_request.push_str(&line);
            }
            let mut content_length = 0usize;
            for header in raw_request.lines() {
                let lower = header.to_ascii_lowercase();
                if let Some(value) = lower.strip_prefix("content-length:") {
                    content_length = value.trim().parse::<usize>().expect("parse content-length");
                }
            }
            if content_length > 0 {
                let mut discard = vec![0u8; content_length];
                reader.read_exact(&mut discard).expect("read body");
            }
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("write response");
            stream.flush().expect("flush response");
        }
    });

    (format!("http://{}", address), handle)
}

fn openai_stream_message_response(response_id: &str, text: &str) -> String {
    let text = serde_json::to_string(text).expect("serialize text");
    format!(
        "data: {{\"type\":\"response.completed\",\"response\":{{\"id\":\"{response_id}\",\"model\":\"gpt-5.4\",\"output\":[{{\"id\":\"msg_1\",\"type\":\"message\",\"status\":\"completed\",\"role\":\"assistant\",\"content\":[{{\"type\":\"output_text\",\"text\":{text},\"annotations\":[]}}]}}],\"usage\":{{\"input_tokens\":16,\"output_tokens\":3,\"total_tokens\":19}}}}}}\n\n"
    )
}

fn openai_stream_tool_call_response(
    response_id: &str,
    call_id: &str,
    tool_name: &str,
    arguments: &str,
) -> String {
    let arguments = serde_json::to_string(arguments).expect("serialize arguments");
    format!(
        "data: {{\"type\":\"response.completed\",\"response\":{{\"id\":\"{response_id}\",\"model\":\"gpt-5.4\",\"output\":[{{\"id\":\"fc_1\",\"type\":\"function_call\",\"status\":\"completed\",\"call_id\":\"{call_id}\",\"name\":\"{tool_name}\",\"arguments\":{arguments}}}],\"usage\":{{\"input_tokens\":19,\"output_tokens\":7,\"total_tokens\":26}}}}}}\n\n"
    )
}

fn spawn_openai_priority_server() -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind priority server");
    listener
        .set_nonblocking(true)
        .expect("set nonblocking listener");
    let address = listener.local_addr().expect("priority local addr");

    let handle = thread::spawn(move || {
        let mut last_activity = Instant::now();
        loop {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    last_activity = Instant::now();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .expect("set read timeout");
                    let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
                    let mut raw_request = String::new();
                    loop {
                        let mut line = String::new();
                        let read = reader.read_line(&mut line).expect("read request line");
                        if read == 0 {
                            break;
                        }
                        raw_request.push_str(&line);
                        if line == "\r\n" {
                            break;
                        }
                    }
                    let content_length = raw_request
                        .lines()
                        .find_map(|header| {
                            let lower = header.to_ascii_lowercase();
                            lower
                                .strip_prefix("content-length:")
                                .map(str::trim)
                                .and_then(|value| value.parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if content_length > 0 {
                        let mut discard = vec![0u8; content_length];
                        reader.read_exact(&mut discard).expect("read body");
                        raw_request.push_str(&String::from_utf8(discard).expect("utf8 body"));
                    }

                    let body = if raw_request.contains("urgent follow-up") {
                        openai_stream_message_response("resp_tui_priority_fresh", "fresh answer")
                    } else if raw_request
                        .to_ascii_lowercase()
                        .contains("\"type\":\"function_call_output\"")
                    {
                        openai_stream_message_response("resp_tui_priority_stale", "stale answer")
                    } else {
                        openai_stream_tool_call_response(
                            "resp_tui_priority_tool",
                            "call_plan_write",
                            "plan_write",
                            r#"{"items":[{"id":"p1","content":"inspect request","status":"in_progress"}]}"#,
                        )
                    };

                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    stream
                        .write_all(response.as_bytes())
                        .expect("write response");
                    stream.flush().expect("flush response");
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if last_activity.elapsed() > Duration::from_millis(300) {
                        break;
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("priority server accept failed: {error}"),
            }
        }
    });

    (format!("http://{}", address), handle)
}

fn spawn_json_server_sequence(responses: Vec<String>) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let address = listener.local_addr().expect("local addr");

    let handle = thread::spawn(move || {
        for body in responses {
            let (mut stream, _) = listener.accept().expect("accept connection");
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                .expect("set read timeout");
            let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
            let mut raw_request = String::new();
            loop {
                let mut line = String::new();
                let read = reader.read_line(&mut line).expect("read request line");
                if read == 0 || line == "\r\n" {
                    break;
                }
                raw_request.push_str(&line);
            }
            let mut content_length = 0usize;
            for header in raw_request.lines() {
                let lower = header.to_ascii_lowercase();
                if let Some(value) = lower.strip_prefix("content-length:") {
                    content_length = value.trim().parse::<usize>().expect("parse content-length");
                }
            }
            if content_length > 0 {
                let mut discard = vec![0u8; content_length];
                reader.read_exact(&mut discard).expect("read body");
            }
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("write response");
            stream.flush().expect("flush response");
        }
    });

    (format!("http://{}", address), handle)
}

fn spawn_delayed_sse_server_sequence(
    responses: Vec<(Duration, String)>,
) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let address = listener.local_addr().expect("local addr");

    let handle = thread::spawn(move || {
        for (delay, body) in responses {
            let (mut stream, _) = listener.accept().expect("accept connection");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("set read timeout");
            let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
            let mut raw_request = String::new();
            loop {
                let mut line = String::new();
                let read = reader.read_line(&mut line).expect("read request line");
                if read == 0 || line == "\r\n" {
                    break;
                }
                raw_request.push_str(&line);
            }
            let mut content_length = 0usize;
            for header in raw_request.lines() {
                let lower = header.to_ascii_lowercase();
                if let Some(value) = lower.strip_prefix("content-length:") {
                    content_length = value.trim().parse::<usize>().expect("parse content-length");
                }
            }
            if content_length > 0 {
                let mut discard = vec![0u8; content_length];
                reader.read_exact(&mut discard).expect("read body");
            }
            thread::sleep(delay);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("write response");
            stream.flush().expect("flush response");
        }
    });

    (format!("http://{}", address), handle)
}

fn wait_for_tui_idle(
    app: &agentd::bootstrap::App,
    state: &mut TuiAppState,
    redraw: &mut dyn FnMut(&TuiAppState) -> Result<(), agentd::bootstrap::BootstrapError>,
) {
    for _ in 0..100 {
        pump_background(app, state, redraw).expect("pump background");
        if !state.has_active_run() {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("tui did not become idle in time");
}
