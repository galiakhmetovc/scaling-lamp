use super::*;
use crate::http::types::{
    AboutResponse, StatusResponse, UpdateRuntimeRequest, UpdateRuntimeResponse,
};
use agent_persistence::{JobRepository, MissionRepository, SessionRepository};

pub(super) fn handle_status(app: &App, request: Request) -> std::io::Result<()> {
    let store = match app.store() {
        Ok(store) => store,
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            return respond_json(request, status, &payload);
        }
    };
    let session_count = match store.list_sessions() {
        Ok(sessions) => sessions.len(),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(BootstrapError::Store(error));
            return respond_json(request, status, &payload);
        }
    };
    let mission_count = match store.list_missions() {
        Ok(missions) => missions.len(),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(BootstrapError::Store(error));
            return respond_json(request, status, &payload);
        }
    };
    let run_count = match store.load_execution_state() {
        Ok(state) => state.runs.len(),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(BootstrapError::Store(error));
            return respond_json(request, status, &payload);
        }
    };
    let job_count = match store.list_jobs() {
        Ok(jobs) => jobs.len(),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(BootstrapError::Store(error));
            return respond_json(request, status, &payload);
        }
    };
    let response = StatusResponse {
        ok: true,
        bind_host: app.config.daemon.bind_host.clone(),
        bind_port: app.config.daemon.bind_port,
        permission_mode: app.config.permissions.mode.as_str().to_string(),
        session_count,
        mission_count,
        run_count,
        job_count,
        components: app.runtime.component_count(),
        data_dir: app.config.data_dir.display().to_string(),
        state_db: app.persistence.stores.metadata_db.display().to_string(),
    };
    respond_json(request, StatusCode(200), &response)
}

pub(super) fn handle_about(app: &App, request: Request) -> std::io::Result<()> {
    match app.render_version_info() {
        Ok(about) => respond_json(request, StatusCode(200), &AboutResponse { about }),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

pub(super) fn handle_update_runtime(app: &App, mut request: Request) -> std::io::Result<()> {
    let payload = match parse_json_body::<Option<UpdateRuntimeRequest>>(&mut request) {
        Ok(payload) => payload.unwrap_or(UpdateRuntimeRequest { tag: None }),
        Err(error) => {
            return respond_json(
                request,
                StatusCode(400),
                &crate::http::types::ErrorResponse {
                    error: format!("invalid update request: {error}"),
                },
            );
        }
    };

    match app.update_runtime_binary(payload.tag.as_deref()) {
        Ok(message) => respond_json(request, StatusCode(200), &UpdateRuntimeResponse { message }),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}
