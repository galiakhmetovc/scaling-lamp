use super::*;

impl McpRepository for PersistenceStore {
    fn put_mcp_connector(&self, record: &McpConnectorRecord) -> Result<(), StoreError> {
        validate_identifier(&record.id)?;
        self.with_client(|client| {
            client.execute(
                "INSERT INTO mcp_connectors (
                    id, transport, command, args_json, env_json, cwd, enabled, created_at, updated_at
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                 ON CONFLICT(id) DO UPDATE SET
                    transport = excluded.transport,
                    command = excluded.command,
                    args_json = excluded.args_json,
                    env_json = excluded.env_json,
                    cwd = excluded.cwd,
                    enabled = excluded.enabled,
                    created_at = excluded.created_at,
                    updated_at = excluded.updated_at",
                &[
                    &record.id,
                    &record.transport,
                    &record.command,
                    &record.args_json,
                    &record.env_json,
                    &record.cwd,
                    &record.enabled,
                    &record.created_at,
                    &record.updated_at,
                ],
            )?;
            Ok(())
        })
    }

    fn get_mcp_connector(&self, id: &str) -> Result<Option<McpConnectorRecord>, StoreError> {
        self.with_client(|client| {
            client
                .query_opt(
                    "SELECT id, transport, command, args_json, env_json, cwd, enabled, created_at, updated_at
                     FROM mcp_connectors
                     WHERE id = $1",
                    &[&id],
                )
                .map(|row| row.map(|row| mcp_connector_from_row(&row)))
                .map_err(StoreError::from)
        })
    }

    fn list_mcp_connectors(&self) -> Result<Vec<McpConnectorRecord>, StoreError> {
        self.with_client(|client| {
            client
                .query(
                    "SELECT id, transport, command, args_json, env_json, cwd, enabled, created_at, updated_at
                     FROM mcp_connectors
                     ORDER BY created_at ASC, id ASC",
                    &[],
                )
                .map(|rows| rows.iter().map(mcp_connector_from_row).collect())
                .map_err(StoreError::from)
        })
    }

    fn delete_mcp_connector(&self, id: &str) -> Result<bool, StoreError> {
        self.with_client(|client| {
            client
                .execute("DELETE FROM mcp_connectors WHERE id = $1", &[&id])
                .map(|affected| affected > 0)
                .map_err(StoreError::from)
        })
    }
}

fn mcp_connector_from_row(row: &Row) -> McpConnectorRecord {
    McpConnectorRecord {
        id: row.get(0),
        transport: row.get(1),
        command: row.get(2),
        args_json: row.get(3),
        env_json: row.get(4),
        cwd: row.get(5),
        enabled: row.get(6),
        created_at: row.get(7),
        updated_at: row.get(8),
    }
}
