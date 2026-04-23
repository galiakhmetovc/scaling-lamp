use super::*;
use crate::bootstrap::{McpConnectorCreateOptions, McpConnectorUpdatePatch, McpConnectorView};
use crate::http::types::{
    McpConnectorCreateRequest, McpConnectorDetailResponse, McpConnectorUpdateRequest,
};

impl DaemonClient {
    pub fn list_mcp_connectors(&self) -> Result<Vec<McpConnectorView>, BootstrapError> {
        self.get_json("/v1/mcp/connectors")
    }

    pub fn mcp_connector(&self, id: &str) -> Result<McpConnectorView, BootstrapError> {
        let response: McpConnectorDetailResponse =
            self.get_json(&format!("/v1/mcp/connectors/{id}"))?;
        Ok(response.connector)
    }

    pub fn create_mcp_connector(
        &self,
        id: &str,
        options: McpConnectorCreateOptions,
    ) -> Result<McpConnectorView, BootstrapError> {
        let response: McpConnectorDetailResponse = self.post_json(
            "/v1/mcp/connectors",
            &McpConnectorCreateRequest {
                id: id.to_string(),
                options,
            },
        )?;
        Ok(response.connector)
    }

    pub fn update_mcp_connector(
        &self,
        id: &str,
        patch: McpConnectorUpdatePatch,
    ) -> Result<McpConnectorView, BootstrapError> {
        let response: McpConnectorDetailResponse = self.patch_json(
            &format!("/v1/mcp/connectors/{id}"),
            &McpConnectorUpdateRequest { patch },
        )?;
        Ok(response.connector)
    }

    pub fn restart_mcp_connector(&self, id: &str) -> Result<McpConnectorView, BootstrapError> {
        let response: McpConnectorDetailResponse =
            self.post_json(&format!("/v1/mcp/connectors/{id}/restart"), &())?;
        Ok(response.connector)
    }

    pub fn delete_mcp_connector(&self, id: &str) -> Result<(), BootstrapError> {
        let _: serde_json::Value = self.delete_json(&format!("/v1/mcp/connectors/{id}"))?;
        Ok(())
    }
}
