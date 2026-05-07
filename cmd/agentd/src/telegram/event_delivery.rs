use super::client::{TelegramClient, TelegramClientError};
use super::delivery::{
    TelegramDeliveryLimiter, TelegramDeliveryScope, duration_millis,
    telegram_delivery_error_is_permanent,
};
use super::render::render_model_response_chunks;
use crate::bootstrap::App;
use crate::delivery_worker::{DeliverySendError, DeliverySender};
use crate::diagnostics::DiagnosticEventBuilder;
use agent_persistence::{DeliveryTargetRecord, audit::AuditLogConfig};
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub(crate) struct TelegramEventDeliverySender {
    app: App,
    client: TelegramClient,
    limiter: Arc<Mutex<TelegramDeliveryLimiter>>,
    audit: AuditLogConfig,
}

impl TelegramEventDeliverySender {
    pub(crate) fn new(app: App, client: TelegramClient) -> Self {
        let audit = AuditLogConfig::from_config(&app.config);
        Self {
            app,
            client,
            limiter: Arc::new(Mutex::new(TelegramDeliveryLimiter::default())),
            audit,
        }
    }

    async fn send_text_async(
        &self,
        target: &DeliveryTargetRecord,
        text: &str,
    ) -> Result<(), DeliverySendError> {
        if target.kind != "telegram" {
            return Err(DeliverySendError::new(format!(
                "unsupported delivery target kind {}",
                target.kind
            )));
        }
        let chat_id = target.address.parse::<i64>().map_err(|error| {
            DeliverySendError::new(format!(
                "invalid telegram delivery target address {}: {error}",
                target.address
            ))
        })?;
        for chunk in
            render_model_response_chunks(text, self.app.config.telegram.message_text_soft_cap)
        {
            if chunk.parse_mode_html {
                self.deliver_with_retry(chat_id, "send_html", || {
                    let client = self.client.clone();
                    let html = chunk.text.clone();
                    async move { client.send_html(chat_id, &html).await.map(|_| ()) }
                })
                .await?;
            } else {
                self.deliver_with_retry(chat_id, "send_text", || {
                    let client = self.client.clone();
                    let text = chunk.text.clone();
                    async move { client.send_text(chat_id, &text).await.map(|_| ()) }
                })
                .await?;
            }
        }
        Ok(())
    }

    async fn deliver_with_retry<F, Fut>(
        &self,
        chat_id: i64,
        op: &'static str,
        mut action: F,
    ) -> Result<(), DeliverySendError>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<(), TelegramClientError>>,
    {
        let started = Instant::now();
        for attempt in 1..=self.app.config.telegram.delivery_retry_attempts {
            self.wait_for_delivery_slot(chat_id).await;
            match action().await {
                Ok(()) => {
                    self.emit_delivery_event(chat_id, op, attempt, started.elapsed(), None);
                    return Ok(());
                }
                Err(error)
                    if attempt < self.app.config.telegram.delivery_retry_attempts
                        && !telegram_delivery_error_is_permanent(&error) =>
                {
                    DiagnosticEventBuilder::new(
                        &self.app.config,
                        "warn",
                        "telegram",
                        "delivery.retry",
                        "telegram event delivery attempt failed",
                    )
                    .surface("telegram")
                    .entrypoint("event_runtime")
                    .outcome("retry")
                    .elapsed_ms(duration_millis(started.elapsed()))
                    .field("chat_id", chat_id)
                    .field("delivery_op", op)
                    .field("attempt", attempt)
                    .error(error.to_string())
                    .emit(&self.audit);
                    tokio::time::sleep(Duration::from_millis(
                        self.app.config.telegram.delivery_retry_base_delay_ms * attempt as u64,
                    ))
                    .await;
                }
                Err(error) => {
                    let error_message = error.to_string();
                    self.emit_delivery_event(
                        chat_id,
                        op,
                        attempt,
                        started.elapsed(),
                        Some(error_message.clone()),
                    );
                    return Err(DeliverySendError::new(error_message));
                }
            }
        }
        Err(DeliverySendError::new(
            "telegram event delivery retry loop exhausted",
        ))
    }

    async fn wait_for_delivery_slot(&self, chat_id: i64) {
        let scope = if chat_id < 0 {
            TelegramDeliveryScope::Group
        } else {
            TelegramDeliveryScope::Private
        };
        let global_interval =
            Duration::from_millis(self.app.config.telegram.global_send_min_interval_ms);
        let chat_interval = match scope {
            TelegramDeliveryScope::Private => {
                Duration::from_millis(self.app.config.telegram.private_chat_send_min_interval_ms)
            }
            TelegramDeliveryScope::Group => {
                Duration::from_millis(self.app.config.telegram.group_chat_send_min_interval_ms)
            }
        };
        let delay = self
            .limiter
            .lock()
            .expect("telegram event delivery limiter")
            .reserve(chat_id, global_interval, chat_interval);
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
    }

    fn emit_delivery_event(
        &self,
        chat_id: i64,
        op: &'static str,
        attempt: usize,
        elapsed: Duration,
        error: Option<String>,
    ) {
        let mut event = DiagnosticEventBuilder::new(
            &self.app.config,
            if error.is_some() { "error" } else { "info" },
            "telegram",
            "event_delivery.request",
            "telegram event delivery request completed",
        )
        .surface("telegram")
        .entrypoint("event_runtime")
        .outcome(if error.is_some() { "error" } else { "ok" })
        .elapsed_ms(duration_millis(elapsed))
        .field("chat_id", chat_id)
        .field("delivery_op", op)
        .field("attempt", attempt);
        if let Some(error) = error {
            event = event.error(error);
        }
        event.emit(&self.audit);
    }
}

impl DeliverySender for TelegramEventDeliverySender {
    fn send_text(
        &self,
        target: &DeliveryTargetRecord,
        text: &str,
    ) -> Result<(), DeliverySendError> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.send_text_async(target, text))
        })
    }
}
