use super::*;

pub(super) fn handle_tool_catalog(app: &App, request: Request) -> std::io::Result<()> {
    match app.tool_catalog() {
        Ok(catalog) => respond_json(request, StatusCode(200), &catalog),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}
