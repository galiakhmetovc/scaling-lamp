mod chat;
mod internal;
mod mcp;
mod sessions;
mod status;

use crate::about::{APP_BUILD_ID, APP_COMMIT, APP_TREE_STATE, APP_VERSION};
use crate::bootstrap::{
    BootstrapError, SessionBackgroundJob, SessionPendingApproval, SessionPreferencesPatch,
    SessionScheduleSummary, SessionSkillStatus, SessionSummary, SessionTranscriptView,
};
use crate::diagnostics::DiagnosticEventBuilder;
use crate::execution::{ApprovalContinuationReport, ChatExecutionEvent, ChatTurnExecutionReport};
use crate::http::types::{
    AboutResponse, DaemonStopResponse, ErrorResponse, SessionBackgroundJobResponse,
    SessionSummaryResponse, StatusResponse, UpdateRuntimeResponse,
};
use agent_persistence::RuntimeTimingConfig;
use agent_persistence::{AppConfig, audit::AuditLogConfig};
use reqwest::StatusCode;
use reqwest::blocking::Client;
use serde::de::DeserializeOwned;
use std::thread;
use std::time::Instant;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DaemonConnectOptions {
    pub host: Option<String>,
    pub port: Option<u16>,
}

#[derive(Debug, Clone)]
pub struct DaemonClient {
    http: Client,
    long_http: Client,
    base_url: String,
    bearer_token: Option<String>,
    data_dir: String,
    audit: AuditLogConfig,
    runtime_timing: RuntimeTimingConfig,
    default_diagnostic_tail_lines: usize,
}

#[derive(Debug, Clone)]
pub struct DaemonConnection {
    client: DaemonClient,
    autospawned: bool,
}

pub fn connect_or_autospawn<F>(
    config: &AppConfig,
    options: &DaemonConnectOptions,
    spawn_local: F,
) -> Result<DaemonClient, BootstrapError>
where
    F: FnOnce() -> Result<(), BootstrapError>,
{
    Ok(connect_or_autospawn_detailed(config, options, spawn_local)?.client)
}

