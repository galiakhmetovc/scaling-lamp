use crate::agent::{AgentChainContinuationError, AgentChainContinuationGrant};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;

pub const DEFAULT_MAX_HOPS: u32 = 3;
pub const CHAIN_METADATA_PREFIX: &str = "interagent_chain:";
pub const AGENT_MESSAGE_PREFIX: &str = "[agent:";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentChainState {
    Active,
    BlockedMaxHops,
    ContinuedOnce,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentMessageChain {
    pub chain_id: String,
    pub origin_session_id: String,
    pub origin_agent_id: String,
    pub hop_count: u32,
    pub max_hops: u32,
    pub parent_interagent_session_id: Option<String>,
    pub state: AgentChainState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentMessageRequest {
    pub target_agent_id: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentMessageError {
    EmptyTargetAgentId,
    EmptyMessage,
    EmptyChainId,
    EmptyOriginSessionId,
    EmptyOriginAgentId,
    ZeroMaxHops,
}

impl AgentMessageRequest {
    pub fn new(
        target_agent_id: impl Into<String>,
        message: impl Into<String>,
    ) -> Result<Self, AgentMessageError> {
        let target_agent_id = target_agent_id.into().trim().to_string();
        if target_agent_id.is_empty() {
            return Err(AgentMessageError::EmptyTargetAgentId);
        }

        let message = message.into().trim().to_string();
        if message.is_empty() {
            return Err(AgentMessageError::EmptyMessage);
        }

        Ok(Self {
            target_agent_id,
            message,
        })
    }
}

impl AgentMessageChain {
    pub fn root(
        chain_id: impl Into<String>,
        origin_session_id: impl Into<String>,
        origin_agent_id: impl Into<String>,
    ) -> Result<Self, AgentMessageError> {
        Self::new(
            chain_id,
            origin_session_id,
            origin_agent_id,
            0,
            DEFAULT_MAX_HOPS,
            None,
            AgentChainState::Active,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        chain_id: impl Into<String>,
        origin_session_id: impl Into<String>,
        origin_agent_id: impl Into<String>,
        hop_count: u32,
        max_hops: u32,
        parent_interagent_session_id: Option<String>,
        state: AgentChainState,
    ) -> Result<Self, AgentMessageError> {
        let chain_id = chain_id.into().trim().to_string();
        if chain_id.is_empty() {
            return Err(AgentMessageError::EmptyChainId);
        }

        let origin_session_id = origin_session_id.into().trim().to_string();
        if origin_session_id.is_empty() {
            return Err(AgentMessageError::EmptyOriginSessionId);
        }

        let origin_agent_id = origin_agent_id.into().trim().to_string();
        if origin_agent_id.is_empty() {
            return Err(AgentMessageError::EmptyOriginAgentId);
        }

        if max_hops == 0 {
            return Err(AgentMessageError::ZeroMaxHops);
        }

        Ok(Self {
            chain_id,
            origin_session_id,
            origin_agent_id,
            hop_count,
            max_hops,
            parent_interagent_session_id,
            state,
        })
    }

    pub fn can_advance_without_grant(&self) -> bool {
        self.hop_count < self.max_hops
    }

    pub fn next_hop(
        &self,
        parent_interagent_session_id: impl Into<String>,
        used_grant: bool,
    ) -> Result<Self, AgentMessageError> {
        Self::new(
            self.chain_id.clone(),
            self.origin_session_id.clone(),
            self.origin_agent_id.clone(),
            self.hop_count.saturating_add(1),
            self.max_hops,
            Some(parent_interagent_session_id.into()),
            if used_grant {
                AgentChainState::ContinuedOnce
            } else {
                AgentChainState::Active
            },
        )
    }

    pub fn blocked_max_hops(&self) -> Result<Self, AgentMessageError> {
        Self::new(
            self.chain_id.clone(),
            self.origin_session_id.clone(),
            self.origin_agent_id.clone(),
            self.hop_count,
            self.max_hops,
            self.parent_interagent_session_id.clone(),
            AgentChainState::BlockedMaxHops,
        )
    }

    pub fn to_transcript_metadata(&self) -> String {
        format!(
            "{CHAIN_METADATA_PREFIX}{}",
            serde_json::to_string(self).expect("serialize interagent chain")
        )
    }

    pub fn from_transcript_metadata(content: &str) -> Option<Self> {
        content
            .strip_prefix(CHAIN_METADATA_PREFIX)
            .and_then(|payload| serde_json::from_str(payload).ok())
    }
}

pub fn continued_chain_from_grant(
    chain: &AgentMessageChain,
    grant: &AgentChainContinuationGrant,
    parent_interagent_session_id: impl Into<String>,
) -> Result<Option<AgentMessageChain>, AgentMessageError> {
    if chain.chain_id != grant.chain_id {
        return Ok(None);
    }

    chain.next_hop(parent_interagent_session_id, true).map(Some)
}

pub fn format_agent_input_message(source_agent_name: &str, message: &str) -> String {
    format!("{AGENT_MESSAGE_PREFIX}{source_agent_name}]\n{message}")
}

pub fn parse_agent_input_message(message: &str) -> Option<(&str, &str)> {
    let payload = message.strip_prefix(AGENT_MESSAGE_PREFIX)?;
    let (agent_name, body) = payload.split_once("]\n")?;
    Some((agent_name, body))
}

impl fmt::Display for AgentMessageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyTargetAgentId => write!(formatter, "target_agent_id must not be empty"),
            Self::EmptyMessage => write!(formatter, "message must not be empty"),
            Self::EmptyChainId => write!(formatter, "chain_id must not be empty"),
            Self::EmptyOriginSessionId => write!(formatter, "origin_session_id must not be empty"),
            Self::EmptyOriginAgentId => write!(formatter, "origin_agent_id must not be empty"),
            Self::ZeroMaxHops => write!(formatter, "max_hops must be greater than zero"),
        }
    }
}

impl Error for AgentMessageError {}

pub type ContinuationGrantError = AgentChainContinuationError;
pub type ContinuationGrant = AgentChainContinuationGrant;

#[cfg(test)]
mod tests {
    use super::{
        AgentChainState, AgentMessageChain, AgentMessageError, AgentMessageRequest,
        ContinuationGrant, DEFAULT_MAX_HOPS, continued_chain_from_grant,
        format_agent_input_message, parse_agent_input_message,
    };

    #[test]
    fn message_request_rejects_blank_target_and_message() {
        assert_eq!(
            AgentMessageRequest::new("   ", "hello").expect_err("blank target"),
            AgentMessageError::EmptyTargetAgentId
        );
        assert_eq!(
            AgentMessageRequest::new("judge", "   ").expect_err("blank message"),
            AgentMessageError::EmptyMessage
        );
    }

    #[test]
    fn chain_defaults_to_three_hops_for_root_sessions() {
        let chain = AgentMessageChain::root("chain-1", "session-origin", "default").expect("chain");

        assert_eq!(chain.hop_count, 0);
        assert_eq!(chain.max_hops, DEFAULT_MAX_HOPS);
        assert_eq!(chain.state, AgentChainState::Active);
        assert!(chain.can_advance_without_grant());
    }

    #[test]
    fn continuation_grant_is_consumed_for_exactly_one_extra_hop() {
        let chain = AgentMessageChain::new(
            "chain-1",
            "session-origin",
            "judge",
            DEFAULT_MAX_HOPS,
            DEFAULT_MAX_HOPS,
            Some("session-parent".to_string()),
            AgentChainState::BlockedMaxHops,
        )
        .expect("blocked chain");
        let grant =
            ContinuationGrant::new("chain-1", "allow one more review hop", 10).expect("grant");

        let continued = continued_chain_from_grant(&chain, &grant, "session-next")
            .expect("continue")
            .expect("grant should apply");

        assert_eq!(continued.hop_count, DEFAULT_MAX_HOPS + 1);
        assert_eq!(continued.state, AgentChainState::ContinuedOnce);
        assert!(!continued.can_advance_without_grant());
    }

    #[test]
    fn chain_round_trips_through_transcript_metadata() {
        let chain = AgentMessageChain::new(
            "chain-1",
            "session-origin",
            "judge",
            2,
            DEFAULT_MAX_HOPS,
            Some("session-parent".to_string()),
            AgentChainState::Active,
        )
        .expect("chain");

        let encoded = chain.to_transcript_metadata();
        let decoded =
            AgentMessageChain::from_transcript_metadata(&encoded).expect("decode chain metadata");

        assert_eq!(decoded, chain);
    }

    #[test]
    fn agent_input_message_round_trips_with_visible_prefix() {
        let formatted = format_agent_input_message("judge", "Short verdict.");
        let (agent_name, body) =
            parse_agent_input_message(&formatted).expect("parse agent input message");

        assert_eq!(agent_name, "judge");
        assert_eq!(body, "Short verdict.");
    }
}
