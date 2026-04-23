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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentScheduleMode {
    Interval,
    AfterCompletion,
    Once,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentScheduleDeliveryMode {
    FreshSession,
    ExistingSession,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentTemplateKindParseError {
    value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentScheduleModeParseError {
    value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentScheduleDeliveryModeParseError {
    value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentProfile {
    pub id: String,
    pub name: String,
    pub template_kind: AgentTemplateKind,
    pub agent_home: PathBuf,
    pub allowed_tools: Vec<String>,
    pub created_from_template_id: Option<String>,
    pub created_by_session_id: Option<String>,
    pub created_by_agent_profile_id: Option<String>,
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
    pub mode: AgentScheduleMode,
    pub delivery_mode: AgentScheduleDeliveryMode,
    pub target_session_id: Option<String>,
    pub interval_seconds: u64,
    pub next_fire_at: i64,
    pub enabled: bool,
    pub last_triggered_at: Option<i64>,
    pub last_finished_at: Option<i64>,
    pub last_session_id: Option<String>,
    pub last_job_id: Option<String>,
    pub last_result: Option<String>,
    pub last_error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentScheduleInit {
    pub id: String,
    pub agent_profile_id: String,
    pub workspace_root: PathBuf,
    pub prompt: String,
    pub mode: AgentScheduleMode,
    pub delivery_mode: AgentScheduleDeliveryMode,
    pub target_session_id: Option<String>,
    pub interval_seconds: u64,
    pub next_fire_at: i64,
    pub enabled: bool,
    pub last_triggered_at: Option<i64>,
    pub last_finished_at: Option<i64>,
    pub last_session_id: Option<String>,
    pub last_job_id: Option<String>,
    pub last_result: Option<String>,
    pub last_error: Option<String>,
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
    MissingTargetSessionId,
    UnexpectedTargetSessionId,
    EmptyTargetSessionId,
    EmptyLastSessionId,
    EmptyLastJobId,
    EmptyLastResult,
    EmptyLastError,
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

impl AgentScheduleMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Interval => "interval",
            Self::AfterCompletion => "after_completion",
            Self::Once => "once",
        }
    }
}

impl AgentScheduleDeliveryMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FreshSession => "fresh_session",
            Self::ExistingSession => "existing_session",
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

impl TryFrom<&str> for AgentScheduleMode {
    type Error = AgentScheduleModeParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "interval" => Ok(Self::Interval),
            "after_completion" => Ok(Self::AfterCompletion),
            "once" => Ok(Self::Once),
            other => Err(AgentScheduleModeParseError {
                value: other.to_string(),
            }),
        }
    }
}

impl TryFrom<&str> for AgentScheduleDeliveryMode {
    type Error = AgentScheduleDeliveryModeParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "fresh_session" => Ok(Self::FreshSession),
            "existing_session" => Ok(Self::ExistingSession),
            other => Err(AgentScheduleDeliveryModeParseError {
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

impl fmt::Display for AgentScheduleModeParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid agent schedule mode {}", self.value)
    }
}

impl Error for AgentScheduleModeParseError {}

impl fmt::Display for AgentScheduleDeliveryModeParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid agent schedule delivery mode {}",
            self.value
        )
    }
}

impl Error for AgentScheduleDeliveryModeParseError {}

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
        Self::new_with_provenance(
            id,
            name,
            template_kind,
            agent_home,
            allowed_tools,
            None,
            None,
            None,
            created_at,
            updated_at,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_provenance(
        id: impl Into<String>,
        name: impl Into<String>,
        template_kind: AgentTemplateKind,
        agent_home: impl AsRef<Path>,
        allowed_tools: Vec<String>,
        created_from_template_id: Option<String>,
        created_by_session_id: Option<String>,
        created_by_agent_profile_id: Option<String>,
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
            created_from_template_id: created_from_template_id
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            created_by_session_id: created_by_session_id
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            created_by_agent_profile_id: created_by_agent_profile_id
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            created_at,
            updated_at,
        })
    }

    pub fn allows_tool_id(&self, tool_id: &str) -> bool {
        self.allowed_tools
            .binary_search_by(|candidate| candidate.as_str().cmp(tool_id))
            .is_ok()
            || (tool_id.starts_with("mcp__")
                && (self
                    .allowed_tools
                    .binary_search_by(|candidate| candidate.as_str().cmp("mcp"))
                    .is_ok()
                    || self
                        .allowed_tools
                        .binary_search_by(|candidate| candidate.as_str().cmp("mcp_call"))
                        .is_ok()))
    }
}

