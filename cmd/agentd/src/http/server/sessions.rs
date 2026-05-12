use super::*;
use crate::http::types::{
    ClearSessionRequest, CreateSessionRequest, DebugBundleResponse, ErrorResponse,
    MemoryRenderResponse, SessionAgentMessageRequest, SessionArtifactFileResponse,
    SessionArtifactFileSummaryResponse, SessionArtifactFilesResponse, SessionArtifactResponse,
    SessionArtifactsResponse, SessionBackgroundJobResponse, SessionBackgroundJobsResponse,
    SessionChainGrantRequest, SessionDebugResponse, SessionDetailResponse,
    SessionPendingApprovalsResponse, SessionPreferencesRequest, SessionRunControlResponse,
    SessionRunStatusResponse, SessionSkillsResponse, SessionSummaryResponse, SessionSystemResponse,
    SessionTaskResponse, SessionTasksResponse, SessionTranscriptResponse,
    SessionWorkspaceEntryResponse, SessionWorkspaceFileResponse, SessionWorkspaceListResponse,
    SkillCommandRequest, TaskControlResponse, TaskRenderResponse,
};
use agent_persistence::{AgentRepository, ArtifactRepository, SessionRepository};
use agent_runtime::tool::{
    KnowledgeReadInput, KnowledgeSearchInput, SessionReadInput, SessionSearchInput,
};
use agent_runtime::workspace::{WorkspaceEntryKind, WorkspaceRef};
use std::collections::BTreeMap;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};
use tiny_http::Method;

const WEB_WORKSPACE_LIST_DEFAULT_LIMIT: usize = 100;
const WEB_WORKSPACE_LIST_MAX_LIMIT: usize = 500;
const WEB_FILE_READ_DEFAULT_MAX_BYTES: usize = 256 * 1024;
const WEB_FILE_READ_MAX_BYTES: usize = 2 * 1024 * 1024;

pub(super) fn handle_create_session(app: &App, mut request: Request) -> std::io::Result<()> {
    let mut body = String::new();
    request.as_reader().read_to_string(&mut body)?;
    let payload = if body.trim().is_empty() {
        CreateSessionRequest {
            id: None,
            title: None,
            agent_identifier: None,
        }
    } else {
        match serde_json::from_str::<CreateSessionRequest>(&body) {
            Ok(payload) => payload,
            Err(error) => {
                return respond_json(
                    request,
                    StatusCode(400),
                    &ErrorResponse {
                        error: format!("invalid session request: {error}"),
                    },
                );
            }
        }
    };

    let session_result = match payload.id.as_deref() {
        Some(id) => app.create_session_for_agent(
            id,
            payload.title.as_deref().unwrap_or("New Session"),
            payload.agent_identifier.as_deref(),
        ),
        None => app.create_session_auto_for_agent(
            payload.title.as_deref(),
            payload.agent_identifier.as_deref(),
        ),
    };
    let session = match session_result {
        Ok(session) => session,
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            return respond_json(request, status, &payload);
        }
    };
    respond_json(
        request,
        StatusCode(201),
        &SessionSummaryResponse::from(session),
    )
}

pub(super) fn handle_list_sessions(app: &App, request: Request) -> std::io::Result<()> {
    let limit = query_usize(&request, "limit").map(|value| value.clamp(1, 100));
    let offset = query_usize(&request, "offset").unwrap_or(0);
    let sessions_result = match limit {
        Some(limit) => app.list_session_summaries_page(limit, offset),
        None => app.list_session_summaries(),
    };
    let sessions = match sessions_result {
        Ok(sessions) => sessions
            .into_iter()
            .map(SessionSummaryResponse::from)
            .collect::<Vec<_>>(),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            return respond_json(request, status, &payload);
        }
    };
    respond_json(request, StatusCode(200), &sessions)
}

fn query_usize(request: &Request, name: &str) -> Option<usize> {
    let query = request.url().split_once('?')?.1;
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        if key == name {
            value.parse::<usize>().ok()
        } else {
            None
        }
    })
}

pub(super) fn handle_memory_session_search(app: &App, mut request: Request) -> std::io::Result<()> {
    match parse_json_body::<SessionSearchInput>(&mut request) {
        Ok(input) => match app.render_session_memory_search(input) {
            Ok(memory) => respond_json(request, StatusCode(200), &MemoryRenderResponse { memory }),
            Err(error) => {
                let (status, payload) = map_bootstrap_error(error);
                respond_json(request, status, &payload)
            }
        },
        Err(error) => respond_json(
            request,
            StatusCode(400),
            &ErrorResponse {
                error: format!("invalid session memory search request: {error}"),
            },
        ),
    }
}

