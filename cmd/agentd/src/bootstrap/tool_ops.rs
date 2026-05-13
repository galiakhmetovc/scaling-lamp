use super::{App, BootstrapError, unix_timestamp};
use agent_runtime::tool::{ToolCatalog, ToolFamily, ToolName};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCatalogView {
    pub generated_at: i64,
    pub tools: Vec<ToolCatalogItemView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCatalogItemView {
    pub id: String,
    pub family: String,
    pub origin: String,
    #[serde(default)]
    pub connector_id: Option<String>,
    #[serde(default)]
    pub remote_name: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    pub description: String,
    pub read_only: bool,
    pub destructive: bool,
    pub requires_approval: bool,
    pub automatic: bool,
    pub available: bool,
    #[serde(default)]
    pub availability_note: Option<String>,
    pub input_schema: serde_json::Value,
}

impl App {
    pub fn tool_catalog(&self) -> Result<ToolCatalogView, BootstrapError> {
        let catalog = ToolCatalog::default();
        let automatic_tool_ids = catalog
            .automatic_model_definitions()
            .into_iter()
            .map(|definition| definition.name.as_str())
            .collect::<BTreeSet<_>>();

        let mut tools = catalog
            .all_definitions()
            .iter()
            .map(|definition| {
                let (available, availability_note) =
                    self.tool_runtime_availability(definition.name, definition.family);
                ToolCatalogItemView {
                    id: definition.name.as_str().to_string(),
                    family: definition.family.as_str().to_string(),
                    origin: "built_in".to_string(),
                    connector_id: None,
                    remote_name: None,
                    title: None,
                    description: definition.description.to_string(),
                    read_only: definition.policy.read_only,
                    destructive: definition.policy.destructive,
                    requires_approval: definition.policy.requires_approval,
                    automatic: automatic_tool_ids.contains(definition.name.as_str()),
                    available,
                    availability_note,
                    input_schema: definition.name.input_schema(),
                }
            })
            .collect::<Vec<_>>();

        tools.extend(self.mcp.list_discovered_tools().into_iter().map(|tool| {
            ToolCatalogItemView {
                id: tool.exposed_name.clone(),
                family: "mcp".to_string(),
                origin: "mcp".to_string(),
                connector_id: connector_id_from_exposed_mcp_tool(tool.exposed_name.as_str()),
                remote_name: Some(tool.remote_name),
                title: tool.title,
                description: tool
                    .description
                    .unwrap_or_else(|| format!("MCP tool {}", tool.exposed_name)),
                read_only: tool.read_only,
                destructive: tool.destructive,
                requires_approval: tool.destructive,
                automatic: true,
                available: true,
                availability_note: None,
                input_schema: tool.input_schema,
            }
        }));

        tools.sort_by(|left, right| {
            left.family
                .cmp(&right.family)
                .then_with(|| left.origin.cmp(&right.origin))
                .then_with(|| left.id.cmp(&right.id))
        });

        Ok(ToolCatalogView {
            generated_at: unix_timestamp()?,
            tools,
        })
    }

    fn tool_runtime_availability(
        &self,
        name: ToolName,
        family: ToolFamily,
    ) -> (bool, Option<String>) {
        if family == ToolFamily::Browser && !self.config.browser.enabled {
            return (
                false,
                Some("browser backend disabled in runtime config".to_string()),
            );
        }
        if name.is_semantic_memory_tool() && !self.config.mem0.enabled {
            return (
                false,
                Some("mem0 semantic memory disabled in runtime config".to_string()),
            );
        }
        (true, None)
    }
}

fn connector_id_from_exposed_mcp_tool(exposed_name: &str) -> Option<String> {
    let tail = exposed_name.strip_prefix("mcp__")?;
    let (connector_id, _) = tail.split_once("__")?;
    if connector_id.is_empty() {
        None
    } else {
        Some(connector_id.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::connector_id_from_exposed_mcp_tool;

    #[test]
    fn connector_id_from_exposed_mcp_tool_parses_sanitized_name() {
        assert_eq!(
            connector_id_from_exposed_mcp_tool("mcp__silverbullet__read_note").as_deref(),
            Some("silverbullet")
        );
        assert_eq!(connector_id_from_exposed_mcp_tool("web_search"), None);
    }
}
