use super::*;

#[test]
fn trace_repository_round_trips_links() {
    let temp = tempfile::tempdir().expect("tempdir");
    let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..crate::AppConfig::default()
    });

    let run_link = TraceLinkRecord {
        entity_kind: "run".to_string(),
        entity_id: "run-trace-1".to_string(),
        trace_id: "0123456789abcdef0123456789abcdef".to_string(),
        span_id: "0123456789abcdef".to_string(),
        parent_span_id: None,
        surface: Some("telegram".to_string()),
        entrypoint: Some("telegram.message".to_string()),
        attributes_json: serde_json::json!({
            "session_id": "session-trace-1",
            "run_id": "run-trace-1"
        })
        .to_string(),
        created_at: 100,
    };
    let tool_link = TraceLinkRecord {
        entity_kind: "tool_call".to_string(),
        entity_id: "tool-call-trace-1".to_string(),
        trace_id: run_link.trace_id.clone(),
        span_id: "fedcba9876543210".to_string(),
        parent_span_id: Some(run_link.span_id.clone()),
        surface: Some("telegram".to_string()),
        entrypoint: Some("telegram.message".to_string()),
        attributes_json: serde_json::json!({
            "run_id": "run-trace-1",
            "tool_name": "web_fetch",
            "status": "completed"
        })
        .to_string(),
        created_at: 101,
    };

    {
        let store = super::super::PersistenceStore::open(&scaffold).expect("open store");
        store.put_trace_link(&run_link).expect("put run trace");
        store.put_trace_link(&tool_link).expect("put tool trace");
    }

    let reopened = super::super::PersistenceStore::open(&scaffold).expect("reopen store");

    assert_eq!(
        reopened
            .get_trace_link("run", "run-trace-1")
            .expect("get run trace"),
        Some(run_link.clone())
    );
    assert_eq!(
        reopened
            .list_trace_links_for_trace(&run_link.trace_id)
            .expect("list trace links"),
        vec![run_link.clone(), tool_link.clone()]
    );
    assert_eq!(
        reopened
            .list_recent_trace_links(1)
            .expect("list recent trace links"),
        vec![tool_link]
    );
}
