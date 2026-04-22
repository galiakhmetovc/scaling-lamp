use super::support::*;
use agent_runtime::interagent::{AgentChainState, AgentMessageChain, DEFAULT_MAX_HOPS};
use agent_runtime::session::TranscriptEntry;
use agent_runtime::tool::{GrantAgentChainContinuationInput, MessageAgentInput};

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
            objective: "interagent".to_string(),
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
fn message_agent_direct_tool_execution_queues_recipient_session_and_job() {
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
        "session-interagent-origin",
        "default",
        "mission-interagent-origin",
        "job-interagent-origin",
        "run-interagent-origin",
    );

    let report = app
        .request_tool_approval(
            "job-interagent-origin",
            "run-interagent-origin",
            &ToolCall::MessageAgent(MessageAgentInput {
                target_agent_id: "judge".to_string(),
                message: "Проверь последний результат.".to_string(),
            }),
            20,
        )
        .expect("message_agent should queue");

    assert_eq!(report.run_status, RunStatus::Completed);
    let sessions = store.list_sessions().expect("list sessions");
    let recipient = sessions
        .into_iter()
        .find(|record| record.id != "session-interagent-origin")
        .expect("recipient session");
    assert_eq!(recipient.agent_profile_id, "judge");
    assert_eq!(
        recipient.parent_session_id.as_deref(),
        Some("session-interagent-origin")
    );

    let jobs = store.list_jobs().expect("list jobs");
    let recipient_job = jobs
        .into_iter()
        .find(|record| record.id != "job-interagent-origin")
        .expect("recipient job");
    assert_eq!(recipient_job.kind, "interagent_message");
    assert_eq!(recipient_job.status, "running");
    assert!(
        report
            .output_summary
            .as_deref()
            .unwrap_or_default()
            .contains("message_agent")
    );
}

#[test]
fn background_worker_routes_interagent_reply_back_to_origin_session() {
    let (api_base, requests, handle) = spawn_json_server_sequence(vec![
        r#"{
                "id":"resp_agent_judge",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_agent_judge",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Критических замечаний нет."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":18,"output_tokens":8,"total_tokens":26}
            }"#
        .to_string(),
        r#"{
                "id":"resp_origin_wakeup",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_origin_wakeup",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Я получил ответ судьи и продолжаю."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":16,"output_tokens":8,"total_tokens":24}
            }"#
        .to_string(),
    ]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        daemon: Default::default(),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(format!("{api_base}/v1")),
            api_key: Some("test-key".to_string()),
            default_model: Some("gpt-5.4".to_string()),
            ..ConfiguredProvider::default()
        },
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
        "session-agent-origin",
        "default",
        "mission-agent-origin",
        "job-agent-origin",
        "run-agent-origin",
    );

    app.request_tool_approval(
        "job-agent-origin",
        "run-agent-origin",
        &ToolCall::MessageAgent(MessageAgentInput {
            target_agent_id: "judge".to_string(),
            message: "Дай короткий вердикт.".to_string(),
        }),
        20,
    )
    .expect("queue agent message");

    let report = app
        .background_worker_tick(40)
        .expect("background worker tick");
    let child_request = requests.recv().expect("child request");
    let wake_request = requests.recv().expect("wake request");
    handle.join().expect("join server");

    assert_eq!(report.executed_jobs, 1);
    assert_eq!(report.emitted_inbox_events, 1);
    assert_eq!(report.woken_sessions, 1);

    let transcripts = store
        .list_transcripts_for_session("session-agent-origin")
        .expect("list origin transcripts");
    assert!(
        transcripts
            .iter()
            .any(|record| record.kind == "user" && record.content.contains("[agent:Judge]"))
    );
    assert!(transcripts.iter().any(|record| record.kind == "assistant"
        && record.content == "Я получил ответ судьи и продолжаю."));

    let inbox = store
        .list_session_inbox_events_for_session("session-agent-origin")
        .expect("list inbox");
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].kind, "external_input_received");
    assert_eq!(inbox[0].status, "processed");

    assert!(child_request.contains("[agent:Ассистент]"));
    let wake_request = wake_request.to_ascii_lowercase();
    assert!(wake_request.contains("[agent:judge]"));
}

