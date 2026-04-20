use super::*;

impl DaemonClient {
    pub fn status(&self) -> Result<StatusResponse, BootstrapError> {
        self.get_json("/v1/status")
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
