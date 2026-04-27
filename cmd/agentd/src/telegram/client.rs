use reqwest::Url;
use std::error::Error;
use std::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use teloxide::Bot;
use teloxide::net::Download;
use teloxide::payloads::{
    EditMessageTextSetters, GetUpdatesSetters, SendDocumentSetters, SendMessageSetters,
};
use teloxide::requests::{Request, Requester};
use teloxide::types::{
    BotCommand, ChatAction, ChatId, File, FileId, InputFile, Me, Message, MessageId, ParseMode,
    Update,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelegramClientConfig {
    pub token: String,
    pub api_url: Option<String>,
    pub poll_request_timeout_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelegramCommandSpec {
    pub command: String,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct TelegramClient {
    bot: Bot,
}

#[derive(Debug)]
pub enum TelegramClientError {
    InvalidApiUrl {
        value: String,
        reason: String,
    },
    HttpClient(reqwest::Error),
    Request(teloxide::RequestError),
    Download(teloxide::DownloadError),
    Io {
        context: &'static str,
        source: std::io::Error,
    },
}

impl TelegramCommandSpec {
    pub fn new(command: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            description: description.into(),
        }
    }
}

impl From<&TelegramCommandSpec> for BotCommand {
    fn from(value: &TelegramCommandSpec) -> Self {
        BotCommand::new(value.command.clone(), value.description.clone())
    }
}

impl TelegramClient {
    pub fn new(config: TelegramClientConfig) -> Result<Self, TelegramClientError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.poll_request_timeout_seconds))
            .build()
            .map_err(TelegramClientError::HttpClient)?;
        let mut bot = Bot::with_client(config.token, client);
        if let Some(api_url) = config.api_url {
            let parsed =
                Url::parse(&api_url).map_err(|source| TelegramClientError::InvalidApiUrl {
                    value: api_url,
                    reason: source.to_string(),
                })?;
            bot = bot.set_api_url(parsed);
        }
        Ok(Self { bot })
    }

    pub async fn poll_updates(
        &self,
        offset: Option<i32>,
        limit: u8,
        timeout_seconds: u32,
    ) -> Result<Vec<Update>, TelegramClientError> {
        let request = self.bot.get_updates().limit(limit).timeout(timeout_seconds);
        let request = match offset {
            Some(offset) => request.offset(offset),
            None => request,
        };
        request.send().await.map_err(TelegramClientError::Request)
    }

    pub async fn send_text(
        &self,
        chat_id: i64,
        text: &str,
    ) -> Result<Message, TelegramClientError> {
        self.bot
            .send_message(ChatId(chat_id), text)
            .send()
            .await
            .map_err(TelegramClientError::Request)
    }

    pub async fn edit_text(
        &self,
        chat_id: i64,
        message_id: i32,
        text: &str,
    ) -> Result<Message, TelegramClientError> {
        self.bot
            .edit_message_text(ChatId(chat_id), MessageId(message_id), text)
            .send()
            .await
            .map_err(TelegramClientError::Request)
    }

    pub async fn send_html(
        &self,
        chat_id: i64,
        html: &str,
    ) -> Result<Message, TelegramClientError> {
        self.bot
            .send_message(ChatId(chat_id), html)
            .parse_mode(ParseMode::Html)
            .send()
            .await
            .map_err(TelegramClientError::Request)
    }

    pub async fn edit_html(
        &self,
        chat_id: i64,
        message_id: i32,
        html: &str,
    ) -> Result<Message, TelegramClientError> {
        self.bot
            .edit_message_text(ChatId(chat_id), MessageId(message_id), html)
            .parse_mode(ParseMode::Html)
            .send()
            .await
            .map_err(TelegramClientError::Request)
    }

    pub async fn delete_message(
        &self,
        chat_id: i64,
        message_id: i32,
    ) -> Result<(), TelegramClientError> {
        self.bot
            .delete_message(ChatId(chat_id), MessageId(message_id))
            .send()
            .await
            .map(|_| ())
            .map_err(TelegramClientError::Request)
    }

    pub async fn send_typing(&self, chat_id: i64) -> Result<(), TelegramClientError> {
        self.bot
            .send_chat_action(ChatId(chat_id), ChatAction::Typing)
            .send()
            .await
            .map(|_| ())
            .map_err(TelegramClientError::Request)
    }

    pub async fn send_document(
        &self,
        chat_id: i64,
        bytes: Vec<u8>,
        file_name: &str,
        caption: Option<&str>,
    ) -> Result<Message, TelegramClientError> {
        let document = InputFile::memory(bytes).file_name(file_name.to_string());
        let request = self.bot.send_document(ChatId(chat_id), document);
        let request = match caption {
            Some(caption) if !caption.trim().is_empty() => request.caption(caption.to_string()),
            _ => request,
        };
        request.send().await.map_err(TelegramClientError::Request)
    }

    pub async fn register_commands(
        &self,
        commands: &[TelegramCommandSpec],
    ) -> Result<(), TelegramClientError> {
        let commands = commands.iter().map(BotCommand::from).collect::<Vec<_>>();
        self.bot
            .set_my_commands(commands)
            .send()
            .await
            .map(|_| ())
            .map_err(TelegramClientError::Request)
    }

    pub async fn get_me(&self) -> Result<Me, TelegramClientError> {
        self.bot
            .get_me()
            .send()
            .await
            .map_err(TelegramClientError::Request)
    }

    pub async fn get_file(&self, file_id: &str) -> Result<File, TelegramClientError> {
        self.bot
            .get_file(FileId(file_id.to_string()))
            .send()
            .await
            .map_err(TelegramClientError::Request)
    }

    pub async fn download_file(&self, path: &str) -> Result<Vec<u8>, TelegramClientError> {
        let temp_path = std::env::temp_dir().join(format!(
            "agentd-telegram-download-{}-{}",
            std::process::id(),
            unique_download_suffix()
        ));
        let mut file = tokio::fs::File::create(&temp_path)
            .await
            .map_err(|source| TelegramClientError::Io {
                context: "create telegram download temp file",
                source,
            })?;

        self.bot
            .download_file(path, &mut file)
            .await
            .map_err(TelegramClientError::Download)?;
        drop(file);

        let bytes =
            tokio::fs::read(&temp_path)
                .await
                .map_err(|source| TelegramClientError::Io {
                    context: "read telegram download temp file",
                    source,
                })?;
        let _ = tokio::fs::remove_file(&temp_path).await;
        Ok(bytes)
    }
}

impl fmt::Display for TelegramClientError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidApiUrl { value, reason } => {
                write!(formatter, "invalid telegram api url {value}: {reason}")
            }
            Self::HttpClient(source) => {
                write!(formatter, "failed to build telegram http client: {source}")
            }
            Self::Request(source) => write!(formatter, "telegram bot api request failed: {source}"),
            Self::Download(source) => write!(formatter, "telegram file download failed: {source}"),
            Self::Io { context, source } => write!(formatter, "{context}: {source}"),
        }
    }
}

impl Error for TelegramClientError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidApiUrl { .. } => None,
            Self::HttpClient(source) => Some(source),
            Self::Request(source) => Some(source),
            Self::Download(source) => Some(source),
            Self::Io { source, .. } => Some(source),
        }
    }
}

fn unique_download_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}
