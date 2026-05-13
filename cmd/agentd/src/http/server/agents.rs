use super::*;
use crate::bootstrap::AgentProfileUpdatePatch;
use crate::http::types::{
    AgentCreateRequest, AgentDeleteResponse, AgentDetailResponse, AgentFileEntryResponse,
    AgentFileReadResponse, AgentFileWriteRequest, AgentFileWriteResponse, AgentFilesResponse,
    AgentRenderResponse, AgentResolveRequest, AgentScheduleCreateRequest,
    AgentScheduleDetailResponse, AgentScheduleResolveRequest, AgentScheduleUpdateRequest,
    AgentSelectRequest, AgentSummaryResponse, AgentUpdateRequest, ErrorResponse,
};
use agent_persistence::{AgentProfileRecord, AgentRepository};
use agent_runtime::agent::AgentProfile;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tiny_http::{Method, StatusCode};

pub(super) fn handle_list_agents(app: &App, request: Request) -> std::io::Result<()> {
    match app.render_agents() {
        Ok(message) => respond_json(request, StatusCode(200), &AgentRenderResponse { message }),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

pub(super) fn handle_list_agent_summaries(app: &App, request: Request) -> std::io::Result<()> {
    match app.list_agents() {
        Ok(agents) => {
            let agents = agents
                .into_iter()
                .map(|agent| AgentSummaryResponse {
                    id: agent.id,
                    name: agent.name,
                    template_kind: agent.template_kind.as_str().to_string(),
                })
                .collect::<Vec<_>>();
            respond_json(request, StatusCode(200), &agents)
        }
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

pub(super) fn handle_resolve_agent(app: &App, mut request: Request) -> std::io::Result<()> {
    let payload = match parse_json_body::<AgentResolveRequest>(&mut request) {
        Ok(payload) => payload,
        Err(error) => {
            return respond_json(
                request,
                StatusCode(400),
                &ErrorResponse {
                    error: format!("invalid agent resolve request: {error}"),
                },
            );
        }
    };
    let Some(identifier) = payload.identifier.as_deref() else {
        return respond_json(
            request,
            StatusCode(400),
            &ErrorResponse {
                error: "agent identifier is required".to_string(),
            },
        );
    };

    match app.agent_profile(identifier) {
        Ok(agent) => respond_json(
            request,
            StatusCode(200),
            &AgentSummaryResponse {
                id: agent.id,
                name: agent.name,
                template_kind: agent.template_kind.as_str().to_string(),
            },
        ),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

pub(super) fn handle_current_agent(app: &App, request: Request) -> std::io::Result<()> {
    match app.render_agent_profile(None) {
        Ok(message) => respond_json(request, StatusCode(200), &AgentRenderResponse { message }),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

pub(super) fn handle_show_agent(app: &App, mut request: Request) -> std::io::Result<()> {
    let payload = match parse_json_body::<Option<AgentResolveRequest>>(&mut request) {
        Ok(payload) => payload.unwrap_or(AgentResolveRequest { identifier: None }),
        Err(error) => {
            return respond_json(
                request,
                StatusCode(400),
                &ErrorResponse {
                    error: format!("invalid agent show request: {error}"),
                },
            );
        }
    };

    match app.render_agent_profile(payload.identifier.as_deref()) {
        Ok(message) => respond_json(request, StatusCode(200), &AgentRenderResponse { message }),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

pub(super) fn handle_select_agent(app: &App, mut request: Request) -> std::io::Result<()> {
    let payload = match parse_json_body::<AgentSelectRequest>(&mut request) {
        Ok(payload) => payload,
        Err(error) => {
            return respond_json(
                request,
                StatusCode(400),
                &ErrorResponse {
                    error: format!("invalid agent select request: {error}"),
                },
            );
        }
    };

    match app.select_agent_profile(&payload.identifier) {
        Ok(profile) => respond_json(
            request,
            StatusCode(200),
            &AgentRenderResponse {
                message: format!("текущий агент: {} ({})", profile.name, profile.id),
            },
        ),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

pub(super) fn handle_create_agent(app: &App, mut request: Request) -> std::io::Result<()> {
    let payload = match parse_json_body::<AgentCreateRequest>(&mut request) {
        Ok(payload) => payload,
        Err(error) => {
            return respond_json(
                request,
                StatusCode(400),
                &ErrorResponse {
                    error: format!("invalid agent create request: {error}"),
                },
            );
        }
    };

    match app.create_agent_from_template(&payload.name, payload.template_identifier.as_deref()) {
        Ok(profile) => respond_json(
            request,
            StatusCode(201),
            &AgentRenderResponse {
                message: format!(
                    "создан агент {} ({}) из шаблона {}",
                    profile.name,
                    profile.id,
                    profile.template_kind.as_str()
                ),
            },
        ),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

pub(super) fn handle_agent_nested_routes(app: &App, request: Request) -> std::io::Result<()> {
    let path = request
        .url()
        .split('?')
        .next()
        .unwrap_or_default()
        .to_string();
    let method = request.method().clone();
    let Some(agent_tail) = path.strip_prefix("/v1/agents/") else {
        return respond_json(
            request,
            StatusCode(404),
            &ErrorResponse {
                error: "route not found".to_string(),
            },
        );
    };
    let segments = agent_tail
        .split('/')
        .map(str::to_string)
        .collect::<Vec<_>>();
    match (method, segments.as_slice()) {
        (Method::Get, [agent_id]) => handle_agent_detail(app, request, agent_id),
        (Method::Patch, [agent_id]) => handle_update_agent(app, request, agent_id),
        (Method::Delete, [agent_id]) => handle_delete_agent(app, request, agent_id),
        (Method::Get, [agent_id, files]) if files == "files" => {
            handle_list_agent_files(app, request, agent_id)
        }
        (Method::Get, [agent_id, files, action]) if files == "files" && action == "read" => {
            handle_read_agent_file(app, request, agent_id)
        }
        (Method::Post, [agent_id, files, action]) if files == "files" && action == "write" => {
            handle_write_agent_file(app, request, agent_id)
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

fn handle_agent_detail(app: &App, request: Request, agent_id: &str) -> std::io::Result<()> {
    match app.agent_profile(agent_id) {
        Ok(profile) => respond_json(request, StatusCode(200), &agent_detail_response(profile)),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn handle_update_agent(app: &App, mut request: Request, agent_id: &str) -> std::io::Result<()> {
    let body = match parse_json_body::<AgentUpdateRequest>(&mut request) {
        Ok(body) => body,
        Err(error) => {
            return respond_json(
                request,
                StatusCode(400),
                &ErrorResponse {
                    error: format!("invalid agent update request: {error}"),
                },
            );
        }
    };
    let patch = AgentProfileUpdatePatch {
        name: body.name,
        allowed_tools: body.allowed_tools,
        default_workspace_root: body.default_workspace_root.map(|value| {
            value.and_then(|path| {
                let trimmed = path.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(trimmed))
                }
            })
        }),
    };
    match app.update_agent_profile(agent_id, patch) {
        Ok(profile) => respond_json(request, StatusCode(200), &agent_detail_response(profile)),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn handle_delete_agent(app: &App, request: Request, agent_id: &str) -> std::io::Result<()> {
    match app.delete_agent_profile(agent_id) {
        Ok(deleted) => respond_json(request, StatusCode(200), &AgentDeleteResponse { deleted }),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn handle_list_agent_files(app: &App, request: Request, agent_id: &str) -> std::io::Result<()> {
    let profile = match app.agent_profile(agent_id) {
        Ok(profile) => profile,
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            return respond_json(request, status, &payload);
        }
    };
    match list_agent_files(profile.agent_home.as_path()) {
        Ok(files) => respond_json(
            request,
            StatusCode(200),
            &AgentFilesResponse {
                agent_id: profile.id,
                agent_name: profile.name,
                agent_home: profile.agent_home.display().to_string(),
                files,
            },
        ),
        Err(error) => respond_json(request, StatusCode(500), &ErrorResponse { error }),
    }
}

fn handle_read_agent_file(app: &App, request: Request, agent_id: &str) -> std::io::Result<()> {
    let query = parse_agent_query(request.url());
    let file_path = query.get("path").cloned().unwrap_or_default();
    let profile = match app.agent_profile(agent_id) {
        Ok(profile) => profile,
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            return respond_json(request, status, &payload);
        }
    };
    let resolved = match resolve_agent_file(profile.agent_home.as_path(), file_path.as_str()) {
        Ok(resolved) => resolved,
        Err(error) => return respond_json(request, StatusCode(400), &ErrorResponse { error }),
    };
    let content = match fs::read_to_string(&resolved.path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return respond_json(
                request,
                StatusCode(404),
                &ErrorResponse {
                    error: format!("agent file {} not found", resolved.relative_path),
                },
            );
        }
        Err(error) => {
            return respond_json(
                request,
                StatusCode(500),
                &ErrorResponse {
                    error: format!(
                        "failed to read agent file {}: {error}",
                        resolved.path.display()
                    ),
                },
            );
        }
    };
    respond_json(
        request,
        StatusCode(200),
        &AgentFileReadResponse {
            agent_id: profile.id,
            agent_home: profile.agent_home.display().to_string(),
            path: resolved.relative_path,
            kind: resolved.kind,
            byte_len: content.len() as u64,
            content,
        },
    )
}

fn handle_write_agent_file(app: &App, mut request: Request, agent_id: &str) -> std::io::Result<()> {
    let body = match parse_json_body::<AgentFileWriteRequest>(&mut request) {
        Ok(body) => body,
        Err(error) => {
            return respond_json(
                request,
                StatusCode(400),
                &ErrorResponse {
                    error: format!("invalid agent file write request: {error}"),
                },
            );
        }
    };
    let mut profile = match app.agent_profile(agent_id) {
        Ok(profile) => profile,
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            return respond_json(request, status, &payload);
        }
    };
    let resolved = match resolve_agent_file(profile.agent_home.as_path(), body.path.as_str()) {
        Ok(resolved) => resolved,
        Err(error) => return respond_json(request, StatusCode(400), &ErrorResponse { error }),
    };
    let existed = resolved.path.exists();
    let mode = body.mode.as_deref().unwrap_or("upsert").trim();
    match mode {
        "create" if existed => {
            return respond_json(
                request,
                StatusCode(409),
                &ErrorResponse {
                    error: format!("agent file {} already exists", resolved.relative_path),
                },
            );
        }
        "overwrite" if !existed => {
            return respond_json(
                request,
                StatusCode(404),
                &ErrorResponse {
                    error: format!("agent file {} does not exist", resolved.relative_path),
                },
            );
        }
        "create" | "overwrite" | "upsert" => {}
        other => {
            return respond_json(
                request,
                StatusCode(400),
                &ErrorResponse {
                    error: format!(
                        "agent file mode must be create, overwrite, or upsert; got {other}"
                    ),
                },
            );
        }
    }
    if let Some(parent) = resolved.path.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        return respond_json(
            request,
            StatusCode(500),
            &ErrorResponse {
                error: format!(
                    "failed to create parent directory {}: {error}",
                    parent.display()
                ),
            },
        );
    }
    if let Err(error) = fs::write(&resolved.path, body.content.as_bytes()) {
        return respond_json(
            request,
            StatusCode(500),
            &ErrorResponse {
                error: format!(
                    "failed to write agent file {}: {error}",
                    resolved.path.display()
                ),
            },
        );
    }
    if let Err(error) = touch_agent_profile(app, &mut profile) {
        let (status, payload) = map_bootstrap_error(error);
        return respond_json(request, status, &payload);
    }
    respond_json(
        request,
        StatusCode(200),
        &AgentFileWriteResponse {
            agent_id: profile.id,
            agent_home: profile.agent_home.display().to_string(),
            path: resolved.relative_path,
            kind: resolved.kind,
            bytes_written: body.content.len(),
            created: !existed,
            overwritten: existed,
        },
    )
}

pub(super) fn handle_open_agent_home(app: &App, mut request: Request) -> std::io::Result<()> {
    let payload = match parse_json_body::<Option<AgentResolveRequest>>(&mut request) {
        Ok(payload) => payload.unwrap_or(AgentResolveRequest { identifier: None }),
        Err(error) => {
            return respond_json(
                request,
                StatusCode(400),
                &ErrorResponse {
                    error: format!("invalid agent open request: {error}"),
                },
            );
        }
    };

    match app.render_agent_home(payload.identifier.as_deref()) {
        Ok(message) => respond_json(request, StatusCode(200), &AgentRenderResponse { message }),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedAgentFile {
    relative_path: String,
    path: PathBuf,
    kind: String,
}

fn list_agent_files(agent_home: &Path) -> Result<Vec<AgentFileEntryResponse>, String> {
    let mut files = Vec::new();
    for path in ["SYSTEM.md", "AGENTS.md"] {
        let absolute = agent_home.join(path);
        if let Ok(metadata) = fs::metadata(&absolute)
            && metadata.is_file()
        {
            files.push(AgentFileEntryResponse {
                path: path.to_string(),
                kind: "prompt".to_string(),
                byte_len: metadata.len(),
            });
        }
    }
    let skills_dir = agent_home.join("skills");
    match fs::read_dir(&skills_dir) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry.map_err(|error| {
                    format!(
                        "failed to read skills directory {}: {error}",
                        skills_dir.display()
                    )
                })?;
                let skill_path = entry.path().join("SKILL.md");
                let Ok(metadata) = fs::metadata(&skill_path) else {
                    continue;
                };
                if !metadata.is_file() {
                    continue;
                }
                let skill_name = entry.file_name().to_string_lossy().into_owned();
                files.push(AgentFileEntryResponse {
                    path: format!("skills/{skill_name}/SKILL.md"),
                    kind: "skill".to_string(),
                    byte_len: metadata.len(),
                });
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "failed to read skills directory {}: {error}",
                skills_dir.display()
            ));
        }
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(files)
}

fn resolve_agent_file(agent_home: &Path, raw_path: &str) -> Result<ResolvedAgentFile, String> {
    let trimmed = raw_path.trim();
    if trimmed.is_empty() {
        return Err("agent file path must not be empty".to_string());
    }
    let candidate = Path::new(trimmed);
    if candidate.is_absolute() {
        return Err("agent file path must be relative to agent_home".to_string());
    }
    let mut parts = Vec::new();
    for component in candidate.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err("agent file path must not escape agent_home".to_string());
            }
        }
    }
    let kind = match parts.as_slice() {
        [file] if file == "SYSTEM.md" || file == "AGENTS.md" => "prompt",
        [skills, skill_name, file]
            if skills == "skills"
                && file == "SKILL.md"
                && !skill_name.trim().is_empty()
                && !skill_name.starts_with('.') =>
        {
            "skill"
        }
        _ => {
            return Err(
                "agent file path must be SYSTEM.md, AGENTS.md, or skills/<name>/SKILL.md"
                    .to_string(),
            );
        }
    };
    let relative_path = parts.join("/");
    Ok(ResolvedAgentFile {
        path: agent_home.join(&relative_path),
        relative_path,
        kind: kind.to_string(),
    })
}

fn touch_agent_profile(
    app: &App,
    profile: &mut agent_runtime::agent::AgentProfile,
) -> Result<(), BootstrapError> {
    profile.updated_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(profile.updated_at);
    let store = app.store()?;
    store.put_agent_profile(
        &AgentProfileRecord::try_from(&*profile).map_err(BootstrapError::RecordConversion)?,
    )?;
    Ok(())
}

fn agent_detail_response(profile: AgentProfile) -> AgentDetailResponse {
    AgentDetailResponse {
        id: profile.id,
        name: profile.name,
        template_kind: profile.template_kind.as_str().to_string(),
        agent_home: profile.agent_home.display().to_string(),
        allowed_tools: profile.allowed_tools,
        default_workspace_root: profile
            .default_workspace_root
            .map(|path| path.display().to_string()),
        created_from_template_id: profile.created_from_template_id,
        created_by_session_id: profile.created_by_session_id,
        created_by_agent_profile_id: profile.created_by_agent_profile_id,
        created_at: profile.created_at,
        updated_at: profile.updated_at,
    }
}

fn parse_agent_query(url: &str) -> BTreeMap<String, String> {
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

pub(super) fn handle_list_agent_schedules(app: &App, request: Request) -> std::io::Result<()> {
    match app.render_agent_schedules() {
        Ok(message) => respond_json(request, StatusCode(200), &AgentRenderResponse { message }),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

pub(super) fn handle_show_agent_schedule(app: &App, mut request: Request) -> std::io::Result<()> {
    let payload = match parse_json_body::<AgentScheduleResolveRequest>(&mut request) {
        Ok(payload) => payload,
        Err(error) => {
            return respond_json(
                request,
                StatusCode(400),
                &ErrorResponse {
                    error: format!("invalid agent schedule show request: {error}"),
                },
            );
        }
    };

    match app.render_agent_schedule(&payload.id) {
        Ok(message) => respond_json(request, StatusCode(200), &AgentRenderResponse { message }),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

pub(super) fn handle_resolve_agent_schedule(
    app: &App,
    mut request: Request,
) -> std::io::Result<()> {
    let payload = match parse_json_body::<AgentScheduleResolveRequest>(&mut request) {
        Ok(payload) => payload,
        Err(error) => {
            return respond_json(
                request,
                StatusCode(400),
                &ErrorResponse {
                    error: format!("invalid agent schedule resolve request: {error}"),
                },
            );
        }
    };

    match app.agent_schedule_view(&payload.id) {
        Ok(schedule) => respond_json(
            request,
            StatusCode(200),
            &AgentScheduleDetailResponse { schedule },
        ),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

pub(super) fn handle_create_agent_schedule(app: &App, mut request: Request) -> std::io::Result<()> {
    let payload = match parse_json_body::<AgentScheduleCreateRequest>(&mut request) {
        Ok(payload) => payload,
        Err(error) => {
            return respond_json(
                request,
                StatusCode(400),
                &ErrorResponse {
                    error: format!("invalid agent schedule create request: {error}"),
                },
            );
        }
    };

    match app.create_agent_schedule_with_options(&payload.id, payload.options) {
        Ok(schedule) => respond_json(
            request,
            StatusCode(201),
            &AgentRenderResponse {
                message: format!(
                    "создано расписание {} agent={} interval={}s",
                    schedule.id, schedule.agent_profile_id, schedule.interval_seconds
                ),
            },
        ),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

pub(super) fn handle_agent_schedule_nested_routes(
    app: &App,
    request: Request,
) -> std::io::Result<()> {
    let path = request
        .url()
        .split('?')
        .next()
        .unwrap_or_default()
        .to_string();
    let method = request.method().clone();
    let Some(tail) = path.strip_prefix("/v1/agent-schedules/") else {
        return respond_json(
            request,
            StatusCode(404),
            &ErrorResponse {
                error: "route not found".to_string(),
            },
        );
    };

    match (method, tail) {
        (Method::Patch, schedule_id) => {
            let mut request = request;
            let payload = match parse_json_body::<AgentScheduleUpdateRequest>(&mut request) {
                Ok(payload) => payload,
                Err(error) => {
                    return respond_json(
                        request,
                        StatusCode(400),
                        &ErrorResponse {
                            error: format!("invalid agent schedule update request: {error}"),
                        },
                    );
                }
            };

            match app.update_agent_schedule(schedule_id, payload.patch) {
                Ok(schedule) => respond_json(
                    request,
                    StatusCode(200),
                    &AgentRenderResponse {
                        message: format!(
                            "обновлено расписание {} agent={} mode={} delivery={} enabled={} interval={}s",
                            schedule.id,
                            schedule.agent_profile_id,
                            schedule.mode.as_str(),
                            schedule.delivery_mode.as_str(),
                            schedule.enabled,
                            schedule.interval_seconds
                        ),
                    },
                ),
                Err(error) => {
                    let (status, payload) = map_bootstrap_error(error);
                    respond_json(request, status, &payload)
                }
            }
        }
        (Method::Delete, schedule_id) => match app.delete_agent_schedule(schedule_id) {
            Ok(true) => respond_json(
                request,
                StatusCode(200),
                &AgentRenderResponse {
                    message: format!("расписание {schedule_id} удалено"),
                },
            ),
            Ok(false) => respond_json(
                request,
                StatusCode(404),
                &ErrorResponse {
                    error: format!("agent schedule {schedule_id} not found"),
                },
            ),
            Err(error) => {
                let (status, payload) = map_bootstrap_error(error);
                respond_json(request, status, &payload)
            }
        },
        _ => respond_json(
            request,
            StatusCode(404),
            &ErrorResponse {
                error: "route not found".to_string(),
            },
        ),
    }
}
