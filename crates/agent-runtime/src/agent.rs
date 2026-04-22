use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentTemplateKind {
    Default,
    Judge,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentTemplateKindParseError {
    value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentProfile {
    pub id: String,
    pub name: String,
    pub template_kind: AgentTemplateKind,
    pub agent_home: PathBuf,
    pub allowed_tools: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentProfileError {
    EmptyId,
    EmptyName,
    EmptyAgentHome,
    EmptyAllowedTool { index: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSchedule {
    pub id: String,
    pub agent_profile_id: String,
    pub workspace_root: PathBuf,
    pub prompt: String,
    pub interval_seconds: u64,
    pub next_fire_at: i64,
    pub last_triggered_at: Option<i64>,
    pub last_session_id: Option<String>,
    pub last_job_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentScheduleError {
    EmptyId,
    EmptyAgentProfileId,
    EmptyWorkspaceRoot,
    EmptyPrompt,
    ZeroIntervalSeconds,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentChainContinuationGrant {
    pub chain_id: String,
    pub reason: String,
    pub granted_hops: u32,
    pub granted_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentChainContinuationError {
    EmptyChainId,
    EmptyReason,
}

impl AgentTemplateKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Judge => "judge",
            Self::Custom => "custom",
        }
    }
}

impl TryFrom<&str> for AgentTemplateKind {
    type Error = AgentTemplateKindParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "default" => Ok(Self::Default),
            "judge" => Ok(Self::Judge),
            "custom" => Ok(Self::Custom),
            other => Err(AgentTemplateKindParseError {
                value: other.to_string(),
            }),
        }
    }
}

impl fmt::Display for AgentTemplateKindParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid agent template kind {}", self.value)
    }
}

impl Error for AgentTemplateKindParseError {}

impl AgentProfile {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        template_kind: AgentTemplateKind,
        agent_home: impl AsRef<Path>,
        allowed_tools: Vec<String>,
        created_at: i64,
        updated_at: i64,
    ) -> Result<Self, AgentProfileError> {
        let id = id.into().trim().to_string();
        if id.is_empty() {
            return Err(AgentProfileError::EmptyId);
        }

        let name = name.into().trim().to_string();
        if name.is_empty() {
            return Err(AgentProfileError::EmptyName);
        }

        let agent_home = agent_home.as_ref().to_path_buf();
        if agent_home.as_os_str().is_empty() {
            return Err(AgentProfileError::EmptyAgentHome);
        }

        let mut normalized_tools = Vec::with_capacity(allowed_tools.len());
        for (index, tool_id) in allowed_tools.into_iter().enumerate() {
            let tool_id = tool_id.trim().to_string();
            if tool_id.is_empty() {
                return Err(AgentProfileError::EmptyAllowedTool { index });
            }
            normalized_tools.push(tool_id);
        }
        normalized_tools.sort();
        normalized_tools.dedup();

        Ok(Self {
            id,
            name,
            template_kind,
            agent_home,
            allowed_tools: normalized_tools,
            created_at,
            updated_at,
        })
    }

    pub fn allows_tool_id(&self, tool_id: &str) -> bool {
        self.allowed_tools
            .binary_search_by(|candidate| candidate.as_str().cmp(tool_id))
            .is_ok()
    }
}

impl AgentSchedule {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<String>,
        agent_profile_id: impl Into<String>,
        workspace_root: impl AsRef<Path>,
        prompt: impl Into<String>,
        interval_seconds: u64,
        next_fire_at: i64,
        last_triggered_at: Option<i64>,
        last_session_id: Option<String>,
        last_job_id: Option<String>,
        created_at: i64,
        updated_at: i64,
    ) -> Result<Self, AgentScheduleError> {
        let id = id.into().trim().to_string();
        if id.is_empty() {
            return Err(AgentScheduleError::EmptyId);
        }

        let agent_profile_id = agent_profile_id.into().trim().to_string();
        if agent_profile_id.is_empty() {
            return Err(AgentScheduleError::EmptyAgentProfileId);
        }

        let workspace_root = workspace_root.as_ref().to_path_buf();
        if workspace_root.as_os_str().is_empty() {
            return Err(AgentScheduleError::EmptyWorkspaceRoot);
        }

        let prompt = prompt.into().trim().to_string();
        if prompt.is_empty() {
            return Err(AgentScheduleError::EmptyPrompt);
        }

        if interval_seconds == 0 {
            return Err(AgentScheduleError::ZeroIntervalSeconds);
        }

        Ok(Self {
            id,
            agent_profile_id,
            workspace_root,
            prompt,
            interval_seconds,
            next_fire_at,
            last_triggered_at,
            last_session_id,
            last_job_id,
            created_at,
            updated_at,
        })
    }

    pub fn is_due(&self, now: i64) -> bool {
        now >= self.next_fire_at
    }
}

impl AgentChainContinuationGrant {
    pub const DEFAULT_GRANTED_HOPS: u32 = 1;