impl AgentSchedule {
    pub fn new(init: AgentScheduleInit) -> Result<Self, AgentScheduleError> {
        let id = init.id.trim().to_string();
        if id.is_empty() {
            return Err(AgentScheduleError::EmptyId);
        }

        let agent_profile_id = init.agent_profile_id.trim().to_string();
        if agent_profile_id.is_empty() {
            return Err(AgentScheduleError::EmptyAgentProfileId);
        }

        let workspace_root = init.workspace_root;
        if workspace_root.as_os_str().is_empty() {
            return Err(AgentScheduleError::EmptyWorkspaceRoot);
        }

        let prompt = init.prompt.trim().to_string();
        if prompt.is_empty() {
            return Err(AgentScheduleError::EmptyPrompt);
        }

        if init.interval_seconds == 0 {
            return Err(AgentScheduleError::ZeroIntervalSeconds);
        }

        let target_session_id = normalize_optional_string(
            init.target_session_id,
            AgentScheduleError::EmptyTargetSessionId,
        )?;
        match init.delivery_mode {
            AgentScheduleDeliveryMode::FreshSession if target_session_id.is_some() => {
                return Err(AgentScheduleError::UnexpectedTargetSessionId);
            }
            AgentScheduleDeliveryMode::ExistingSession if target_session_id.is_none() => {
                return Err(AgentScheduleError::MissingTargetSessionId);
            }
            _ => {}
        }

        let last_session_id = normalize_optional_string(
            init.last_session_id,
            AgentScheduleError::EmptyLastSessionId,
        )?;
        let last_job_id =
            normalize_optional_string(init.last_job_id, AgentScheduleError::EmptyLastJobId)?;
        let last_result =
            normalize_optional_string(init.last_result, AgentScheduleError::EmptyLastResult)?;
        let last_error =
            normalize_optional_string(init.last_error, AgentScheduleError::EmptyLastError)?;

        Ok(Self {
            id,
            agent_profile_id,
            workspace_root,
            prompt,
            mode: init.mode,
            delivery_mode: init.delivery_mode,
            target_session_id,
            interval_seconds: init.interval_seconds,
            next_fire_at: init.next_fire_at,
            enabled: init.enabled,
            last_triggered_at: init.last_triggered_at,
            last_finished_at: init.last_finished_at,
            last_session_id,
            last_job_id,
            last_result,
            last_error,
            created_at: init.created_at,
            updated_at: init.updated_at,
        })
    }

    pub fn is_due(&self, now: i64) -> bool {
        now >= self.next_fire_at
    }
}