#[test]
fn judge_continuation_grant_allows_exactly_one_extra_hop() {
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
        "session-judge-chain",
        "judge",
        "mission-judge-chain",
        "job-judge-chain",
        "run-judge-chain",
    );

    let blocked_chain = AgentMessageChain::new(
        "chain-judge",
        "session-origin",
        "default",
        DEFAULT_MAX_HOPS,
        DEFAULT_MAX_HOPS,
        Some("session-parent".to_string()),
        AgentChainState::BlockedMaxHops,
    )
    .expect("blocked chain");
    store
        .put_transcript(&agent_persistence::TranscriptRecord::from(
            &TranscriptEntry::system(
                "transcript-judge-chain",
                "session-judge-chain",
                None,
                blocked_chain.to_transcript_metadata(),
                10,
            ),
        ))
        .expect("put chain transcript");

    let blocked = app
        .request_tool_approval(
            "job-judge-chain",
            "run-judge-chain",
            &ToolCall::MessageAgent(MessageAgentInput {
                target_agent_id: "default".to_string(),
                message: "Продолжай разбор.".to_string(),
            }),
            20,
        )
        .expect_err("blocked chain without grant");
    assert!(
        blocked
            .to_string()
            .contains("inter-agent chain chain-judge is blocked")
    );

    seed_running_tool_context(
        &store,
        "session-judge-chain",
        "judge",
        "mission-judge-chain",
        "job-judge-grant",
        "run-judge-grant",
    );
    app.request_tool_approval(
        "job-judge-grant",
        "run-judge-grant",
        &ToolCall::GrantAgentChainContinuation(GrantAgentChainContinuationInput {
            chain_id: "chain-judge".to_string(),
            reason: "Нужен ещё один hop.".to_string(),
        }),
        21,
    )
    .expect("grant continuation");
    assert!(
        store
            .get_agent_chain_continuation("chain-judge")
            .expect("get grant")
            .is_some()
    );

    seed_running_tool_context(
        &store,
        "session-judge-chain",
        "judge",
        "mission-judge-chain",
        "job-judge-continue",
        "run-judge-continue",
    );
    app.request_tool_approval(
        "job-judge-continue",
        "run-judge-continue",
        &ToolCall::MessageAgent(MessageAgentInput {
            target_agent_id: "default".to_string(),
            message: "Продолжай разбор.".to_string(),
        }),
        22,
    )
    .expect("grant should allow one hop");
    assert!(
        store
            .get_agent_chain_continuation("chain-judge")
            .expect("get grant")
            .is_none()
    );

    let sessions = store.list_sessions().expect("list sessions");
    let recipient = sessions
        .into_iter()
        .find(|record| record.id != "session-judge-chain")
        .expect("continued recipient");
    let chain_transcript = store
        .list_transcripts_for_session(&recipient.id)
        .expect("list recipient transcripts")
        .into_iter()
        .find(|record| record.content.starts_with("interagent_chain:"))
        .expect("chain transcript");
    let continued_chain = AgentMessageChain::from_transcript_metadata(&chain_transcript.content)
        .expect("parse continued chain");
    assert_eq!(continued_chain.hop_count, DEFAULT_MAX_HOPS + 1);
    assert_eq!(continued_chain.state, AgentChainState::ContinuedOnce);
}

#[test]
fn build_from_config_keeps_active_interagent_job_runs_recoverable_across_restart() {
    let temp = tempfile::tempdir().expect("tempdir");
    let data_dir = temp.path().join("state-root");
    let app = build_from_config(AppConfig {
        data_dir: data_dir.clone(),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    for (session_id, title, agent_profile_id) in [
        ("session-origin-restart", "Origin", "default"),
        ("session-recipient-restart", "Agent: Judge", "judge"),
    ] {
        store
            .put_session(&SessionRecord {
                id: session_id.to_string(),
                title: title.to_string(),
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
    }

    let mut run = RunEngine::new(
        "run-interagent-restart",
        "session-recipient-restart",
        None,
        3,
    );
    run.start(3).expect("start run");
    store
        .put_run(&RunRecord::try_from(run.snapshot()).expect("run record"))
        .expect("put run");

    let mut job = JobSpec::interagent_message(
        "job-interagent-restart",
        "session-recipient-restart",
        Some("run-interagent-restart"),
        None,
        "session-origin-restart",
        "default",
        "Default",
        "judge",
        "Judge",
        "Проверь план.",
        AgentMessageChain::root(
            "chain-interagent-restart",
            "session-origin-restart",
            "default",
        )
        .expect("root chain"),
        2,
    );
    job.status = agent_runtime::mission::JobStatus::Running;
    job.started_at = Some(3);
    job.updated_at = 3;
    job.lease_owner = Some("daemon".to_string());
    job.lease_expires_at = Some(300);
    store
        .put_job(&JobRecord::try_from(&job).expect("job record"))
        .expect("put job");

    drop(store);
    drop(app);

    let reopened = build_from_config(AppConfig {
        data_dir,
        ..AppConfig::default()
    })
    .expect("reopen app");
    let reopened_store = PersistenceStore::open(&reopened.persistence).expect("reopen store");

    let restored = RunSnapshot::try_from(
        reopened_store
            .get_run("run-interagent-restart")
            .expect("get run")
            .expect("run exists"),
    )
    .expect("restore run");
    assert_eq!(restored.status, RunStatus::Running);
    assert!(restored.error.is_none());
}
