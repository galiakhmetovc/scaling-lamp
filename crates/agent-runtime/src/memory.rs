#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryIndex {
    working: WorkingMemory,
    project: ProjectMemory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRetentionState {
    pub session_id: String,
    pub tier: SessionRetentionTier,
    pub last_accessed_at: i64,
    pub archived_at: Option<i64>,
    pub archive_manifest_path: Option<String>,
    pub archive_version: Option<u32>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkingMemory {
    limit: usize,
    notes: Vec<MemoryNote>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProjectMemory {
    notes: Vec<MemoryNote>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryNote {
    pub topic: String,
    pub detail: String,
    pub source: MemorySource,
    pub recorded_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemorySource {
    Operator,
    Planner,
    Transcript,
    Tool,
    Derived,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionRetentionTier {
    Active,
    Warm,
    Cold,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRetentionTierParseError {
    value: String,
}

impl Default for MemoryIndex {
    fn default() -> Self {
        Self::with_working_limit(64)
    }
}

impl SessionRetentionTier {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Warm => "warm",
            Self::Cold => "cold",
        }
    }
}

impl TryFrom<&str> for SessionRetentionTier {
    type Error = SessionRetentionTierParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "active" => Ok(Self::Active),
            "warm" => Ok(Self::Warm),
            "cold" => Ok(Self::Cold),
            _ => Err(SessionRetentionTierParseError {
                value: value.to_string(),
            }),
        }
    }
}

impl std::fmt::Display for SessionRetentionTierParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid session retention tier: {}", self.value)
    }
}

impl std::error::Error for SessionRetentionTierParseError {}

impl MemoryIndex {
    pub fn with_working_limit(limit: usize) -> Self {
        Self {
            working: WorkingMemory::new(limit),
            project: ProjectMemory::default(),
        }
    }

    pub fn remember_working(&mut self, note: MemoryNote) {
        self.working.remember(note);
    }

    pub fn working_notes(&self) -> &[MemoryNote] {
        &self.working.notes
    }

    pub fn remember_project(&mut self, note: MemoryNote) {
        self.project.remember(note);
    }

    pub fn project_notes(&self) -> &[MemoryNote] {
        &self.project.notes
    }
}

impl WorkingMemory {
    fn new(limit: usize) -> Self {
        Self {
            limit: limit.max(1),
            notes: Vec::new(),
        }
    }

    fn remember(&mut self, note: MemoryNote) {
        if self.notes.len() == self.limit {
            self.notes.remove(0);
        }

        self.notes.push(note);
    }
}

impl ProjectMemory {
    fn remember(&mut self, note: MemoryNote) {
        if let Some(existing) = self
            .notes
            .iter_mut()
            .find(|existing| existing.topic == note.topic)
        {
            *existing = note;
            return;
        }

        self.notes.push(note);
    }
}

impl MemoryNote {
    pub fn new(
        topic: impl Into<String>,
        detail: impl Into<String>,
        source: MemorySource,
        recorded_at: i64,
    ) -> Self {
        Self {
            topic: topic.into(),
            detail: detail.into(),
            source,
            recorded_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MemoryIndex, MemoryNote, MemorySource, SessionRetentionTier, SessionRetentionTierParseError,
    };

    #[test]
    fn working_memory_evicts_oldest_note_when_limit_is_reached() {
        let mut memory = MemoryIndex::with_working_limit(2);

        memory.remember_working(MemoryNote::new(
            "goal",
            "ship the runtime",
            MemorySource::Operator,
            1,
        ));
        memory.remember_working(MemoryNote::new(
            "constraint",
            "stay event-sourcing free",
            MemorySource::Operator,
            2,
        ));
        memory.remember_working(MemoryNote::new(
            "next-step",
            "add run engine",
            MemorySource::Planner,
            3,
        ));

        let working = memory.working_notes();

        assert_eq!(working.len(), 2);
        assert_eq!(working[0].topic, "constraint");
        assert_eq!(working[1].topic, "next-step");
    }

    #[test]
    fn project_memory_upserts_notes_by_topic() {
        let mut memory = MemoryIndex::default();

        memory.remember_project(MemoryNote::new(
            "repo-shape",
            "prefer modular monolith",
            MemorySource::Derived,
            10,
        ));
        memory.remember_project(MemoryNote::new(
            "repo-shape",
            "prefer modular monolith in Rust",
            MemorySource::Derived,
            11,
        ));

        let project = memory.project_notes();

        assert_eq!(project.len(), 1);
        assert_eq!(project[0].topic, "repo-shape");
        assert_eq!(project[0].detail, "prefer modular monolith in Rust");
        assert_eq!(project[0].recorded_at, 11);
    }

    #[test]
    fn session_retention_tier_round_trips_through_str() {
        assert_eq!(SessionRetentionTier::Active.as_str(), "active");
        assert_eq!(SessionRetentionTier::Warm.as_str(), "warm");
        assert_eq!(SessionRetentionTier::Cold.as_str(), "cold");

        assert_eq!(
            SessionRetentionTier::try_from("active").expect("active tier"),
            SessionRetentionTier::Active
        );
        assert_eq!(
            SessionRetentionTier::try_from("warm").expect("warm tier"),
            SessionRetentionTier::Warm
        );
        assert_eq!(
            SessionRetentionTier::try_from("cold").expect("cold tier"),
            SessionRetentionTier::Cold
        );
    }

    #[test]
    fn session_retention_tier_rejects_unknown_value() {
        let error = SessionRetentionTier::try_from("frozen").expect_err("unknown tier");
        assert_eq!(
            error,
            SessionRetentionTierParseError {
                value: "frozen".to_string()
            }
        );
    }
}
