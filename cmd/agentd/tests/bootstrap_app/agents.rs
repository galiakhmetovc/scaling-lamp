use super::support::*;
use agent_runtime::agent::AgentTemplateKind;
use agent_runtime::tool::{AgentCreateInput, AgentListInput, AgentReadInput};
use std::path::Path;

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
            workspace_root: fs::canonicalize(".")
                .expect("canonical workspace")
                .display()
                .to_string(),
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
        vec!["default"]
    );

    let current = app.current_agent_profile().expect("current agent");
    assert_eq!(current.id, "default");
    assert_eq!(current.name, "Ассистент");
    assert_eq!(current.template_kind, AgentTemplateKind::Default);
    let default_workspace = current
        .default_workspace_root
        .as_ref()
        .expect("default agent workspace");
    assert!(default_workspace.is_dir());
    assert!(default_workspace.ends_with(Path::new("workspaces/agents/default")));
    assert!(!default_workspace.starts_with(&data_dir));

    let profile = app.agent_profile("default").expect("agent profile");
    assert_eq!(profile.agent_home.as_path(), default_workspace.as_path());
    assert!(profile.agent_home.join("SYSTEM.md").is_file());
    assert!(profile.agent_home.join("AGENTS.md").is_file());
    assert!(profile.agent_home.join("skills").is_dir());
    let workspace = profile
        .default_workspace_root
        .as_ref()
        .expect("agent workspace");
    assert!(workspace.is_dir());
    assert!(workspace.ends_with(Path::new("workspaces/agents/default")));
    assert!(!workspace.starts_with(&data_dir));
    let default_system =
        fs::read_to_string(default_workspace.join("SYSTEM.md")).expect("read default system");
    assert!(default_system.contains("Self-learning"));
    assert!(default_system.contains("Do not rely on hidden memory"));
    assert!(default_system.contains("Keep the workspace clean"));
    assert!(
        default_workspace
            .join("skills/silverbullet-space/SKILL.md")
            .is_file()
    );
    assert!(
        default_workspace
            .join("skills/mem0-memory/SKILL.md")
            .is_file()
    );
    assert!(
        !default_workspace
            .join("skills/obsidian-vault/SKILL.md")
            .exists()
    );
    assert!(
        !default_workspace
            .join("skills/logseq-graph/SKILL.md")
            .exists()
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

    let created = app
        .create_agent_from_template("Worker", None)
        .expect("create worker profile");
    let selected = app
        .select_agent_profile("worker")
        .expect("select worker profile");
    assert_eq!(selected.id, created.id);

    let session = app
        .create_session_auto(Some("Judge Session"))
        .expect("create session");
    let store = PersistenceStore::open(&app.persistence).expect("open store");
    let stored = store
        .get_session(&session.id)
        .expect("get session")
        .expect("session exists");

    assert_eq!(stored.agent_profile_id, "worker");
}

#[test]
fn create_session_prefers_agent_default_workspace_over_global_default() {
    let temp = tempfile::tempdir().expect("tempdir");
    let global_workspace = temp.path().join("global-workspace");
    fs::create_dir_all(&global_workspace).expect("create global workspace");

    let mut config = AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    };
    config.workspace.default_root = Some(global_workspace.clone());
    let app = build_from_config(config).expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    let worker = app
        .create_agent_from_template("Worker", None)
        .expect("create worker");

    app.select_agent_profile("worker")
        .expect("select worker profile");

    let session = app
        .create_session_auto(Some("Worker Workspace Session"))
        .expect("create session");
    let stored = store
        .get_session(&session.id)
        .expect("get session")
        .expect("session exists");

    assert_eq!(stored.agent_profile_id, "worker");
    assert_eq!(
        stored.workspace_root,
        worker.agent_home.display().to_string()
    );
    assert_ne!(
        stored.workspace_root,
        global_workspace.display().to_string()
    );
}

