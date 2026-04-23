use super::{App, BootstrapError, unix_timestamp};
use crate::mcp::McpConnectorRuntimeStatus;
use agent_persistence::{McpConnectorRecord, McpRepository};
use agent_runtime::mcp::{McpConnectorConfig, McpConnectorTransport};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpConnectorCreateOptions {
    pub transport: McpConnectorTransport,
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub cwd: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpConnectorUpdatePatch {
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub env: Option<BTreeMap<String, String>>,
    pub cwd: Option<Option<String>>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpConnectorView {
    pub id: String,
    pub transport: McpConnectorTransport,
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub cwd: Option<String>,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
    pub runtime: McpConnectorRuntimeStatus,
}

impl App {
    pub(crate) fn ensure_mcp_connectors_bootstrapped(&self) -> Result<(), BootstrapError> {
        let store = self.store()?;
        let now = unix_timestamp()?;

        for (id, seed) in &self.config.daemon.mcp_connectors {
            let created_at = store
                .get_mcp_connector(id)?
                .map(|record| record.created_at)
                .unwrap_or(now);
            let connector = McpConnectorConfig {
                id: id.clone(),
                transport: seed.transport,
                command: seed.command.clone(),
                args: seed.args.clone(),
                env: seed.env.clone(),
                cwd: seed.cwd.as_ref().map(|path| path.display().to_string()),
                enabled: seed.enabled,
                created_at,
                updated_at: now,
            };
            store.put_mcp_connector(
                &McpConnectorRecord::try_from(&connector)
                    .map_err(BootstrapError::RecordConversion)?,
            )?;
            self.mcp.ensure_placeholder(id);
        }

        for record in store.list_mcp_connectors()? {
            self.mcp.ensure_placeholder(&record.id);
        }

        Ok(())
    }

    pub fn list_mcp_connectors(&self) -> Result<Vec<McpConnectorView>, BootstrapError> {
        let store = self.store()?;
        let mut connectors = store
            .list_mcp_connectors()?
            .into_iter()
            .map(|record| self.build_mcp_connector_view(record))
            .collect::<Result<Vec<_>, _>>()?;
        connectors.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(connectors)
    }

    pub fn mcp_connector(&self, id: &str) -> Result<McpConnectorView, BootstrapError> {
        let store = self.store()?;
        let record = store
            .get_mcp_connector(id)?
            .ok_or_else(|| BootstrapError::MissingRecord {
                kind: "mcp connector",
                id: id.to_string(),
            })?;
        self.build_mcp_connector_view(record)
    }

    pub fn create_mcp_connector(
        &self,
        id: &str,
        options: McpConnectorCreateOptions,
    ) -> Result<McpConnectorView, BootstrapError> {
        if options.command.trim().is_empty() {
            return Err(BootstrapError::Usage {
                reason: "mcp connector command must not be blank".to_string(),
            });
        }
        let store = self.store()?;
        let now = unix_timestamp()?;
        let connector = McpConnectorConfig {
            id: id.to_string(),
            transport: options.transport,
            command: options.command.trim().to_string(),
            args: options.args,
            env: options.env,
            cwd: options.cwd,
            enabled: options.enabled,
            created_at: now,
            updated_at: now,
        };
        store.put_mcp_connector(
            &McpConnectorRecord::try_from(&connector).map_err(BootstrapError::RecordConversion)?,
        )?;
        self.mcp.ensure_placeholder(&connector.id);
        self.mcp_connector(&connector.id)
    }

    pub fn update_mcp_connector(
        &self,
        id: &str,
        patch: McpConnectorUpdatePatch,
    ) -> Result<McpConnectorView, BootstrapError> {
        let store = self.store()?;
        let record = store
            .get_mcp_connector(id)?
            .ok_or_else(|| BootstrapError::MissingRecord {
                kind: "mcp connector",
                id: id.to_string(),
            })?;
        let mut connector =
            McpConnectorConfig::try_from(record).map_err(BootstrapError::RecordConversion)?;

        let mut restart_required = false;

        if let Some(command) = patch.command {
            if command.trim().is_empty() {
                return Err(BootstrapError::Usage {
                    reason: "mcp connector command must not be blank".to_string(),
                });
            }
            restart_required = restart_required || connector.command != command.trim();
            connector.command = command.trim().to_string();
        }
        if let Some(args) = patch.args {
            restart_required = restart_required || connector.args != args;
            connector.args = args;
        }
        if let Some(env) = patch.env {
            restart_required = restart_required || connector.env != env;
            connector.env = env;
        }
        if let Some(cwd) = patch.cwd {
            restart_required = restart_required || connector.cwd != cwd;
            connector.cwd = cwd;
        }
        if let Some(enabled) = patch.enabled {
            restart_required = restart_required || connector.enabled != enabled;
            connector.enabled = enabled;
        }
        connector.updated_at = unix_timestamp()?;

        store.put_mcp_connector(
            &McpConnectorRecord::try_from(&connector).map_err(BootstrapError::RecordConversion)?,
        )?;
        self.mcp.ensure_placeholder(&connector.id);
        if restart_required {
            self.mcp.ensure_stopped(&connector.id, connector.updated_at);
        }
        self.mcp_connector(&connector.id)
    }

    pub fn set_mcp_connector_enabled(
        &self,
        id: &str,
        enabled: bool,
    ) -> Result<McpConnectorView, BootstrapError> {
        self.update_mcp_connector(
            id,
            McpConnectorUpdatePatch {
                enabled: Some(enabled),
                ..McpConnectorUpdatePatch::default()
            },
        )
    }

    pub fn restart_mcp_connector(&self, id: &str) -> Result<McpConnectorView, BootstrapError> {
        let store = self.store()?;
        let record = store
            .get_mcp_connector(id)?
            .ok_or_else(|| BootstrapError::MissingRecord {
                kind: "mcp connector",
                id: id.to_string(),
            })?;
        let connector =
            McpConnectorConfig::try_from(record).map_err(BootstrapError::RecordConversion)?;
        let now = unix_timestamp()?;

        self.mcp.ensure_placeholder(id);
        self.mcp.ensure_stopped(id, now);
        if connector.enabled {
            self.mcp
                .ensure_started(&connector, now)
                .map_err(|error| BootstrapError::Stream(std::io::Error::other(error)))?;
        }
        self.mcp_connector(id)
    }

    pub fn delete_mcp_connector(&self, id: &str) -> Result<bool, BootstrapError> {
        let store = self.store()?;
        let deleted = store.delete_mcp_connector(id)?;
        if deleted {
            self.mcp.remove(id);
        }
        Ok(deleted)
    }

    pub fn render_mcp_connectors(&self) -> Result<String, BootstrapError> {
        Ok(render_mcp_connectors_view(&self.list_mcp_connectors()?))
    }

    pub fn render_mcp_connector(&self, id: &str) -> Result<String, BootstrapError> {
        Ok(render_mcp_connector_view(&self.mcp_connector(id)?))
    }

    fn build_mcp_connector_view(
        &self,
        record: McpConnectorRecord,
    ) -> Result<McpConnectorView, BootstrapError> {
        let connector =
            McpConnectorConfig::try_from(record).map_err(BootstrapError::RecordConversion)?;
        Ok(McpConnectorView {
            id: connector.id.clone(),
            transport: connector.transport,
            command: connector.command,
            args: connector.args,
            env: connector.env,
            cwd: connector.cwd,
            enabled: connector.enabled,
            created_at: connector.created_at,
            updated_at: connector.updated_at,
            runtime: self.mcp.status(&connector.id),
        })
    }
}

pub(crate) fn render_mcp_connectors_view(connectors: &[McpConnectorView]) -> String {
    if connectors.is_empty() {
        return "MCP коннекторы: ничего не настроено".to_string();
    }

    let mut lines = vec!["MCP коннекторы:".to_string()];
    for connector in connectors {
        let args = render_mcp_args(&connector.args);
        let env = render_mcp_env(&connector.env);
        let cwd = connector.cwd.as_deref().unwrap_or("<none>");
        let pid = connector
            .runtime
            .pid
            .map(|value| value.to_string())
            .unwrap_or_else(|| "<none>".to_string());
        lines.push(format!(
            "- {} transport={} enabled={} state={} pid={} restarts={} command={} args={} cwd={}",
            connector.id,
            connector.transport.as_str(),
            yes_no(connector.enabled),
            connector.runtime.state.as_str(),
            pid,
            connector.runtime.restart_count,
            connector.command,
            args,
            cwd
        ));
        if env != "<none>" {
            lines.push(format!("  env={env}"));
        }
        if let Some(error) = connector.runtime.last_error.as_deref() {
            lines.push(format!("  last_error={error}"));
        }
    }
    lines.join("\n")
}

pub(crate) fn render_mcp_connector_view(connector: &McpConnectorView) -> String {
    let mut lines = vec![
        format!("id={}", connector.id),
        format!("transport={}", connector.transport.as_str()),
        format!("enabled={}", connector.enabled),
        format!("state={}", connector.runtime.state.as_str()),
        format!(
            "pid={}",
            connector
                .runtime
                .pid
                .map(|value| value.to_string())
                .unwrap_or_else(|| "<none>".to_string())
        ),
        format!(
            "started_at={}",
            connector
                .runtime
                .started_at
                .map(|value| value.to_string())
                .unwrap_or_else(|| "<none>".to_string())
        ),
        format!(
            "stopped_at={}",
            connector
                .runtime
                .stopped_at
                .map(|value| value.to_string())
                .unwrap_or_else(|| "<none>".to_string())
        ),
        format!("restart_count={}", connector.runtime.restart_count),
        format!(
            "last_error={}",
            connector.runtime.last_error.as_deref().unwrap_or("<none>")
        ),
        format!("command={}", connector.command),
        format!("args={}", render_mcp_args(&connector.args)),
        format!("cwd={}", connector.cwd.as_deref().unwrap_or("<none>")),
        format!("env={}", render_mcp_env(&connector.env)),
        format!("created_at={}", connector.created_at),
        format!("updated_at={}", connector.updated_at),
    ];
    lines.shrink_to_fit();
    lines.join("\n")
}

fn render_mcp_args(args: &[String]) -> String {
    if args.is_empty() {
        "<none>".to_string()
    } else {
        args.join(",")
    }
}

fn render_mcp_env(env: &BTreeMap<String, String>) -> String {
    if env.is_empty() {
        "<none>".to_string()
    } else {
        env.iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join(";")
    }
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}
