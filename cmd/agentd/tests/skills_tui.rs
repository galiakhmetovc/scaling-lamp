use agent_persistence::AppConfig;
use agentd::bootstrap::build_from_config;
use agentd::tui::app::TuiAppState;
use agentd::tui::events::TuiAction;
use agentd::tui::{TuiScreen, dispatch_action};

fn write_skill(dir: &std::path::Path, name: &str, description: &str) {
    let skill_dir = dir.join(name);
    std::fs::create_dir_all(&skill_dir).expect("create skill dir");
    std::fs::write(
        skill_dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n"),
    )
    .expect("write skill");
}

#[test]
fn skills_tui_russian_commands_list_enable_and_disable_session_skills() {
    let temp = tempfile::tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("skills dir");
    write_skill(
        &skills_dir,
        "rust-debug",
        "Debug Rust compiler errors and cargo regressions.",
    );
    write_skill(
        &skills_dir,
        "postgres",
        "Investigate PostgreSQL queries and migration issues.",
    );

    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        daemon: agent_persistence::DaemonConfig {
            skills_dir: skills_dir.clone(),
            ..Default::default()
        },
        ..AppConfig::default()
    })
    .expect("build app");
    let session = app
        .create_session_auto(Some("Skill Session"))
        .expect("create session");
    let mut state = TuiAppState::new(
        app.list_session_summaries().expect("list sessions"),
        Some(session.id.clone()),
    );
    let mut render = |_state: &TuiAppState| Ok::<_, agentd::bootstrap::BootstrapError>(());

    dispatch_action(
        &app,
        &mut state,
        TuiAction::SubmitChatInput("\\скиллы".to_string()),
        &mut render,
    )
    .expect("list skills");
    dispatch_action(
        &app,
        &mut state,
        TuiAction::SubmitChatInput("\\включить rust-debug".to_string()),
        &mut render,
    )
    .expect("enable skill");
    dispatch_action(
        &app,
        &mut state,
        TuiAction::SubmitChatInput("\\выключить rust-debug".to_string()),
        &mut render,
    )
    .expect("disable skill");

    assert_eq!(state.active_screen(), TuiScreen::Chat);
    let rendered = state
        .timeline()
        .entries(true)
        .into_iter()
        .map(|entry| entry.content.clone())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("rust-debug"));
    assert!(rendered.contains("postgres"));
    assert!(rendered.contains("manual"));
    assert!(rendered.contains("disabled"));
}