pub(super) fn handle_memory_session_read(app: &App, mut request: Request) -> std::io::Result<()> {
    match parse_json_body::<SessionReadInput>(&mut request) {
        Ok(input) => match app.render_session_memory_read(input) {
            Ok(memory) => respond_json(request, StatusCode(200), &MemoryRenderResponse { memory }),
            Err(error) => {
                let (status, payload) = map_bootstrap_error(error);
                respond_json(request, status, &payload)
            }
        },
        Err(error) => respond_json(
            request,
            StatusCode(400),
            &ErrorResponse {
                error: format!("invalid session memory read request: {error}"),
            },
        ),
    }
}

pub(super) fn handle_memory_knowledge_search(
    app: &App,
    mut request: Request,
) -> std::io::Result<()> {
    match parse_json_body::<KnowledgeSearchInput>(&mut request) {
        Ok(input) => match app.render_knowledge_search(input) {
            Ok(memory) => respond_json(request, StatusCode(200), &MemoryRenderResponse { memory }),
            Err(error) => {
                let (status, payload) = map_bootstrap_error(error);
                respond_json(request, status, &payload)
            }
        },
        Err(error) => respond_json(
            request,
            StatusCode(400),
            &ErrorResponse {
                error: format!("invalid knowledge memory search request: {error}"),
            },
        ),
    }
}

pub(super) fn handle_memory_knowledge_read(app: &App, mut request: Request) -> std::io::Result<()> {
    match parse_json_body::<KnowledgeReadInput>(&mut request) {
        Ok(input) => match app.render_knowledge_read(input) {
            Ok(memory) => respond_json(request, StatusCode(200), &MemoryRenderResponse { memory }),
            Err(error) => {
                let (status, payload) = map_bootstrap_error(error);
                respond_json(request, status, &payload)
            }
        },
        Err(error) => respond_json(
            request,
            StatusCode(400),
            &ErrorResponse {
                error: format!("invalid knowledge memory read request: {error}"),
            },
        ),
    }
}

