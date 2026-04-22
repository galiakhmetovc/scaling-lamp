use super::support::*;
use agent_runtime::agent::AgentTemplateKind;

#[test]
fn build_from_config_bootstraps_builtin_agents_and_selects_default() {
    let temp = tempfile::tempdir().expect("tempdir");
    let data_dir = temp.path().join("state-root");
    let app = build_from_config(AppConfig {
        data_dir: data_dir.clone(),
        ..AppConfig::default()
    })
    .expect("build app");

    let agents = app.list_agents().expect("list agents");
    assert_eq!(
        agents
            .iter()
            .map(|profile| profile.id.as_str())
            .collect::<Vec<_>>(),
        vec!["default", "judge"]
    );

    let current = app.current_agent_profile().expect("current agent");
    assert_eq!(current.id, "default");
    assert_eq!(current.template_kind, AgentTemplateKind::Default);

    for agent_id in ["default", "judge"] {
        let agent_home = data_dir.join("agents").join(agent_id);
        assert!(agent_home.join("SYSTEM.md").is_file());
        assert!(agent_home.join("AGENTS.md").is_file());
        assert!(agent_home.join("skills").is_dir());
    }

    let store = PersistenceStore::open(&app.persistence).expect("open store");
    assert_eq!(
        store
            .get_current_agent_profile_id()
            .expect("get current selected agent"),
        Some("default".to_string())
    );
}

#[test]
fn create_session_binds_the_current_selected_agent_profile() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");

    let selected = app
        .select_agent_profile("judge")
        .expect("select judge profile");
    assert_eq!(selected.id, "judge");

    let session = app
        .create_session_auto(Some("Judge Session"))
        .expect("create session");
    let store = PersistenceStore::open(&app.persistence).expect("open store");
    let stored = store
        .get_session(&session.id)
        .expect("get session")
        .expect("session exists");

    assert_eq!(stored.agent_profile_id, "judge");
}

#[test]
fn create_agent_from_template_copies_template_files_independently() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");

    let judge_home = app.agent_home_path("judge").expect("judge home");
    let judge_agents_before =
        fs::read_to_string(judge_home.join("AGENTS.md")).expect("read judge agents");
    let judge_system_before =
        fs::read_to_string(judge_home.join("SYSTEM.md")).expect("read judge system");

    let created = app
        .create_agent_from_template("Judge Copy", Some("judge"))
        .expect("create agent from judge");
    assert_eq!(created.template_kind, AgentTemplateKind::Custom);
    assert_ne!(created.agent_home, judge_home);

    assert_eq!(
        fs::read_to_string(created.agent_home.join("SYSTEM.md")).expect("read copied system"),
        judge_system_before
    );
    assert_eq!(
        fs::read_to_string(created.agent_home.join("AGENTS.md")).expect("read copied agents"),
        judge_agents_before
    );

    fs::write(created.agent_home.join("AGENTS.md"), "customized copy").expect("mutate copy");
    assert_eq!(
        fs::read_to_string(judge_home.join("AGENTS.md")).expect("re-read judge agents"),
        judge_agents_before
    );

    let store = PersistenceStore::open(&app.persistence).expect("open store");
    let stored = store
        .get_agent_profile(&created.id)
        .expect("get created agent profile")
        .expect("created profile exists");
    assert_eq!(stored.template_kind, "custom");
}
