use crate::bootstrap::App;
use crate::http::server::respond_json;
use crate::http::types::ErrorResponse;
use crate::telegram::webhook::{TelegramWebhookErrorKind, TelegramWebhookOutcome};
use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};
use tiny_http::{Request, StatusCode};

const TELEGRAM_WEBHOOK_PREFIX: &str = "/v1/telegram/webhook/";

#[derive(Debug, Serialize)]
struct TelegramWebhookResponse {
    ok: bool,
    event_id: String,
    duplicate: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    outbox_id: Option<String>,
}

pub(super) fn is_telegram_webhook_request(request: &Request) -> bool {
    request.method() == &tiny_http::Method::Post
        && request
            .url()
            .split('?')
            .next()
            .unwrap_or(request.url())
            .starts_with(TELEGRAM_WEBHOOK_PREFIX)
}

pub(super) fn handle_telegram_webhook(app: &App, mut request: Request) -> std::io::Result<()> {
    let path = request
        .url()
        .split('?')
        .next()
        .unwrap_or(request.url())
        .to_string();
    let secret = path
        .strip_prefix(TELEGRAM_WEBHOOK_PREFIX)
        .unwrap_or_default()
        .trim_matches('/');
    let mut body = String::new();
    request.as_reader().read_to_string(&mut body)?;

    match crate::telegram::webhook::handle_webhook_update(app, secret, &body, unix_timestamp()) {
        Ok(outcome) => respond_json(request, StatusCode(200), &response_from_outcome(outcome)),
        Err(error) => {
            let status = match error.kind() {
                TelegramWebhookErrorKind::Unauthorized => StatusCode(403),
                TelegramWebhookErrorKind::InvalidPayload => StatusCode(400),
                TelegramWebhookErrorKind::Config
                | TelegramWebhookErrorKind::Store
                | TelegramWebhookErrorKind::Encode => StatusCode(500),
            };
            respond_json(
                request,
                status,
                &ErrorResponse {
                    error: error.to_string(),
                },
            )
        }
    }
}

fn response_from_outcome(outcome: TelegramWebhookOutcome) -> TelegramWebhookResponse {
    TelegramWebhookResponse {
        ok: true,
        event_id: outcome.event_id,
        duplicate: outcome.duplicate,
        outbox_id: outcome.outbox_id,
    }
}

fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}
