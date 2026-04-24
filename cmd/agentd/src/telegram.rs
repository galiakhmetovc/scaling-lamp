pub mod backend;
pub mod client;
pub mod polling;
pub mod render;
pub mod router;

use crate::bootstrap::{App, BootstrapError};
use crate::http::client::{DaemonConnectOptions, connect_or_autospawn_detailed};
use crate::telegram::backend::DaemonTelegramBackend;
use crate::telegram::client::TelegramClientConfig;
use crate::telegram::router::TelegramWorker;
use agent_persistence::{TelegramRepository, TelegramUserPairingRecord};
use std::time::{SystemTime, UNIX_EPOCH};

const TELEGRAM_PAIRING_STATUS_PENDING: &str = "pending";
const TELEGRAM_PAIRING_STATUS_ACTIVATED: &str = "activated";

pub(crate) fn run(app: &App) -> Result<(), BootstrapError> {
    if !app.config.telegram.enabled {
        return Err(BootstrapError::Usage {
            reason: "telegram is disabled in config".to_string(),
        });
    }

    let token = app
        .config
        .telegram
        .bot_token
        .clone()
        .ok_or_else(|| BootstrapError::Usage {
            reason: "telegram.bot_token is not configured".to_string(),
        })?;
    let connection =
        connect_or_autospawn_detailed(&app.config, &DaemonConnectOptions::default(), || {
            crate::daemon::spawn_local_process().map_err(BootstrapError::Stream)
        })?;
    let backend = DaemonTelegramBackend::new(connection.client().clone());
    let client = crate::telegram::client::TelegramClient::new(TelegramClientConfig {
        token,
        api_url: None,
        poll_request_timeout_seconds: app.config.telegram.poll_request_timeout_seconds,
    })
    .map_err(|error| BootstrapError::Stream(std::io::Error::other(error.to_string())))?;
    let worker = TelegramWorker::new(app.clone(), backend, client);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(BootstrapError::Stream)?;
    let result = runtime.block_on(async {
        worker.register_commands().await?;
        worker.run_forever().await
    });
    let shutdown_result = connection.shutdown_if_autospawned();
    match result {
        Ok(()) => shutdown_result,
        Err(error) => {
            let _ = shutdown_result;
            Err(error)
        }
    }
}

pub(crate) fn activate_pairing(app: &App, key: &str) -> Result<String, BootstrapError> {
    let store = app.store()?;
    let mut pairing = store
        .get_telegram_user_pairing_by_token(key)?
        .ok_or_else(|| BootstrapError::MissingRecord {
            kind: "telegram pairing",
            id: key.to_string(),
        })?;
    let now = unix_timestamp()?;

    if pairing.expires_at < now {
        return Err(BootstrapError::Usage {
            reason: format!("telegram pairing token {key} expired"),
        });
    }

    pairing.status = TELEGRAM_PAIRING_STATUS_ACTIVATED.to_string();
    pairing.activated_at = Some(now);
    store.put_telegram_user_pairing(&pairing)?;

    Ok(format!(
        "telegram pairing activated token={} user_id={} chat_id={} status={} username={} display_name={}",
        pairing.token,
        pairing.telegram_user_id,
        pairing.telegram_chat_id,
        pairing.status,
        pairing.telegram_username.as_deref().unwrap_or("<none>"),
        pairing.telegram_display_name,
    ))
}

#[allow(dead_code)]
fn is_pending_pairing(pairing: &TelegramUserPairingRecord) -> bool {
    pairing.status == TELEGRAM_PAIRING_STATUS_PENDING
}

pub(crate) fn render_pairings(app: &App) -> Result<String, BootstrapError> {
    let store = app.store()?;
    let pairings = store.list_telegram_user_pairings()?;

    if pairings.is_empty() {
        return Ok("telegram pairings none".to_string());
    }

    Ok(pairings
        .iter()
        .map(render_pairing_line)
        .collect::<Vec<_>>()
        .join("\n"))
}

fn render_pairing_line(pairing: &TelegramUserPairingRecord) -> String {
    format!(
        "token={} user_id={} chat_id={} status={} username={} display_name={} created_at={} expires_at={} activated_at={}",
        pairing.token,
        pairing.telegram_user_id,
        pairing.telegram_chat_id,
        pairing.status,
        pairing.telegram_username.as_deref().unwrap_or("<none>"),
        pairing.telegram_display_name,
        pairing.created_at,
        pairing.expires_at,
        pairing
            .activated_at
            .map(|value| value.to_string())
            .unwrap_or_else(|| "<none>".to_string()),
    )
}

fn unix_timestamp() -> Result<i64, BootstrapError> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(BootstrapError::Clock)?
        .as_secs() as i64)
}