pub(super) fn handle_nested_routes(app: &App, request: Request) -> std::io::Result<()> {
    let path = request
        .url()
        .split('?')
        .next()
        .unwrap_or_default()
        .to_string();
    let method = request.method().clone();
    let Some(session_tail) = path.strip_prefix("/v1/sessions/") else {
        return respond_json(
            request,
            StatusCode(404),
            &ErrorResponse {
                error: "route not found".to_string(),
            },
        );
    };
    let segments = session_tail
        .split('/')
        .map(str::to_string)
        .collect::<Vec<_>>();
    match (method, segments.as_slice()) {
        (Method::Get, [session_id]) => handle_session_summary(app, request, session_id.as_str()),
        (Method::Get, [session_id, detail]) if detail == "detail" => {
            handle_session_detail(app, request, session_id.as_str())
        }
        (Method::Delete, [session_id]) => handle_delete_session(app, request, session_id.as_str()),
        (Method::Get, [session_id, transcript]) if transcript == "transcript" => {
            handle_session_transcript(app, request, session_id.as_str())
        }
        (Method::Get, [session_id, debug]) if debug == "debug" => {
            handle_session_debug(app, request, session_id.as_str())
        }
        (Method::Get, [session_id, transcript_tail, limit])
            if transcript_tail == "transcript-tail" =>
        {
            handle_session_transcript_tail(app, request, session_id.as_str(), limit.as_str())
        }
        (Method::Get, [session_id, approvals]) if approvals == "approvals" => {
            handle_pending_approvals(app, request, session_id.as_str())
        }
        (Method::Get, [session_id, jobs]) if jobs == "jobs" => {
            handle_session_background_jobs(app, request, session_id.as_str())
        }
        (Method::Get, [session_id, tasks]) if tasks == "tasks" => {
            handle_session_tasks(app, request, session_id.as_str())
        }
        (Method::Get, [session_id, skills]) if skills == "skills" => {
            handle_session_skills(app, request, session_id.as_str())
        }
        (Method::Post, [session_id, skills, action])
            if skills == "skills" && action == "enable" =>
        {
            handle_enable_session_skill(app, request, session_id.as_str())
        }
        (Method::Post, [session_id, skills, action])
            if skills == "skills" && action == "disable" =>
        {
            handle_disable_session_skill(app, request, session_id.as_str())
        }
        (Method::Patch, [session_id, preferences]) if preferences == "preferences" => {
            handle_update_preferences(app, request, session_id.as_str())
        }
        (Method::Post, [session_id, clear]) if clear == "clear" => {
            handle_clear_session(app, request, session_id.as_str())
        }
        (Method::Post, [session_id, compact]) if compact == "compact" => {
            handle_compact_session(app, request, session_id.as_str())
        }
        (Method::Post, [session_id, debug]) if debug == "debug-bundle" => {
            handle_write_debug_bundle(app, request, session_id.as_str())
        }
        (Method::Get, [session_id, system]) if system == "system" => {
            handle_render_system(app, request, session_id.as_str())
        }
        (Method::Get, [session_id, context]) if context == "context" => {
            handle_render_context(app, request, session_id.as_str())
        }
        (Method::Get, [session_id, artifacts]) if artifacts == "artifacts" => {
            handle_render_artifacts(app, request, session_id.as_str())
        }
        (Method::Get, [session_id, artifacts, artifact_id]) if artifacts == "artifacts" => {
            handle_read_artifact(app, request, session_id.as_str(), artifact_id.as_str())
        }
        (Method::Get, [session_id, artifact_files]) if artifact_files == "artifact-files" => {
            handle_list_artifact_files(app, request, session_id.as_str())
        }
        (Method::Get, [session_id, artifact_files, artifact_id])
            if artifact_files == "artifact-files" =>
        {
            handle_read_artifact_file(app, request, session_id.as_str(), artifact_id.as_str())
        }
        (Method::Get, [session_id, artifact_files, artifact_id, download])
            if artifact_files == "artifact-files" && download == "download" =>
        {
            handle_download_artifact_file(app, request, session_id.as_str(), artifact_id.as_str())
        }
        (Method::Get, [session_id, workspace, action])
            if workspace == "workspace" && action == "list" =>
        {
            handle_workspace_list(app, request, session_id.as_str())
        }
        (Method::Get, [session_id, workspace, action])
            if workspace == "workspace" && action == "read" =>
        {
            handle_workspace_read(app, request, session_id.as_str())
        }
        (Method::Get, [session_id, workspace, action])
            if workspace == "workspace" && action == "download" =>
        {
            handle_workspace_download(app, request, session_id.as_str())
        }
        (Method::Get, [session_id, plan]) if plan == "plan" => {
            handle_render_plan(app, request, session_id.as_str())
        }
        (Method::Get, [session_id, run]) if run == "run" => {
            handle_render_active_run(app, request, session_id.as_str())
        }
        (Method::Post, [session_id, cancel_run]) if cancel_run == "cancel-run" => {
            handle_cancel_active_run(app, request, session_id.as_str())
        }
        (Method::Post, [session_id, cancel_all]) if cancel_all == "cancel-all-work" => {
            handle_cancel_all_session_work(app, request, session_id.as_str())
        }
        (Method::Post, [session_id, agent_message]) if agent_message == "agent-message" => {
            handle_send_agent_message(app, request, session_id.as_str())
        }
        (Method::Post, [session_id, chain_grant]) if chain_grant == "chain-grant" => {
            handle_grant_chain_continuation(app, request, session_id.as_str())
        }
        _ => respond_json(
            request,
            StatusCode(404),
            &ErrorResponse {
                error: "route not found".to_string(),
            },
        ),
    }
}

pub(super) fn handle_task_routes(app: &App, request: Request) -> std::io::Result<()> {
    let path = request
        .url()
        .split('?')
        .next()
        .unwrap_or_default()
        .to_string();
    let method = request.method().clone();
    let Some(task_tail) = path.strip_prefix("/v1/tasks/") else {
        return respond_json(
            request,
            StatusCode(404),
            &ErrorResponse {
                error: "route not found".to_string(),
            },
        );
    };
    let segments = task_tail.split('/').map(str::to_string).collect::<Vec<_>>();
    match (method, segments.as_slice()) {
        (Method::Get, [task_id]) => handle_task(app, request, task_id.as_str()),
        (Method::Post, [task_id, action]) if action == "cancel" => {
            handle_cancel_task(app, request, task_id.as_str())
        }
        _ => respond_json(
            request,
            StatusCode(404),
            &ErrorResponse {
                error: "route not found".to_string(),
            },
        ),
    }
}

fn handle_session_detail(app: &App, request: Request, session_id: &str) -> std::io::Result<()> {
    let store = match app.store() {
        Ok(store) => store,
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            return respond_json(request, status, &payload);
        }
    };

    match store.get_session(session_id) {
        Ok(Some(record)) => {
            let agent_name = match store.get_agent_profile(&record.agent_profile_id) {
                Ok(Some(agent)) => agent.name,
                Ok(None) | Err(_) => record.agent_profile_id.clone(),
            };
            respond_json(
                request,
                StatusCode(200),
                &SessionDetailResponse {
                    id: record.id,
                    title: record.title,
                    agent_profile_id: record.agent_profile_id,
                    agent_name,
                    workspace_root: record.workspace_root,
                    prompt_override: record.prompt_override,
                    settings_json: record.settings_json,
                    active_mission_id: record.active_mission_id,
                    parent_session_id: record.parent_session_id,
                    parent_job_id: record.parent_job_id,
                    delegation_label: record.delegation_label,
                    created_at: record.created_at,
                    updated_at: record.updated_at,
                },
            )
        }
        Ok(None) => respond_json(
            request,
            StatusCode(404),
            &ErrorResponse {
                error: format!("session {session_id} not found"),
            },
        ),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(BootstrapError::Store(error));
            respond_json(request, status, &payload)
        }
    }
}

