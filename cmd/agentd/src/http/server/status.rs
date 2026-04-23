use super::*;
use crate::about::{APP_BUILD_ID, APP_COMMIT, APP_TREE_STATE, APP_VERSION};
use crate::http::types::{
    AboutResponse, DiagnosticsTailRequest, DiagnosticsTailResponse, StatusResponse,
    UpdateRuntimeRequest, UpdateRuntimeResponse,
};

pub(super) fn handle_status(app: &App, request: Request) -> std::io::Result<()> {
    let status_snapshot = match app.runtime_status_snapshot() {
        Ok(snapshot) => snapshot,
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            return respond_json(request, status, &payload);
        }
    };
    let response = StatusResponse {
        ok: true,
        version: Some(APP_VERSION.to_string()),
        commit: Some(APP_COMMIT.to_string()),
        tree_state: Some(APP_TREE_STATE.to_string()),
        build_id: Some(APP_BUILD_ID.to_string()),
        bind_host: app.config.daemon.bind_host.clone(),
        bind_port: app.config.daemon.bind_port,
        permission_mode: status_snapshot.permission_mode,
        session_count: status_snapshot.session_count,
        mission_count: status_snapshot.mission_count,
        run_count: status_snapshot.run_count,
        job_count: status_snapshot.job_count,
        components: status_snapshot.components,
        data_dir: status_snapshot.data_dir,
        state_db: status_snapshot.state_db,
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

pub(super) fn handle_diagnostics_tail(app: &App, mut request: Request) -> std::io::Result<()> {
    let payload = match parse_json_body::<Option<DiagnosticsTailRequest>>(&mut request) {
        Ok(payload) => payload.unwrap_or(DiagnosticsTailRequest { max_lines: None }),
        Err(error) => {
            return respond_json(
                request,
                StatusCode(400),
                &crate::http::types::ErrorResponse {
                    error: format!("invalid diagnostics tail request: {error}"),
                },
            );
        }
    };

    let max_lines = payload
        .max_lines
        .unwrap_or(app.config.runtime_limits.diagnostic_tail_lines);
    match app.render_diagnostics_tail(max_lines) {
        Ok(diagnostics) => respond_json(
            request,
            StatusCode(200),
            &DiagnosticsTailResponse { diagnostics },
        ),
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
