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
}
