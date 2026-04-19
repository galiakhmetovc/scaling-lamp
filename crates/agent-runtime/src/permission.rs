use crate::tool::{ToolCall, ToolDefinition, ToolFamily};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    #[default]
    Default,
    AcceptEdits,
    Plan,
    Auto,
    BypassPermissions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PermissionAction {
    Allow,
    #[default]
    Ask,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PermissionRule {
    pub action: PermissionAction,
    pub tool: Option<String>,
    pub family: Option<String>,
    pub path_prefix: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PermissionConfig {
    pub mode: PermissionMode,
    pub rules: Vec<PermissionRule>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionResolution {
    pub action: PermissionAction,
    pub reason: String,
}

impl PermissionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::AcceptEdits => "accept_edits",
            Self::Plan => "plan",
            Self::Auto => "auto",
            Self::BypassPermissions => "bypass_permissions",
        }
    }
}

impl TryFrom<&str> for PermissionMode {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "default" => Ok(Self::Default),
            "accept_edits" => Ok(Self::AcceptEdits),
            "plan" => Ok(Self::Plan),
            "auto" => Ok(Self::Auto),
            "bypass_permissions" => Ok(Self::BypassPermissions),
            _ => Err(()),
        }
    }
}

impl PermissionConfig {
    pub fn resolve(&self, definition: &ToolDefinition, call: &ToolCall) -> PermissionResolution {
        if let Some(rule) = self
            .rules
            .iter()
            .find(|rule| rule.matches(definition, call))
        {
            return PermissionResolution {
                action: rule.action,
                reason: format!("matched rule {}", rule.summary()),
            };
        }

        let action = match self.mode {
            PermissionMode::Default => {
                if definition.policy.requires_approval {
                    PermissionAction::Ask
                } else {
                    PermissionAction::Allow
                }
            }
            PermissionMode::AcceptEdits => match definition.family {
                ToolFamily::Filesystem => PermissionAction::Allow,
                _ if definition.policy.requires_approval => PermissionAction::Ask,
                _ => PermissionAction::Allow,
            },
            PermissionMode::Plan => {
                if definition.policy.read_only {
                    PermissionAction::Allow
                } else {
                    PermissionAction::Deny
                }
            }
            PermissionMode::Auto | PermissionMode::BypassPermissions => PermissionAction::Allow,
        };

        PermissionResolution {
            action,
            reason: format!("default mode {}", self.mode.as_str()),
        }
    }
}

impl PermissionRule {
    fn matches(&self, definition: &ToolDefinition, call: &ToolCall) -> bool {
        let tool_matches = self
            .tool
            .as_deref()
            .map(|tool| tool == definition.name.as_str())
            .unwrap_or(true);
        let family_matches = self
            .family
            .as_deref()
            .map(|family| family == definition.family.as_str())
            .unwrap_or(true);
        let path_matches = self
            .path_prefix
            .as_deref()
            .map(|prefix| {
                call.scope_target()
                    .as_deref()
                    .map(|target| target == prefix || target.starts_with(prefix))
                    .unwrap_or(false)
            })
            .unwrap_or(true);

        tool_matches && family_matches && path_matches
    }

    fn summary(&self) -> String {
        let mut parts = Vec::new();
        if let Some(tool) = &self.tool {
            parts.push(format!("tool={tool}"));
        }
        if let Some(family) = &self.family {
            parts.push(format!("family={family}"));
        }
        if let Some(path_prefix) = &self.path_prefix {
            parts.push(format!("path_prefix={path_prefix}"));
        }
        if parts.is_empty() {
            "global".to_string()
        } else {
            parts.join(",")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{PermissionAction, PermissionConfig, PermissionMode, PermissionRule};
    use crate::tool::{
        ExecStartInput, FsPatchEdit, FsPatchInput, FsReadInput, FsWriteInput, ToolCall,
        ToolCatalog, ToolName,
    };

    #[test]
    fn default_mode_asks_for_tools_marked_with_approval() {
        let config = PermissionConfig::default();
        let catalog = ToolCatalog::default();
        let decision = config.resolve(
            catalog.definition(ToolName::FsWrite).expect("fs_write"),
            &ToolCall::FsWrite(FsWriteInput {
                path: "notes/out.txt".to_string(),
                content: "hello".to_string(),
            }),
        );

        assert_eq!(decision.action, PermissionAction::Ask);
        assert_eq!(decision.reason, "default mode default");
    }

    #[test]
    fn accept_edits_allows_filesystem_writes_but_still_asks_for_exec() {
        let config = PermissionConfig {
            mode: PermissionMode::AcceptEdits,
            rules: Vec::new(),
        };
        let catalog = ToolCatalog::default();

        let write = config.resolve(
            catalog.definition(ToolName::FsPatch).expect("fs_patch"),
            &ToolCall::FsPatch(FsPatchInput {
                path: "src/main.rs".to_string(),
                edits: vec![FsPatchEdit {
                    old: "old".to_string(),
                    new: "new".to_string(),
                    replace_all: false,
                }],
            }),
        );
        let exec = config.resolve(
            catalog.definition(ToolName::ExecStart).expect("exec_start"),
            &ToolCall::ExecStart(ExecStartInput {
                executable: "/bin/echo".to_string(),
                args: vec!["hello".to_string()],
                cwd: None,
            }),
        );

        assert_eq!(write.action, PermissionAction::Allow);
        assert_eq!(exec.action, PermissionAction::Ask);
    }

    #[test]
    fn plan_mode_denies_non_readonly_tools() {
        let config = PermissionConfig {
            mode: PermissionMode::Plan,
            rules: Vec::new(),
        };
        let catalog = ToolCatalog::default();

        let read = config.resolve(
            catalog.definition(ToolName::FsRead).expect("fs_read"),
            &ToolCall::FsRead(FsReadInput {
                path: "docs/readme.md".to_string(),
            }),
        );
        let write = config.resolve(
            catalog.definition(ToolName::FsWrite).expect("fs_write"),
            &ToolCall::FsWrite(FsWriteInput {
                path: "docs/readme.md".to_string(),
                content: "updated".to_string(),
            }),
        );

        assert_eq!(read.action, PermissionAction::Allow);
        assert_eq!(write.action, PermissionAction::Deny);
    }

    #[test]
    fn explicit_rule_overrides_default_mode_for_matching_paths() {
        let config = PermissionConfig {
            mode: PermissionMode::Plan,
            rules: vec![PermissionRule {
                action: PermissionAction::Allow,
                tool: Some("fs_write".to_string()),
                family: None,
                path_prefix: Some("notes/".to_string()),
            }],
        };
        let catalog = ToolCatalog::default();

        let allowed = config.resolve(
            catalog.definition(ToolName::FsWrite).expect("fs_write"),
            &ToolCall::FsWrite(FsWriteInput {
                path: "notes/out.txt".to_string(),
                content: "ok".to_string(),
            }),
        );
        let denied = config.resolve(
            catalog.definition(ToolName::FsWrite).expect("fs_write"),
            &ToolCall::FsWrite(FsWriteInput {
                path: "secrets/out.txt".to_string(),
                content: "no".to_string(),
            }),
        );

        assert_eq!(allowed.action, PermissionAction::Allow);
        assert!(allowed.reason.contains("path_prefix=notes/"));
        assert_eq!(denied.action, PermissionAction::Deny);
    }
}