    pub fn new(
        chain_id: impl Into<String>,
        reason: impl Into<String>,
        granted_at: i64,
    ) -> Result<Self, AgentChainContinuationError> {
        let chain_id = chain_id.into().trim().to_string();
        if chain_id.is_empty() {
            return Err(AgentChainContinuationError::EmptyChainId);
        }

        let reason = reason.into().trim().to_string();
        if reason.is_empty() {
            return Err(AgentChainContinuationError::EmptyReason);
        }

        Ok(Self {
            chain_id,
            reason,
            granted_hops: Self::DEFAULT_GRANTED_HOPS,
            granted_at,
        })
    }
}

impl fmt::Display for AgentProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyId => write!(formatter, "agent id must not be empty"),
            Self::EmptyName => write!(formatter, "agent name must not be empty"),
            Self::EmptyAgentHome => write!(formatter, "agent home must not be empty"),
            Self::EmptyAllowedTool { index } => {
                write!(
                    formatter,
                    "agent allowed tool at index {index} must not be empty"
                )
            }
        }
    }
}

impl Error for AgentProfileError {}

impl fmt::Display for AgentScheduleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyId => write!(formatter, "agent schedule id must not be empty"),
            Self::EmptyAgentProfileId => {
                write!(
                    formatter,
                    "agent schedule agent profile id must not be empty"
                )
            }
            Self::EmptyWorkspaceRoot => {
                write!(formatter, "agent schedule workspace root must not be empty")
            }
            Self::EmptyPrompt => write!(formatter, "agent schedule prompt must not be empty"),
            Self::ZeroIntervalSeconds => {
                write!(
                    formatter,
                    "agent schedule interval_seconds must be greater than zero"
                )
            }
        }
    }
}

impl Error for AgentScheduleError {}

impl fmt::Display for AgentChainContinuationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyChainId => write!(formatter, "agent chain id must not be empty"),
            Self::EmptyReason => write!(
                formatter,
                "agent chain continuation reason must not be empty"
            ),
        }
    }
}

impl Error for AgentChainContinuationError {}

#[cfg(test)]
mod tests {
    use super::{
        AgentChainContinuationGrant, AgentProfile, AgentProfileError, AgentSchedule,
        AgentScheduleError, AgentTemplateKind,
    };
    use std::path::PathBuf;

    #[test]
    fn agent_profile_normalizes_allowed_tools() {
        let profile = AgentProfile::new(
            "judge",
            "Judge",
            AgentTemplateKind::Judge,
            PathBuf::from("/tmp/judge"),
            vec![
                " fs_read_text ".to_string(),
                "plan_snapshot".to_string(),
                "fs_read_text".to_string(),
            ],
            1,
            2,
        )
        .expect("profile");

        assert_eq!(
            profile.allowed_tools,
            vec!["fs_read_text".to_string(), "plan_snapshot".to_string()]
        );
    }

    #[test]
    fn agent_profile_rejects_blank_allowed_tool() {
        let error = AgentProfile::new(
            "judge",
            "Judge",
            AgentTemplateKind::Judge,
            PathBuf::from("/tmp/judge"),
            vec!["   ".to_string()],
            1,
            2,
        )
        .expect_err("blank tool should fail");

        assert_eq!(error, AgentProfileError::EmptyAllowedTool { index: 0 });
    }

    #[test]
    fn agent_profile_allows_tool_id_from_normalized_allowlist() {
        let profile = AgentProfile::new(
            "judge",
            "Judge",
            AgentTemplateKind::Judge,
            PathBuf::from("/tmp/judge"),
            vec!["plan_snapshot".to_string(), "fs_read_text".to_string()],
            1,
            2,
        )
        .expect("profile");

        assert!(profile.allows_tool_id("fs_read_text"));
        assert!(profile.allows_tool_id("plan_snapshot"));
        assert!(!profile.allows_tool_id("exec_start"));
    }

    #[test]
    fn chain_continuation_defaults_to_single_extra_hop() {
        let grant =
            AgentChainContinuationGrant::new("chain-1", "judge approved", 3).expect("grant");

        assert_eq!(grant.granted_hops, 1);
    }

    #[test]
    fn agent_schedule_rejects_zero_interval_seconds() {
        let error = AgentSchedule::new(
            "judge-pulse",
            "judge",
            PathBuf::from("/tmp/project"),
            "check latest changes",
            0,
            10,
            None,
            None,
            None,
            1,
            1,
        )
        .expect_err("zero interval should fail");

        assert_eq!(error, AgentScheduleError::ZeroIntervalSeconds);
    }

    #[test]
    fn agent_schedule_is_due_when_next_fire_at_has_arrived() {
        let schedule = AgentSchedule::new(
            "judge-pulse",
            "judge",
            PathBuf::from("/tmp/project"),
            "check latest changes",
            300,
            10,
            None,
            None,
            None,
            1,
            1,
        )
        .expect("schedule");

        assert!(!schedule.is_due(9));
        assert!(schedule.is_due(10));
        assert!(schedule.is_due(11));
    }
}