fn handle_session_summary(app: &App, request: Request, session_id: &str) -> std::io::Result<()> {
    match app.session_summary(session_id) {
        Ok(summary) => respond_json(
            request,
            StatusCode(200),
            &SessionSummaryResponse::from(summary),
        ),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn handle_session_transcript(app: &App, request: Request, session_id: &str) -> std::io::Result<()> {
    match app.session_transcript(session_id) {
        Ok(transcript) => {
            respond_json::<SessionTranscriptResponse>(request, StatusCode(200), &transcript)
        }
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn handle_session_debug(app: &App, request: Request, session_id: &str) -> std::io::Result<()> {
    match app.session_debug_view(session_id) {
        Ok(debug) => respond_json::<SessionDebugResponse>(request, StatusCode(200), &debug),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn handle_session_transcript_tail(
    app: &App,
    request: Request,
    session_id: &str,
    limit: &str,
) -> std::io::Result<()> {
    let max_entries = match limit.parse::<usize>() {
        Ok(limit) => limit,
        Err(error) => {
            let (status, payload) = map_bootstrap_error(BootstrapError::Usage {
                reason: format!("invalid transcript tail limit {limit}: {error}"),
            });
            return respond_json(request, status, &payload);
        }
    };
    match app.session_transcript_tail(session_id, max_entries) {
        Ok(transcript) => {
            respond_json::<SessionTranscriptResponse>(request, StatusCode(200), &transcript)
        }
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn handle_pending_approvals(app: &App, request: Request, session_id: &str) -> std::io::Result<()> {
    match app.pending_approvals(session_id) {
        Ok(approvals) => {
            respond_json::<SessionPendingApprovalsResponse>(request, StatusCode(200), &approvals)
        }
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn handle_session_background_jobs(
    app: &App,
    request: Request,
    session_id: &str,
) -> std::io::Result<()> {
    match app.session_background_jobs(session_id) {
        Ok(jobs) => {
            let response = jobs
                .into_iter()
                .map(SessionBackgroundJobResponse::from)
                .collect::<Vec<_>>();
            respond_json::<SessionBackgroundJobsResponse>(request, StatusCode(200), &response)
        }
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn handle_session_tasks(app: &App, request: Request, session_id: &str) -> std::io::Result<()> {
    match app.session_tasks(session_id) {
        Ok(tasks) => {
            let response = tasks
                .into_iter()
                .map(SessionTaskResponse::from)
                .collect::<Vec<_>>();
            respond_json::<SessionTasksResponse>(request, StatusCode(200), &response)
        }
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn handle_task(app: &App, request: Request, task_id: &str) -> std::io::Result<()> {
    match app.render_task(task_id) {
        Ok(task) => respond_json(request, StatusCode(200), &TaskRenderResponse { task }),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn handle_cancel_task(app: &App, request: Request, task_id: &str) -> std::io::Result<()> {
    match app.cancel_task(task_id) {
        Ok(message) => respond_json(request, StatusCode(200), &TaskControlResponse { message }),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn handle_write_debug_bundle(app: &App, request: Request, session_id: &str) -> std::io::Result<()> {
    match app.write_debug_bundle(session_id) {
        Ok(path) => respond_json(
            request,
            StatusCode(200),
            &DebugBundleResponse {
                path: path.display().to_string(),
            },
        ),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn handle_render_context(app: &App, request: Request, session_id: &str) -> std::io::Result<()> {
    match app.render_context_state(session_id) {
        Ok(context) => respond_json(
            request,
            StatusCode(200),
            &serde_json::json!({ "context": context }),
        ),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn handle_render_system(app: &App, request: Request, session_id: &str) -> std::io::Result<()> {
    match app.render_system_blocks(session_id) {
        Ok(system) => respond_json(request, StatusCode(200), &SessionSystemResponse { system }),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn handle_render_artifacts(app: &App, request: Request, session_id: &str) -> std::io::Result<()> {
    match app.render_session_artifacts(session_id) {
        Ok(artifacts) => respond_json(
            request,
            StatusCode(200),
            &SessionArtifactsResponse { artifacts },
        ),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn handle_read_artifact(
    app: &App,
    request: Request,
    session_id: &str,
    artifact_id: &str,
) -> std::io::Result<()> {
    match app.read_session_artifact(session_id, artifact_id) {
        Ok(artifact) => respond_json(
            request,
            StatusCode(200),
            &SessionArtifactResponse { artifact },
        ),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn handle_list_artifact_files(
    app: &App,
    request: Request,
    session_id: &str,
) -> std::io::Result<()> {
    let store = match app.store() {
        Ok(store) => store,
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            return respond_json(request, status, &payload);
        }
    };
    match store.get_session(session_id) {
        Ok(Some(_)) => {}
        Ok(None) => {
            return respond_json(
                request,
                StatusCode(404),
                &ErrorResponse {
                    error: format!("session {session_id} not found"),
                },
            );
        }
        Err(error) => {
            let (status, payload) = map_bootstrap_error(BootstrapError::Store(error));
            return respond_json(request, status, &payload);
        }
    }

    match store.list_artifacts_for_session(session_id) {
        Ok(artifacts) => {
            let response = SessionArtifactFilesResponse {
                artifacts: artifacts
                    .into_iter()
                    .map(artifact_summary_response)
                    .collect(),
            };
            respond_json(request, StatusCode(200), &response)
        }
        Err(error) => {
            let (status, payload) = map_bootstrap_error(BootstrapError::Store(error));
            respond_json(request, status, &payload)
        }
    }
}

fn handle_read_artifact_file(
    app: &App,
    request: Request,
    session_id: &str,
    artifact_id: &str,
) -> std::io::Result<()> {
    let query = parse_query(request.url());
    let max_bytes = query_map_usize(&query, "max_bytes")
        .unwrap_or(WEB_FILE_READ_DEFAULT_MAX_BYTES)
        .min(WEB_FILE_READ_MAX_BYTES);
    match load_session_artifact(app, session_id, artifact_id) {
        Ok(artifact) => {
            let response = artifact_file_response(artifact, max_bytes);
            respond_json(request, StatusCode(200), &response)
        }
        Err((status, payload)) => respond_json(request, status, &payload),
    }
}

fn handle_download_artifact_file(
    app: &App,
    request: Request,
    session_id: &str,
    artifact_id: &str,
) -> std::io::Result<()> {
    match load_session_artifact(app, session_id, artifact_id) {
        Ok(artifact) => respond_download(
            request,
            artifact.bytes,
            artifact_download_name(artifact.id.as_str(), artifact.path.as_path()),
        ),
        Err((status, payload)) => respond_json(request, status, &payload),
    }
}

fn handle_workspace_list(app: &App, request: Request, session_id: &str) -> std::io::Result<()> {
    let query = parse_query(request.url());
    let path = query.get("path").cloned().unwrap_or_default();
    let recursive = query_bool(&query, "recursive").unwrap_or(false);
    let limit = query_map_usize(&query, "limit")
        .unwrap_or(WEB_WORKSPACE_LIST_DEFAULT_LIMIT)
        .min(WEB_WORKSPACE_LIST_MAX_LIMIT);
    let offset = query_map_usize(&query, "offset").unwrap_or(0);
    match session_workspace(app, session_id) {
        Ok(workspace) => match workspace.list(path.as_str(), recursive) {
            Ok(entries) => {
                let total = entries.len();
                let page = entries
                    .into_iter()
                    .skip(offset)
                    .take(limit)
                    .map(|entry| SessionWorkspaceEntryResponse {
                        path: entry.path,
                        kind: match entry.kind {
                            WorkspaceEntryKind::File => "file".to_string(),
                            WorkspaceEntryKind::Directory => "directory".to_string(),
                        },
                        bytes: entry.bytes,
                    })
                    .collect::<Vec<_>>();
                let next_offset = (offset + page.len() < total).then_some(offset + page.len());
                respond_json(
                    request,
                    StatusCode(200),
                    &SessionWorkspaceListResponse {
                        workspace_root: workspace.root.display().to_string(),
                        path,
                        entries: page,
                        total,
                        limit,
                        offset,
                        next_offset,
                    },
                )
            }
            Err(error) => respond_json(
                request,
                workspace_error_status(&error),
                &ErrorResponse {
                    error: error.to_string(),
                },
            ),
        },
        Err((status, payload)) => respond_json(request, status, &payload),
    }
}

fn handle_workspace_read(app: &App, request: Request, session_id: &str) -> std::io::Result<()> {
    let query = parse_query(request.url());
    let path = query.get("path").cloned().unwrap_or_default();
    let max_bytes = query_map_usize(&query, "max_bytes")
        .unwrap_or(WEB_FILE_READ_DEFAULT_MAX_BYTES)
        .min(WEB_FILE_READ_MAX_BYTES);
    match session_workspace(app, session_id) {
        Ok(workspace) => match read_workspace_bytes(&workspace, path.as_str()) {
            Ok(bytes) => {
                let response = workspace_file_response(&workspace, path, bytes, max_bytes);
                respond_json(request, StatusCode(200), &response)
            }
            Err((status, payload)) => respond_json(request, status, &payload),
        },
        Err((status, payload)) => respond_json(request, status, &payload),
    }
}

fn handle_workspace_download(app: &App, request: Request, session_id: &str) -> std::io::Result<()> {
    let query = parse_query(request.url());
    let path = query.get("path").cloned().unwrap_or_default();
    match session_workspace(app, session_id) {
        Ok(workspace) => match read_workspace_bytes(&workspace, path.as_str()) {
            Ok(bytes) => respond_download(request, bytes, download_name_from_path(path.as_str())),
            Err((status, payload)) => respond_json(request, status, &payload),
        },
        Err((status, payload)) => respond_json(request, status, &payload),
    }
}

fn session_workspace(
    app: &App,
    session_id: &str,
) -> Result<WorkspaceRef, (StatusCode, ErrorResponse)> {
    let store = app.store().map_err(map_bootstrap_error)?;
    let record = store
        .get_session(session_id)
        .map_err(|error| map_bootstrap_error(BootstrapError::Store(error)))?
        .ok_or_else(|| {
            (
                StatusCode(404),
                ErrorResponse {
                    error: format!("session {session_id} not found"),
                },
            )
        })?;
    Ok(WorkspaceRef::new(record.workspace_root))
}

fn load_session_artifact(
    app: &App,
    session_id: &str,
    artifact_id: &str,
) -> Result<agent_persistence::ArtifactRecord, (StatusCode, ErrorResponse)> {
    let store = app.store().map_err(map_bootstrap_error)?;
    if !store
        .session_exists(session_id)
        .map_err(|error| map_bootstrap_error(BootstrapError::Store(error)))?
    {
        return Err((
            StatusCode(404),
            ErrorResponse {
                error: format!("session {session_id} not found"),
            },
        ));
    }
    let artifact = store
        .get_artifact(artifact_id)
        .map_err(|error| map_bootstrap_error(BootstrapError::Store(error)))?
        .ok_or_else(|| {
            (
                StatusCode(404),
                ErrorResponse {
                    error: format!("artifact {artifact_id} not found"),
                },
            )
        })?;
    if artifact.session_id != session_id {
        return Err((
            StatusCode(404),
            ErrorResponse {
                error: format!("artifact {artifact_id} not found in session {session_id}"),
            },
        ));
    }
    Ok(artifact)
}

fn read_workspace_bytes(
    workspace: &WorkspaceRef,
    path: &str,
) -> Result<Vec<u8>, (StatusCode, ErrorResponse)> {
    let resolved = workspace.resolve(path).map_err(|error| {
        (
            workspace_error_status(&error),
            ErrorResponse {
                error: error.to_string(),
            },
        )
    })?;
    let metadata = fs::metadata(&resolved).map_err(|source| {
        (
            StatusCode(404),
            ErrorResponse {
                error: format!(
                    "workspace filesystem error at {}: {source}",
                    resolved.display()
                ),
            },
        )
    })?;
    if !metadata.is_file() {
        return Err((
            StatusCode(400),
            ErrorResponse {
                error: "workspace path must point to a file".to_string(),
            },
        ));
    }
    fs::read(&resolved).map_err(|source| {
        (
            StatusCode(500),
            ErrorResponse {
                error: format!(
                    "workspace filesystem error at {}: {source}",
                    resolved.display()
                ),
            },
        )
    })
}

fn artifact_summary_response(
    artifact: agent_persistence::ArtifactRecord,
) -> SessionArtifactFileSummaryResponse {
    SessionArtifactFileSummaryResponse {
        id: artifact.id,
        session_id: artifact.session_id,
        kind: artifact.kind,
        metadata_json: artifact.metadata_json,
        path: artifact.path.display().to_string(),
        byte_len: artifact.bytes.len(),
        created_at: artifact.created_at,
    }
}

fn artifact_file_response(
    artifact: agent_persistence::ArtifactRecord,
    max_bytes: usize,
) -> SessionArtifactFileResponse {
    let (content, content_truncated, text) = preview_text(artifact.bytes.as_slice(), max_bytes);
    SessionArtifactFileResponse {
        id: artifact.id,
        session_id: artifact.session_id,
        kind: artifact.kind,
        metadata_json: artifact.metadata_json,
        path: artifact.path.display().to_string(),
        byte_len: artifact.bytes.len(),
        created_at: artifact.created_at,
        content,
        content_truncated,
        text,
    }
}

fn workspace_file_response(
    workspace: &WorkspaceRef,
    path: String,
    bytes: Vec<u8>,
    max_bytes: usize,
) -> SessionWorkspaceFileResponse {
    let (content, content_truncated, text) = preview_text(bytes.as_slice(), max_bytes);
    SessionWorkspaceFileResponse {
        workspace_root: workspace.root.display().to_string(),
        path,
        byte_len: bytes.len() as u64,
        content,
        content_truncated,
        text,
    }
}

fn preview_text(bytes: &[u8], max_bytes: usize) -> (Option<String>, bool, bool) {
    match std::str::from_utf8(bytes) {
        Ok(text) => {
            let mut selected_len = bytes.len().min(max_bytes);
            while !text.is_char_boundary(selected_len) {
                selected_len -= 1;
            }
            (
                Some(text[..selected_len].to_string()),
                bytes.len() > selected_len,
                true,
            )
        }
        Err(_) => (None, bytes.len() > max_bytes, false),
    }
}

fn respond_download(request: Request, bytes: Vec<u8>, file_name: String) -> std::io::Result<()> {
    let mut response = Response::from_data(bytes).with_status_code(StatusCode(200));
    response.add_header(
        Header::from_bytes("Content-Type", "application/octet-stream")
            .map_err(|_| std::io::Error::other("invalid content type header"))?,
    );
    response.add_header(
        Header::from_bytes(
            "Content-Disposition",
            format!(
                "attachment; filename=\"{}\"",
                header_safe_filename(file_name)
            )
            .as_bytes(),
        )
        .map_err(|_| std::io::Error::other("invalid content disposition header"))?,
    );
    request.respond(response)
}

fn workspace_error_status(error: &agent_runtime::workspace::WorkspaceError) -> StatusCode {
    match error {
        agent_runtime::workspace::WorkspaceError::InvalidPath { .. } => StatusCode(400),
        agent_runtime::workspace::WorkspaceError::Io { .. } => StatusCode(500),
    }
}

fn parse_query(url: &str) -> BTreeMap<String, String> {
    let Some((_, query)) = url.split_once('?') else {
        return BTreeMap::new();
    };
    query
        .split('&')
        .filter(|part| !part.is_empty())
        .filter_map(|part| {
            let (key, value) = part.split_once('=').unwrap_or((part, ""));
            let key = percent_decode_query(key).ok()?;
            let value = percent_decode_query(value).ok()?;
            Some((key, value))
        })
        .collect()
}

fn percent_decode_query(input: &str) -> Result<String, ()> {
    let mut bytes = Vec::with_capacity(input.len());
    let input_bytes = input.as_bytes();
    let mut index = 0;
    while index < input_bytes.len() {
        match input_bytes[index] {
            b'+' => {
                bytes.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < input_bytes.len() => {
                let high = hex_value(input_bytes[index + 1]).ok_or(())?;
                let low = hex_value(input_bytes[index + 2]).ok_or(())?;
                bytes.push((high << 4) | low);
                index += 3;
            }
            value => {
                bytes.push(value);
                index += 1;
            }
        }
    }
    String::from_utf8(bytes).map_err(|_| ())
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn query_map_usize(query: &BTreeMap<String, String>, key: &str) -> Option<usize> {
    query.get(key).and_then(|value| value.parse::<usize>().ok())
}

fn query_bool(query: &BTreeMap<String, String>, key: &str) -> Option<bool> {
    query.get(key).and_then(|value| match value.as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    })
}

fn download_name_from_path(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "workspace-file.bin".to_string())
}

fn artifact_download_name(artifact_id: &str, path: &std::path::Path) -> String {
    path.file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| format!("{artifact_id}.bin"))
}

fn header_safe_filename(file_name: String) -> String {
    file_name
        .chars()
        .map(|value| match value {
            '"' | '\\' | '\r' | '\n' => '_',
            _ => value,
        })
        .collect()
}

fn handle_render_active_run(app: &App, request: Request, session_id: &str) -> std::io::Result<()> {
    match app.render_active_run(session_id) {
        Ok(run) => respond_json(request, StatusCode(200), &SessionRunStatusResponse { run }),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn handle_cancel_active_run(app: &App, request: Request, session_id: &str) -> std::io::Result<()> {
    let now = current_unix_timestamp();
    match app.cancel_latest_session_run(session_id, now) {
        Ok(message) => respond_json(
            request,
            StatusCode(200),
            &SessionRunControlResponse { message },
        ),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn handle_cancel_all_session_work(
    app: &App,
    request: Request,
    session_id: &str,
) -> std::io::Result<()> {
    let now = current_unix_timestamp();

    match app.cancel_all_session_work(session_id, now) {
        Ok(message) => respond_json(
            request,
            StatusCode(200),
            &SessionRunControlResponse { message },
        ),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn handle_send_agent_message(
    app: &App,
    mut request: Request,
    session_id: &str,
) -> std::io::Result<()> {
    let payload = match parse_json_body::<SessionAgentMessageRequest>(&mut request) {
        Ok(payload) => payload,
        Err(error) => {
            return respond_json(
                request,
                StatusCode(400),
                &ErrorResponse {
                    error: format!("invalid session agent message request: {error}"),
                },
            );
        }
    };

    match app.send_session_agent_message(
        session_id,
        payload.target_agent_id.as_str(),
        payload.message.as_str(),
        current_unix_timestamp(),
    ) {
        Ok(message) => respond_json(
            request,
            StatusCode(200),
            &SessionRunControlResponse { message },
        ),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn handle_grant_chain_continuation(
    app: &App,
    mut request: Request,
    session_id: &str,
) -> std::io::Result<()> {
    let payload = match parse_json_body::<SessionChainGrantRequest>(&mut request) {
        Ok(payload) => payload,
        Err(error) => {
            return respond_json(
                request,
                StatusCode(400),
                &ErrorResponse {
                    error: format!("invalid session chain grant request: {error}"),
                },
            );
        }
    };

    match app.grant_session_chain_continuation(
        session_id,
        payload.chain_id.as_str(),
        payload.reason.as_str(),
        current_unix_timestamp(),
    ) {
        Ok(message) => respond_json(
            request,
            StatusCode(200),
            &SessionRunControlResponse { message },
        ),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn current_unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

fn handle_session_skills(app: &App, request: Request, session_id: &str) -> std::io::Result<()> {
    match app.session_skills(session_id) {
        Ok(skills) => respond_json::<SessionSkillsResponse>(request, StatusCode(200), &skills),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn handle_enable_session_skill(
    app: &App,
    mut request: Request,
    session_id: &str,
) -> std::io::Result<()> {
    let body: SkillCommandRequest = match parse_json_body(&mut request) {
        Ok(body) => body,
        Err(error) => {
            return respond_json(
                request,
                StatusCode(400),
                &ErrorResponse {
                    error: format!("invalid json body: {error}"),
                },
            );
        }
    };

    match app.enable_session_skill(session_id, &body.name) {
        Ok(skills) => respond_json::<SessionSkillsResponse>(request, StatusCode(200), &skills),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn handle_disable_session_skill(
    app: &App,
    mut request: Request,
    session_id: &str,
) -> std::io::Result<()> {
    let body: SkillCommandRequest = match parse_json_body(&mut request) {
        Ok(body) => body,
        Err(error) => {
            return respond_json(
                request,
                StatusCode(400),
                &ErrorResponse {
                    error: format!("invalid json body: {error}"),
                },
            );
        }
    };

    match app.disable_session_skill(session_id, &body.name) {
        Ok(skills) => respond_json::<SessionSkillsResponse>(request, StatusCode(200), &skills),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn handle_update_preferences(
    app: &App,
    mut request: Request,
    session_id: &str,
) -> std::io::Result<()> {
    let patch: SessionPreferencesRequest = match parse_json_body(&mut request) {
        Ok(patch) => patch,
        Err(error) => {
            return respond_json(
                request,
                StatusCode(400),
                &ErrorResponse {
                    error: format!("invalid json body: {error}"),
                },
            );
        }
    };
    match app.update_session_preferences(session_id, patch) {
        Ok(summary) => respond_json(
            request,
            StatusCode(200),
            &SessionSummaryResponse::from(summary),
        ),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn handle_delete_session(app: &App, request: Request, session_id: &str) -> std::io::Result<()> {
    match app.delete_session(session_id) {
        Ok(()) => respond_json(
            request,
            StatusCode(200),
            &serde_json::json!({ "deleted": true }),
        ),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn handle_clear_session(app: &App, mut request: Request, session_id: &str) -> std::io::Result<()> {
    let body: ClearSessionRequest = match parse_json_body(&mut request) {
        Ok(body) => body,
        Err(error) => {
            return respond_json(
                request,
                StatusCode(400),
                &ErrorResponse {
                    error: format!("invalid json body: {error}"),
                },
            );
        }
    };
    match app.clear_session(session_id, body.title.as_deref()) {
        Ok(summary) => respond_json(
            request,
            StatusCode(200),
            &SessionSummaryResponse::from(summary),
        ),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn handle_compact_session(app: &App, request: Request, session_id: &str) -> std::io::Result<()> {
    match app.compact_session(session_id) {
        Ok(summary) => respond_json(
            request,
            StatusCode(200),
            &SessionSummaryResponse::from(summary),
        ),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn handle_render_plan(app: &App, request: Request, session_id: &str) -> std::io::Result<()> {
    match app.render_plan(session_id) {
        Ok(plan) => respond_json(
            request,
            StatusCode(200),
            &serde_json::json!({ "plan": plan }),
        ),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}
