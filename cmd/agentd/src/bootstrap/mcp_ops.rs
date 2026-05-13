use super::{App, BootstrapError, unix_timestamp};
use crate::mcp::{
    McpConnectorRuntimeStatus, McpDiscoveredPrompt, McpDiscoveredPromptArgument,
    McpDiscoveredResource,
};
use agent_persistence::{McpConnectorRecord, McpRepository};
use agent_runtime::mcp::{McpConnectorConfig, McpConnectorTransport};
use agent_runtime::tool::{McpGetPromptOutput, McpReadResourceOutput};
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpResourceView {
    pub connector_id: String,
    pub uri: String,
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpResourceListView {
    pub connector_id: Option<String>,
    pub query: Option<String>,
    pub results: Vec<McpResourceView>,
    pub truncated: bool,
    pub offset: usize,
    pub limit: usize,
    pub total_results: usize,
    pub next_offset: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpPromptArgumentView {
    pub name: String,
    pub description: Option<String>,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpPromptView {
    pub connector_id: String,
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub arguments: Vec<McpPromptArgumentView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpPromptListView {
    pub connector_id: Option<String>,
    pub query: Option<String>,
    pub results: Vec<McpPromptView>,
    pub truncated: bool,
    pub offset: usize,
    pub limit: usize,
    pub total_results: usize,
    pub next_offset: Option<usize>,
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

    pub fn list_mcp_resources(
        &self,
        connector_id: Option<&str>,
        query: Option<&str>,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> McpResourceListView {
        let query = normalized_optional_query(query);
        let query_lower = query.as_ref().map(|value| value.to_ascii_lowercase());
        let mut results = self
            .mcp
            .list_discovered_resources(connector_id)
            .into_iter()
            .filter(|resource| {
                query_lower.as_ref().is_none_or(|needle| {
                    resource.uri.to_ascii_lowercase().contains(needle)
                        || resource.name.to_ascii_lowercase().contains(needle)
                        || resource
                            .title
                            .as_ref()
                            .is_some_and(|value| value.to_ascii_lowercase().contains(needle))
                        || resource
                            .description
                            .as_ref()
                            .is_some_and(|value| value.to_ascii_lowercase().contains(needle))
                        || resource
                            .mime_type
                            .as_ref()
                            .is_some_and(|value| value.to_ascii_lowercase().contains(needle))
                })
            })
            .map(McpResourceView::from)
            .collect::<Vec<_>>();
        results.sort_by(|left, right| {
            left.connector_id
                .cmp(&right.connector_id)
                .then_with(|| left.uri.cmp(&right.uri))
        });
        let (offset, limit, next_offset) = self.mcp_pagination(results.len(), limit, offset);
        let end = offset.saturating_add(limit).min(results.len());
        let page = results[offset..end].to_vec();
        McpResourceListView {
            connector_id: connector_id.map(str::to_string),
            query,
            results: page,
            truncated: next_offset.is_some(),
            offset,
            limit,
            total_results: results.len(),
            next_offset,
        }
    }

    pub fn read_mcp_resource(
        &self,
        connector_id: &str,
        uri: &str,
    ) -> Result<McpReadResourceOutput, BootstrapError> {
        self.mcp
            .read_resource(connector_id, uri)
            .map_err(|error| BootstrapError::Stream(std::io::Error::other(error)))
    }

    pub fn list_mcp_prompts(
        &self,
        connector_id: Option<&str>,
        query: Option<&str>,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> McpPromptListView {
        let query = normalized_optional_query(query);
        let query_lower = query.as_ref().map(|value| value.to_ascii_lowercase());
        let mut results = self
            .mcp
            .list_discovered_prompts(connector_id)
            .into_iter()
            .filter(|prompt| {
                query_lower.as_ref().is_none_or(|needle| {
                    prompt.name.to_ascii_lowercase().contains(needle)
                        || prompt
                            .title
                            .as_ref()
                            .is_some_and(|value| value.to_ascii_lowercase().contains(needle))
                        || prompt
                            .description
                            .as_ref()
                            .is_some_and(|value| value.to_ascii_lowercase().contains(needle))
                        || prompt.arguments.iter().any(|argument| {
                            argument.name.to_ascii_lowercase().contains(needle)
                                || argument.description.as_ref().is_some_and(|value| {
                                    value.to_ascii_lowercase().contains(needle)
                                })
                        })
                })
            })
            .map(McpPromptView::from)
            .collect::<Vec<_>>();
        results.sort_by(|left, right| {
            left.connector_id
                .cmp(&right.connector_id)
                .then_with(|| left.name.cmp(&right.name))
        });
        let (offset, limit, next_offset) = self.mcp_pagination(results.len(), limit, offset);
        let end = offset.saturating_add(limit).min(results.len());
        let page = results[offset..end].to_vec();
        McpPromptListView {
            connector_id: connector_id.map(str::to_string),
            query,
            results: page,
            truncated: next_offset.is_some(),
            offset,
            limit,
            total_results: results.len(),
            next_offset,
        }
    }

    pub fn get_mcp_prompt(
        &self,
        connector_id: &str,
        name: &str,
        arguments: Option<BTreeMap<String, String>>,
    ) -> Result<McpGetPromptOutput, BootstrapError> {
        self.mcp
            .get_prompt(connector_id, name, arguments)
            .map_err(|error| BootstrapError::Stream(std::io::Error::other(error)))
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

    fn mcp_pagination(
        &self,
        total: usize,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> (usize, usize, Option<usize>) {
        let offset = offset.unwrap_or(0);
        let limit = limit
            .unwrap_or(self.config.runtime_limits.mcp_search_default_limit)
            .clamp(1, self.config.runtime_limits.mcp_search_max_limit);
        let offset = offset.min(total);
        let next = offset.saturating_add(limit);
        let next_offset = (next < total).then_some(next);
        (offset, limit, next_offset)
    }
}

impl From<McpDiscoveredResource> for McpResourceView {
    fn from(resource: McpDiscoveredResource) -> Self {
        Self {
            connector_id: resource.connector_id,
            uri: resource.uri,
            name: resource.name,
            title: resource.title,
            description: resource.description,
            mime_type: resource.mime_type,
        }
    }
}

impl From<McpDiscoveredPromptArgument> for McpPromptArgumentView {
    fn from(argument: McpDiscoveredPromptArgument) -> Self {
        Self {
            name: argument.name,
            description: argument.description,
            required: argument.required,
        }
    }
}

impl From<McpDiscoveredPrompt> for McpPromptView {
    fn from(prompt: McpDiscoveredPrompt) -> Self {
        Self {
            connector_id: prompt.connector_id,
            name: prompt.name,
            title: prompt.title,
            description: prompt.description,
            arguments: prompt
                .arguments
                .into_iter()
                .map(McpPromptArgumentView::from)
                .collect(),
        }
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

fn normalized_optional_query(query: Option<&str>) -> Option<String> {
    query
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}
