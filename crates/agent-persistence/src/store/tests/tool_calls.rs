use super::*;

#[test]
fn tool_call_repository_round_trips_session_and_run_ledgers() {
    let temp = tempfile::tempdir().expect("tempdir");
    let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..crate::AppConfig::default()
    });
    let store = super::super::PersistenceStore::open(&scaffold).expect("open store");

    let session = SessionRecord {
        id: "session-1".to_string(),
        title: "Tool ledger".to_string(),
        prompt_override: None,
        settings_json: "{}".to_string(),
        workspace_root: "/workspace/test".to_string(),
        agent_profile_id: "default".to_string(),
        active_mission_id: None,
        parent_session_id: None,
        parent_job_id: None,
        delegation_label: None,
        created_at: 1,
        updated_at: 1,
    };
    let run = RunRecord {
        id: "run-1".to_string(),
        session_id: session.id.clone(),
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
    };
    store.put_session(&session).expect("put session");
    store.put_run(&run).expect("put run");

    let first = ToolCallRecord {
        id: "tool-call-1".to_string(),
        session_id: session.id.clone(),
        run_id: run.id.clone(),
        provider_tool_call_id: "provider-call-1".to_string(),
        tool_name: "fs_read_text".to_string(),
        arguments_json: "{\"path\":\"README.md\"}".to_string(),
        summary: "fs_read_text path=README.md".to_string(),
        status: "requested".to_string(),
        error: None,
        result_summary: None,
        result_preview: None,
        result_artifact_id: None,
        result_truncated: false,
        result_byte_len: None,
        requested_at: 10,
        updated_at: 10,
    };
    let second = ToolCallRecord {
        id: "tool-call-2".to_string(),
        provider_tool_call_id: "provider-call-2".to_string(),
        tool_name: "exec_start".to_string(),
        arguments_json: "{\"cmd\":\"cargo test\"}".to_string(),
        summary: "exec_start cmd=cargo test".to_string(),
        requested_at: 9,
        updated_at: 9,
        ..first.clone()
    };

    store.put_tool_call(&first).expect("put first tool call");
    store.put_tool_call(&second).expect("put second tool call");

    assert_eq!(
        store
            .list_tool_calls_for_session(&session.id)
            .expect("list session tool calls")
            .into_iter()
            .map(|call| call.id)
            .collect::<Vec<_>>(),
        vec!["tool-call-2", "tool-call-1"]
    );

    let completed = ToolCallRecord {
        status: "completed".to_string(),
        result_summary: Some("read README.md".to_string()),
        result_preview: Some("{\"content\":\"hello\"}".to_string()),
        result_artifact_id: Some("artifact-tool-result-1".to_string()),
        result_truncated: true,
        result_byte_len: Some(1234),
        updated_at: 12,
        ..first.clone()
    };
    store
        .put_tool_call(&completed)
        .expect("update first tool call");

    assert_eq!(
        store
            .get_tool_call("tool-call-1")
            .expect("get updated call"),
        Some(completed.clone())
    );
    assert_eq!(
        store
            .list_tool_calls_for_run(&run.id)
            .expect("list run tool calls")
            .len(),
        2
    );
}
