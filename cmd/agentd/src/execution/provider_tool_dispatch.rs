use agent_persistence::PersistenceStore;
use agent_runtime::provider::{ProviderDriver, ProviderToolCall};
use agent_runtime::tool::{
    ToolCall, ToolCatalog, ToolDefinition, ToolFamily, ToolName, ToolPolicy,
};

use super::{ExecutionError, ExecutionService};

#[derive(Clone, Copy)]
pub(super) struct ProviderToolExecutionContext<'a> {
    pub(super) store: &'a PersistenceStore,
    pub(super) provider: &'a dyn ProviderDriver,
    pub(super) session_id: &'a str,
    pub(super) run_id: &'a str,
    pub(super) now: i64,
}

#[derive(Clone, Copy)]
pub(super) struct ModelToolExecutionContext<'a> {
    pub(super) store: &'a PersistenceStore,
    pub(super) provider: Option<&'a dyn ProviderDriver>,
    pub(super) session_id: &'a str,
    pub(super) run_id: &'a str,
    pub(super) now: i64,
}

pub(super) struct ProviderToolCallInvocation<'a> {
    pub(super) tool_call_id: &'a str,
    pub(super) arguments_json: &'a str,
    pub(super) parsed: &'a ToolCall,
}

impl ExecutionService {
    pub(super) fn resolve_provider_tool_call(
        &self,
        catalog: &ToolCatalog,
        tool_call: &ProviderToolCall,
    ) -> Result<(ToolCall, ToolDefinition), ExecutionError> {
        let parsed = ToolCall::from_openai_function(&tool_call.name, &tool_call.arguments)
            .map_err(|source| ExecutionError::ToolCallParse {
                name: tool_call.name.clone(),
                reason: source.to_string(),
            })?;
        if let ToolCall::McpCall(input) = &parsed {
            let discovered = self
                .mcp
                .list_discovered_tools()
                .into_iter()
                .find(|tool| tool.exposed_name == input.exposed_name)
                .ok_or_else(|| ExecutionError::ToolCallParse {
                    name: tool_call.name.clone(),
                    reason: format!("unknown MCP tool {}", input.exposed_name),
                })?;
            return Ok((
                parsed,
                ToolDefinition {
                    name: ToolName::McpCall,
                    family: ToolFamily::Mcp,
                    description: "invoke a discovered MCP tool",
                    policy: ToolPolicy {
                        read_only: discovered.read_only,
                        destructive: discovered.destructive,
                        requires_approval: discovered.destructive || !discovered.read_only,
                    },
                },
            ));
        }
        let definition = catalog
            .definition_for_call(&parsed)
            .ok_or_else(|| ExecutionError::ToolCallParse {
                name: tool_call.name.clone(),
                reason: "tool is not in the catalog".to_string(),
            })?
            .clone();
        Ok((parsed, definition))
    }
}
