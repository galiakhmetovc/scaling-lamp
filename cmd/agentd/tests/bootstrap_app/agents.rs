use super::support::*;
use agent_runtime::agent::AgentTemplateKind;
use agent_runtime::tool::{AgentCreateInput, AgentListInput, AgentReadInput};

fn seed_running_tool_context(
    store: &PersistenceStore,
    session_id: &str,
    agent_profile_id: &str,
    mission_id: &str,
    job_id: &str,
    run_id: &str,
) {
    store
        .put_session(&SessionRecord {
            id: session_id.to_string(),
            title: session_id.to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            agent_profile_id: agent_profile_id.to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");
    store
        .put_mission(&MissionRecord {
            id: mission_id.to_string(),
            session_id: session_id.to_string(),
            objective: "agent tool".to_string(),
            status: MissionStatus::Running.as_str().to_string(),
            execution_intent: MissionExecutionIntent::Autonomous.as_str().to_string(),
            schedule_json: serde_json::to_string(&MissionSchedule::once())
                .expect("serialize schedule"),
            acceptance_json: "[]".to_string(),
            created_at: 2,
            updated_at: 2,
            completed_at: None,
        })
        .expect("put mission");

    let mut job = JobSpec::mission_turn(
        job_id,
        session_id,
        mission_id,
        Some(run_id),
        None,
        "tool",
        3,
    );
    job.status = agent_runtime::mission::JobStatus::Running;
    job.started_at = Some(4);
    job.updated_at = 4;

    let mut run = RunEngine::new(run_id, session_id, Some(mission_id), 4);
    run.start(4).expect("start run");
    store
        .put_run(&RunRecord::try_from(run.snapshot()).expect("run record"))
        .expect("put run");
    store
        .put_job(&JobRecord::try_from(&job).expect("job record"))
        .expect("put job");
}

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
    assert_eq!(current.name, "Ассистент");
    assert_eq!(current.template_kind, AgentTemplateKind::Default);

    for agent_id in ["default", "judge"] {
        let agent_home = data_dir.join("agents").join(agent_id);
        assert!(agent_home.join("SYSTEM.md").is_file());
        assert!(agent_home.join("AGENTS.md").is_file());
        assert!(agent_home.join("skills").is_dir());
    }
    assert!(
        data_dir
            .join("agents/default/skills/obsidian-vault/SKILL.md")
            .is_file()
    );

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

#[test]
fn build_from_config_refreshes_legacy_default_prompts_but_preserves_custom_edits() {
    let temp = tempfile::tempdir().expect("tempdir");
    let data_dir = temp.path().join("state-root");
    let app = build_from_config(AppConfig {
        data_dir: data_dir.clone(),
        ..AppConfig::default()
    })
    .expect("build app");

    let default_home = app.agent_home_path("default").expect("default home");
    fs::write(
        default_home.join("SYSTEM.md"),
        "You are the default autonomous coding agent runtime profile.\n\nWork directly, preserve the canonical runtime path, and keep outputs concise and operational.\n",
    )
    .expect("write legacy system");
    fs::write(
        default_home.join("AGENTS.md"),
        "Default agent profile.\n\n- Primary role: general-purpose coding agent\n- Prefer direct execution over unnecessary planning\n- Keep tool usage explicit and minimal\n",
    )
    .expect("write legacy agents");

    let _ = build_from_config(AppConfig {
        data_dir: data_dir.clone(),
        ..AppConfig::default()
    })
    .expect("rebuild app");

    let refreshed_system =
        fs::read_to_string(default_home.join("SYSTEM.md")).expect("read refreshed system");
    let refreshed_agents =
        fs::read_to_string(default_home.join("AGENTS.md")).expect("read refreshed agents");
    assert!(refreshed_system.contains("assistant autonomous coding agent runtime profile"));
    assert!(refreshed_agents.contains("Assistant agent profile."));
    assert!(refreshed_agents.contains("Never invent tool names"));
    assert!(refreshed_agents.contains("exec_read_output"));
    assert!(refreshed_agents.contains("knowledge_search"));
    assert!(refreshed_agents.contains("session_search"));
    assert!(refreshed_agents.contains("use `continue_later` with `delay_seconds`"));
    assert!(refreshed_agents.contains("set `delivery_mode` to `existing_session`"));
    let obsidian_skill = fs::read_to_string(default_home.join("skills/obsidian-vault/SKILL.md"))
        .expect("read obsidian skill");
    assert!(obsidian_skill.contains("name: obsidian-vault"));
    assert!(obsidian_skill.contains("Use the `obsidian` MCP connector first"));

    fs::write(default_home.join("AGENTS.md"), "custom prompt preserved\n")
        .expect("write custom agents");
    let _ = build_from_config(AppConfig {
        data_dir,
        ..AppConfig::default()
    })
    .expect("rebuild app after custom edit");

    let preserved_agents =
        fs::read_to_string(default_home.join("AGENTS.md")).expect("read preserved agents");
    assert_eq!(preserved_agents, "custom prompt preserved\n");
}

#[test]
fn agent_tools_can_create_list_and_read_custom_agents() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        permissions: PermissionConfig {
            mode: PermissionMode::AcceptEdits,
            rules: Vec::new(),
        },
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");
    seed_running_tool_context(
        &store,
        "session-agent-tool",
        "default",
        "mission-agent-tool",
        "job-agent-tool",
        "run-agent-tool",
    );

    let create_report = app
        .request_tool_approval(
            "job-agent-tool",
            "run-agent-tool",
            &ToolCall::AgentCreate(AgentCreateInput {
                name: "Builder Copy".to_string(),
                template_identifier: None,
            }),
            20,
        )
        .expect("agent create tool");
    assert_eq!(create_report.run_status, RunStatus::WaitingApproval);
    assert_eq!(
        create_report.approval_id.as_deref(),
        Some("approval-job-agent-tool-agent_create")
    );

    let resumed = app
        .resume_tool_call(execution::ToolResumeRequest {
            job_id: "job-agent-tool",
            run_id: "run-agent-tool",
            approval_id: create_report.approval_id.as_deref().expect("approval id"),
            tool_call: &ToolCall::AgentCreate(AgentCreateInput {
                name: "Builder Copy".to_string(),
                template_identifier: None,
            }),
            workspace_root: app.runtime.workspace.root.as_path(),
            evidence: None,
            now: 21,
        })
        .expect("resume agent create tool");
    assert_eq!(resumed.run_status, RunStatus::Completed);
    assert!(
        resumed
            .output_summary
            .as_deref()
            .unwrap_or_default()
            .contains("agent_create")
    );

    let created = app.agent_profile("builder-copy").expect("created agent");
    assert_eq!(created.template_kind, AgentTemplateKind::Custom);
    assert!(created.agent_home.join("SYSTEM.md").is_file());
    assert!(created.agent_home.join("AGENTS.md").is_file());
    assert_eq!(created.created_from_template_id.as_deref(), Some("default"));
    assert_eq!(
        created.created_by_session_id.as_deref(),
        Some("session-agent-tool")
    );
    assert_eq!(
        created.created_by_agent_profile_id.as_deref(),
        Some("default")
    );
    let rendered = app
        .render_agent_profile(Some("builder-copy"))
        .expect("render agent profile");
    assert!(rendered.contains("created_from_template=default"));
    assert!(rendered.contains("created_by_session=session-agent-tool"));
    assert!(rendered.contains("created_by_agent=default"));

    let read_report = app
        .request_tool_approval(
            {
                seed_running_tool_context(
                    &store,
                    "session-agent-tool",
                    "default",
                    "mission-agent-tool-read",
                    "job-agent-tool-read",
                    "run-agent-tool-read",
                );
                "job-agent-tool-read"
            },
            "run-agent-tool-read",
            &ToolCall::AgentRead(AgentReadInput {
                identifier: "builder-copy".to_string(),
            }),
            21,
        )
        .expect("agent read tool");
    assert_eq!(read_report.run_status, RunStatus::Completed);

    let list_report = app
        .request_tool_approval(
            {
                seed_running_tool_context(
                    &store,
                    "session-agent-tool",
                    "default",
                    "mission-agent-tool-list",
                    "job-agent-tool-list",
                    "run-agent-tool-list",
                );
                "job-agent-tool-list"
            },
            "run-agent-tool-list",
            &ToolCall::AgentList(AgentListInput {
                limit: None,
                offset: None,
            }),
            22,
        )
        .expect("agent list tool");
    assert_eq!(list_report.run_status, RunStatus::Completed);
    assert!(
        list_report
            .output_summary
            .as_deref()
            .unwrap_or_default()
            .contains("agent_list")
    );
}

#[test]
fn agent_create_tool_rejects_unapproved_custom_template_agents() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        permissions: PermissionConfig {
            mode: PermissionMode::AcceptEdits,
            rules: Vec::new(),
        },
        ..AppConfig::default()
    })
    .expect("build app");
    let copied = app
        .create_agent_from_template("Judge Copy", Some("judge"))
        .expect("create operator copy");
    let store = PersistenceStore::open(&app.persistence).expect("open store");
    seed_running_tool_context(
        &store,
        "session-agent-template-policy",
        "default",
        "mission-agent-template-policy",
        "job-agent-template-policy",
        "run-agent-template-policy",
    );

    let approval = app
        .request_tool_approval(
            "job-agent-template-policy",
            "run-agent-template-policy",
            &ToolCall::AgentCreate(AgentCreateInput {
                name: "Nested Copy".to_string(),
                template_identifier: Some(copied.id.clone()),
            }),
            30,
        )
        .expect("request approval");
    assert_eq!(approval.run_status, RunStatus::WaitingApproval);

    let error = app
        .resume_tool_call(execution::ToolResumeRequest {
            job_id: "job-agent-template-policy",
            run_id: "run-agent-template-policy",
            approval_id: approval.approval_id.as_deref().expect("approval id"),
            tool_call: &ToolCall::AgentCreate(AgentCreateInput {
                name: "Nested Copy".to_string(),
                template_identifier: Some(copied.id.clone()),
            }),
            workspace_root: app.runtime.workspace.root.as_path(),
            evidence: None,
            now: 31,
        })
        .expect_err("custom template should be rejected");
    let message = format!("{error}");
    assert!(message.contains("built-in or the current session agent"));
}
