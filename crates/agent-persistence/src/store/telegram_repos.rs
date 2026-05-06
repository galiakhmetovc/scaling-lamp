use super::*;
use crate::records::{
    TelegramChatBindingRecord, TelegramChatStatusRecord, TelegramUpdateCursorRecord,
    TelegramUserPairingRecord,
};

impl TelegramRepository for PersistenceStore {
    fn put_telegram_user_pairing(
        &self,
        record: &TelegramUserPairingRecord,
    ) -> Result<(), StoreError> {
        validate_identifier(&record.token)?;
        let mut client = self.client()?;
        let mut transaction = client.transaction()?;
        transaction.execute(
            "SELECT pg_advisory_xact_lock($1)",
            &[&record.telegram_user_id],
        )?;
        transaction.execute(
            "DELETE FROM telegram_user_pairings WHERE telegram_user_id = $1 OR token = $2",
            &[&record.telegram_user_id, &record.token],
        )?;
        transaction.execute(
            "INSERT INTO telegram_user_pairings (
                token, telegram_user_id, telegram_chat_id, telegram_username,
                telegram_display_name, status, created_at, expires_at, activated_at
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
            &[
                &record.token,
                &record.telegram_user_id,
                &record.telegram_chat_id,
                &record.telegram_username,
                &record.telegram_display_name,
                &record.status,
                &record.created_at,
                &record.expires_at,
                &record.activated_at,
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    fn get_telegram_user_pairing_by_token(
        &self,
        token: &str,
    ) -> Result<Option<TelegramUserPairingRecord>, StoreError> {
        self.with_client(|client| {
            client
                .query_opt(
                    "SELECT token, telegram_user_id, telegram_chat_id, telegram_username,
                            telegram_display_name, status, created_at, expires_at, activated_at
                     FROM telegram_user_pairings
                     WHERE token = $1",
                    &[&token],
                )
                .map(|row| row.map(|row| telegram_user_pairing_from_row(&row)))
                .map_err(StoreError::from)
        })
    }

    fn get_telegram_user_pairing_by_user_id(
        &self,
        telegram_user_id: i64,
    ) -> Result<Option<TelegramUserPairingRecord>, StoreError> {
        self.with_client(|client| {
            client
                .query_opt(
                    "SELECT token, telegram_user_id, telegram_chat_id, telegram_username,
                            telegram_display_name, status, created_at, expires_at, activated_at
                     FROM telegram_user_pairings
                     WHERE telegram_user_id = $1",
                    &[&telegram_user_id],
                )
                .map(|row| row.map(|row| telegram_user_pairing_from_row(&row)))
                .map_err(StoreError::from)
        })
    }

    fn list_telegram_user_pairings(&self) -> Result<Vec<TelegramUserPairingRecord>, StoreError> {
        self.query_telegram_user_pairings(
            "SELECT token, telegram_user_id, telegram_chat_id, telegram_username,
                    telegram_display_name, status, created_at, expires_at, activated_at
             FROM telegram_user_pairings
             ORDER BY created_at ASC, telegram_user_id ASC",
            &[],
        )
    }

    fn put_telegram_chat_binding(
        &self,
        record: &TelegramChatBindingRecord,
    ) -> Result<(), StoreError> {
        if let Some(selected_session_id) = record.selected_session_id.as_deref() {
            validate_identifier(selected_session_id)?;
        }
        if let Some(default_agent_profile_id) = record.default_agent_profile_id.as_deref() {
            validate_identifier(default_agent_profile_id)?;
        }
        self.with_client(|client| {
            client.execute(
                "INSERT INTO telegram_chat_bindings (
                    telegram_chat_id, scope, owner_telegram_user_id, selected_session_id, default_agent_profile_id,
                    last_delivered_transcript_created_at, last_delivered_transcript_id,
                    inbound_queue_mode, inbound_coalesce_window_ms, created_at, updated_at
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                 ON CONFLICT(telegram_chat_id) DO UPDATE SET
                    scope = excluded.scope,
                    owner_telegram_user_id = excluded.owner_telegram_user_id,
                    selected_session_id = excluded.selected_session_id,
                    default_agent_profile_id = excluded.default_agent_profile_id,
                    last_delivered_transcript_created_at = excluded.last_delivered_transcript_created_at,
                    last_delivered_transcript_id = excluded.last_delivered_transcript_id,
                    inbound_queue_mode = excluded.inbound_queue_mode,
                    inbound_coalesce_window_ms = excluded.inbound_coalesce_window_ms,
                    created_at = excluded.created_at,
                    updated_at = excluded.updated_at",
                &[
                    &record.telegram_chat_id,
                    &record.scope,
                    &record.owner_telegram_user_id,
                    &record.selected_session_id,
                    &record.default_agent_profile_id,
                    &record.last_delivered_transcript_created_at,
                    &record.last_delivered_transcript_id,
                    &record.inbound_queue_mode,
                    &record.inbound_coalesce_window_ms,
                    &record.created_at,
                    &record.updated_at,
                ],
            )?;
            Ok(())
        })
    }

    fn get_telegram_chat_binding(
        &self,
        telegram_chat_id: i64,
    ) -> Result<Option<TelegramChatBindingRecord>, StoreError> {
        self.with_client(|client| {
            client
                .query_opt(
                    "SELECT telegram_chat_id, scope, owner_telegram_user_id, selected_session_id,
                            default_agent_profile_id, last_delivered_transcript_created_at, last_delivered_transcript_id,
                            inbound_queue_mode, inbound_coalesce_window_ms, created_at, updated_at
                     FROM telegram_chat_bindings
                     WHERE telegram_chat_id = $1",
                    &[&telegram_chat_id],
                )
                .map(|row| row.map(|row| telegram_chat_binding_from_row(&row)))
                .map_err(StoreError::from)
        })
    }

    fn list_telegram_chat_bindings(&self) -> Result<Vec<TelegramChatBindingRecord>, StoreError> {
        self.query_telegram_chat_bindings(
            "SELECT telegram_chat_id, scope, owner_telegram_user_id, selected_session_id,
                    default_agent_profile_id, last_delivered_transcript_created_at, last_delivered_transcript_id,
                    inbound_queue_mode, inbound_coalesce_window_ms, created_at, updated_at
             FROM telegram_chat_bindings
             ORDER BY telegram_chat_id ASC",
            &[],
        )
    }

    fn put_telegram_chat_status(
        &self,
        record: &TelegramChatStatusRecord,
    ) -> Result<(), StoreError> {
        self.with_client(|client| {
            client.execute(
                "INSERT INTO telegram_chat_statuses (
                    telegram_chat_id, message_id, state, expires_at, created_at, updated_at
                 ) VALUES ($1, $2, $3, $4, $5, $6)
                 ON CONFLICT(telegram_chat_id) DO UPDATE SET
                    message_id = excluded.message_id,
                    state = excluded.state,
                    expires_at = excluded.expires_at,
                    created_at = excluded.created_at,
                    updated_at = excluded.updated_at",
                &[
                    &record.telegram_chat_id,
                    &record.message_id,
                    &record.state,
                    &record.expires_at,
                    &record.created_at,
                    &record.updated_at,
                ],
            )?;
            Ok(())
        })
    }

    fn get_telegram_chat_status(
        &self,
        telegram_chat_id: i64,
    ) -> Result<Option<TelegramChatStatusRecord>, StoreError> {
        self.with_client(|client| {
            client
                .query_opt(
                    "SELECT telegram_chat_id, message_id, state, expires_at, created_at, updated_at
                     FROM telegram_chat_statuses
                     WHERE telegram_chat_id = $1",
                    &[&telegram_chat_id],
                )
                .map(|row| row.map(|row| telegram_chat_status_from_row(&row)))
                .map_err(StoreError::from)
        })
    }

    fn list_telegram_chat_statuses(&self) -> Result<Vec<TelegramChatStatusRecord>, StoreError> {
        self.query_telegram_chat_statuses(
            "SELECT telegram_chat_id, message_id, state, expires_at, created_at, updated_at
             FROM telegram_chat_statuses
             ORDER BY telegram_chat_id ASC",
            &[],
        )
    }

    fn delete_telegram_chat_status(&self, telegram_chat_id: i64) -> Result<bool, StoreError> {
        self.with_client(|client| {
            client
                .execute(
                    "DELETE FROM telegram_chat_statuses WHERE telegram_chat_id = $1",
                    &[&telegram_chat_id],
                )
                .map(|affected| affected > 0)
                .map_err(StoreError::from)
        })
    }

    fn put_telegram_update_cursor(
        &self,
        record: &TelegramUpdateCursorRecord,
    ) -> Result<(), StoreError> {
        validate_identifier(&record.consumer)?;
        self.with_client(|client| {
            client.execute(
                "INSERT INTO telegram_update_cursors (
                    consumer, update_id, updated_at
                 ) VALUES ($1, $2, $3)
                 ON CONFLICT(consumer) DO UPDATE SET
                    update_id = excluded.update_id,
                    updated_at = excluded.updated_at",
                &[&record.consumer, &record.update_id, &record.updated_at],
            )?;
            Ok(())
        })
    }

    fn get_telegram_update_cursor(
        &self,
        consumer: &str,
    ) -> Result<Option<TelegramUpdateCursorRecord>, StoreError> {
        self.with_client(|client| {
            client
                .query_opt(
                    "SELECT consumer, update_id, updated_at
                     FROM telegram_update_cursors
                     WHERE consumer = $1",
                    &[&consumer],
                )
                .map(|row| row.map(|row| telegram_update_cursor_from_row(&row)))
                .map_err(StoreError::from)
        })
    }
}

impl PersistenceStore {
    fn query_telegram_user_pairings(
        &self,
        sql: &str,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<TelegramUserPairingRecord>, StoreError> {
        self.with_client(|client| {
            client
                .query(sql, params)
                .map(|rows| rows.iter().map(telegram_user_pairing_from_row).collect())
                .map_err(StoreError::from)
        })
    }

    fn query_telegram_chat_bindings(
        &self,
        sql: &str,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<TelegramChatBindingRecord>, StoreError> {
        self.with_client(|client| {
            client
                .query(sql, params)
                .map(|rows| rows.iter().map(telegram_chat_binding_from_row).collect())
                .map_err(StoreError::from)
        })
    }

    fn query_telegram_chat_statuses(
        &self,
        sql: &str,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<TelegramChatStatusRecord>, StoreError> {
        self.with_client(|client| {
            client
                .query(sql, params)
                .map(|rows| rows.iter().map(telegram_chat_status_from_row).collect())
                .map_err(StoreError::from)
        })
    }
}

fn telegram_user_pairing_from_row(row: &Row) -> TelegramUserPairingRecord {
    TelegramUserPairingRecord {
        token: row.get(0),
        telegram_user_id: row.get(1),
        telegram_chat_id: row.get(2),
        telegram_username: row.get(3),
        telegram_display_name: row.get(4),
        status: row.get(5),
        created_at: row.get(6),
        expires_at: row.get(7),
        activated_at: row.get(8),
    }
}

fn telegram_chat_binding_from_row(row: &Row) -> TelegramChatBindingRecord {
    TelegramChatBindingRecord {
        telegram_chat_id: row.get(0),
        scope: row.get(1),
        owner_telegram_user_id: row.get(2),
        selected_session_id: row.get(3),
        default_agent_profile_id: row.get(4),
        last_delivered_transcript_created_at: row.get(5),
        last_delivered_transcript_id: row.get(6),
        inbound_queue_mode: row.get(7),
        inbound_coalesce_window_ms: row.get(8),
        created_at: row.get(9),
        updated_at: row.get(10),
    }
}

fn telegram_chat_status_from_row(row: &Row) -> TelegramChatStatusRecord {
    TelegramChatStatusRecord {
        telegram_chat_id: row.get(0),
        message_id: row.get(1),
        state: row.get(2),
        expires_at: row.get(3),
        created_at: row.get(4),
        updated_at: row.get(5),
    }
}

fn telegram_update_cursor_from_row(row: &Row) -> TelegramUpdateCursorRecord {
    TelegramUpdateCursorRecord {
        consumer: row.get(0),
        update_id: row.get(1),
        updated_at: row.get(2),
    }
}
