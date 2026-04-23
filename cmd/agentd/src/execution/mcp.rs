use super::*;
use agent_runtime::mcp::McpConnectorConfig;

impl ExecutionService {
    pub fn maintain_mcp_connectors(
        &self,
        store: &PersistenceStore,
        now: i64,
    ) -> Result<(), ExecutionError> {
        let connectors = store
            .list_mcp_connectors()
            .map_err(ExecutionError::Store)?
            .into_iter()
            .map(McpConnectorConfig::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ExecutionError::RecordConversion)?;

        for connector in connectors {
            self.mcp.ensure_placeholder(&connector.id);
            if connector.enabled {
                let _ = self.mcp.ensure_started(&connector, now);
            } else {
                self.mcp.ensure_stopped(&connector.id, now);
            }
        }

        Ok(())
    }
}
