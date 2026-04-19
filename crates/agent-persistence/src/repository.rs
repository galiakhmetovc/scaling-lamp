use crate::records::{
    ArtifactRecord, JobRecord, MissionRecord, RunRecord, SessionRecord, TranscriptRecord,
};
use crate::store::StoreError;

pub trait SessionRepository {
    fn put_session(&self, record: &SessionRecord) -> Result<(), StoreError>;
    fn get_session(&self, id: &str) -> Result<Option<SessionRecord>, StoreError>;
}

pub trait MissionRepository {
    fn put_mission(&self, record: &MissionRecord) -> Result<(), StoreError>;
    fn get_mission(&self, id: &str) -> Result<Option<MissionRecord>, StoreError>;
}

pub trait RunRepository {
    fn put_run(&self, record: &RunRecord) -> Result<(), StoreError>;
    fn get_run(&self, id: &str) -> Result<Option<RunRecord>, StoreError>;
}

pub trait JobRepository {
    fn put_job(&self, record: &JobRecord) -> Result<(), StoreError>;
    fn get_job(&self, id: &str) -> Result<Option<JobRecord>, StoreError>;
}

pub trait TranscriptRepository {
    fn put_transcript(&self, record: &TranscriptRecord) -> Result<(), StoreError>;
    fn get_transcript(&self, id: &str) -> Result<Option<TranscriptRecord>, StoreError>;
    fn list_transcripts_for_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<TranscriptRecord>, StoreError>;
}

pub trait ArtifactRepository {
    fn put_artifact(&self, record: &ArtifactRecord) -> Result<(), StoreError>;
    fn get_artifact(&self, id: &str) -> Result<Option<ArtifactRecord>, StoreError>;
}
