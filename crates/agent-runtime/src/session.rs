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
    pub parent_session_id: Option<String>,
    pub parent_job_id: Option<String>,
    pub delegation_label: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionSettings {
    pub working_memory_limit: usize,
    pub project_memory_enabled: bool,
    pub model: Option<String>,
    pub reasoning_visible: bool,
    pub think_level: Option<String>,
    pub compactifications: u32,
    pub completion_nudges: Option<u32>,
    pub auto_approve: bool,
    pub enabled_skills: Vec<String>,
    pub disabled_skills: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptOverride(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionError {
    EmptyPromptOverride,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: None,
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
            model: None,
            reasoning_visible: true,
            think_level: None,
            compactifications: 0,
            completion_nudges: None,
            auto_approve: false,
            enabled_skills: Vec::new(),
            disabled_skills: Vec::new(),
        }
    }
}

impl SessionSettings {
    pub fn enable_skill(&mut self, skill_name: &str) -> bool {
        let Some(skill_name) = normalize_skill_name(skill_name) else {
            return false;
        };
        let mut changed = false;
        let original_disabled = self.disabled_skills.len();
        self.disabled_skills.retain(|existing| {
            normalize_skill_name(existing).as_deref() != Some(skill_name.as_str())
        });
        if self.disabled_skills.len() != original_disabled {
            changed = true;
        }
        if !self
            .enabled_skills
            .iter()
            .any(|existing| normalize_skill_name(existing).as_deref() == Some(skill_name.as_str()))
        {
            self.enabled_skills.push(skill_name);
            self.enabled_skills.sort();
            changed = true;
        }
        changed
    }

    pub fn disable_skill(&mut self, skill_name: &str) -> bool {
        let Some(skill_name) = normalize_skill_name(skill_name) else {
            return false;
        };
        let mut changed = false;
        let original_enabled = self.enabled_skills.len();
        self.enabled_skills.retain(|existing| {
            normalize_skill_name(existing).as_deref() != Some(skill_name.as_str())
        });
        if self.enabled_skills.len() != original_enabled {
            changed = true;
        }
        if !self
            .disabled_skills
            .iter()
            .any(|existing| normalize_skill_name(existing).as_deref() == Some(skill_name.as_str()))
        {
            self.disabled_skills.push(skill_name);
            self.disabled_skills.sort();
            changed = true;
        }
        changed
    }
}

fn normalize_skill_name(skill_name: &str) -> Option<String> {
    let trimmed = skill_name.trim().to_lowercase();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
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
    use super::{
        MessageRole, PromptOverride, Session, SessionSettings, Transcript, TranscriptEntry,
    };

    #[test]
    fn session_defaults_include_prompt_and_memory_settings() {
        let session = Session::default();

        assert_eq!(session.id, "session-bootstrap");
        assert_eq!(session.title, "bootstrap");
        assert!(session.prompt_override.is_none());
        assert_eq!(session.settings.working_memory_limit, 64);
        assert!(session.settings.project_memory_enabled);
        assert_eq!(session.settings.model, None);
        assert!(session.settings.reasoning_visible);
        assert_eq!(session.settings.think_level, None);
        assert_eq!(session.settings.compactifications, 0);
        assert!(session.settings.enabled_skills.is_empty());
        assert!(session.settings.disabled_skills.is_empty());
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

    #[test]
    fn session_settings_skill_overrides_stay_unique_and_move_between_lists() {
        let mut settings = SessionSettings::default();

        assert!(settings.enable_skill(" Rust-Debug "));
        assert!(!settings.enable_skill("rust-debug"));
        assert_eq!(settings.enabled_skills, vec!["rust-debug".to_string()]);
        assert!(settings.disable_skill("rust-debug"));
        assert!(settings.enabled_skills.is_empty());
        assert_eq!(settings.disabled_skills, vec!["rust-debug".to_string()]);
        assert!(settings.enable_skill("rust-debug"));
        assert_eq!(settings.enabled_skills, vec!["rust-debug".to_string()]);
        assert!(settings.disabled_skills.is_empty());
    }
}
