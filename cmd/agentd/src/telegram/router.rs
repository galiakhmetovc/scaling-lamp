use super::backend::TelegramBackend;
use super::client::{TelegramClient, TelegramClientError, TelegramCommandSpec};
use super::commands::{
    ParsedTelegramCommand, TELEGRAM_INBOUND_QUEUE_MODE_COALESCE, TELEGRAM_INBOUND_QUEUE_MODE_QUEUE,
    TELEGRAM_INBOUND_QUEUE_MODE_REJECT, TELEGRAM_INBOUND_QUEUE_MODE_RESTART,
    TELEGRAM_MIN_COALESCE_WINDOW_MS, TelegramQueueAction, coalesce_window_seconds,
    is_session_operator_command, is_valid_telegram_queue_mode, parse_command,
    parse_command_for_bot,
};
use super::delivery::{
    DeliveryCursor, TelegramDeliveryLimiter, TelegramDeliveryScope, TelegramDeliveryTrace,
    duration_millis, telegram_delivery_error_is_permanent, telegram_span_id, telegram_trace_id,
};
use super::files::{
    IncomingTelegramFile, artifact_metadata_value, extract_incoming_file, metadata_string,
    render_uploaded_file_turn_input, telegram_artifact_id,
};
use super::polling::next_confirmed_offset;
use super::progress::{
    TelegramProgressTracker, render_failed_temporary_status_html, render_file_delivery_failed_html,
    render_temporary_status_html,
};
use super::render::{
    TELEGRAM_CAPTION_SOFT_CAP, TELEGRAM_MESSAGE_TEXT_SOFT_CAP, chunk_message_text,
    render_help_message, render_model_response_chunks, render_pairing_message,
    render_pairing_required_message, render_session_created, render_session_list,
    render_session_selected, truncate_caption,
};
use crate::bootstrap::{App, BootstrapError, SessionPreferencesPatch, SessionSummary};
use crate::diagnostics::DiagnosticEventBuilder;
use crate::execution::{ChatExecutionEvent, ChatTurnExecutionReport};
use crate::store_retry::{
    SQLITE_LOCK_RETRY_ATTEMPTS, SQLITE_LOCK_RETRY_DELAY_MS, retry_store_sync,
};
use agent_persistence::{
    ArtifactRecord, ArtifactRepository, FileDeliveryRepository, FileDeliveryRequestRecord,
    PersistenceStore, SessionInboxEventRecord, SessionInboxRepository, StoreError,
    TelegramChatBindingRecord, TelegramChatStatusRecord, TelegramRepository,
    TelegramUpdateCursorRecord, TelegramUserPairingRecord, TraceRepository, TranscriptRecord,
    TranscriptRepository, audit::AuditLogConfig,
};
use agent_runtime::inbox::{SessionInboxEvent, SessionInboxEventPayload};
use serde_json::json;
use std::collections::HashSet;
use std::future::Future;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use teloxide::types::{Message, Update, UpdateKind, User};

const TELEGRAM_CONSUMER_DEFAULT: &str = "telegram-main";
const TELEGRAM_SCOPE_PRIVATE: &str = "private";
const TELEGRAM_SCOPE_GROUP: &str = "group";
const TELEGRAM_PAIRING_STATUS_PENDING: &str = "pending";
const TELEGRAM_PAIRING_STATUS_ACTIVATED: &str = "activated";
const TELEGRAM_CHAT_STATUS_ACTIVE: &str = "active";
const TELEGRAM_CHAT_STATUS_STALE: &str = "stale";
const TELEGRAM_TYPING_INITIAL_DELAY_MILLIS: u64 = 750;
const TELEGRAM_TYPING_HEARTBEAT_INTERVAL_SECONDS: u64 = 4;
const TELEGRAM_STATUS_TTL_SECONDS: i64 = 30 * 60;
const TELEGRAM_DELIVERY_RETRY_ATTEMPTS: usize = 3;
const TELEGRAM_DELIVERY_RETRY_BASE_DELAY_MS: u64 = 250;
const TELEGRAM_CHAT_TURN_FAST_SETTLE_MILLIS: u64 = 50;
const TELEGRAM_INBOUND_QUEUE_SOURCE: &str = "telegram";