pub fn connect_or_autospawn_detailed<F>(
    config: &AppConfig,
    options: &DaemonConnectOptions,
    spawn_local: F,
) -> Result<DaemonConnection, BootstrapError>
where
    F: FnOnce() -> Result<(), BootstrapError>,
{
    let client = DaemonClient::new(config, options);
    let audit = AuditLogConfig::from_config(config);
    let started = Instant::now();
    let explicit_remote_target = options.host.is_some() || options.port.is_some();
    DiagnosticEventBuilder::new(
        config,
        "info",
        "daemon_client",
        "connect_or_autospawn.start",
        "connecting to daemon",
    )
    .daemon_base_url(client.base_url.clone())
    .field("explicit_remote_target", explicit_remote_target)
    .field("requested_host", options.host.clone())
    .field("requested_port", options.port)
    .emit(&audit);
    if let Ok(status) = client.status() {
        let compatible = if explicit_remote_target {
            daemon_matches_current_build(&status)
        } else {
            local_daemon_matches_current_instance(config, &status)
        };
        DiagnosticEventBuilder::new(
            config,
            "info",
            "daemon_client",
            "connect_or_autospawn.status_probe",
            "received daemon status",
        )
        .daemon_base_url(client.base_url.clone())
        .elapsed_ms(started.elapsed().as_millis() as u64)
        .field("status_version", status.version.clone())
        .field("status_commit", status.commit.clone())
        .field("status_tree_state", status.tree_state.clone())
        .field("status_build_id", status.build_id.clone())
        .field("status_data_dir", status.data_dir.clone())
        .field("compatible", compatible)
        .emit(&audit);
        if compatible {
            DiagnosticEventBuilder::new(
                config,
                "info",
                "daemon_client",
                "connect_or_autospawn.reuse",
                "reusing compatible daemon",
            )
            .daemon_base_url(client.base_url.clone())
            .elapsed_ms(started.elapsed().as_millis() as u64)
            .outcome("reused")
            .emit(&audit);
            return Ok(DaemonConnection {
                client,
                autospawned: false,
            });
        }

        if explicit_remote_target {
            DiagnosticEventBuilder::new(
                config,
                "error",
                "daemon_client",
                "connect_or_autospawn.incompatible_remote",
                "remote daemon build is incompatible",
            )
            .daemon_base_url(client.base_url.clone())
            .elapsed_ms(started.elapsed().as_millis() as u64)
            .field("status_version", status.version.clone())
            .field("status_commit", status.commit.clone())
            .field("status_tree_state", status.tree_state.clone())
            .field("status_build_id", status.build_id.clone())
            .field("status_data_dir", status.data_dir.clone())
            .emit(&audit);
            return Err(BootstrapError::Usage {
                reason: format!(
                    "daemon {} is running incompatible build {} ({})",
                    client.base_url,
                    status.version.as_deref().unwrap_or("unknown"),
                    status.commit.as_deref().unwrap_or("unknown")
                ),
            });
        }

        DiagnosticEventBuilder::new(
            config,
            "warn",
            "daemon_client",
            "connect_or_autospawn.restart_needed",
            "local daemon instance is incompatible and will be restarted",
        )
        .daemon_base_url(client.base_url.clone())
        .field("status_version", status.version.clone())
        .field("status_commit", status.commit.clone())
        .field("status_tree_state", status.tree_state.clone())
        .field("status_build_id", status.build_id.clone())
        .field("status_data_dir", status.data_dir.clone())
        .emit(&audit);
        restart_incompatible_local_daemon(config, &audit, &client)?;
    }

    if options.host.is_some() || options.port.is_some() {
        DiagnosticEventBuilder::new(
            config,
            "error",
            "daemon_client",
            "connect_or_autospawn.unavailable_remote",
            "remote daemon is unavailable",
        )
        .daemon_base_url(client.base_url.clone())
        .elapsed_ms(started.elapsed().as_millis() as u64)
        .emit(&audit);
        return Err(BootstrapError::Usage {
            reason: format!("daemon {} is unavailable", client.base_url),
        });
    }

    DiagnosticEventBuilder::new(
        config,
        "info",
        "daemon_client",
        "connect_or_autospawn.spawn",
        "spawning local daemon",
    )
    .daemon_base_url(client.base_url.clone())
    .emit(&audit);
    spawn_local()?;
    for _ in 0..config.runtime_timing.autospawn_status_poll_attempts {
        if let Ok(status) = client.status() {
            let compatible = if explicit_remote_target {
                daemon_matches_current_build(&status)
            } else {
                local_daemon_matches_current_instance(config, &status)
            };
            if compatible {
                DiagnosticEventBuilder::new(
                    config,
                    "info",
                    "daemon_client",
                    "connect_or_autospawn.finish",
                    "connected to autospawned daemon",
                )
                .daemon_base_url(client.base_url.clone())
                .elapsed_ms(started.elapsed().as_millis() as u64)
                .outcome("autospawned")
                .field("status_version", status.version.clone())
                .field("status_commit", status.commit.clone())
                .field("status_tree_state", status.tree_state.clone())
                .field("status_build_id", status.build_id.clone())
                .field("status_data_dir", status.data_dir.clone())
                .emit(&audit);
                return Ok(DaemonConnection {
                    client,
                    autospawned: true,
                });
            }
        }
        thread::sleep(config.runtime_timing.autospawn_status_poll_interval());
    }
    let status = client.status()?;
    let compatible = if explicit_remote_target {
        daemon_matches_current_build(&status)
    } else {
        local_daemon_matches_current_instance(config, &status)
    };
    if !compatible {
        DiagnosticEventBuilder::new(
            config,
            "error",
            "daemon_client",
            "connect_or_autospawn.incompatible_after_spawn",
            "spawned daemon is still incompatible",
        )
        .daemon_base_url(client.base_url.clone())
        .elapsed_ms(started.elapsed().as_millis() as u64)
        .field("status_version", status.version.clone())
        .field("status_commit", status.commit.clone())
        .field("status_tree_state", status.tree_state.clone())
        .field("status_build_id", status.build_id.clone())
        .field("status_data_dir", status.data_dir.clone())
        .emit(&audit);
        return Err(BootstrapError::Usage {
            reason: if explicit_remote_target {
                format!(
                    "daemon {} started with incompatible build {} ({})",
                    client.base_url,
                    status.version.as_deref().unwrap_or("unknown"),
                    status.commit.as_deref().unwrap_or("unknown")
                )
            } else {
                format!(
                    "daemon {} started incompatible instance: build {} ({}) data_dir={}",
                    client.base_url,
                    status.version.as_deref().unwrap_or("unknown"),
                    status.commit.as_deref().unwrap_or("unknown"),
                    status.data_dir
                )
            },
        });
    }
    DiagnosticEventBuilder::new(
        config,
        "info",
        "daemon_client",
        "connect_or_autospawn.finish",
        "connected to local daemon after compatibility check",
    )
    .daemon_base_url(client.base_url.clone())
    .elapsed_ms(started.elapsed().as_millis() as u64)
    .outcome("autospawned")
    .field("status_version", status.version.clone())
    .field("status_commit", status.commit.clone())
    .field("status_tree_state", status.tree_state.clone())
    .field("status_build_id", status.build_id.clone())
    .field("status_data_dir", status.data_dir.clone())
    .emit(&audit);
    Ok(DaemonConnection {
        client,
        autospawned: true,
    })
}

