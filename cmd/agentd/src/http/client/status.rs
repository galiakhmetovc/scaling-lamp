use super::*;
use crate::http::types::{
    DiagnosticsTailRequest, DiagnosticsTailResponse, DiskPruneRequest, UpdateRuntimeRequest,
};

impl DaemonClient {
    pub fn about(&self) -> Result<String, BootstrapError> {
        let response: AboutResponse = self.get_json("/v1/about")?;
        Ok(response.about)
    }

    pub fn update_runtime(&self, tag: Option<&str>) -> Result<String, BootstrapError> {
        let response: UpdateRuntimeResponse = self.post_json(
            "/v1/update",
            &UpdateRuntimeRequest {
                tag: tag.map(str::to_string),
            },
        )?;
        Ok(response.message)
    }

    pub fn status(&self) -> Result<StatusResponse, BootstrapError> {
        self.get_json("/v1/status")
    }

    pub fn disk_usage(&self) -> Result<crate::bootstrap::DiskUsageReport, BootstrapError> {
        self.get_json("/v1/disk/usage")
    }

    pub fn disk_prune(
        &self,
        options: crate::bootstrap::DiskPruneOptions,
    ) -> Result<crate::bootstrap::DiskPruneReport, BootstrapError> {
        self.post_json(
            "/v1/disk/prune",
            &DiskPruneRequest {
                dry_run: options.dry_run,
            },
        )
    }

    pub fn render_diagnostics_tail(
        &self,
        max_lines: Option<usize>,
    ) -> Result<String, BootstrapError> {
        let response: DiagnosticsTailResponse = self.post_json(
            "/v1/diagnostics/tail",
            &DiagnosticsTailRequest {
                max_lines: Some(max_lines.unwrap_or(self.default_diagnostic_tail_lines)),
            },
        )?;
        Ok(response.diagnostics)
    }

    pub fn shutdown(&self) -> Result<(), BootstrapError> {
        let response: DaemonStopResponse =
            self.post_json("/v1/daemon/stop", &serde_json::json!({}))?;
        if response.stopping {
            return Ok(());
        }
        Err(BootstrapError::Usage {
            reason: "daemon refused to stop".to_string(),
        })
    }
}