static TELEGRAM_PAIRING_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
pub struct TelegramWorker<B> {
    app: App,
    backend: B,
    client: TelegramClient,
    consumer: String,
    audit: AuditLogConfig,
    bot_username: Arc<Mutex<Option<String>>>,
    delivery_limiter: Arc<Mutex<TelegramDeliveryLimiter>>,
    active_chat_turns: Arc<Mutex<HashSet<String>>>,
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
            delivery_limiter: Arc::new(Mutex::new(TelegramDeliveryLimiter::default())),
            active_chat_turns: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    pub async fn poll_once(&self) -> Result<usize, BootstrapError> {
        self.cleanup_expired_chat_statuses().await?;
        self.deliver_pending_session_notifications().await?;

        let offset = self.with_store_retry(|store| {
            store
                .get_telegram_update_cursor(&self.consumer)
                .map(|cursor| cursor.and_then(|record| i32::try_from(record.update_id).ok()))
        })?;
        let updates = self
            .client
            .poll_updates(offset, 100, self.poll_timeout_seconds())
            .await
            .map_err(map_client_error)?;
        let count = updates.len();

        self.deliver_pending_session_notifications().await?;
        self.cleanup_expired_chat_statuses().await?;

        for update in updates {
            let update_id = update.id.0;
            let next_offset = next_confirmed_offset(std::slice::from_ref(&update))
                .map(i64::from)
                .unwrap_or_default();
            if let Err(error) = self.handle_update(update).await {
                DiagnosticEventBuilder::new(
                    &self.app.config,
                    "error",
                    "telegram",
                    "update.error",
                    "telegram update handling failed",
                )
                .error(error.to_string())
                .field("update_id", i64::from(update_id))
                .emit(&self.audit);
            }
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
        self.cleanup_chat_status_for_new_input(message.chat.id.0)
            .await?;

        if let Some(file) = extract_incoming_file(&message) {
            return self.handle_file_message(&message, from, file).await;
        }

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
        self.start_or_queue_chat_turn(chat_id, message.id.0, session.id, text.to_string(), now)
            .await
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
        self.start_or_queue_chat_turn(chat_id, message.id.0, session.id, content, now)
            .await
    }

    async fn handle_file_message(
        &self,
        message: &Message,
        from: &User,
        file: IncomingTelegramFile,
    ) -> Result<(), BootstrapError> {
        let chat_id = message.chat.id.0;
        let telegram_user_id = telegram_user_id(from)?;
        let now = unix_timestamp()?;

        if self.load_activated_pairing(telegram_user_id)?.is_none() {
            self.send_text_chunks(chat_id, &render_pairing_required_message())
                .await?;
            return Ok(());
        }

        let session = if message.chat.is_private() {
            self.resolve_or_create_private_session(chat_id, telegram_user_id, now)
                .await?
        } else if message.chat.is_group() || message.chat.is_supergroup() {
            if self.app.config.telegram.group_require_mention {
                let caption = message.caption().unwrap_or_default();
                if strip_bot_mention(caption, &self.bot_username().await?).is_none() {
                    return Ok(());
                }
            }
            self.resolve_or_create_group_session(chat_id, now).await?
        } else {
            return Ok(());
        };

        let artifact = self
            .download_and_store_telegram_file(message, &session, &file, now)
            .await?;
        let caption = message
            .caption()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let turn_input = render_uploaded_file_turn_input(&artifact, &file, caption);
        self.start_or_queue_chat_turn(chat_id, message.id.0, session.id, turn_input, now)
            .await
    }

    async fn handle_group_command(
        &self,
        message: &Message,
        from: &User,
        command: ParsedTelegramCommand,
    ) -> Result<(), BootstrapError> {
        let chat_id = message.chat.id.0;
        let telegram_user_id = telegram_user_id(from)?;
        if is_session_operator_command(&command) {
            if self.load_activated_pairing(telegram_user_id)?.is_none() {
                self.send_text_chunks(chat_id, &render_pairing_required_message())
                    .await?;
                return Ok(());
            }
            return self.handle_session_operator_command(chat_id, command).await;
        }
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
            ParsedTelegramCommand::Files => {
                if self.load_activated_pairing(telegram_user_id)?.is_none() {
                    self.send_text_chunks(chat_id, &render_pairing_required_message())
                        .await?;
                    return Ok(());
                }
                self.send_chat_files_list(chat_id).await
            }
            ParsedTelegramCommand::File { artifact_id } => {
                if self.load_activated_pairing(telegram_user_id)?.is_none() {
                    self.send_text_chunks(chat_id, &render_pairing_required_message())
                        .await?;
                    return Ok(());
                }
                self.send_chat_artifact_file(chat_id, artifact_id.as_str())
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
            ParsedTelegramCommand::Status
            | ParsedTelegramCommand::Jobs
            | ParsedTelegramCommand::Queue { .. }
            | ParsedTelegramCommand::Stop
            | ParsedTelegramCommand::Cancel
            | ParsedTelegramCommand::Model { .. }
            | ParsedTelegramCommand::Think { .. }
            | ParsedTelegramCommand::Reasoning { .. }
            | ParsedTelegramCommand::AutoApprove { .. }
            | ParsedTelegramCommand::Compact
            | ParsedTelegramCommand::Skills
            | ParsedTelegramCommand::EnableSkill { .. }
            | ParsedTelegramCommand::DisableSkill { .. } => {
                unreachable!("session operator commands are handled before regular group commands")
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
        if is_session_operator_command(&command) {
            if self.load_activated_pairing(telegram_user_id)?.is_none() {
                self.send_text_chunks(chat_id, &render_pairing_required_message())
                    .await?;
                return Ok(());
            }
            return self.handle_session_operator_command(chat_id, command).await;
        }
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
            ParsedTelegramCommand::Files => {
                if self.load_activated_pairing(telegram_user_id)?.is_none() {
                    self.send_text_chunks(chat_id, &render_pairing_required_message())
                        .await?;
                    return Ok(());
                }
                self.send_chat_files_list(chat_id).await
            }
            ParsedTelegramCommand::File { artifact_id } => {
                if self.load_activated_pairing(telegram_user_id)?.is_none() {
                    self.send_text_chunks(chat_id, &render_pairing_required_message())
                        .await?;
                    return Ok(());
                }
                self.send_chat_artifact_file(chat_id, artifact_id.as_str())
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
            ParsedTelegramCommand::Status
            | ParsedTelegramCommand::Jobs
            | ParsedTelegramCommand::Queue { .. }
            | ParsedTelegramCommand::Stop
            | ParsedTelegramCommand::Cancel
            | ParsedTelegramCommand::Model { .. }
            | ParsedTelegramCommand::Think { .. }
            | ParsedTelegramCommand::Reasoning { .. }
            | ParsedTelegramCommand::AutoApprove { .. }
            | ParsedTelegramCommand::Compact
            | ParsedTelegramCommand::Skills
            | ParsedTelegramCommand::EnableSkill { .. }
            | ParsedTelegramCommand::DisableSkill { .. } => unreachable!(
                "session operator commands are handled before regular private commands"
            ),
        }
    }

    async fn download_and_store_telegram_file(
        &self,
        message: &Message,
        session: &SessionSummary,
        file: &IncomingTelegramFile,
        now: i64,
    ) -> Result<ArtifactRecord, BootstrapError> {
        let advertised_size = usize::try_from(file.size).unwrap_or(usize::MAX);
        if advertised_size > self.app.config.telegram.max_download_bytes {
            return Err(BootstrapError::Usage {
                reason: format!(
                    "telegram file {} is too large: {} bytes > {} bytes",
                    file.file_name, advertised_size, self.app.config.telegram.max_download_bytes
                ),
            });
        }

        let remote_file = self
            .client
            .get_file(&file.file_id)
            .await
            .map_err(map_client_error)?;
        let bytes = self
            .client
            .download_file(remote_file.path.as_str())
            .await
            .map_err(map_client_error)?;
        if bytes.len() > self.app.config.telegram.max_download_bytes {
            return Err(BootstrapError::Usage {
                reason: format!(
                    "downloaded telegram file {} is too large: {} bytes > {} bytes",
                    file.file_name,
                    bytes.len(),
                    self.app.config.telegram.max_download_bytes
                ),
            });
        }

        let artifact_id = telegram_artifact_id(message.chat.id.0, message.id.0);
        let metadata = json!({
            "source": "telegram",
            "telegram_content_kind": file.content_kind,
            "telegram_chat_id": message.chat.id.0,
            "telegram_message_id": message.id.0,
            "telegram_file_id": file.file_id,
            "telegram_file_unique_id": file.file_unique_id,
            "telegram_file_path": remote_file.path,
            "file_name": file.file_name,
            "mime_type": file.mime_type,
            "file_size": file.size,
            "caption": message.caption(),
        });
        let artifact = ArtifactRecord {
            id: artifact_id.clone(),
            session_id: session.id.clone(),
            kind: "telegram_file".to_string(),
            metadata_json: serde_json::to_string(&metadata).map_err(|source| {
                BootstrapError::Usage {
                    reason: format!("failed to serialize telegram artifact metadata: {source}"),
                }
            })?,
            path: PathBuf::from("artifacts").join(format!("{artifact_id}.bin")),
            bytes,
            created_at: now,
        };
        self.with_store_retry(|store| store.put_artifact(&artifact))?;
        Ok(artifact)
    }

    async fn send_chat_files_list(&self, chat_id: i64) -> Result<(), BootstrapError> {
        let Some(session_id) = self.selected_session_id_for_chat(chat_id)? else {
            self.send_text_chunks(
                chat_id,
                "No selected session. Use /new or /use <session_id>.",
            )
            .await?;
            return Ok(());
        };
        let artifacts =
            self.with_store_retry(|store| store.list_artifacts_for_session(session_id.as_str()))?;
        let files = artifacts
            .into_iter()
            .filter(|artifact| artifact.kind == "telegram_file")
            .collect::<Vec<_>>();
        if files.is_empty() {
            self.send_text_chunks(chat_id, "Files in current session: <empty>")
                .await?;
            return Ok(());
        }

        let mut lines = vec![format!("Files in current session ({session_id}):")];
        for artifact in files {
            let metadata = artifact_metadata_value(&artifact);
            let file_name = metadata_string(&metadata, "file_name")
                .unwrap_or_else(|| format!("{}.bin", artifact.id));
            let file_size = metadata
                .get("file_size")
                .and_then(|value| value.as_u64())
                .map(|value| value.to_string())
                .unwrap_or_else(|| artifact.bytes.len().to_string());
            lines.push(format!(
                "- {file_name} ({}) bytes={} command=/file {}",
                artifact.id, file_size, artifact.id
            ));
        }

        self.send_text_chunks(chat_id, &lines.join("\n")).await
    }

    async fn send_chat_artifact_file(
        &self,
        chat_id: i64,
        artifact_id: &str,
    ) -> Result<(), BootstrapError> {
        let Some(session_id) = self.selected_session_id_for_chat(chat_id)? else {
            self.send_text_chunks(
                chat_id,
                "No selected session. Use /new or /use <session_id>.",
            )
            .await?;
            return Ok(());
        };
        let Some(artifact) = self.with_store_retry(|store| store.get_artifact(artifact_id))? else {
            self.send_text_chunks(chat_id, &format!("File not found: {artifact_id}"))
                .await?;
            return Ok(());
        };
        if artifact.session_id != session_id {
            self.send_text_chunks(
                chat_id,
                &format!("File not found in current session: {artifact_id}"),
            )
            .await?;
            return Ok(());
        }
        if artifact.bytes.len() > self.app.config.telegram.max_upload_bytes {
            self.send_text_chunks(
                chat_id,
                &format!(
                    "File is too large for Telegram delivery: {} bytes, limit is {} bytes",
                    artifact.bytes.len(),
                    self.app.config.telegram.max_upload_bytes
                ),
            )
            .await?;
            return Ok(());
        }
        let metadata = artifact_metadata_value(&artifact);
        let file_name = metadata_string(&metadata, "file_name")
            .unwrap_or_else(|| format!("{}.bin", artifact.id));
        let caption = format!("artifact_id={}\nfile_name={file_name}", artifact.id);
        self.send_document_delivered(chat_id, artifact.bytes, &file_name, Some(&caption))
            .await?;
        Ok(())
    }

    async fn handle_session_operator_command(
        &self,
        chat_id: i64,
        command: ParsedTelegramCommand,
    ) -> Result<(), BootstrapError> {
        let Some(session_id) = self.selected_session_id_for_chat(chat_id)? else {
            self.send_text_chunks(
                chat_id,
                "No selected session. Use /new or /use <session_id>.",
            )
            .await?;
            return Ok(());
        };

        match command {
            ParsedTelegramCommand::Status => {
                let summary = self.session_summary(session_id.clone()).await?;
                let active_run = self.render_active_run(session_id).await?;
                let queue_status = self.render_queue_status(chat_id)?;
                self.send_text_chunks(
                    chat_id,
                    &render_session_operator_status(&summary, &active_run, &queue_status),
                )
                .await
            }
            ParsedTelegramCommand::Jobs => {
                let jobs = self.render_session_background_jobs(session_id).await?;
                self.send_text_chunks(chat_id, &jobs).await
            }
            ParsedTelegramCommand::Queue { action } => {
                let response = self
                    .handle_queue_command(chat_id, session_id, action)
                    .await?;
                self.send_text_chunks(chat_id, &response).await
            }
            ParsedTelegramCommand::Stop => {
                let response = self.cancel_active_run(session_id).await?;
                self.send_text_chunks(chat_id, &response).await
            }
            ParsedTelegramCommand::Cancel => {
                let response = self.cancel_all_session_work(session_id).await?;
                self.send_text_chunks(chat_id, &response).await
            }
            ParsedTelegramCommand::Model { model } => {
                let summary = self
                    .update_session_preferences(
                        session_id,
                        SessionPreferencesPatch {
                            model: Some(model),
                            ..SessionPreferencesPatch::default()
                        },
                    )
                    .await?;
                self.send_text_chunks(
                    chat_id,
                    &format!("Model: {}", summary.model.as_deref().unwrap_or("<default>")),
                )
                .await
            }
            ParsedTelegramCommand::Think { level } => {
                let summary = self
                    .update_session_preferences(
                        session_id,
                        SessionPreferencesPatch {
                            think_level: Some(level),
                            ..SessionPreferencesPatch::default()
                        },
                    )
                    .await?;
                self.send_text_chunks(
                    chat_id,
                    &format!(
                        "Think level: {}",
                        summary.think_level.as_deref().unwrap_or("<default>")
                    ),
                )
                .await
            }
            ParsedTelegramCommand::Reasoning { visible } => {
                let summary = self
                    .update_session_preferences(
                        session_id,
                        SessionPreferencesPatch {
                            reasoning_visible: Some(visible),
                            ..SessionPreferencesPatch::default()
                        },
                    )
                    .await?;
                self.send_text_chunks(
                    chat_id,
                    &format!("Reasoning visible: {}", summary.reasoning_visible),
                )
                .await
            }
            ParsedTelegramCommand::AutoApprove { enabled } => {
                let summary = self
                    .update_session_preferences(
                        session_id,
                        SessionPreferencesPatch {
                            auto_approve: Some(enabled),
                            ..SessionPreferencesPatch::default()
                        },
                    )
                    .await?;
                self.send_text_chunks(chat_id, &format!("Auto-approve: {}", summary.auto_approve))
                    .await
            }
            ParsedTelegramCommand::Compact => {
                let summary = self.compact_session(session_id).await?;
                self.send_text_chunks(
                    chat_id,
                    &format!(
                        "Compaction finished: {} ({}) compactifications={}",
                        summary.title, summary.id, summary.compactifications
                    ),
                )
                .await
            }
            ParsedTelegramCommand::Skills => {
                let skills = self.render_session_skills(session_id).await?;
                self.send_text_chunks(chat_id, &skills).await
            }
            ParsedTelegramCommand::EnableSkill { skill_name } => {
                let skills = self.enable_session_skill(session_id, skill_name).await?;
                self.send_text_chunks(chat_id, &skills).await
            }
            ParsedTelegramCommand::DisableSkill { skill_name } => {
                let skills = self.disable_session_skill(session_id, skill_name).await?;
                self.send_text_chunks(chat_id, &skills).await
            }
            _ => Ok(()),
        }
    }

    fn selected_session_id_for_chat(&self, chat_id: i64) -> Result<Option<String>, BootstrapError> {
        Ok(self
            .with_store_retry(|store| store.get_telegram_chat_binding(chat_id))?
            .and_then(|binding| binding.selected_session_id))
    }

    async fn send_text_delivered(
        &self,
        chat_id: i64,
        text: &str,
    ) -> Result<Message, BootstrapError> {
        self.send_text_delivered_with_trace(chat_id, text, None)
            .await
    }

    async fn send_text_delivered_with_trace(
        &self,
        chat_id: i64,
        text: &str,
        trace: Option<&TelegramDeliveryTrace>,
    ) -> Result<Message, BootstrapError> {
        self.deliver_with_retry_with_trace(chat_id, "send_text", trace, || {
            let client = self.client.clone();
            let text = text.to_string();
            async move { client.send_text(chat_id, &text).await }
        })
        .await
    }

    async fn send_html_delivered(
        &self,
        chat_id: i64,
        html: &str,
    ) -> Result<Message, BootstrapError> {
        self.send_html_delivered_with_trace(chat_id, html, None)
            .await
    }

    async fn send_html_delivered_with_trace(
        &self,
        chat_id: i64,
        html: &str,
        trace: Option<&TelegramDeliveryTrace>,
    ) -> Result<Message, BootstrapError> {
        self.deliver_with_retry_with_trace(chat_id, "send_html", trace, || {
            let client = self.client.clone();
            let html = html.to_string();
            async move { client.send_html(chat_id, &html).await }
        })
        .await
    }

    async fn edit_html_delivered(
        &self,
        chat_id: i64,
        message_id: i32,
        html: &str,
    ) -> Result<Message, BootstrapError> {
        self.deliver_with_retry(chat_id, "edit_html", || {
            let client = self.client.clone();
            let html = html.to_string();
            async move { client.edit_html(chat_id, message_id, &html).await }
        })
        .await
    }

    async fn delete_message_delivered(
        &self,
        chat_id: i64,
        message_id: i32,
    ) -> Result<(), BootstrapError> {
        self.deliver_with_retry(chat_id, "delete_message", || {
            let client = self.client.clone();
            async move { client.delete_message(chat_id, message_id).await }
        })
        .await
    }

    async fn send_typing_delivered(&self, chat_id: i64) -> Result<(), BootstrapError> {
        self.deliver_with_retry(chat_id, "send_typing", || {
            let client = self.client.clone();
            async move { client.send_typing(chat_id).await }
        })
        .await
    }

    async fn send_document_delivered(
        &self,
        chat_id: i64,
        bytes: Vec<u8>,
        file_name: &str,
        caption: Option<&str>,
    ) -> Result<Message, BootstrapError> {
        self.send_document_delivered_with_trace(chat_id, bytes, file_name, caption, None)
            .await
    }

    async fn send_document_delivered_with_trace(
        &self,
        chat_id: i64,
        bytes: Vec<u8>,
        file_name: &str,
        caption: Option<&str>,
        trace: Option<&TelegramDeliveryTrace>,
    ) -> Result<Message, BootstrapError> {
        self.deliver_with_retry_with_trace(chat_id, "send_document", trace, || {
            let client = self.client.clone();
            let bytes = bytes.clone();
            let file_name = file_name.to_string();
            let caption = caption.map(str::to_string);
            async move {
                client
                    .send_document(chat_id, bytes, &file_name, caption.as_deref())
                    .await
            }
        })
        .await
    }

    async fn deliver_with_retry<T, F, Fut>(
        &self,
        chat_id: i64,
        op: &'static str,
        action: F,
    ) -> Result<T, BootstrapError>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, TelegramClientError>>,
    {
        self.deliver_with_retry_with_trace(chat_id, op, None, action)
            .await
    }

    async fn deliver_with_retry_with_trace<T, F, Fut>(
        &self,
        chat_id: i64,
        op: &'static str,
        trace: Option<&TelegramDeliveryTrace>,
        mut action: F,
    ) -> Result<T, BootstrapError>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, TelegramClientError>>,
    {
        let started = Instant::now();
        for attempt in 1..=TELEGRAM_DELIVERY_RETRY_ATTEMPTS {
            self.wait_for_delivery_slot(chat_id).await;
            match action().await {
                Ok(value) => {
                    self.emit_delivery_event(chat_id, op, attempt, started.elapsed(), None, trace);
                    return Ok(value);
                }
                Err(error)
                    if attempt < TELEGRAM_DELIVERY_RETRY_ATTEMPTS
                        && !telegram_delivery_error_is_permanent(&error) =>
                {
                    DiagnosticEventBuilder::new(
                        &self.app.config,
                        "warn",
                        "telegram",
                        "delivery.retry",
                        "telegram delivery attempt failed",
                    )
                    .surface("telegram")
                    .entrypoint("telegram")
                    .outcome("retry")
                    .elapsed_ms(duration_millis(started.elapsed()))
                    .field("chat_id", chat_id)
                    .field("delivery_op", op)
                    .field("attempt", attempt)
                    .error(error.to_string())
                    .emit(&self.audit);
                    tokio::time::sleep(Duration::from_millis(
                        TELEGRAM_DELIVERY_RETRY_BASE_DELAY_MS * attempt as u64,
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
                        trace,
                    );
                    return Err(BootstrapError::Stream(std::io::Error::other(error_message)));
                }
            }
        }

        Err(BootstrapError::Stream(std::io::Error::other(
            "telegram delivery retry loop exhausted",
        )))
    }

    async fn wait_for_delivery_slot(&self, chat_id: i64) {
        let scope = self.delivery_scope_for_chat(chat_id);
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
            .delivery_limiter
            .lock()
            .expect("telegram delivery limiter")
            .reserve(chat_id, global_interval, chat_interval);
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
    }

    fn delivery_scope_for_chat(&self, chat_id: i64) -> TelegramDeliveryScope {
        if let Ok(Some(binding)) =
            self.with_store_retry(|store| store.get_telegram_chat_binding(chat_id))
            && binding.scope == TELEGRAM_SCOPE_GROUP
        {
            return TelegramDeliveryScope::Group;
        }
        if chat_id < 0 {
            TelegramDeliveryScope::Group
        } else {
            TelegramDeliveryScope::Private
        }
    }

    fn emit_delivery_event(
        &self,
        chat_id: i64,
        op: &'static str,
        attempt: usize,
        elapsed: Duration,
        error: Option<String>,
        trace: Option<&TelegramDeliveryTrace>,
    ) {
        let trace_id = trace
            .map(|trace| trace.trace_id.clone())
            .unwrap_or_else(|| telegram_trace_id(chat_id));
        let span_id = trace
            .map(|trace| {
                crate::trace::otel_span_id(
                    "telegram_delivery",
                    &format!("{}:{op}:{attempt}", trace.trace_id),
                )
            })
            .unwrap_or_else(|| telegram_span_id(chat_id, op, attempt));
        let mut event = DiagnosticEventBuilder::new(
            &self.app.config,
            if error.is_some() { "error" } else { "info" },
            "telegram",
            "delivery.request",
            "telegram delivery request completed",
        )
        .surface("telegram")
        .entrypoint("telegram")
        .trace_id(trace_id)
        .span_id(span_id)
        .outcome(if error.is_some() { "error" } else { "ok" })
        .elapsed_ms(duration_millis(elapsed))
        .field("chat_id", chat_id)
        .field("delivery_op", op)
        .field("attempt", attempt);
        if let Some(trace) = trace {
            event = event.parent_span_id(trace.parent_span_id.clone());
        }
        if let Some(error) = error {
            event = event.error(error);
        }
        event.emit(&self.audit);
    }

    async fn send_text_chunks(&self, chat_id: i64, text: &str) -> Result<(), BootstrapError> {
        for chunk in chunk_message_text(text, TELEGRAM_MESSAGE_TEXT_SOFT_CAP) {
            self.send_text_delivered(chat_id, &chunk).await?;
        }
        Ok(())
    }

    async fn send_model_text_chunks(&self, chat_id: i64, text: &str) -> Result<(), BootstrapError> {
        self.send_model_text_chunks_with_trace(chat_id, text, None)
            .await
    }

    async fn send_model_text_chunks_with_trace(
        &self,
        chat_id: i64,
        text: &str,
        trace: Option<&TelegramDeliveryTrace>,
    ) -> Result<(), BootstrapError> {
        for chunk in render_model_response_chunks(text, TELEGRAM_MESSAGE_TEXT_SOFT_CAP) {
            if chunk.parse_mode_html {
                self.send_html_delivered_with_trace(chat_id, &chunk.text, trace)
                    .await?;
            } else {
                self.send_text_delivered_with_trace(chat_id, &chunk.text, trace)
                    .await?;
            }
        }
        Ok(())
    }

    async fn send_temporary_status_message(
        &self,
        chat_id: i64,
        now: i64,
    ) -> Result<Message, BootstrapError> {
        let progress = TelegramProgressTracker::default();
        let message = self
            .send_html_delivered(chat_id, &render_temporary_status_html(progress.state()))
            .await?;
        self.put_chat_status_record(
            chat_id,
            message.id.0,
            TELEGRAM_CHAT_STATUS_ACTIVE,
            None,
            now,
        )?;
        Ok(message)
    }

    async fn fail_temporary_status_message(
        &self,
        chat_id: i64,
        message_id: i32,
        error: &str,
        now: i64,
    ) -> Result<(), BootstrapError> {
        self.edit_html_delivered(
            chat_id,
            message_id,
            &render_failed_temporary_status_html(error),
        )
        .await?;
        self.mark_chat_status_stale(chat_id, message_id, now)?;
        Ok(())
    }

    fn put_chat_status_record(
        &self,
        chat_id: i64,
        message_id: i32,
        state: &str,
        expires_at: Option<i64>,
        now: i64,
    ) -> Result<(), BootstrapError> {
        self.with_store_retry(|store| {
            let created_at = store
                .get_telegram_chat_status(chat_id)?
                .map(|record| record.created_at)
                .unwrap_or(now);
            store.put_telegram_chat_status(&TelegramChatStatusRecord {
                telegram_chat_id: chat_id,
                message_id,
                state: state.to_string(),
                expires_at,
                created_at,
                updated_at: now,
            })
        })
        .map(|_| ())
    }

    fn mark_chat_status_stale(
        &self,
        chat_id: i64,
        message_id: i32,
        now: i64,
    ) -> Result<(), BootstrapError> {
        let status_is_current = self.with_store_retry(|store| {
            Ok(store
                .get_telegram_chat_status(chat_id)?
                .is_some_and(|status| status.message_id == message_id))
        })?;
        if !status_is_current {
            return Ok(());
        }
        self.put_chat_status_record(
            chat_id,
            message_id,
            TELEGRAM_CHAT_STATUS_STALE,
            Some(now + TELEGRAM_STATUS_TTL_SECONDS),
            now,
        )
    }

    async fn cleanup_chat_status_for_new_input(&self, chat_id: i64) -> Result<(), BootstrapError> {
        let Some(status) =
            self.with_store_retry(|store| store.get_telegram_chat_status(chat_id))?
        else {
            return Ok(());
        };
        let _ = self
            .delete_message_delivered(chat_id, status.message_id)
            .await;
        self.with_store_retry(|store| store.delete_telegram_chat_status(chat_id))?;
        Ok(())
    }

    async fn cleanup_expired_chat_statuses(&self) -> Result<(), BootstrapError> {
        let now = unix_timestamp()?;
        let statuses = self.with_store_retry(|store| store.list_telegram_chat_statuses())?;
        for status in statuses {
            if status.state != TELEGRAM_CHAT_STATUS_STALE {
                continue;
            }
            if status.expires_at.is_none_or(|expires_at| expires_at > now) {
                continue;
            }
            if self
                .delete_message_delivered(status.telegram_chat_id, status.message_id)
                .await
                .is_ok()
            {
                self.with_store_retry(|store| {
                    store.delete_telegram_chat_status(status.telegram_chat_id)
                })?;
            }
        }
        Ok(())
    }

    async fn deliver_chat_report(
        &self,
        chat_id: i64,
        report: &ChatTurnExecutionReport,
        status_message_id: i32,
        now: i64,
    ) -> Result<(), BootstrapError> {
        let trace = self.telegram_delivery_trace_for_run(&report.run_id)?;
        self.send_model_text_chunks_with_trace(chat_id, &report.output_text, trace.as_ref())
            .await?;
        self.deliver_queued_file_requests(chat_id, &report.session_id, trace.as_ref(), now)
            .await?;
        self.mark_chat_status_stale(chat_id, status_message_id, now)?;
        Ok(())
    }

    async fn deliver_queued_file_requests(
        &self,
        chat_id: i64,
        session_id: &str,
        trace: Option<&TelegramDeliveryTrace>,
        now: i64,
    ) -> Result<(), BootstrapError> {
        let requests = self.with_store_retry(|store| {
            store.list_queued_file_delivery_requests_for_session(session_id)
        })?;
        for request in requests {
            if let Err(error) = self
                .deliver_queued_file_request(chat_id, session_id, &request, trace, now)
                .await
            {
                let error_message = error.to_string();
                self.mark_file_delivery_request_failed(&request, error_message.clone(), now)?;
                DiagnosticEventBuilder::new(
                    &self.app.config,
                    "error",
                    "telegram",
                    "file_delivery.error",
                    "telegram queued file delivery failed",
                )
                .surface("telegram")
                .entrypoint("telegram")
                .outcome("error")
                .field("chat_id", chat_id)
                .field("session_id", session_id)
                .field("delivery_request_id", request.id.as_str())
                .field("artifact_id", request.artifact_id.as_str())
                .error(error_message.clone())
                .emit(&self.audit);
                if let Err(notify_error) = self
                    .send_html_delivered_with_trace(
                        chat_id,
                        &render_file_delivery_failed_html(&request.file_name, &error_message),
                        trace,
                    )
                    .await
                {
                    DiagnosticEventBuilder::new(
                        &self.app.config,
                        "warn",
                        "telegram",
                        "file_delivery.notify_failed",
                        "telegram file delivery failure notification failed",
                    )
                    .surface("telegram")
                    .entrypoint("telegram")
                    .outcome("error")
                    .field("chat_id", chat_id)
                    .field("session_id", session_id)
                    .field("delivery_request_id", request.id.as_str())
                    .field("artifact_id", request.artifact_id.as_str())
                    .error(notify_error.to_string())
                    .emit(&self.audit);
                }
            }
        }
        Ok(())
    }

    async fn deliver_queued_file_request(
        &self,
        chat_id: i64,
        session_id: &str,
        request: &FileDeliveryRequestRecord,
        trace: Option<&TelegramDeliveryTrace>,
        now: i64,
    ) -> Result<(), BootstrapError> {
        if request.target != "current_chat" {
            return Err(BootstrapError::Usage {
                reason: format!(
                    "unsupported file delivery target {}; only current_chat is supported",
                    request.target
                ),
            });
        }
        if request.session_id != session_id {
            return Err(BootstrapError::Usage {
                reason: format!(
                    "file delivery request {} belongs to session {}, not {}",
                    request.id, request.session_id, session_id
                ),
            });
        }
        let artifact = self
            .with_store_retry(|store| store.get_artifact(&request.artifact_id))?
            .ok_or_else(|| BootstrapError::MissingRecord {
                kind: "artifact",
                id: request.artifact_id.clone(),
            })?;
        if artifact.session_id != session_id {
            return Err(BootstrapError::Usage {
                reason: format!(
                    "artifact {} belongs to session {}, not {}",
                    artifact.id, artifact.session_id, session_id
                ),
            });
        }
        if artifact.bytes.len() > self.app.config.telegram.max_upload_bytes {
            return Err(BootstrapError::Usage {
                reason: format!(
                    "artifact {} is too large for Telegram delivery: {} bytes, limit is {} bytes",
                    artifact.id,
                    artifact.bytes.len(),
                    self.app.config.telegram.max_upload_bytes
                ),
            });
        }

        let caption = request
            .caption
            .as_deref()
            .map(|value| truncate_caption(value, TELEGRAM_CAPTION_SOFT_CAP));
        self.send_document_delivered_with_trace(
            chat_id,
            artifact.bytes,
            &request.file_name,
            caption.as_deref(),
            trace,
        )
        .await?;

        let mut delivered = request.clone();
        delivered.status = "delivered".to_string();
        delivered.updated_at = now;
        delivered.delivered_at = Some(now);
        delivered.error = None;
        self.with_store_retry(|store| store.put_file_delivery_request(&delivered))?;
        Ok(())
    }

    fn mark_file_delivery_request_failed(
        &self,
        request: &FileDeliveryRequestRecord,
        error: String,
        now: i64,
    ) -> Result<(), BootstrapError> {
        let mut failed = request.clone();
        failed.status = "failed".to_string();
        failed.updated_at = now;
        failed.error = Some(error);
        self.with_store_retry(|store| store.put_file_delivery_request(&failed))?;
        Ok(())
    }

    fn telegram_delivery_trace_for_run(
        &self,
        run_id: &str,
    ) -> Result<Option<TelegramDeliveryTrace>, BootstrapError> {
        Ok(self
            .with_store_retry(|store| store.get_trace_link("run", run_id))?
            .map(|link| TelegramDeliveryTrace {
                trace_id: link.trace_id,
                parent_span_id: link.span_id,
            }))
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
            return self.session_summary(session_id).await;
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
            return self.session_summary(session_id).await;
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
        let existing = self.with_store_retry(|store| store.get_telegram_chat_binding(chat_id))?;
        let cursor =
            self.binding_cursor_for_selection(existing.as_ref(), selected_session_id.as_deref())?;
        let existing_created_at = existing
            .as_ref()
            .map(|record| record.created_at)
            .unwrap_or(now);
        let cursor_created_at = cursor.created_at;
        let cursor_transcript_id = cursor.transcript_id;
        self.with_store_retry(|store| {
            store.put_telegram_chat_binding(&TelegramChatBindingRecord {
                telegram_chat_id: chat_id,
                scope: TELEGRAM_SCOPE_PRIVATE.to_string(),
                owner_telegram_user_id: Some(telegram_user_id),
                selected_session_id: selected_session_id.clone(),
                last_delivered_transcript_created_at: cursor_created_at,
                last_delivered_transcript_id: cursor_transcript_id.clone(),
                inbound_queue_mode: existing
                    .as_ref()
                    .map(|record| record.inbound_queue_mode.clone())
                    .unwrap_or_else(|| self.app.config.telegram.inbound_queue_default_mode.clone()),
                inbound_coalesce_window_ms: existing
                    .as_ref()
                    .and_then(|record| record.inbound_coalesce_window_ms),
                created_at: existing_created_at,
                updated_at: now,
            })
        })
        .map(|_| ())
    }

    fn put_group_binding(
        &self,
        chat_id: i64,
        selected_session_id: Option<String>,
        now: i64,
    ) -> Result<(), BootstrapError> {
        let existing = self.with_store_retry(|store| store.get_telegram_chat_binding(chat_id))?;
        let cursor =
            self.binding_cursor_for_selection(existing.as_ref(), selected_session_id.as_deref())?;
        let existing_created_at = existing
            .as_ref()
            .map(|record| record.created_at)
            .unwrap_or(now);
        let cursor_created_at = cursor.created_at;
        let cursor_transcript_id = cursor.transcript_id;
        self.with_store_retry(|store| {
            store.put_telegram_chat_binding(&TelegramChatBindingRecord {
                telegram_chat_id: chat_id,
                scope: TELEGRAM_SCOPE_GROUP.to_string(),
                owner_telegram_user_id: None,
                selected_session_id: selected_session_id.clone(),
                last_delivered_transcript_created_at: cursor_created_at,
                last_delivered_transcript_id: cursor_transcript_id.clone(),
                inbound_queue_mode: existing
                    .as_ref()
                    .map(|record| record.inbound_queue_mode.clone())
                    .unwrap_or_else(|| self.app.config.telegram.inbound_queue_default_mode.clone()),
                inbound_coalesce_window_ms: existing
                    .as_ref()
                    .and_then(|record| record.inbound_coalesce_window_ms),
                created_at: existing_created_at,
                updated_at: now,
            })
        })
        .map(|_| ())
    }

    fn ensure_chat_delivery_cursor_initialized(
        &self,
        chat_id: i64,
        session_id: &str,
    ) -> Result<(), BootstrapError> {
        let Some(binding) =
            self.with_store_retry(|store| store.get_telegram_chat_binding(chat_id))?
        else {
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
            self.latest_delivery_cursor(session_id)?,
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
            self.latest_delivery_cursor(session_id)?,
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
        let Some(mut binding) =
            self.with_store_retry(|store| store.get_telegram_chat_binding(chat_id))?
        else {
            return Ok(());
        };
        if binding.selected_session_id.as_deref() != Some(session_id) {
            return Ok(());
        }
        binding.last_delivered_transcript_created_at = cursor.created_at.or(Some(0));
        binding.last_delivered_transcript_id = cursor.transcript_id.or_else(|| Some(String::new()));
        binding.updated_at = unix_timestamp()?;
        self.with_store_retry(|store| store.put_telegram_chat_binding(&binding))?;
        Ok(())
    }

    async fn deliver_pending_session_notifications(&self) -> Result<(), BootstrapError> {
        let bindings = self.with_store_retry(|store| store.list_telegram_chat_bindings())?;
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
                self.latest_delivery_cursor(session_id)?,
            )?;
            return Ok(Vec::new());
        }

        let transcripts =
            self.with_store_retry(|store| store.list_transcripts_for_session(session_id))?;
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
        let telegram_user_id = telegram_user_id(from)?;
        if let Some(existing) = self.load_activated_pairing(telegram_user_id)? {
            return Ok(existing.token);
        }

        let token = generate_pairing_token();
        let telegram_username = from.username.clone();
        let telegram_display_name = telegram_display_name(from);
        let expires_at =
            now + i64::try_from(self.app.config.telegram.pairing_token_ttl_seconds).unwrap_or(0);
        self.with_store_retry(|store| {
            store.put_telegram_user_pairing(&TelegramUserPairingRecord {
                token: token.clone(),
                telegram_user_id,
                telegram_chat_id: chat_id,
                telegram_username: telegram_username.clone(),
                telegram_display_name: telegram_display_name.clone(),
                status: TELEGRAM_PAIRING_STATUS_PENDING.to_string(),
                created_at: now,
                expires_at,
                activated_at: None,
            })
        })?;
        Ok(token)
    }

    fn load_activated_pairing(
        &self,
        telegram_user_id: i64,
    ) -> Result<Option<TelegramUserPairingRecord>, BootstrapError> {
        Ok(self
            .with_store_retry(|store| store.get_telegram_user_pairing_by_user_id(telegram_user_id))?
            .filter(|record| record.status == TELEGRAM_PAIRING_STATUS_ACTIVATED))
    }

    fn persist_update_cursor(&self, next_offset: i64) -> Result<(), BootstrapError> {
        if next_offset <= 0 {
            return Ok(());
        }
        let updated_at = unix_timestamp()?;
        self.with_store_retry(|store| {
            store.put_telegram_update_cursor(&TelegramUpdateCursorRecord {
                consumer: self.consumer.clone(),
                update_id: next_offset,
                updated_at,
            })
        })
        .map(|_| ())
    }

    fn binding_cursor_for_selection(
        &self,
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
                transcript_id: existing
                    .and_then(|record| record.last_delivered_transcript_id.clone()),
            });
        }
        self.latest_delivery_cursor(selected_session_id)
    }

