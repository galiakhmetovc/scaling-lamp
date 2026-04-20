mod chat;
mod internal;
mod sessions;
mod status;

use crate::bootstrap::{
    BootstrapError, SessionPendingApproval, SessionPreferencesPatch, SessionSkillStatus,
    SessionSummary, SessionTranscriptView,
};
use crate::execution::{ApprovalContinuationReport, ChatExecutionEvent, ChatTurnExecutionReport};
use crate::http::types::{
    DaemonStopResponse, ErrorResponse, SessionSummaryResponse, StatusResponse,
};
use agent_persistence::AppConfig;
use reqwest::StatusCode;
use reqwest::blocking::Client;
use serde::de::DeserializeOwned;
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DaemonConnectOptions {
    pub host: Option<String>,
    pub port: Option<u16>,
}

#[derive(Debug, Clone)]
pub struct DaemonClient {
    http: Client,
    base_url: String,
    bearer_token: Option<String>,
}

pub fn connect_or_autospawn<F>(
    config: &AppConfig,
    options: &DaemonConnectOptions,
    spawn_local: F,
) -> Result<DaemonClient, BootstrapError>
where
    F: FnOnce() -> Result<(), BootstrapError>,
{
    let client = DaemonClient::new(config, options);
    if client.status().is_ok() {
        return Ok(client);
    }

    if options.host.is_some() || options.port.is_some() {
        return Err(BootstrapError::Usage {
            reason: format!("daemon {} is unavailable", client.base_url),
        });
    }

    spawn_local()?;
    for _ in 0..50 {
        if client.status().is_ok() {
            return Ok(client);
        }
        thread::sleep(Duration::from_millis(100));
    }
    client.status()?;
    Ok(client)
}

fn decode_response<T>(response: reqwest::blocking::Response) -> Result<T, BootstrapError>
where
    T: DeserializeOwned,
{
    let status = response.status();
    if status.is_success() {
        return response.json::<T>().map_err(|error| {
            BootstrapError::Stream(std::io::Error::other(format!(
                "invalid daemon response: {error}"
            )))
        });
    }

    let error = response
        .json::<ErrorResponse>()
        .ok()
        .map(|error| error.error);
    let reason = error.unwrap_or_else(|| {
        status
            .canonical_reason()
            .unwrap_or("daemon error")
            .to_string()
    });
    let kind = if status == StatusCode::UNAUTHORIZED {
        "daemon authorization failed"
    } else {
        "daemon request failed"
    };
    Err(BootstrapError::Usage {
        reason: format!("{kind}: {reason}"),
    })
}

impl From<SessionSummaryResponse> for SessionSummary {
    fn from(value: SessionSummaryResponse) -> Self {
        Self {
            id: value.id,
            title: value.title,
            model: value.model,
            reasoning_visible: value.reasoning_visible,
            think_level: value.think_level,
            compactifications: value.compactifications,
            context_tokens: value.context_tokens,
            has_pending_approval: value.has_pending_approval,
            last_message_preview: value.last_message_preview,
            message_count: value.message_count,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}