fn normalize_optional_string(
    value: Option<String>,
    error: AgentScheduleError,
) -> Result<Option<String>, AgentScheduleError> {
    value
        .map(|raw| {
            let trimmed = raw.trim().to_string();
            if trimmed.is_empty() {
                Err(error)
            } else {
                Ok(trimmed)
            }
        })
        .transpose()
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
            Self::MissingTargetSessionId => {
                write!(
                    formatter,
                    "agent schedule existing_session delivery requires target_session_id"
                )
            }
            Self::UnexpectedTargetSessionId => {
                write!(
                    formatter,
                    "agent schedule fresh_session delivery must not include target_session_id"
                )
            }
            Self::EmptyTargetSessionId => {
                write!(
                    formatter,
                    "agent schedule target_session_id must not be empty"
                )
            }
            Self::EmptyLastSessionId => {
                write!(
                    formatter,
                    "agent schedule last_session_id must not be empty"
                )
            }
            Self::EmptyLastJobId => {
                write!(formatter, "agent schedule last_job_id must not be empty")
            }
            Self::EmptyLastResult => {
                write!(formatter, "agent schedule last_result must not be empty")
            }
            Self::EmptyLastError => {
                write!(formatter, "agent schedule last_error must not be empty")
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
        AgentScheduleDeliveryMode, AgentScheduleError, AgentScheduleInit, AgentScheduleMode,
        AgentTemplateKind,
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
        let error = AgentSchedule::new(AgentScheduleInit {
            id: "judge-pulse".to_string(),
            agent_profile_id: "judge".to_string(),
            workspace_root: PathBuf::from("/tmp/project"),
            prompt: "check latest changes".to_string(),
            mode: AgentScheduleMode::Interval,
            delivery_mode: AgentScheduleDeliveryMode::FreshSession,
            target_session_id: None,
            interval_seconds: 0,
            next_fire_at: 10,
            enabled: true,
            last_triggered_at: None,
            last_finished_at: None,
            last_session_id: None,
            last_job_id: None,
            last_result: None,
            last_error: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect_err("zero interval should fail");

        assert_eq!(error, AgentScheduleError::ZeroIntervalSeconds);
    }

    #[test]
    fn agent_schedule_supports_interval_and_fresh_session_delivery() {
        let schedule = AgentSchedule::new(AgentScheduleInit {
            id: "judge-pulse".to_string(),
            agent_profile_id: "judge".to_string(),
            workspace_root: PathBuf::from("/tmp/project"),
            prompt: "check latest changes".to_string(),
            mode: AgentScheduleMode::Interval,
            delivery_mode: AgentScheduleDeliveryMode::FreshSession,
            target_session_id: None,
            interval_seconds: 300,
            next_fire_at: 10,
            enabled: true,
            last_triggered_at: None,
            last_finished_at: None,
            last_session_id: None,
            last_job_id: None,
            last_result: None,
            last_error: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("schedule");

        assert_eq!(schedule.mode, AgentScheduleMode::Interval);
        assert_eq!(
            schedule.delivery_mode,
            AgentScheduleDeliveryMode::FreshSession
        );
        assert_eq!(schedule.target_session_id, None);
        assert!(schedule.enabled);
        assert!(!schedule.is_due(9));
        assert!(schedule.is_due(10));
        assert!(schedule.is_due(11));
    }

    #[test]
    fn agent_schedule_supports_existing_session_disabled_and_terminal_fields() {
        let schedule = AgentSchedule::new(AgentScheduleInit {
            id: "judge-review-loop".to_string(),
            agent_profile_id: "judge".to_string(),
            workspace_root: PathBuf::from("/tmp/project"),
            prompt: "review the previous result".to_string(),
            mode: AgentScheduleMode::AfterCompletion,
            delivery_mode: AgentScheduleDeliveryMode::ExistingSession,
            target_session_id: Some("session-bound".to_string()),
            interval_seconds: 600,
            next_fire_at: 42,
            enabled: false,
            last_triggered_at: Some(20),
            last_finished_at: Some(30),
            last_session_id: Some("session-bound".to_string()),
            last_job_id: Some("job-schedule-prev".to_string()),
            last_result: Some("failed".to_string()),
            last_error: Some("tool execution failed".to_string()),
            created_at: 10,
            updated_at: 11,
        })
        .expect("schedule");

        assert_eq!(schedule.mode, AgentScheduleMode::AfterCompletion);
        assert_eq!(
            schedule.delivery_mode,
            AgentScheduleDeliveryMode::ExistingSession
        );
        assert_eq!(schedule.target_session_id.as_deref(), Some("session-bound"));
        assert!(!schedule.enabled);
        assert_eq!(schedule.last_finished_at, Some(30));
        assert_eq!(schedule.last_result.as_deref(), Some("failed"));
        assert_eq!(
            schedule.last_error.as_deref(),
            Some("tool execution failed")
        );
    }

    #[test]
    fn agent_schedule_supports_once_mode_for_one_shot_continuations() {
        let schedule = AgentSchedule::new(AgentScheduleInit {
            id: "continue-later".to_string(),
            agent_profile_id: "default".to_string(),
            workspace_root: PathBuf::from("/tmp/project"),
            prompt: "resume from handoff".to_string(),
            mode: AgentScheduleMode::Once,
            delivery_mode: AgentScheduleDeliveryMode::ExistingSession,
            target_session_id: Some("session-bound".to_string()),
            interval_seconds: 900,
            next_fire_at: 42,
            enabled: true,
            last_triggered_at: None,
            last_finished_at: None,
            last_session_id: None,
            last_job_id: None,
            last_result: None,
            last_error: None,
            created_at: 10,
            updated_at: 11,
        })
        .expect("schedule");

        assert_eq!(schedule.mode, AgentScheduleMode::Once);
        assert_eq!(schedule.interval_seconds, 900);
        assert!(schedule.enabled);
    }

    #[test]
    fn agent_schedule_rejects_existing_session_without_target_session_id() {
        let error = AgentSchedule::new(AgentScheduleInit {
            id: "judge-review-loop".to_string(),
            agent_profile_id: "judge".to_string(),
            workspace_root: PathBuf::from("/tmp/project"),
            prompt: "review the previous result".to_string(),
            mode: AgentScheduleMode::AfterCompletion,
            delivery_mode: AgentScheduleDeliveryMode::ExistingSession,
            target_session_id: None,
            interval_seconds: 600,
            next_fire_at: 42,
            enabled: true,
            last_triggered_at: None,
            last_finished_at: None,
            last_session_id: None,
            last_job_id: None,
            last_result: None,
            last_error: None,
            created_at: 10,
            updated_at: 11,
        })
        .expect_err("existing_session should require target_session_id");

        assert_eq!(error, AgentScheduleError::MissingTargetSessionId);
    }

    #[test]
    fn agent_schedule_rejects_target_session_id_for_fresh_session() {
        let error = AgentSchedule::new(AgentScheduleInit {
            id: "judge-pulse".to_string(),
            agent_profile_id: "judge".to_string(),
            workspace_root: PathBuf::from("/tmp/project"),
            prompt: "check latest changes".to_string(),
            mode: AgentScheduleMode::Interval,
            delivery_mode: AgentScheduleDeliveryMode::FreshSession,
            target_session_id: Some("session-bound".to_string()),
            interval_seconds: 300,
            next_fire_at: 10,
            enabled: true,
            last_triggered_at: None,
            last_finished_at: None,
            last_session_id: None,
            last_job_id: None,
            last_result: None,
            last_error: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect_err("fresh_session should reject target_session_id");

        assert_eq!(error, AgentScheduleError::UnexpectedTargetSessionId);
    }
}
