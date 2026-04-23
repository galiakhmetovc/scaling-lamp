#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SessionArchiveManifest {
    pub session_id: String,
    pub archive_version: u32,
    pub archived_at: i64,
    pub transcript_path: String,
    pub transcript_count: u32,
    pub summary_path: Option<String>,
    pub artifacts: Vec<ArchivedArtifactEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ArchivedTranscriptEntry {
    pub id: String,
    pub run_id: Option<String>,
    pub kind: String,
    pub content: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ArchivedSummary {
    pub summary_text: String,
    pub covered_message_count: u32,
    pub summary_token_estimate: u32,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ArchivedArtifactEntry {
    pub artifact_id: String,
    pub kind: String,
    pub relative_path: String,
    pub byte_len: u64,
    pub sha256: String,
    pub created_at: i64,
}