    fn latest_delivery_cursor(&self, session_id: &str) -> Result<DeliveryCursor, BootstrapError> {
        let latest =
            self.with_store_retry(|store| store.get_latest_transcript_for_session(session_id))?;
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

    fn with_store_retry<T, F>(&self, mut operation: F) -> Result<T, BootstrapError>
    where
        F: FnMut(&PersistenceStore) -> Result<T, StoreError>,
    {
        retry_store_sync(
            SQLITE_LOCK_RETRY_ATTEMPTS,
            Duration::from_millis(SQLITE_LOCK_RETRY_DELAY_MS),
            || {
                let store = PersistenceStore::open_runtime(&self.app.persistence)?;
                operation(&store)
            },
        )
        .map_err(BootstrapError::Store)
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

    async fn render_active_run(&self, session_id: String) -> Result<String, BootstrapError> {
        let backend = self.backend.clone();
        tokio::task::spawn_blocking(move || backend.render_active_run(&session_id))
            .await
            .map_err(map_join_error)?
    }

    async fn cancel_active_run(&self, session_id: String) -> Result<String, BootstrapError> {
        let backend = self.backend.clone();
        tokio::task::spawn_blocking(move || backend.cancel_active_run(&session_id))
            .await
            .map_err(map_join_error)?
    }

    async fn cancel_all_session_work(&self, session_id: String) -> Result<String, BootstrapError> {
        let backend = self.backend.clone();
        tokio::task::spawn_blocking(move || backend.cancel_all_session_work(&session_id))
            .await
            .map_err(map_join_error)?
    }

    async fn render_session_background_jobs(
        &self,
        session_id: String,
    ) -> Result<String, BootstrapError> {
        let backend = self.backend.clone();
        tokio::task::spawn_blocking(move || backend.render_session_background_jobs(&session_id))
            .await
            .map_err(map_join_error)?
    }

    async fn render_session_skills(&self, session_id: String) -> Result<String, BootstrapError> {
        let backend = self.backend.clone();
        tokio::task::spawn_blocking(move || backend.render_session_skills(&session_id))
            .await
            .map_err(map_join_error)?
    }

    async fn enable_session_skill(
        &self,
        session_id: String,
        skill_name: String,
    ) -> Result<String, BootstrapError> {
        let backend = self.backend.clone();
        tokio::task::spawn_blocking(move || backend.enable_session_skill(&session_id, &skill_name))
            .await
            .map_err(map_join_error)?
    }

    async fn disable_session_skill(
        &self,
        session_id: String,
        skill_name: String,
    ) -> Result<String, BootstrapError> {
        let backend = self.backend.clone();
        tokio::task::spawn_blocking(move || backend.disable_session_skill(&session_id, &skill_name))
            .await
            .map_err(map_join_error)?
    }

    async fn compact_session(&self, session_id: String) -> Result<SessionSummary, BootstrapError> {
        let backend = self.backend.clone();
        tokio::task::spawn_blocking(move || backend.compact_session(&session_id))
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

    async fn start_or_queue_chat_turn(
        &self,
        chat_id: i64,
        telegram_message_id: i32,
        session_id: String,
        message: String,
        now: i64,
    ) -> Result<(), BootstrapError> {
        if !self.mark_chat_turn_active(&session_id) {
            return self
                .handle_inbound_while_turn_running(
                    chat_id,
                    telegram_message_id,
                    session_id,
                    message,
                    now,
                )
                .await;
        }

        self.start_marked_chat_turn_background(chat_id, session_id, message, now)
            .await
    }

    async fn start_marked_chat_turn_background(
        &self,
        chat_id: i64,
        session_id: String,
        message: String,
        now: i64,
    ) -> Result<(), BootstrapError> {
        let ack = match self.send_temporary_status_message(chat_id, now).await {
            Ok(ack) => ack,
            Err(error) => {
                self.clear_chat_turn_active(&session_id);
                return Err(error);
            }
        };

        let worker = self.clone();
        let cleanup_session_id = session_id.clone();
        let task = tokio::spawn(async move {
            let task_result = worker
                .finish_chat_turn_background(chat_id, ack.id.0, session_id, message, now)
                .await;
            worker.clear_chat_turn_active(&cleanup_session_id);
            if let Err(error) = task_result {
                DiagnosticEventBuilder::new(
                    &worker.app.config,
                    "error",
                    "telegram",
                    "chat_turn.background_error",
                    "telegram chat turn background delivery failed",
                )
                .error(error.to_string())
                .field("chat_id", chat_id)
                .emit(&worker.audit);
            }
        });

        tokio::select! {
            result = task => {
                if let Err(error) = result {
                    DiagnosticEventBuilder::new(
                        &self.app.config,
                        "error",
                        "telegram",
                        "chat_turn.background_join_error",
                        "telegram chat turn background task failed",
                    )
                    .error(error.to_string())
                    .field("chat_id", chat_id)
                    .emit(&self.audit);
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(TELEGRAM_CHAT_TURN_FAST_SETTLE_MILLIS)) => {}
        }

        Ok(())
    }

    async fn handle_inbound_while_turn_running(
        &self,
        chat_id: i64,
        telegram_message_id: i32,
        session_id: String,
        message: String,
        now: i64,
    ) -> Result<(), BootstrapError> {
        let Some(binding) =
            self.with_store_retry(|store| store.get_telegram_chat_binding(chat_id))?
        else {
            self.send_text_chunks(
                chat_id,
                "A turn is already running, but this chat is not bound to a session. Use /new or /use <session_id>.",
            )
            .await?;
            return Ok(());
        };
        let mode = self.effective_inbound_queue_mode(&binding);
        match mode.as_str() {
            TELEGRAM_INBOUND_QUEUE_MODE_REJECT => {
                self.send_text_chunks(
                    chat_id,
                    "A turn is already running in this session. Use /status to inspect it, /stop to stop the active turn, or /queue queue|coalesce to queue new messages.",
                )
                .await
            }
            TELEGRAM_INBOUND_QUEUE_MODE_QUEUE => {
                self.queue_telegram_inbound_message(
                    &session_id,
                    chat_id,
                    telegram_message_id,
                    &message,
                    now,
                    now,
                )?;
                let queued_count = self.queued_telegram_inbox_count(&session_id)?;
                self.send_text_chunks(
                    chat_id,
                    &format!(
                        "Queued inbound message.\n- mode: queue\n- queued_inbound: {queued_count}"
                    ),
                )
                .await
            }
            TELEGRAM_INBOUND_QUEUE_MODE_COALESCE => {
                let window_ms = self.effective_inbound_coalesce_window_ms(&binding);
                let window_seconds = coalesce_window_seconds(window_ms);
                let available_at = now.saturating_add(window_seconds);
                self.queue_telegram_inbound_message(
                    &session_id,
                    chat_id,
                    telegram_message_id,
                    &message,
                    now,
                    available_at,
                )?;
                let queued_count =
                    self.refresh_queued_telegram_inbox_available_at(&session_id, available_at)?;
                self.send_text_chunks(
                    chat_id,
                    &format!(
                        "Queued inbound message.\n- mode: coalesce\n- queued_inbound: {queued_count}\n- coalesce_window_ms: {window_ms}\n- available_in: ~{window_seconds}s"
                    ),
                )
                .await
            }
            TELEGRAM_INBOUND_QUEUE_MODE_RESTART => {
                let _ = self.cancel_active_run(session_id.clone()).await?;
                self.queue_telegram_inbound_message(
                    &session_id,
                    chat_id,
                    telegram_message_id,
                    &message,
                    now,
                    now,
                )?;
                let queued_count = self.queued_telegram_inbox_count(&session_id)?;
                self.send_text_chunks(
                    chat_id,
                    &format!(
                        "Active turn stop requested; queued inbound message.\n- mode: restart\n- queued_inbound: {queued_count}"
                    ),
                )
                .await
            }
            _ => {
                self.send_text_chunks(
                    chat_id,
                    "A turn is already running in this session. Use /status to inspect it.",
                )
                .await
            }
        }
    }

    fn render_queue_status(&self, chat_id: i64) -> Result<String, BootstrapError> {
        let binding = self.with_store_retry(|store| store.get_telegram_chat_binding(chat_id))?;
        let mode = binding
            .as_ref()
            .map(|binding| self.effective_inbound_queue_mode(binding))
            .unwrap_or_else(|| self.app.config.telegram.inbound_queue_default_mode.clone());
        let coalesce_window_ms = binding
            .as_ref()
            .map(|binding| self.effective_inbound_coalesce_window_ms(binding))
            .unwrap_or_else(|| self.configured_inbound_coalesce_window_ms());
        let queued_count = binding
            .as_ref()
            .and_then(|binding| binding.selected_session_id.as_deref())
            .map(|session_id| self.queued_telegram_inbox_count(session_id))
            .transpose()?
            .unwrap_or_default();
        Ok(format!(
            "Telegram queue:\n- mode: {mode}\n- queued_inbound: {queued_count}\n- coalesce_window_ms: {coalesce_window_ms}"
        ))
    }

    async fn handle_queue_command(
        &self,
        chat_id: i64,
        session_id: String,
        action: TelegramQueueAction,
    ) -> Result<String, BootstrapError> {
        match action {
            TelegramQueueAction::Show => self.render_queue_status(chat_id),
            TelegramQueueAction::Set {
                mode,
                coalesce_window_ms,
            } => {
                self.update_telegram_queue_binding(chat_id, &mode, coalesce_window_ms)?;
                self.render_queue_status(chat_id)
            }
            TelegramQueueAction::Flush => {
                let now = unix_timestamp()?;
                let count = self.refresh_queued_telegram_inbox_available_at(&session_id, now)?;
                Ok(format!(
                    "Telegram queue flushed: {count} inbound message(s) are available now.\n\n{}",
                    self.render_queue_status(chat_id)?
                ))
            }
            TelegramQueueAction::Clear => {
                let now = unix_timestamp()?;
                let count = self.clear_queued_telegram_inbox_events(&session_id, now)?;
                Ok(format!(
                    "Telegram queue cleared: {count} inbound message(s).\n\n{}",
                    self.render_queue_status(chat_id)?
                ))
            }
        }
    }

    fn update_telegram_queue_binding(
        &self,
        chat_id: i64,
        mode: &str,
        coalesce_window_ms: Option<u64>,
    ) -> Result<(), BootstrapError> {
        let mut binding = self
            .with_store_retry(|store| store.get_telegram_chat_binding(chat_id))?
            .ok_or_else(|| BootstrapError::Usage {
                reason: "No selected session. Use /new or /use <session_id>.".to_string(),
            })?;
        binding.inbound_queue_mode = mode.to_string();
        if let Some(window_ms) = coalesce_window_ms {
            binding.inbound_coalesce_window_ms = Some(i64::try_from(window_ms).unwrap_or(i64::MAX));
        }
        binding.updated_at = unix_timestamp()?;
        self.with_store_retry(|store| store.put_telegram_chat_binding(&binding))?;
        Ok(())
    }

    fn queue_telegram_inbound_message(
        &self,
        session_id: &str,
        chat_id: i64,
        telegram_message_id: i32,
        message: &str,
        now: i64,
        available_at: i64,
    ) -> Result<(), BootstrapError> {
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
        let record =
            SessionInboxEventRecord::try_from(&event).map_err(BootstrapError::RecordConversion)?;
        self.with_store_retry(|store| store.put_session_inbox_event(&record))?;
        Ok(())
    }

    fn queued_telegram_inbox_count(&self, session_id: &str) -> Result<usize, BootstrapError> {
        let records = self.with_store_retry(|store| {
            store.list_queued_session_inbox_events_for_session(session_id)
        })?;
        Ok(records
            .iter()
            .filter(|record| is_telegram_inbox_event(record))
            .count())
    }

    fn refresh_queued_telegram_inbox_available_at(
        &self,
        session_id: &str,
        available_at: i64,
    ) -> Result<usize, BootstrapError> {
        self.with_store_retry(|store| {
            let mut count = 0usize;
            for mut record in store.list_queued_session_inbox_events_for_session(session_id)? {
                if !is_telegram_inbox_event(&record) {
                    continue;
                }
                record.status = "queued".to_string();
                record.available_at = available_at;
                record.claimed_at = None;
                record.processed_at = None;
                record.error = None;
                store.put_session_inbox_event(&record)?;
                count += 1;
            }
            Ok(count)
        })
    }

    fn clear_queued_telegram_inbox_events(
        &self,
        session_id: &str,
        now: i64,
    ) -> Result<usize, BootstrapError> {
        self.with_store_retry(|store| {
            let mut count = 0usize;
            for mut record in store.list_queued_session_inbox_events_for_session(session_id)? {
                if !is_telegram_inbox_event(&record) {
                    continue;
                }
                record.status = "processed".to_string();
                record.claimed_at = None;
                record.processed_at = Some(now);
                record.error = None;
                store.put_session_inbox_event(&record)?;
                count += 1;
            }
            Ok(count)
        })
    }

    fn effective_inbound_queue_mode(&self, binding: &TelegramChatBindingRecord) -> String {
        if is_valid_telegram_queue_mode(binding.inbound_queue_mode.as_str()) {
            binding.inbound_queue_mode.clone()
        } else {
            self.app.config.telegram.inbound_queue_default_mode.clone()
        }
    }

    fn effective_inbound_coalesce_window_ms(&self, binding: &TelegramChatBindingRecord) -> u64 {
        binding
            .inbound_coalesce_window_ms
            .and_then(|value| u64::try_from(value).ok())
            .unwrap_or_else(|| self.configured_inbound_coalesce_window_ms())
            .max(TELEGRAM_MIN_COALESCE_WINDOW_MS)
    }

    fn configured_inbound_coalesce_window_ms(&self) -> u64 {
        self.app
            .config
            .telegram
            .inbound_coalesce_window_ms
            .max(TELEGRAM_MIN_COALESCE_WINDOW_MS)
    }

    async fn finish_chat_turn_background(
        &self,
        chat_id: i64,
        status_message_id: i32,
        session_id: String,
        message: String,
        now: i64,
    ) -> Result<(), BootstrapError> {
        match self
            .execute_chat_turn(chat_id, status_message_id, session_id, message, now)
            .await
        {
            Ok(report) => {
                let finished_at = unix_timestamp().unwrap_or(now);
                self.deliver_chat_report(chat_id, &report, status_message_id, finished_at)
                    .await?;
                self.mark_chat_delivered_to_latest_transcript(chat_id, report.session_id.as_str())?;
                Ok(())
            }
            Err(error) => {
                let finished_at = unix_timestamp().unwrap_or(now);
                self.fail_temporary_status_message(
                    chat_id,
                    status_message_id,
                    &error.to_string(),
                    finished_at,
                )
                .await?;
                Ok(())
            }
        }
    }

    fn mark_chat_turn_active(&self, session_id: &str) -> bool {
        self.active_chat_turns
            .lock()
            .expect("active telegram chat turns")
            .insert(session_id.to_string())
    }

    fn clear_chat_turn_active(&self, session_id: &str) {
        self.active_chat_turns
            .lock()
            .expect("active telegram chat turns")
            .remove(session_id);
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

        let mut progress = TelegramProgressTracker::default();
        let mut pending_status_html = None::<String>;
        let mut last_status_html = render_temporary_status_html(progress.state());
        let edit_interval = self.progress_update_min_interval();
        let edit_sleep = tokio::time::sleep(edit_interval);
        tokio::pin!(edit_sleep);
        let typing_interval = Duration::from_secs(TELEGRAM_TYPING_HEARTBEAT_INTERVAL_SECONDS);
        let typing_sleep =
            tokio::time::sleep(Duration::from_millis(TELEGRAM_TYPING_INITIAL_DELAY_MILLIS));
        tokio::pin!(typing_sleep);

        loop {
            tokio::select! {
                result = &mut join_handle => {
                    return result.map_err(map_join_error)?;
                }
                maybe_event = event_receiver.recv() => {
                    let Some(event) = maybe_event else {
                        continue;
                    };
                    if progress.apply(&event) {
                        let status_html = render_temporary_status_html(progress.state());
                        if status_html != last_status_html {
                            pending_status_html = Some(status_html);
                        }
                    }
                }
                _ = &mut edit_sleep, if pending_status_html.is_some() => {
                    let status_html = pending_status_html.take().expect("pending status");
                    if status_html != last_status_html {
                        if let Err(error) = self.edit_html_delivered(chat_id, message_id, &status_html).await {
                            DiagnosticEventBuilder::new(
                                &self.app.config,
                                "warn",
                                "telegram",
                                "status_edit.skipped",
                                "telegram progress status edit failed",
                            )
                            .error(error.to_string())
                            .field("chat_id", chat_id)
                            .field("message_id", i64::from(message_id))
                            .emit(&self.audit);
                        }
                        last_status_html = status_html;
                    }
                    edit_sleep.as_mut().reset(tokio::time::Instant::now() + edit_interval);
                }
                _ = &mut typing_sleep => {
                    let _ = self.send_typing_delivered(chat_id).await;
                    typing_sleep.as_mut().reset(tokio::time::Instant::now() + typing_interval);
                }
            }
        }
    }
}

pub fn default_command_specs() -> Vec<TelegramCommandSpec> {
    vec![
        TelegramCommandSpec::new("start", "Get a pairing key"),
        TelegramCommandSpec::new("help", "Show Telegram help"),
        TelegramCommandSpec::new("new", "Create and select a session"),
        TelegramCommandSpec::new("sessions", "List sessions"),
        TelegramCommandSpec::new("use", "Select a session by id"),
        TelegramCommandSpec::new("status", "Show current session status"),
        TelegramCommandSpec::new("jobs", "Show current session jobs"),
        TelegramCommandSpec::new("queue", "Show or set inbound queue mode"),
        TelegramCommandSpec::new("stop", "Stop the active turn"),
        TelegramCommandSpec::new("pause", "Alias for stop"),
        TelegramCommandSpec::new("cancel", "Cancel current session work"),
        TelegramCommandSpec::new("model", "Set session model"),
        TelegramCommandSpec::new("think", "Set session think level"),
        TelegramCommandSpec::new("reasoning", "Toggle reasoning visibility"),
        TelegramCommandSpec::new("autoapprove", "Toggle auto-approve"),
        TelegramCommandSpec::new("compact", "Compact current session context"),
        TelegramCommandSpec::new("skills", "List session skills"),
        TelegramCommandSpec::new("enable", "Enable a session skill"),
        TelegramCommandSpec::new("disable", "Disable a session skill"),
        TelegramCommandSpec::new("files", "List files in the current session"),
        TelegramCommandSpec::new("file", "Send a session file by artifact id"),
        TelegramCommandSpec::new("judge", "Send a message to Judge"),
        TelegramCommandSpec::new("agent", "Send a message to another agent"),
    ]
}

fn render_session_operator_status(
    summary: &SessionSummary,
    active_run: &str,
    queue_status: &str,
) -> String {
    let mut lines = vec![
        "Current session:".to_string(),
        format!("- title: {}", summary.title),
        format!("- id: {}", summary.id),
        format!(
            "- agent: {} ({})",
            summary.agent_name, summary.agent_profile_id
        ),
        format!(
            "- model: {}",
            summary.model.as_deref().unwrap_or("<default>")
        ),
        format!(
            "- think: {}",
            summary.think_level.as_deref().unwrap_or("<default>")
        ),
        format!("- reasoning_visible: {}", summary.reasoning_visible),
        format!("- auto_approve: {}", summary.auto_approve),
        format!("- messages: {}", summary.message_count),
        format!("- context_tokens: {}", summary.context_tokens),
        format!(
            "- background_jobs: {} total, {} running, {} queued",
            summary.background_job_count,
            summary.running_background_job_count,
            summary.queued_background_job_count
        ),
    ];
    if summary.has_pending_approval {
        lines.push("- pending_approval: yes".to_string());
    }
    lines.push(String::new());
    lines.push(queue_status.to_string());
    lines.push(String::new());
    lines.push(active_run.to_string());
    lines.join("\n")
}

fn is_telegram_inbox_event(record: &SessionInboxEventRecord) -> bool {
    match serde_json::from_str::<SessionInboxEventPayload>(&record.payload_json) {
        Ok(SessionInboxEventPayload::ExternalInputReceived { source, .. }) => {
            source == TELEGRAM_INBOUND_QUEUE_SOURCE
        }
        _ => false,
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_session_operator_commands() {
        let commands = default_command_specs()
            .into_iter()
            .map(|command| command.command)
            .collect::<Vec<_>>();

        for expected in [
            "status",
            "jobs",
            "queue",
            "stop",
            "pause",
            "cancel",
            "model",
            "think",
            "reasoning",
            "autoapprove",
            "compact",
            "skills",
            "enable",
            "disable",
        ] {
            assert!(
                commands.iter().any(|command| command == expected),
                "missing Telegram command: {expected}"
            );
        }
    }
}
