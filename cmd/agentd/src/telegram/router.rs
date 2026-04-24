use super::backend::TelegramBackend;
use super::client::{TelegramClient, TelegramClientError, TelegramCommandSpec};
use super::polling::next_confirmed_offset;
use super::render::{
    TELEGRAM_MESSAGE_TEXT_SOFT_CAP, TelegramRenderedChunk, chunk_message_text, render_help_message,
    render_model_response_chunks, render_pairing_message, render_pairing_required_message,
    render_session_created, render_session_list, render_session_selected, render_usage,
};
use crate::bootstrap::{App, BootstrapError, SessionPreferencesPatch, SessionSummary};
use crate::diagnostics::DiagnosticEventBuilder;
use crate::execution::{ChatExecutionEvent, ChatTurnExecutionReport};
use agent_persistence::{
    TelegramChatBindingRecord, TelegramRepository, TelegramUpdateCursorRecord,
    TelegramUserPairingRecord, TranscriptRecord, TranscriptRepository, audit::AuditLogConfig,
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use teloxide::types::{Message, Update, UpdateKind, User};

const TELEGRAM_CONSUMER_DEFAULT: &str = "telegram-main";
const TELEGRAM_SCOPE_PRIVATE: &str = "private";
const TELEGRAM_SCOPE_GROUP: &str = "group";
const TELEGRAM_PAIRING_STATUS_PENDING: &str = "pending";
const TELEGRAM_PAIRING_STATUS_ACTIVATED: &str = "activated";
const TELEGRAM_WORKING_TEXT: &str = "Working...";

static TELEGRAM_PAIRING_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
enum ParsedTelegramCommand {
    Start,
    Help,
    New {
        title: Option<String>,
    },
    Sessions,
    Use {
        session_id: String,
    },
    Judge {
        message: String,
    },
    Agent {
        target_agent_id: String,
        message: String,
    },
    InvalidUsage(String),
}

#[derive(Debug, Clone)]
pub struct TelegramWorker<B> {
    app: App,
    backend: B,
    client: TelegramClient,
    consumer: String,
    audit: AuditLogConfig,
    bot_username: Arc<Mutex<Option<String>>>,
}

impl<B> TelegramWorker<B>
where
    B: TelegramBackend,
{
    pub fn new(app: App, backend: B, client: TelegramClient) -> Self {
        Self::with_consumer(app, backend, client, TELEGRAM_CONSUMER_DEFAULT)
    }

    pub fn with_consumer(
        app: App,
        backend: B,
        client: TelegramClient,
        consumer: impl Into<String>,
    ) -> Self {
        let audit = AuditLogConfig::from_config(&app.config);
        Self {
            app,
            backend,
            client,
            consumer: consumer.into(),
            audit,
            bot_username: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn poll_once(&self) -> Result<usize, BootstrapError> {
        self.deliver_pending_session_notifications().await?;

        let offset = {
            let store = self.app.store()?;
            store
                .get_telegram_update_cursor(&self.consumer)?
                .and_then(|record| i32::try_from(record.update_id).ok())
        };
        let updates = self
            .client
            .poll_updates(offset, 100, self.poll_timeout_seconds())
            .await
            .map_err(map_client_error)?;
        let count = updates.len();

        self.deliver_pending_session_notifications().await?;

        for update in updates {
            let next_offset = next_confirmed_offset(std::slice::from_ref(&update))
                .map(i64::from)
                .unwrap_or_default();
            self.handle_update(update).await?;
            self.persist_update_cursor(next_offset)?;
        }

        Ok(count)
    }

    pub async fn run_forever(&self) -> Result<(), BootstrapError> {
        loop {
            match self.poll_once().await {
                Ok(_) => {}
                Err(error) => {
                    DiagnosticEventBuilder::new(
                        &self.app.config,
                        "error",
                        "telegram",
                        "poll.error",
                        "telegram polling iteration failed",
                    )
                    .error(error.to_string())
                    .emit(&self.audit);
                    tokio::time::sleep(self.poll_interval()).await;
                    continue;
                }
            }
            tokio::time::sleep(self.poll_interval()).await;
        }
    }

    pub async fn register_commands(&self) -> Result<(), BootstrapError> {
        self.client
            .register_commands(&default_command_specs())
            .await
            .map_err(map_client_error)
    }

    async fn handle_update(&self, update: Update) -> Result<(), BootstrapError> {
        match update.kind {
            UpdateKind::Message(message) => self.handle_message(message).await,
            _ => Ok(()),
        }
    }

    async fn handle_message(&self, message: Message) -> Result<(), BootstrapError> {
        let Some(from) = message.from.as_ref() else {
            return Ok(());
        };
        let Some(raw_text) = message.text() else {
            return Ok(());
        };
        let text = raw_text.trim();
        if text.is_empty() {
            return Ok(());
        }

        if message.chat.is_private() {
            return self.handle_private_message(&message, from, text).await;
        }

        if message.chat.is_group() || message.chat.is_supergroup() {
            return self.handle_group_message(&message, from, text).await;
        }

        Ok(())
    }

    async fn handle_private_message(
        &self,
        message: &Message,
        from: &User,
        text: &str,
    ) -> Result<(), BootstrapError> {
        let chat_id = message.chat.id.0;

        if let Some(command) = parse_command(text) {
            return self.handle_command(message, from, command).await;
        }

        let now = unix_timestamp()?;
        if self
            .load_activated_pairing(telegram_user_id(from)?)?
            .is_none()
        {
            self.send_text_chunks(chat_id, &render_pairing_required_message())
                .await?;
            return Ok(());
        }

        let session = self
            .resolve_or_create_private_session(chat_id, telegram_user_id(from)?, now)
            .await?;
        let ack = self
            .client
            .send_text(chat_id, TELEGRAM_WORKING_TEXT)
            .await
            .map_err(map_client_error)?;
        let result = self
            .execute_chat_turn(chat_id, ack.id.0, session.id.clone(), text.to_string(), now)
            .await;

        match result {
            Ok(report) => {
                self.deliver_chat_report(chat_id, ack.id.0, &report).await?;
                self.mark_chat_delivered_to_latest_transcript(chat_id, report.session_id.as_str())?;
                Ok(())
            }
            Err(error) => {
                self.client
                    .edit_text(chat_id, ack.id.0, &format!("Chat turn failed: {error}"))
                    .await
                    .map_err(map_client_error)?;
                Ok(())
            }
        }
    }

    async fn handle_group_message(
        &self,
        message: &Message,
        from: &User,
        text: &str,
    ) -> Result<(), BootstrapError> {
        if text.starts_with('/') {
            let command = if command_targets_named_bot(text) {
                parse_command_for_bot(text, &self.bot_username().await?)
            } else {
                parse_command(text)
            };
            if let Some(command) = command {
                return self.handle_group_command(message, from, command).await;
            }
            return Ok(());
        }

        let chat_id = message.chat.id.0;
        let telegram_user_id = telegram_user_id(from)?;
        let content = if self.app.config.telegram.group_require_mention {
            match strip_bot_mention(text, &self.bot_username().await?) {
                Some(content) => content,
                None => return Ok(()),
            }
        } else {
            text.to_string()
        };

        if content.trim().is_empty() {
            return Ok(());
        }

        if self.load_activated_pairing(telegram_user_id)?.is_none() {
            self.send_text_chunks(chat_id, &render_pairing_required_message())
                .await?;
            return Ok(());
        }

        let now = unix_timestamp()?;
        let session = self.resolve_or_create_group_session(chat_id, now).await?;
        let ack = self
            .client
            .send_text(chat_id, TELEGRAM_WORKING_TEXT)
            .await
            .map_err(map_client_error)?;
        let result = self
            .execute_chat_turn(chat_id, ack.id.0, session.id.clone(), content, now)
            .await;

        match result {
            Ok(report) => {
                self.deliver_chat_report(chat_id, ack.id.0, &report).await?;
                self.mark_chat_delivered_to_latest_transcript(chat_id, report.session_id.as_str())?;
                Ok(())
            }
            Err(error) => {
                self.client
                    .edit_text(chat_id, ack.id.0, &format!("Chat turn failed: {error}"))
                    .await
                    .map_err(map_client_error)?;
                Ok(())
            }
        }
    }

    async fn handle_group_command(
        &self,
        message: &Message,
        from: &User,
        command: ParsedTelegramCommand,
    ) -> Result<(), BootstrapError> {
        let chat_id = message.chat.id.0;
        let telegram_user_id = telegram_user_id(from)?;
        match command {
            ParsedTelegramCommand::Start => {
                self.send_text_chunks(
                    chat_id,
                    "Open a private chat with the bot and send /start to get a pairing key.",
                )
                .await
            }
            ParsedTelegramCommand::Help => {
                if self.load_activated_pairing(telegram_user_id)?.is_none() {
                    self.send_text_chunks(chat_id, &render_pairing_required_message())
                        .await?;
                    return Ok(());
                }
                self.send_text_chunks(chat_id, &render_help_message()).await
            }
            ParsedTelegramCommand::New { title } => {
                if self.load_activated_pairing(telegram_user_id)?.is_none() {
                    self.send_text_chunks(chat_id, &render_pairing_required_message())
                        .await?;
                    return Ok(());
                }
                let now = unix_timestamp()?;
                let summary = self
                    .create_and_bind_group_session(chat_id, title.as_deref(), now)
                    .await?;
                self.send_text_chunks(chat_id, &render_session_created(&summary))
                    .await
            }
            ParsedTelegramCommand::Sessions => {
                if self.load_activated_pairing(telegram_user_id)?.is_none() {
                    self.send_text_chunks(chat_id, &render_pairing_required_message())
                        .await?;
                    return Ok(());
                }
                let selected = self
                    .app
                    .store()?
                    .get_telegram_chat_binding(chat_id)?
                    .and_then(|record| record.selected_session_id);
                let summaries = self.list_session_summaries().await?;
                self.send_text_chunks(
                    chat_id,
                    &render_session_list(&summaries, selected.as_deref()),
                )
                .await
            }
            ParsedTelegramCommand::Use { session_id } => {
                if self.load_activated_pairing(telegram_user_id)?.is_none() {
                    self.send_text_chunks(chat_id, &render_pairing_required_message())
                        .await?;
                    return Ok(());
                }
                let now = unix_timestamp()?;
                let summary = self
                    .normalize_telegram_session_preferences(
                        self.session_summary(session_id.clone()).await?,
                    )
                    .await?;
                self.put_group_binding(chat_id, Some(summary.id.clone()), now)?;
                self.send_text_chunks(chat_id, &render_session_selected(&summary))
                    .await
            }
            ParsedTelegramCommand::Judge { message } => {
                if self.load_activated_pairing(telegram_user_id)?.is_none() {
                    self.send_text_chunks(chat_id, &render_pairing_required_message())
                        .await?;
                    return Ok(());
                }
                let now = unix_timestamp()?;
                let session = self.resolve_or_create_group_session(chat_id, now).await?;
                let response = self
                    .send_agent_message(session.id.as_str(), "judge", message.as_str())
                    .await?;
                self.send_text_chunks(chat_id, &response).await
            }
            ParsedTelegramCommand::Agent {
                target_agent_id,
                message,
            } => {
                if self.load_activated_pairing(telegram_user_id)?.is_none() {
                    self.send_text_chunks(chat_id, &render_pairing_required_message())
                        .await?;
                    return Ok(());
                }
                let now = unix_timestamp()?;
                let session = self.resolve_or_create_group_session(chat_id, now).await?;
                let response = self
                    .send_agent_message(
                        session.id.as_str(),
                        target_agent_id.as_str(),
                        message.as_str(),
                    )
                    .await?;
                self.send_text_chunks(chat_id, &response).await
            }
            ParsedTelegramCommand::InvalidUsage(message) => {
                self.send_text_chunks(chat_id, &message).await
            }
        }
    }

    async fn handle_command(
        &self,
        message: &Message,
        from: &User,
        command: ParsedTelegramCommand,
    ) -> Result<(), BootstrapError> {
        let chat_id = message.chat.id.0;
        let telegram_user_id = telegram_user_id(from)?;
        match command {
            ParsedTelegramCommand::Start => {
                let token = self.create_or_refresh_pairing(from, chat_id)?;
                self.send_text_chunks(chat_id, &render_pairing_message(&token))
                    .await
            }
            ParsedTelegramCommand::Help => {
                if self.load_activated_pairing(telegram_user_id)?.is_none() {
                    self.send_text_chunks(chat_id, &render_pairing_required_message())
                        .await?;
                    return Ok(());
                }
                self.send_text_chunks(chat_id, &render_help_message()).await
            }
            ParsedTelegramCommand::New { title } => {
                if self.load_activated_pairing(telegram_user_id)?.is_none() {
                    self.send_text_chunks(chat_id, &render_pairing_required_message())
                        .await?;
                    return Ok(());
                }
                let now = unix_timestamp()?;
                let summary = self
                    .create_and_bind_session(chat_id, telegram_user_id, title.as_deref(), now)
                    .await?;
                self.send_text_chunks(chat_id, &render_session_created(&summary))
                    .await
            }
            ParsedTelegramCommand::Sessions => {
                if self.load_activated_pairing(telegram_user_id)?.is_none() {
                    self.send_text_chunks(chat_id, &render_pairing_required_message())
                        .await?;
                    return Ok(());
                }
                let selected = self
                    .app
                    .store()?
                    .get_telegram_chat_binding(chat_id)?
                    .and_then(|record| record.selected_session_id);
                let summaries = self.list_session_summaries().await?;
                self.send_text_chunks(
                    chat_id,
                    &render_session_list(&summaries, selected.as_deref()),
                )
                .await
            }
            ParsedTelegramCommand::Use { session_id } => {
                if self.load_activated_pairing(telegram_user_id)?.is_none() {
                    self.send_text_chunks(chat_id, &render_pairing_required_message())
                        .await?;
                    return Ok(());
                }
                let now = unix_timestamp()?;
                let summary = self
                    .normalize_telegram_session_preferences(
                        self.session_summary(session_id.clone()).await?,
                    )
                    .await?;
                self.put_private_binding(chat_id, telegram_user_id, Some(summary.id.clone()), now)?;
                self.send_text_chunks(chat_id, &render_session_selected(&summary))
                    .await
            }
            ParsedTelegramCommand::Judge { message } => {
                if self.load_activated_pairing(telegram_user_id)?.is_none() {
                    self.send_text_chunks(chat_id, &render_pairing_required_message())
                        .await?;
                    return Ok(());
                }
                let now = unix_timestamp()?;
                let session = self
                    .resolve_or_create_private_session(chat_id, telegram_user_id, now)
                    .await?;
                let response = self
                    .send_agent_message(session.id.as_str(), "judge", message.as_str())
                    .await?;
                self.send_text_chunks(chat_id, &response).await
            }
            ParsedTelegramCommand::Agent {
                target_agent_id,
                message,
            } => {
                if self.load_activated_pairing(telegram_user_id)?.is_none() {
                    self.send_text_chunks(chat_id, &render_pairing_required_message())
                        .await?;
                    return Ok(());
                }
                let now = unix_timestamp()?;
                let session = self
                    .resolve_or_create_private_session(chat_id, telegram_user_id, now)
                    .await?;
                let response = self
                    .send_agent_message(
                        session.id.as_str(),
                        target_agent_id.as_str(),
                        message.as_str(),
                    )
                    .await?;
                self.send_text_chunks(chat_id, &response).await
            }
            ParsedTelegramCommand::InvalidUsage(message) => {
                self.send_text_chunks(chat_id, &message).await
            }
        }
    }

    async fn send_text_chunks(&self, chat_id: i64, text: &str) -> Result<(), BootstrapError> {
        for chunk in chunk_message_text(text, TELEGRAM_MESSAGE_TEXT_SOFT_CAP) {
            self.client
                .send_text(chat_id, &chunk)
                .await
                .map_err(map_client_error)?;
        }
        Ok(())
    }

    async fn send_model_text_chunks(&self, chat_id: i64, text: &str) -> Result<(), BootstrapError> {
        for chunk in render_model_response_chunks(text, TELEGRAM_MESSAGE_TEXT_SOFT_CAP) {
            if chunk.parse_mode_html {
                self.client
                    .send_html(chat_id, &chunk.text)
                    .await
                    .map_err(map_client_error)?;
            } else {
                self.client
                    .send_text(chat_id, &chunk.text)
                    .await
                    .map_err(map_client_error)?;
            }
        }
        Ok(())
    }

    async fn deliver_chat_report(
        &self,
        chat_id: i64,
        message_id: i32,
        report: &ChatTurnExecutionReport,
    ) -> Result<(), BootstrapError> {
        let mut chunks =
            render_model_response_chunks(&report.output_text, TELEGRAM_MESSAGE_TEXT_SOFT_CAP)
                .into_iter();
        let first = chunks.next().unwrap_or(TelegramRenderedChunk {
            text: String::new(),
            parse_mode_html: false,
        });
        if first.parse_mode_html {
            self.client
                .edit_html(chat_id, message_id, &first.text)
                .await
                .map_err(map_client_error)?;
        } else {
            self.client
                .edit_text(chat_id, message_id, &first.text)
                .await
                .map_err(map_client_error)?;
        }
        for chunk in chunks {
            if chunk.parse_mode_html {
                self.client
                    .send_html(chat_id, &chunk.text)
                    .await
                    .map_err(map_client_error)?;
            } else {
                self.client
                    .send_text(chat_id, &chunk.text)
                    .await
                    .map_err(map_client_error)?;
            }
        }
        Ok(())
    }

    async fn resolve_or_create_private_session(
        &self,
        chat_id: i64,
        telegram_user_id: i64,
        now: i64,
    ) -> Result<SessionSummary, BootstrapError> {
        let selected_session_id = self
            .app
            .store()?
            .get_telegram_chat_binding(chat_id)?
            .and_then(|record| record.selected_session_id);
        if let Some(session_id) = selected_session_id {
            self.ensure_chat_delivery_cursor_initialized(chat_id, session_id.as_str())?;
            return self
                .normalize_telegram_session_preferences(self.session_summary(session_id).await?)
                .await;
        }

        if !self.app.config.telegram.private_chat_auto_create_session {
            return Err(BootstrapError::Usage {
                reason: "no telegram session selected; use /new or /use".to_string(),
            });
        }

        self.create_and_bind_session(chat_id, telegram_user_id, None, now)
            .await
    }

    async fn create_and_bind_session(
        &self,
        chat_id: i64,
        telegram_user_id: i64,
        title: Option<&str>,
        now: i64,
    ) -> Result<SessionSummary, BootstrapError> {
        let summary = self.create_session_auto(title.map(str::to_string)).await?;
        let summary = self.normalize_telegram_session_preferences(summary).await?;
        self.put_private_binding(chat_id, telegram_user_id, Some(summary.id.clone()), now)?;
        Ok(summary)
    }

    async fn create_and_bind_group_session(
        &self,
        chat_id: i64,
        title: Option<&str>,
        now: i64,
    ) -> Result<SessionSummary, BootstrapError> {
        let summary = self.create_session_auto(title.map(str::to_string)).await?;
        let summary = self.normalize_telegram_session_preferences(summary).await?;
        self.put_group_binding(chat_id, Some(summary.id.clone()), now)?;
        Ok(summary)
    }

    async fn resolve_or_create_group_session(
        &self,
        chat_id: i64,
        now: i64,
    ) -> Result<SessionSummary, BootstrapError> {
        let selected_session_id = self
            .app
            .store()?
            .get_telegram_chat_binding(chat_id)?
            .and_then(|record| record.selected_session_id);
        if let Some(session_id) = selected_session_id {
            self.ensure_chat_delivery_cursor_initialized(chat_id, session_id.as_str())?;
            return self
                .normalize_telegram_session_preferences(self.session_summary(session_id).await?)
                .await;
        }

        let summary = self.create_session_auto(None).await?;
        let summary = self.normalize_telegram_session_preferences(summary).await?;
        self.put_group_binding(chat_id, Some(summary.id.clone()), now)?;
        Ok(summary)
    }

    fn put_private_binding(
        &self,
        chat_id: i64,
        telegram_user_id: i64,
        selected_session_id: Option<String>,
        now: i64,
    ) -> Result<(), BootstrapError> {
        let existing = self.app.store()?.get_telegram_chat_binding(chat_id)?;
        let cursor = binding_cursor_for_selection(
            &self.app,
            existing.as_ref(),
            selected_session_id.as_deref(),
        )?;
        self.app
            .store()?
            .put_telegram_chat_binding(&TelegramChatBindingRecord {
                telegram_chat_id: chat_id,
                scope: TELEGRAM_SCOPE_PRIVATE.to_string(),
                owner_telegram_user_id: Some(telegram_user_id),
                selected_session_id,
                last_delivered_transcript_created_at: cursor.created_at,
                last_delivered_transcript_id: cursor.transcript_id,
                created_at: existing
                    .as_ref()
                    .map(|record| record.created_at)
                    .unwrap_or(now),
                updated_at: now,
            })?;
        Ok(())
    }

    fn put_group_binding(
        &self,
        chat_id: i64,
        selected_session_id: Option<String>,
        now: i64,
    ) -> Result<(), BootstrapError> {
        let existing = self.app.store()?.get_telegram_chat_binding(chat_id)?;
        let cursor = binding_cursor_for_selection(
            &self.app,
            existing.as_ref(),
            selected_session_id.as_deref(),
        )?;
        self.app
            .store()?
            .put_telegram_chat_binding(&TelegramChatBindingRecord {
                telegram_chat_id: chat_id,
                scope: TELEGRAM_SCOPE_GROUP.to_string(),
                owner_telegram_user_id: None,
                selected_session_id,
                last_delivered_transcript_created_at: cursor.created_at,
                last_delivered_transcript_id: cursor.transcript_id,
                created_at: existing
                    .as_ref()
                    .map(|record| record.created_at)
                    .unwrap_or(now),
                updated_at: now,
            })?;
        Ok(())
    }

    fn ensure_chat_delivery_cursor_initialized(
        &self,
        chat_id: i64,
        session_id: &str,
    ) -> Result<(), BootstrapError> {
        let Some(binding) = self.app.store()?.get_telegram_chat_binding(chat_id)? else {
            return Ok(());
        };
        if binding.last_delivered_transcript_created_at.is_some()
            || binding.last_delivered_transcript_id.is_some()
        {
            return Ok(());
        }
        self.update_chat_delivery_cursor(
            chat_id,
            session_id,
            latest_delivery_cursor(&self.app, session_id)?,
        )
    }

    fn mark_chat_delivered_to_latest_transcript(
        &self,
        chat_id: i64,
        session_id: &str,
    ) -> Result<(), BootstrapError> {
        self.update_chat_delivery_cursor(
            chat_id,
            session_id,
            latest_delivery_cursor(&self.app, session_id)?,
        )
    }

    fn mark_chat_delivered_to_transcript(
        &self,
        chat_id: i64,
        session_id: &str,
        transcript: &TranscriptRecord,
    ) -> Result<(), BootstrapError> {
        self.update_chat_delivery_cursor(
            chat_id,
            session_id,
            DeliveryCursor {
                created_at: Some(transcript.created_at),
                transcript_id: Some(transcript.id.clone()),
            },
        )
    }

    fn update_chat_delivery_cursor(
        &self,
        chat_id: i64,
        session_id: &str,
        cursor: DeliveryCursor,
    ) -> Result<(), BootstrapError> {
        let Some(mut binding) = self.app.store()?.get_telegram_chat_binding(chat_id)? else {
            return Ok(());
        };
        if binding.selected_session_id.as_deref() != Some(session_id) {
            return Ok(());
        }
        binding.last_delivered_transcript_created_at = cursor.created_at.or(Some(0));
        binding.last_delivered_transcript_id = cursor.transcript_id.or_else(|| Some(String::new()));
        binding.updated_at = unix_timestamp()?;
        self.app.store()?.put_telegram_chat_binding(&binding)?;
        Ok(())
    }

    async fn deliver_pending_session_notifications(&self) -> Result<(), BootstrapError> {
        let bindings = self.app.store()?.list_telegram_chat_bindings()?;
        for binding in bindings {
            let Some(session_id) = binding.selected_session_id.as_deref() else {
                continue;
            };
            let pending = self.pending_assistant_transcripts_for_binding(&binding, session_id)?;
            for transcript in pending {
                self.send_model_text_chunks(binding.telegram_chat_id, &transcript.content)
                    .await?;
                self.mark_chat_delivered_to_transcript(
                    binding.telegram_chat_id,
                    session_id,
                    &transcript,
                )?;
            }
        }
        Ok(())
    }

    fn pending_assistant_transcripts_for_binding(
        &self,
        binding: &TelegramChatBindingRecord,
        session_id: &str,
    ) -> Result<Vec<TranscriptRecord>, BootstrapError> {
        if binding.last_delivered_transcript_created_at.is_none()
            && binding.last_delivered_transcript_id.is_none()
        {
            self.update_chat_delivery_cursor(
                binding.telegram_chat_id,
                session_id,
                latest_delivery_cursor(&self.app, session_id)?,
            )?;
            return Ok(Vec::new());
        }

        let transcripts = self.app.store()?.list_transcripts_for_session(session_id)?;
        Ok(transcripts
            .into_iter()
            .filter(|transcript| transcript.kind == "assistant")
            .filter(|transcript| transcript_is_after_binding_cursor(transcript, binding))
            .collect())
    }

    fn create_or_refresh_pairing(
        &self,
        from: &User,
        chat_id: i64,
    ) -> Result<String, BootstrapError> {
        let now = unix_timestamp()?;
        if let Some(existing) = self.load_activated_pairing(telegram_user_id(from)?)? {
            return Ok(existing.token);
        }

        let token = generate_pairing_token();
        self.app
            .store()?
            .put_telegram_user_pairing(&TelegramUserPairingRecord {
                token: token.clone(),
                telegram_user_id: telegram_user_id(from)?,
                telegram_chat_id: chat_id,
                telegram_username: from.username.clone(),
                telegram_display_name: telegram_display_name(from),
                status: TELEGRAM_PAIRING_STATUS_PENDING.to_string(),
                created_at: now,
                expires_at: now
                    + i64::try_from(self.app.config.telegram.pairing_token_ttl_seconds)
                        .unwrap_or(0),
                activated_at: None,
            })?;
        Ok(token)
    }

    fn load_activated_pairing(
        &self,
        telegram_user_id: i64,
    ) -> Result<Option<TelegramUserPairingRecord>, BootstrapError> {
        Ok(self
            .app
            .store()?
            .get_telegram_user_pairing_by_user_id(telegram_user_id)?
            .filter(|record| record.status == TELEGRAM_PAIRING_STATUS_ACTIVATED))
    }

    fn persist_update_cursor(&self, next_offset: i64) -> Result<(), BootstrapError> {
        if next_offset <= 0 {
            return Ok(());
        }
        self.app
            .store()?
            .put_telegram_update_cursor(&TelegramUpdateCursorRecord {
                consumer: self.consumer.clone(),
                update_id: next_offset,
                updated_at: unix_timestamp()?,
            })?;
        Ok(())
    }

    fn poll_interval(&self) -> Duration {
        Duration::from_millis(self.app.config.telegram.poll_interval_ms)
    }

    fn poll_timeout_seconds(&self) -> u32 {
        u32::try_from(self.app.config.telegram.poll_request_timeout_seconds).unwrap_or(u32::MAX)
    }

    fn progress_update_min_interval(&self) -> Duration {
        Duration::from_millis(self.app.config.telegram.progress_update_min_interval_ms)
    }

    async fn bot_username(&self) -> Result<String, BootstrapError> {
        if let Some(username) = self
            .bot_username
            .lock()
            .expect("telegram bot username")
            .clone()
        {
            return Ok(username);
        }

        let username = self
            .client
            .get_me()
            .await
            .map_err(map_client_error)?
            .username()
            .to_string();
        let mut cached = self.bot_username.lock().expect("telegram bot username");
        if cached.is_none() {
            *cached = Some(username.clone());
        }
        Ok(cached.clone().unwrap_or(username))
    }

    async fn list_session_summaries(&self) -> Result<Vec<SessionSummary>, BootstrapError> {
        let backend = self.backend.clone();
        tokio::task::spawn_blocking(move || backend.list_session_summaries())
            .await
            .map_err(map_join_error)?
    }

    async fn create_session_auto(
        &self,
        title: Option<String>,
    ) -> Result<SessionSummary, BootstrapError> {
        let backend = self.backend.clone();
        tokio::task::spawn_blocking(move || backend.create_session_auto(title.as_deref()))
            .await
            .map_err(map_join_error)?
    }

    async fn update_session_preferences(
        &self,
        session_id: String,
        patch: SessionPreferencesPatch,
    ) -> Result<SessionSummary, BootstrapError> {
        let backend = self.backend.clone();
        tokio::task::spawn_blocking(move || backend.update_session_preferences(&session_id, patch))
            .await
            .map_err(map_join_error)?
    }

    async fn session_summary(&self, session_id: String) -> Result<SessionSummary, BootstrapError> {
        let backend = self.backend.clone();
        tokio::task::spawn_blocking(move || backend.session_summary(&session_id))
            .await
            .map_err(map_join_error)?
    }

    async fn send_agent_message(
        &self,
        session_id: &str,
        target_agent_id: &str,
        message: &str,
    ) -> Result<String, BootstrapError> {
        let backend = self.backend.clone();
        let session_id = session_id.to_string();
        let target_agent_id = target_agent_id.to_string();
        let message = message.to_string();
        tokio::task::spawn_blocking(move || {
            backend.send_agent_message(&session_id, &target_agent_id, &message)
        })
        .await
        .map_err(map_join_error)?
    }

    async fn normalize_telegram_session_preferences(
        &self,
        summary: SessionSummary,
    ) -> Result<SessionSummary, BootstrapError> {
        let mut patch = SessionPreferencesPatch::default();
        let mut changed = false;

        if summary.auto_approve != self.app.config.telegram.default_autoapprove {
            patch.auto_approve = Some(self.app.config.telegram.default_autoapprove);
            changed = true;
        }
        if summary.reasoning_visible {
            patch.reasoning_visible = Some(false);
            changed = true;
        }
        if summary.think_level.as_deref() != Some("off") {
            patch.think_level = Some(Some("off".to_string()));
            changed = true;
        }

        if changed {
            self.update_session_preferences(summary.id.clone(), patch)
                .await
        } else {
            Ok(summary)
        }
    }

    async fn execute_chat_turn(
        &self,
        chat_id: i64,
        message_id: i32,
        session_id: String,
        message: String,
        now: i64,
    ) -> Result<ChatTurnExecutionReport, BootstrapError> {
        let backend = self.backend.clone();
        let (event_sender, mut event_receiver) = tokio::sync::mpsc::unbounded_channel();
        let join_handle = tokio::task::spawn_blocking(move || {
            let mut observer = |event: ChatExecutionEvent| {
                let _ = event_sender.send(event);
            };
            backend.execute_chat_turn(&session_id, &message, now, &mut observer)
        });
        tokio::pin!(join_handle);

        let mut pending_status = None::<String>;
        let mut last_status = TELEGRAM_WORKING_TEXT.to_string();
        let interval = self.progress_update_min_interval();
        let sleep = tokio::time::sleep(interval);
        tokio::pin!(sleep);

        loop {
            tokio::select! {
                result = &mut join_handle => {
                    return result.map_err(map_join_error)?;
                }
                maybe_event = event_receiver.recv() => {
                    let Some(event) = maybe_event else {
                        continue;
                    };
                    if let Some(status_text) = render_progress_status(&event) {
                        pending_status = Some(status_text);
                    }
                }
                _ = &mut sleep, if pending_status.is_some() => {
                    let status_text = pending_status.take().expect("pending status");
                    if status_text != last_status {
                        self.client
                            .edit_text(chat_id, message_id, &status_text)
                            .await
                            .map_err(map_client_error)?;
                        last_status = status_text;
                    }
                    sleep.as_mut().reset(tokio::time::Instant::now() + interval);
                }
            }
        }
    }
}

fn render_progress_status(event: &ChatExecutionEvent) -> Option<String> {
    let details = match event {
        ChatExecutionEvent::ReasoningDelta(_) => vec!["Phase: thinking".to_string()],
        ChatExecutionEvent::AssistantTextDelta(_) => vec!["Phase: drafting".to_string()],
        ChatExecutionEvent::ProviderLoopProgress {
            current_round,
            max_rounds,
        } => vec![
            "Phase: continuation".to_string(),
            format!("Round: {current_round}/{max_rounds}"),
        ],
        ChatExecutionEvent::ToolStatus {
            tool_name,
            summary,
            status,
        } => {
            let mut details = vec![
                "Phase: tool".to_string(),
                format!("Tool: {tool_name}"),
                format!("Status: {}", render_tool_status_label(status)),
            ];
            if !summary.trim().is_empty() {
                details.push(format!("Detail: {summary}"));
            }
            details
        }
    };
    Some(format!("{TELEGRAM_WORKING_TEXT}\n{}", details.join("\n")))
}

fn render_tool_status_label(status: &crate::execution::ToolExecutionStatus) -> &'static str {
    match status {
        crate::execution::ToolExecutionStatus::Requested => "requested",
        crate::execution::ToolExecutionStatus::WaitingApproval => "waiting approval",
        crate::execution::ToolExecutionStatus::Approved => "approved",
        crate::execution::ToolExecutionStatus::Running => "running",
        crate::execution::ToolExecutionStatus::Completed => "completed",
        crate::execution::ToolExecutionStatus::Failed => "failed",
    }
}

pub fn default_command_specs() -> Vec<TelegramCommandSpec> {
    vec![
        TelegramCommandSpec::new("start", "Get a pairing key"),
        TelegramCommandSpec::new("help", "Show Telegram help"),
        TelegramCommandSpec::new("new", "Create and select a session"),
        TelegramCommandSpec::new("sessions", "List sessions"),
        TelegramCommandSpec::new("use", "Select a session by id"),
        TelegramCommandSpec::new("judge", "Send a message to Judge"),
        TelegramCommandSpec::new("agent", "Send a message to another agent"),
    ]
}

fn parse_command(text: &str) -> Option<ParsedTelegramCommand> {
    let trimmed = text.trim();
    if !trimmed.starts_with('/') {
        return None;
    }
    let mut parts = trimmed.splitn(2, char::is_whitespace);
    let command = parts.next()?.trim_start_matches('/');
    let command = command.split('@').next().unwrap_or(command);
    let args = parts.next().map(str::trim).unwrap_or("");
    parse_command_parts(command, args)
}

fn parse_command_for_bot(text: &str, bot_username: &str) -> Option<ParsedTelegramCommand> {
    let trimmed = text.trim();
    if !trimmed.starts_with('/') {
        return None;
    }
    let mut parts = trimmed.splitn(2, char::is_whitespace);
    let command = parts.next()?.trim_start_matches('/');
    let (command, target_bot) = match command.split_once('@') {
        Some((command, target_bot)) => (command, Some(target_bot)),
        None => (command, None),
    };
    if let Some(target_bot) = target_bot
        && !target_bot.eq_ignore_ascii_case(bot_username)
    {
        return None;
    }
    let args = parts.next().map(str::trim).unwrap_or("");
    parse_command_parts(command, args)
}

fn parse_command_parts(command: &str, args: &str) -> Option<ParsedTelegramCommand> {
    match command {
        "start" => Some(ParsedTelegramCommand::Start),
        "help" => Some(ParsedTelegramCommand::Help),
        "new" => Some(ParsedTelegramCommand::New {
            title: (!args.is_empty()).then(|| args.to_string()),
        }),
        "sessions" => Some(ParsedTelegramCommand::Sessions),
        "use" => {
            if args.is_empty() {
                Some(ParsedTelegramCommand::InvalidUsage(render_usage(
                    "use",
                    "<session_id>",
                )))
            } else {
                Some(ParsedTelegramCommand::Use {
                    session_id: args.to_string(),
                })
            }
        }
        "judge" => {
            if args.is_empty() {
                Some(ParsedTelegramCommand::InvalidUsage(render_usage(
                    "judge",
                    "<message>",
                )))
            } else {
                Some(ParsedTelegramCommand::Judge {
                    message: args.to_string(),
                })
            }
        }
        "agent" => {
            let mut parts = args.splitn(2, char::is_whitespace);
            let Some(target_agent_id) = parts.next().map(str::trim).filter(|part| !part.is_empty())
            else {
                return Some(ParsedTelegramCommand::InvalidUsage(render_usage(
                    "agent",
                    "<agent_id> <message>",
                )));
            };
            let Some(message) = parts.next().map(str::trim).filter(|part| !part.is_empty()) else {
                return Some(ParsedTelegramCommand::InvalidUsage(render_usage(
                    "agent",
                    "<agent_id> <message>",
                )));
            };
            Some(ParsedTelegramCommand::Agent {
                target_agent_id: target_agent_id.to_string(),
                message: message.to_string(),
            })
        }
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DeliveryCursor {
    created_at: Option<i64>,
    transcript_id: Option<String>,
}

fn binding_cursor_for_selection(
    app: &App,
    existing: Option<&TelegramChatBindingRecord>,
    selected_session_id: Option<&str>,
) -> Result<DeliveryCursor, BootstrapError> {
    let Some(selected_session_id) = selected_session_id else {
        return Ok(DeliveryCursor {
            created_at: None,
            transcript_id: None,
        });
    };
    if existing.and_then(|record| record.selected_session_id.as_deref())
        == Some(selected_session_id)
    {
        return Ok(DeliveryCursor {
            created_at: existing.and_then(|record| record.last_delivered_transcript_created_at),
            transcript_id: existing.and_then(|record| record.last_delivered_transcript_id.clone()),
        });
    }
    latest_delivery_cursor(app, selected_session_id)
}

fn latest_delivery_cursor(app: &App, session_id: &str) -> Result<DeliveryCursor, BootstrapError> {
    let latest = app.store()?.get_latest_transcript_for_session(session_id)?;
    Ok(match latest {
        Some(transcript) => DeliveryCursor {
            created_at: Some(transcript.created_at),
            transcript_id: Some(transcript.id),
        },
        None => DeliveryCursor {
            created_at: Some(0),
            transcript_id: Some(String::new()),
        },
    })
}

fn transcript_is_after_binding_cursor(
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

fn map_client_error(error: TelegramClientError) -> BootstrapError {
    BootstrapError::Stream(std::io::Error::other(error.to_string()))
}

fn map_join_error(error: tokio::task::JoinError) -> BootstrapError {
    BootstrapError::Stream(std::io::Error::other(format!(
        "telegram blocking task failed: {error}"
    )))
}

fn telegram_display_name(user: &User) -> String {
    match user.last_name.as_deref() {
        Some(last_name) if !last_name.trim().is_empty() => {
            format!("{} {}", user.first_name, last_name)
        }
        _ => user.first_name.clone(),
    }
}

fn telegram_user_id(user: &User) -> Result<i64, BootstrapError> {
    i64::try_from(user.id.0).map_err(|_| BootstrapError::Usage {
        reason: format!("telegram user id {} does not fit into i64", user.id.0),
    })
}

fn strip_bot_mention(text: &str, bot_username: &str) -> Option<String> {
    let mention = format!("@{bot_username}");
    if !text.contains(&mention) {
        return None;
    }

    let stripped = text.replace(&mention, " ");
    let normalized = stripped.split_whitespace().collect::<Vec<_>>().join(" ");
    (!normalized.is_empty()).then_some(normalized)
}

fn command_targets_named_bot(text: &str) -> bool {
    text.split_whitespace()
        .next()
        .is_some_and(|command| command.contains('@'))
}

fn generate_pairing_token() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    let sequence = TELEGRAM_PAIRING_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("tg{millis}{sequence}")
}

fn unix_timestamp() -> Result<i64, BootstrapError> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(BootstrapError::Clock)?
        .as_secs() as i64)
}
