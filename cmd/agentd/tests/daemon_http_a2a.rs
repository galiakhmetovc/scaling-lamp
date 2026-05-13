use agent_persistence::{
    JobRecord, JobRepository, PersistenceStore, SessionInboxRepository, SessionRepository,
};
use agent_runtime::delegation::{DelegateResultPackage, DelegateWriteScope};
use agent_runtime::mission::{JobSpec, JobStatus};
use agentd::daemon;
use agentd::http::types::{
    A2ACallbackTargetRequest, A2ADelegationAcceptedResponse, A2ADelegationCompletionOutcomeRequest,
    A2ADelegationCompletionRequest, A2ADelegationCreateRequest,
};
use reqwest::StatusCode;
use reqwest::blocking::Client;

#[path = "daemon_http/support.rs"]
mod support;

use support::test_app;

#[test]
fn daemon_http_a2a_accepts_remote_delegation_and_creates_child_session_and_job() {
    let (_temp, app, base_url) = test_app(Some("secret-token"));
    let handle = daemon::spawn_for_test(app.clone()).expect("spawn daemon");
    let client = Client::new();

    let response = client
        .post(format!("{base_url}/v1/a2a/delegations"))
        .bearer_auth("secret-token")
        .json(&A2ADelegationCreateRequest {
            parent_session_id: "session-parent".to_string(),
            parent_job_id: "job-parent".to_string(),
            label: "judge".to_string(),
            goal: "Review the artifacts and return a verdict.".to_string(),
            bounded_context: vec!["reports/judge.md".to_string()],
            write_scope: DelegateWriteScope::new(vec!["reports".to_string()]).expect("write scope"),
            expected_output: "Short verdict".to_string(),
            owner: "a2a:judge".to_string(),
            callback: A2ACallbackTargetRequest {
                url: "https://daemon-a.example/v1/a2a/delegations/job-parent/complete".to_string(),
                bearer_token: Some("callback-token".to_string()),
            },
            now: 10,
        })
        .send()
        .expect("create a2a delegation");

    assert_eq!(response.status(), StatusCode::CREATED);
    let accepted: A2ADelegationAcceptedResponse = response.json().expect("accepted json");
    assert_eq!(accepted.remote_session_id, "session-a2a-job-parent");
    assert_eq!(accepted.remote_job_id, "job-a2a-job-parent");

    let store = PersistenceStore::open(&app.persistence).expect("open store");
    let session = store
        .get_session("session-a2a-job-parent")
        .expect("get remote session")
        .expect("remote session exists");
    assert_eq!(session.parent_session_id.as_deref(), Some("session-parent"));
    assert_eq!(session.parent_job_id.as_deref(), Some("job-parent"));

    let job = JobSpec::try_from(
        store
            .get_job("job-a2a-job-parent")
            .expect("get remote job")
            .expect("remote job exists"),
    )
    .expect("restore remote job");
    assert_eq!(job.status, JobStatus::Running);
    assert!(job.callback.is_some());

    handle.stop().expect("stop daemon");
}

#[test]
fn daemon_http_a2a_completion_callback_updates_parent_job_and_queues_inbox_event() {
    let (_temp, app, base_url) = test_app(Some("secret-token"));
    let session = app
        .create_session("session-parent", "Parent")
        .expect("create parent");
    let store = PersistenceStore::open(&app.persistence).expect("open store");
    let mut job = JobSpec::delegate(
        "job-parent",
        &session.id,
        None,
        None,
        "judge",
        "Review the artifacts and return a verdict.",
        vec!["reports/judge.md".to_string()],
        DelegateWriteScope::new(vec!["reports".to_string()]).expect("write scope"),
        "Short verdict",
        "a2a:judge",
        5,
    );
    job.status = JobStatus::WaitingExternal;
    store
        .put_job(&JobRecord::try_from(&job).expect("job record"))
        .expect("put parent job");

    let handle = daemon::spawn_for_test(app.clone()).expect("spawn daemon");
    let client = Client::new();
    let response = client
        .post(format!("{base_url}/v1/a2a/delegations/{}/complete", job.id))
        .bearer_auth("secret-token")
        .json(&A2ADelegationCompletionRequest {
            outcome: A2ADelegationCompletionOutcomeRequest::Completed {
                remote_session_id: "session-a2a-job-parent".to_string(),
                remote_job_id: "job-a2a-job-parent".to_string(),
                package: DelegateResultPackage::new(
                    "Judge complete",
                    Vec::new(),
                    vec!["artifact-1".to_string()],
                    Vec::new(),
                )
                .expect("package"),
            },
            now: 20,
        })
        .send()
        .expect("complete remote delegation");

    assert_eq!(response.status(), StatusCode::OK);

    let job = JobSpec::try_from(
        store
            .get_job("job-parent")
            .expect("get updated job")
            .expect("updated job exists"),
    )
    .expect("restore updated job");
    assert_eq!(job.status, JobStatus::Completed);

    let inbox = store
        .list_session_inbox_events_for_session(&session.id)
        .expect("list inbox");
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].kind, "delegation_result_ready");

    handle.stop().expect("stop daemon");
}