impl DaemonConnection {
    pub fn client(&self) -> &DaemonClient {
        &self.client
    }

    pub fn into_client(self) -> DaemonClient {
        self.client
    }

    pub fn was_autospawned(&self) -> bool {
        self.autospawned
    }

    pub fn shutdown_if_autospawned(&self) -> Result<(), BootstrapError> {
        if self.autospawned {
            self.client.shutdown()?;
            for _ in 0..self.client.runtime_timing.shutdown_wait_poll_attempts {
                if self.client.status().is_err() {
                    return Ok(());
                }
                thread::sleep(self.client.runtime_timing.shutdown_wait_poll_interval());
            }
        }
        Ok(())
    }
}

fn daemon_matches_current_build(status: &StatusResponse) -> bool {
    let status_tree_state = status.tree_state.as_deref().unwrap_or(APP_TREE_STATE);
    let same_release_identity = status.version.as_deref() == Some(APP_VERSION)
        && status.commit.as_deref() == Some(APP_COMMIT)
        && status_tree_state == APP_TREE_STATE;
    if !same_release_identity {
        return false;
    }

    if APP_TREE_STATE == "dirty" || status_tree_state == "dirty" {
        return status.build_id.as_deref().unwrap_or(APP_BUILD_ID) == APP_BUILD_ID;
    }

    true
}

fn local_daemon_matches_current_instance(config: &AppConfig, status: &StatusResponse) -> bool {
    daemon_matches_current_build(status) && status.data_dir == config.data_dir.display().to_string()
}

fn restart_incompatible_local_daemon(
    config: &AppConfig,
    audit: &AuditLogConfig,
    client: &DaemonClient,
) -> Result<(), BootstrapError> {
    let started = Instant::now();
    client.shutdown()?;
    let mut consecutive_unavailable = 0usize;
    for _ in 0..config.runtime_timing.restart_stop_poll_attempts {
        if client.status().is_err() {
            consecutive_unavailable += 1;
            if consecutive_unavailable
                >= config
                    .runtime_timing
                    .restart_stop_required_unavailable_probes
            {
                DiagnosticEventBuilder::new(
                    config,
                    "info",
                    "daemon_client",
                    "restart_incompatible_local_daemon.finish",
                    "previous local daemon stopped",
                )
                .daemon_base_url(client.base_url.clone())
                .elapsed_ms(started.elapsed().as_millis() as u64)
                .outcome("stopped")
                .emit(audit);
                return Ok(());
            }
        } else {
            consecutive_unavailable = 0;
        }
        thread::sleep(config.runtime_timing.restart_stop_poll_interval());
    }
    DiagnosticEventBuilder::new(
        config,
        "error",
        "daemon_client",
        "restart_incompatible_local_daemon.timeout",
        "previous local daemon did not stop in time",
    )
    .daemon_base_url(client.base_url.clone())
    .elapsed_ms(started.elapsed().as_millis() as u64)
    .emit(audit);
    Err(BootstrapError::Usage {
        reason: format!("daemon {} did not stop before restart", client.base_url),
    })
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
            agent_profile_id: value.agent_profile_id,
            agent_name: value.agent_name,
            scheduled_by: value.scheduled_by,
            schedule: value.schedule.map(|schedule| SessionScheduleSummary {
                id: schedule.id,
                mode: schedule.mode,
                delivery_mode: schedule.delivery_mode,
                enabled: schedule.enabled,
                next_fire_at: schedule.next_fire_at,
                target_session_id: schedule.target_session_id,
                last_result: schedule.last_result,
                last_error: schedule.last_error,
            }),
            model: value.model,
            reasoning_visible: value.reasoning_visible,
            think_level: value.think_level,
            compactifications: value.compactifications,
            completion_nudges: value.completion_nudges,
            auto_approve: value.auto_approve,
            context_tokens: value.context_tokens,
            usage_input_tokens: value.usage_input_tokens,
            usage_output_tokens: value.usage_output_tokens,
            usage_total_tokens: value.usage_total_tokens,
            has_pending_approval: value.has_pending_approval,
            last_message_preview: value.last_message_preview,
            message_count: value.message_count,
            background_job_count: value.background_job_count,
            running_background_job_count: value.running_background_job_count,
            queued_background_job_count: value.queued_background_job_count,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

impl From<SessionBackgroundJobResponse> for SessionBackgroundJob {
    fn from(value: SessionBackgroundJobResponse) -> Self {
        Self {
            id: value.id,
            kind: value.kind,
            status: value.status,
            queued_at: value.queued_at,
            started_at: value.started_at,
            last_progress_message: value.last_progress_message,
        }
    }
}
