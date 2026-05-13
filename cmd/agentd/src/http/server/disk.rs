use super::*;
use crate::http::types::{DiskPruneRequest, ErrorResponse};

pub(super) fn handle_disk_usage(app: &App, request: Request) -> std::io::Result<()> {
    match app.disk_usage_report() {
        Ok(report) => respond_json(request, StatusCode(200), &report),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

pub(super) fn handle_disk_prune(app: &App, mut request: Request) -> std::io::Result<()> {
    let payload = match parse_json_body::<Option<DiskPruneRequest>>(&mut request) {
        Ok(payload) => payload.unwrap_or(DiskPruneRequest { dry_run: true }),
        Err(error) => {
            return respond_json(
                request,
                StatusCode(400),
                &ErrorResponse {
                    error: format!("invalid disk prune request: {error}"),
                },
            );
        }
    };

    match app.disk_prune_report(crate::bootstrap::DiskPruneOptions {
        dry_run: payload.dry_run,
    }) {
        Ok(report) => respond_json(request, StatusCode(200), &report),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}
