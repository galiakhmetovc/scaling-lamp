use super::*;
use crate::http::types::StatusResponse;
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