#[test]
fn create_agent_from_template_copies_template_files_independently() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");

    let default_workspace = app.agent_home_path("default").expect("default workspace");
    let default_agents_before =
        fs::read_to_string(default_workspace.join("AGENTS.md")).expect("read default agents");
    let default_system_before =
        fs::read_to_string(default_workspace.join("SYSTEM.md")).expect("read default system");

    let created = app
        .create_agent_from_template("Worker Copy", None)
        .expect("create agent from default");
    assert_eq!(created.template_kind, AgentTemplateKind::Custom);
    assert_ne!(created.agent_home, default_workspace);
    let created_workspace = created
        .default_workspace_root
        .as_ref()
        .expect("created agent workspace");
    assert!(created_workspace.is_dir());
    assert!(created_workspace.ends_with(Path::new("workspaces/agents/worker-copy")));
    assert_ne!(
        created.default_workspace_root,
        app.agent_profile("default")
            .expect("default profile")
            .default_workspace_root
    );

    assert_eq!(
        fs::read_to_string(created.agent_home.join("SYSTEM.md")).expect("read copied system"),
        default_system_before
    );
    assert_eq!(
        fs::read_to_string(created.agent_home.join("AGENTS.md")).expect("read copied agents"),
        default_agents_before
    );

    fs::write(created.agent_home.join("AGENTS.md"), "customized copy").expect("mutate copy");
    assert_eq!(
        fs::read_to_string(default_workspace.join("AGENTS.md")).expect("re-read default agents"),
        default_agents_before
    );

    app.select_agent_profile("Worker Copy")
        .expect("select copied agent");
    let session = app
        .create_session_auto(Some("Copied Agent Workspace"))
        .expect("create copied agent session");
    let store = PersistenceStore::open(&app.persistence).expect("open store");
    let stored_session = store
        .get_session(&session.id)
        .expect("get session")
        .expect("session exists");
    assert_eq!(
        stored_session.workspace_root,
        created_workspace.display().to_string()
    );
    let stored = store
        .get_agent_profile(&created.id)
        .expect("get created agent profile")
        .expect("created profile exists");
    assert_eq!(stored.template_kind, "custom");
}

#[test]
fn build_from_config_seeds_current_default_prompts_and_preserves_custom_edits() {
    let temp = tempfile::tempdir().expect("tempdir");
    let data_dir = temp.path().join("state-root");
    let app = build_from_config(AppConfig {
        data_dir: data_dir.clone(),
        ..AppConfig::default()
    })
    .expect("build app");

    let default_home = app.agent_home_path("default").expect("default home");
    let refreshed_system =
        fs::read_to_string(default_home.join("SYSTEM.md")).expect("read seeded system");
    let refreshed_agents =
        fs::read_to_string(default_home.join("AGENTS.md")).expect("read seeded agents");
    assert!(refreshed_system.contains("general-purpose autonomous agent running inside teamD"));
    assert!(refreshed_system.contains("Self-learning"));
    assert!(refreshed_system.contains("Do not rely on hidden memory"));
    assert!(refreshed_system.contains("Keep the workspace clean"));
    assert!(refreshed_agents.contains("Assistant agent profile."));
    assert!(refreshed_agents.contains("Never invent tool names"));
    assert!(refreshed_agents.contains("Use a dedicated scratch path"));
    assert!(refreshed_agents.contains("Record reusable lessons"));
    assert!(refreshed_agents.contains("exec_read_output"));
    assert!(refreshed_agents.contains("knowledge_search"));
    assert!(refreshed_agents.contains("session_search"));
    assert!(refreshed_agents.contains("use `continue_later` with `delay_seconds`"));
    assert!(refreshed_agents.contains("set `delivery_mode` to `existing_session`"));
    assert!(refreshed_agents.contains("Arguments must be strict JSON"));
    assert!(refreshed_agents.contains("do not invent `old`/`new` patch fields"));
    assert!(refreshed_agents.contains("Use `skill_list`"));
    assert!(refreshed_agents.contains("Use `skill_enable` or `skill_disable`"));
    assert!(refreshed_agents.contains("Use `autonomy_state_read`"));
    assert!(refreshed_agents.contains("Use `web_search` first"));
    assert!(refreshed_agents.contains("scope `next_turn`"));
    assert!(refreshed_agents.contains("SilverBullet Space"));
    assert!(refreshed_agents.contains("/var/lib/teamd/knowledge/silverbullet/teamd"));
    let silverbullet_skill =
        fs::read_to_string(default_home.join("skills/silverbullet-space/SKILL.md"))
            .expect("read silverbullet skill");
    assert!(silverbullet_skill.contains("name: silverbullet-space"));
    assert!(silverbullet_skill.contains("/var/lib/teamd/knowledge/silverbullet/teamd"));
    assert!(silverbullet_skill.contains("## Current structure"));
    assert!(silverbullet_skill.contains("Archive.md"));
    assert!(silverbullet_skill.contains("Space Lua / Lua Integrated Query"));
    assert!(silverbullet_skill.contains("short Mem0 pointer memory"));
    let current_stack_skills = [
        ("mem0-memory", "memory_search"),
        ("scoped-kv", "kv_get"),
        ("telegram-operator-workflow", "/status"),
        ("browser-search", "web_search"),
        ("file-artifact-workflow", "deliver_file"),
        ("planning-session-lifecycle", "continue_later"),
        ("agent-browser", "browser_open"),
    ];
    for (skill_name, expected_fragment) in current_stack_skills {
        let skill = fs::read_to_string(default_home.join(format!("skills/{skill_name}/SKILL.md")))
            .unwrap_or_else(|source| panic!("read {skill_name} skill: {source}"));
        assert!(skill.contains(&format!("name: {skill_name}")));
        assert!(skill.contains(expected_fragment));
    }

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
        .create_agent_from_template("Seed Agent", None)
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
    assert!(message.contains("agent_create supports only the default template"));
}
