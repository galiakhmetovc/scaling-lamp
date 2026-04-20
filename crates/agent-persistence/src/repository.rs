use crate::records::{
    ArtifactRecord, ContextSummaryRecord, JobRecord, MissionRecord, PlanRecord, RunRecord,
    SessionRecord, TranscriptRecord,
};
use crate::store::StoreError;

pub trait SessionRepository {
    fn put_session(&self, record: &SessionRecord) -> Result<(), StoreError>;
    fn get_session(&self, id: &str) -> Result<Option<SessionRecord>, StoreError>;
    fn list_sessions(&self) -> Result<Vec<SessionRecord>, StoreError>;
    fn delete_session(&self, id: &str) -> Result<bool, StoreError>;
}

pub trait MissionRepository {
    fn put_mission(&self, record: &MissionRecord) -> Result<(), StoreError>;
    fn get_mission(&self, id: &str) -> Result<Option<MissionRecord>, StoreError>;
    fn list_missions(&self) -> Result<Vec<MissionRecord>, StoreError>;
}

pub trait RunRepository {
    fn put_run(&self, record: &RunRecord) -> Result<(), StoreError>;
    fn get_run(&self, id: &str) -> Result<Option<RunRecord>, StoreError>;
    fn list_runs(&self) -> Result<Vec<RunRecord>, StoreError>;
}

pub trait JobRepository {
    fn put_job(&self, record: &JobRecord) -> Result<(), StoreError>;
    fn get_job(&self, id: &str) -> Result<Option<JobRecord>, StoreError>;
    fn list_jobs(&self) -> Result<Vec<JobRecord>, StoreError>;
}

pub trait TranscriptRepository {
    fn put_transcript(&self, record: &TranscriptRecord) -> Result<(), StoreError>;
    fn get_transcript(&self, id: &str) -> Result<Option<TranscriptRecord>, StoreError>;
    fn list_transcripts_for_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<TranscriptRecord>, StoreError>;
}

pub trait ContextSummaryRepository {
    fn put_context_summary(&self, record: &ContextSummaryRecord) -> Result<(), StoreError>;
    fn get_context_summary(
        &self,
        session_id: &str,
    ) -> Result<Option<ContextSummaryRecord>, StoreError>;
}

pub trait PlanRepository {
    fn put_plan(&self, record: &PlanRecord) -> Result<(), StoreError>;
    fn get_plan(&self, session_id: &str) -> Result<Option<PlanRecord>, StoreError>;
}

pub trait ArtifactRepository {
    fn put_artifact(&self, record: &ArtifactRecord) -> Result<(), StoreError>;
    fn get_artifact(&self, id: &str) -> Result<Option<ArtifactRecord>, StoreError>;
}
