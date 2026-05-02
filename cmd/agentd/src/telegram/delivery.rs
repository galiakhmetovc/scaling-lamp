use super::client::TelegramClientError;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TelegramDeliveryTrace {
    pub(super) trace_id: String,
    pub(super) parent_span_id: String,
}

#[derive(Debug, Default)]
pub(super) struct TelegramDeliveryLimiter {
    global_next_at: Option<Instant>,
    chat_next_at: BTreeMap<i64, Instant>,
}

impl TelegramDeliveryLimiter {
    pub(super) fn reserve(
        &mut self,
        chat_id: i64,
        global_interval: Duration,
        chat_interval: Duration,
    ) -> Duration {
        let now = Instant::now();
        let mut slot = now;
        if let Some(global_next_at) = self.global_next_at {
            slot = slot.max(global_next_at);
        }
        if let Some(chat_next_at) = self.chat_next_at.get(&chat_id).copied() {
            slot = slot.max(chat_next_at);
        }
        self.global_next_at = Some(slot + global_interval);
        self.chat_next_at.insert(chat_id, slot + chat_interval);
        slot.saturating_duration_since(now)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TelegramDeliveryScope {
    Private,
    Group,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DeliveryCursor {
    pub(super) created_at: Option<i64>,
    pub(super) transcript_id: Option<String>,
}

pub(super) fn telegram_trace_id(chat_id: i64) -> String {
    format!("trace-telegram-{}", chat_id.to_string().replace('-', "n"))
}

pub(super) fn telegram_span_id(chat_id: i64, op: &str, attempt: usize) -> String {
    format!(
        "span-telegram-{}-{}-{attempt}",
        chat_id.to_string().replace('-', "n"),
        op.replace('_', "-")
    )
}

pub(super) fn duration_millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

pub(super) fn telegram_delivery_error_is_permanent(error: &TelegramClientError) -> bool {
    let message = error.to_string();
    message.contains("MESSAGE_TOO_LONG") || message.contains("Bad Request")
}
