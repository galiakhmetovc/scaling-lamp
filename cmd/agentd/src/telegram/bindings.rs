use super::delivery::DeliveryCursor;
use agent_persistence::{TelegramChatBindingRecord, TranscriptRecord};

pub(super) const TELEGRAM_SCOPE_PRIVATE: &str = "private";
pub(super) const TELEGRAM_SCOPE_GROUP: &str = "group";

pub(super) fn private_binding_record(
    chat_id: i64,
    telegram_user_id: i64,
    selected_session_id: Option<String>,
    now: i64,
    existing: Option<&TelegramChatBindingRecord>,
    cursor: DeliveryCursor,
    default_queue_mode: &str,
) -> TelegramChatBindingRecord {
    binding_record(BindingRecordInput {
        chat_id,
        scope: TELEGRAM_SCOPE_PRIVATE,
        owner_telegram_user_id: Some(telegram_user_id),
        selected_session_id,
        now,
        existing,
        cursor,
        default_queue_mode,
    })
}

pub(super) fn group_binding_record(
    chat_id: i64,
    selected_session_id: Option<String>,
    now: i64,
    existing: Option<&TelegramChatBindingRecord>,
    cursor: DeliveryCursor,
    default_queue_mode: &str,
) -> TelegramChatBindingRecord {
    binding_record(BindingRecordInput {
        chat_id,
        scope: TELEGRAM_SCOPE_GROUP,
        owner_telegram_user_id: None,
        selected_session_id,
        now,
        existing,
        cursor,
        default_queue_mode,
    })
}

struct BindingRecordInput<'a> {
    chat_id: i64,
    scope: &'a str,
    owner_telegram_user_id: Option<i64>,
    selected_session_id: Option<String>,
    now: i64,
    existing: Option<&'a TelegramChatBindingRecord>,
    cursor: DeliveryCursor,
    default_queue_mode: &'a str,
}

fn binding_record(input: BindingRecordInput<'_>) -> TelegramChatBindingRecord {
    let BindingRecordInput {
        chat_id,
        scope,
        owner_telegram_user_id,
        selected_session_id,
        now,
        existing,
        cursor,
        default_queue_mode,
    } = input;
    let existing_created_at = existing.map(|record| record.created_at).unwrap_or(now);
    TelegramChatBindingRecord {
        telegram_chat_id: chat_id,
        scope: scope.to_string(),
        owner_telegram_user_id,
        selected_session_id,
        last_delivered_transcript_created_at: cursor.created_at,
        last_delivered_transcript_id: cursor.transcript_id,
        inbound_queue_mode: existing
            .map(|record| record.inbound_queue_mode.clone())
            .unwrap_or_else(|| default_queue_mode.to_string()),
        inbound_coalesce_window_ms: existing.and_then(|record| record.inbound_coalesce_window_ms),
        created_at: existing_created_at,
        updated_at: now,
    }
}

pub(super) fn transcript_is_after_binding_cursor(
    transcript: &TranscriptRecord,
    binding: &TelegramChatBindingRecord,
) -> bool {
    let cursor_created_at = binding.last_delivered_transcript_created_at.unwrap_or(0);
    let cursor_id = binding
        .last_delivered_transcript_id
        .as_deref()
        .unwrap_or("");
    transcript.created_at > cursor_created_at
        || (transcript.created_at == cursor_created_at && transcript.id.as_str() > cursor_id)
}
