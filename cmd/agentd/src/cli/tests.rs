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
