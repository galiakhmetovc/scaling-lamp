use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub id: String,
    pub title: String,
    pub prompt_override: Option<PromptOverride>,
    pub settings: SessionSettings,
    pub active_mission_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionSettings {
    pub working_memory_limit: usize,
    pub project_memory_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptOverride(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionError {
    EmptyPromptOverride,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageRoleParseError {
    value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptEntry {
    pub id: String,
    pub session_id: String,
    pub run_id: Option<String>,
    pub role: MessageRole,
    pub content: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transcript {
    session_id: String,
    entries: Vec<TranscriptEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranscriptError {
    SessionMismatch { expected: String, actual: String },
}

impl Default for Session {
    fn default() -> Self {
        Self {
            id: "session-bootstrap".to_string(),
            title: "bootstrap".to_string(),
            prompt_override: None,
            settings: SessionSettings::default(),
            active_mission_id: None,
            created_at: 0,
            updated_at: 0,
        }
    }
}

impl Default for SessionSettings {
    fn default() -> Self {
        Self {
            working_memory_limit: 64,
            project_memory_enabled: true,
        }
    }
}

impl PromptOverride {
    pub fn new(value: impl Into<String>) -> Result<Self, SessionError> {
        let value = value.into().trim().to_string();

        if value.is_empty() {
            return Err(SessionError::EmptyPromptOverride);
        }

        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TranscriptEntry {
    pub fn new(
        id: impl Into<String>,
        session_id: impl Into<String>,
        run_id: Option<&str>,
        role: MessageRole,
        content: impl Into<String>,
        created_at: i64,
    ) -> Self {
        Self {
            id: id.into(),
            session_id: session_id.into(),
            run_id: run_id.map(str::to_owned),
            role,
            content: content.into(),
            created_at,
        }
    }

    pub fn system(
        id: impl Into<String>,
        session_id: impl Into<String>,
        run_id: Option<&str>,
        content: impl Into<String>,
        created_at: i64,
    ) -> Self {
        Self::new(
            id,
            session_id,
            run_id,
            MessageRole::System,
            content,
            created_at,
        )
    }

    pub fn user(
        id: impl Into<String>,
        session_id: impl Into<String>,
        run_id: Option<&str>,
        content: impl Into<String>,
        created_at: i64,
    ) -> Self {
        Self::new(
            id,
            session_id,
            run_id,
            MessageRole::User,
            content,
            created_at,
        )
    }

    pub fn assistant(
        id: impl Into<String>,
        session_id: impl Into<String>,
        run_id: Option<&str>,
        content: impl Into<String>,
        created_at: i64,
    ) -> Self {
        Self::new(
            id,
            session_id,
            run_id,
            MessageRole::Assistant,
            content,
            created_at,
        )
    }

    pub fn tool(
        id: impl Into<String>,
        session_id: impl Into<String>,
        run_id: Option<&str>,
        content: impl Into<String>,
        created_at: i64,
    ) -> Self {
        Self::new(
            id,
            session_id,
            run_id,
            MessageRole::Tool,
            content,
            created_at,
        )
    }
}

impl MessageRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::Tool => "tool",
        }
    }
}

impl TryFrom<&str> for MessageRole {
    type Error = MessageRoleParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "system" => Ok(Self::System),
            "user" => Ok(Self::User),
            "assistant" => Ok(Self::Assistant),
            "tool" => Ok(Self::Tool),
            _ => Err(MessageRoleParseError {
                value: value.to_string(),
            }),
        }
    }
}

impl Transcript {
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            entries: Vec::new(),
        }
    }

    pub fn record(&mut self, entry: TranscriptEntry) -> Result<(), TranscriptError> {
        if entry.session_id != self.session_id {
            return Err(TranscriptError::SessionMismatch {
                expected: self.session_id.clone(),
                actual: entry.session_id,
            });
        }

        self.entries.push(entry);
        Ok(())
    }

    pub fn entries(&self) -> &[TranscriptEntry] {
        &self.entries
    }
}

impl fmt::Display for SessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyPromptOverride => {
                write!(formatter, "prompt override cannot be blank")
            }
        }
    }
}

impl Error for SessionError {}

impl fmt::Display for TranscriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SessionMismatch { expected, actual } => {
                write!(
                    formatter,
                    "transcript entry session mismatch: expected {expected}, got {actual}"
                )
            }
        }
    }
}

impl Error for TranscriptError {}

impl fmt::Display for MessageRoleParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "unknown message role {}", self.value)
    }
}

impl Error for MessageRoleParseError {}

#[cfg(test)]
mod tests {
    use super::{MessageRole, PromptOverride, Session, Transcript, TranscriptEntry};

    #[test]
    fn session_defaults_include_prompt_and_memory_settings() {
        let session = Session::default();

        assert_eq!(session.id, "session-bootstrap");
        assert_eq!(session.title, "bootstrap");
        assert!(session.prompt_override.is_none());
        assert_eq!(session.settings.working_memory_limit, 64);
        assert!(session.settings.project_memory_enabled);
    }

    #[test]
    fn prompt_override_rejects_blank_text() {
        assert!(PromptOverride::new("   ").is_err());
    }

    #[test]
    fn transcript_rejects_entries_for_other_sessions() {
        let mut transcript = Transcript::new("session-bootstrap");
        let wrong_entry =
            TranscriptEntry::user("msg-1", "different-session", Some("run-1"), "hello", 42);

        assert!(transcript.record(wrong_entry).is_err());
    }

    #[test]
    fn transcript_preserves_append_order() {
        let mut transcript = Transcript::new("session-bootstrap");

        transcript
            .record(TranscriptEntry::user(
                "msg-1",
                "session-bootstrap",
                Some("run-1"),
                "draft the runtime",
                10,
            ))
            .unwrap();
        transcript
            .record(TranscriptEntry::assistant(
                "msg-2",
                "session-bootstrap",
                Some("run-1"),
                "starting plan",
                11,
            ))
            .unwrap();

        assert_eq!(transcript.entries().len(), 2);
        assert_eq!(transcript.entries()[0].role, MessageRole::User);
        assert_eq!(transcript.entries()[1].role, MessageRole::Assistant);
        assert_eq!(transcript.entries()[0].content, "draft the runtime");
        assert_eq!(transcript.entries()[1].content, "starting plan");
    }
}
