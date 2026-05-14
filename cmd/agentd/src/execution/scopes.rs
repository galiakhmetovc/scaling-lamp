use super::*;
use sha2::{Digest, Sha256};

pub(super) const AGENT_SHARED_SCOPE_ID: &str = "teamd-agent-shared";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RuntimeScope {
    Operator,
    Agent,
    AgentShared,
    Workspace,
    Session,
}

impl RuntimeScope {
    pub(super) fn parse(raw: Option<&str>, tool_family: &str) -> Result<Self, ExecutionError> {
        match raw.map(str::trim).filter(|value| !value.is_empty()) {
            None | Some("workspace") => Ok(Self::Workspace),
            Some("operator") => Ok(Self::Operator),
            Some("agent") => Ok(Self::Agent),
            Some("agent_shared") | Some("shared") => Ok(Self::AgentShared),
            Some("session") => Ok(Self::Session),
            Some(other) => Err(ExecutionError::Tool(ToolError::InvalidMemoryTool {
                reason: format!(
                    "unsupported {tool_family} scope {other}; use operator, agent, agent_shared, workspace, or session"
                ),
            })),
        }
    }

    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Operator => "operator",
            Self::Agent => "agent",
            Self::AgentShared => "agent_shared",
            Self::Workspace => "workspace",
            Self::Session => "session",
        }
    }

    pub(super) fn requires_session_context(self) -> bool {
        matches!(self, Self::Agent | Self::Workspace | Self::Session)
    }
}

pub(super) fn workspace_scope_id(session: &Session) -> String {
    let workspace = session.workspace_root.display().to_string();
    let digest = Sha256::digest(workspace.as_bytes());
    let hex = format!("{digest:x}");
    format!("teamd-workspace-{}", &hex[..16])
}

pub(super) fn kv_namespace_id(
    session: &Session,
    default_operator_id: &str,
    raw_scope: Option<&str>,
) -> Result<(RuntimeScope, String), ExecutionError> {
    kv_namespace_id_for_context(Some(session), default_operator_id, raw_scope)
}

pub(super) fn kv_namespace_id_for_context(
    session: Option<&Session>,
    default_operator_id: &str,
    raw_scope: Option<&str>,
) -> Result<(RuntimeScope, String), ExecutionError> {
    let scope = RuntimeScope::parse(raw_scope, "kv")?;
    if scope.requires_session_context() && session.is_none() {
        return Err(ExecutionError::Tool(ToolError::InvalidMemoryTool {
            reason: format!(
                "kv {} scope requires a session context; use operator or agent_shared for global KV",
                scope.as_str()
            ),
        }));
    }
    let namespace_id = match scope {
        RuntimeScope::Operator => default_operator_id.trim().to_string(),
        RuntimeScope::Agent => session
            .expect("checked session context")
            .agent_profile_id
            .clone(),
        RuntimeScope::AgentShared => AGENT_SHARED_SCOPE_ID.to_string(),
        RuntimeScope::Workspace => workspace_scope_id(session.expect("checked session context")),
        RuntimeScope::Session => session.expect("checked session context").id.clone(),
    };
    if namespace_id.trim().is_empty() {
        return Err(ExecutionError::Tool(ToolError::InvalidMemoryTool {
            reason: format!(
                "kv {} scope resolved to an empty namespace id",
                scope.as_str()
            ),
        }));
    }
    Ok((scope, namespace_id))
}
