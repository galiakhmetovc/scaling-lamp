use super::commands::{TELEGRAM_MIN_COALESCE_WINDOW_MS, is_valid_telegram_queue_mode};
use agent_persistence::{
    RecordConversionError, SessionInboxEventRecord, TelegramChatBindingRecord,
};
use agent_runtime::inbox::{SessionInboxEvent, SessionInboxEventPayload};

pub(super) const TELEGRAM_INBOUND_QUEUE_SOURCE: &str = "telegram";

pub(super) fn build_telegram_inbound_record(
    session_id: &str,
    chat_id: i64,
    telegram_message_id: i32,
    message: &str,
    now: i64,
    available_at: i64,
) -> Result<SessionInboxEventRecord, RecordConversionError> {
    let event_id = format!("telegram-inbound-{session_id}-{chat_id}-{telegram_message_id}");
    let mut event = SessionInboxEvent::external_input_received(
        event_id,
        session_id,
        None,
        TELEGRAM_INBOUND_QUEUE_SOURCE,
        message,
        now,
    );
    event.available_at = available_at;
    SessionInboxEventRecord::try_from(&event)
}

pub(super) fn is_telegram_inbox_event(record: &SessionInboxEventRecord) -> bool {
    match serde_json::from_str::<SessionInboxEventPayload>(&record.payload_json) {
        Ok(SessionInboxEventPayload::ExternalInputReceived { source, .. }) => {
            source == TELEGRAM_INBOUND_QUEUE_SOURCE
        }
        _ => false,
    }
}

pub(super) fn effective_inbound_queue_mode(
    binding: &TelegramChatBindingRecord,
    default_mode: &str,
) -> String {
    if is_valid_telegram_queue_mode(binding.inbound_queue_mode.as_str()) {
        binding.inbound_queue_mode.clone()
    } else {
        default_mode.to_string()
    }
}

pub(super) fn configured_inbound_coalesce_window_ms(configured_ms: u64) -> u64 {
    configured_ms.max(TELEGRAM_MIN_COALESCE_WINDOW_MS)
}

pub(super) fn effective_inbound_coalesce_window_ms(
    binding: &TelegramChatBindingRecord,
    configured_ms: u64,
) -> u64 {
    binding
        .inbound_coalesce_window_ms
        .and_then(|value| u64::try_from(value).ok())
        .unwrap_or_else(|| configured_inbound_coalesce_window_ms(configured_ms))
        .max(TELEGRAM_MIN_COALESCE_WINDOW_MS)
}

pub(super) fn render_queue_status_text(
    mode: &str,
    queued_count: usize,
    coalesce_window_ms: u64,
) -> String {
    format!(
        "Telegram queue:\n- mode: {mode}\n- queued_inbound: {queued_count}\n- coalesce_window_ms: {coalesce_window_ms}"
    )
}
