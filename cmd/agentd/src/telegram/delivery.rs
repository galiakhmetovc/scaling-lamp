use super::client::TelegramClientError;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TelegramDeliveryTrace {
    pub(crate) trace_id: String,
    pub(crate) parent_span_id: String,
}

#[derive(Debug, Default)]
pub(crate) struct TelegramDeliveryLimiter {
    global_next_at: Option<Instant>,
    chat_next_at: BTreeMap<i64, Instant>,
}

impl TelegramDeliveryLimiter {
    pub(crate) fn reserve(
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
pub(crate) enum TelegramDeliveryScope {
    Private,
    Group,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeliveryCursor {
    pub(crate) created_at: Option<i64>,
    pub(crate) transcript_id: Option<String>,
}

pub(crate) fn telegram_trace_id(chat_id: i64) -> String {
    format!("trace-telegram-{}", chat_id.to_string().replace('-', "n"))
}

pub(crate) fn telegram_span_id(chat_id: i64, op: &str, attempt: usize) -> String {
    format!(
        "span-telegram-{}-{}-{attempt}",
        chat_id.to_string().replace('-', "n"),
        op.replace('_', "-")
    )
}

pub(crate) fn duration_millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

pub(crate) fn telegram_delivery_error_is_permanent(error: &TelegramClientError) -> bool {
    let message = error.to_string();
    message.contains("MESSAGE_TOO_LONG") || message.contains("Bad Request")
}
