use crate::records::{
    AgentChainContinuationRecord, AgentProfileRecord, AgentScheduleRecord, ArtifactRecord,
    ContextOffloadRecord, ContextSummaryRecord, JobRecord, MissionRecord, PlanRecord, RunRecord,
    SessionInboxEventRecord, SessionRecord, TranscriptRecord,
};
use crate::store::StoreError;
use agent_runtime::context::ContextOffloadPayload;

pub trait SessionRepository {
    fn put_session(&self, record: &SessionRecord) -> Result<(), StoreError>;
    fn get_session(&self, id: &str) -> Result<Option<SessionRecord>, StoreError>;
    fn list_sessions(&self) -> Result<Vec<SessionRecord>, StoreError>;
    fn delete_session(&self, id: &str) -> Result<bool, StoreError>;
}

pub trait AgentRepository {
    fn put_agent_profile(&self, record: &AgentProfileRecord) -> Result<(), StoreError>;
    fn get_agent_profile(&self, id: &str) -> Result<Option<AgentProfileRecord>, StoreError>;
    fn list_agent_profiles(&self) -> Result<Vec<AgentProfileRecord>, StoreError>;
    fn delete_agent_profile(&self, id: &str) -> Result<bool, StoreError>;
    fn get_current_agent_profile_id(&self) -> Result<Option<String>, StoreError>;
    fn set_current_agent_profile_id(&self, id: Option<&str>) -> Result<(), StoreError>;
    fn put_agent_chain_continuation(
        &self,
        record: &AgentChainContinuationRecord,
    ) -> Result<(), StoreError>;
    fn get_agent_chain_continuation(
        &self,
        chain_id: &str,
    ) -> Result<Option<AgentChainContinuationRecord>, StoreError>;
    fn delete_agent_chain_continuation(&self, chain_id: &str) -> Result<bool, StoreError>;
    fn put_agent_schedule(&self, record: &AgentScheduleRecord) -> Result<(), StoreError>;
    fn get_agent_schedule(&self, id: &str) -> Result<Option<AgentScheduleRecord>, StoreError>;
    fn list_agent_schedules(&self) -> Result<Vec<AgentScheduleRecord>, StoreError>;
    fn delete_agent_schedule(&self, id: &str) -> Result<bool, StoreError>;
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
    fn list_jobs_for_session(&self, session_id: &str) -> Result<Vec<JobRecord>, StoreError>;
    fn list_active_jobs_for_session(&self, session_id: &str) -> Result<Vec<JobRecord>, StoreError>;
}

pub trait TranscriptRepository {
    fn put_transcript(&self, record: &TranscriptRecord) -> Result<(), StoreError>;
    fn get_transcript(&self, id: &str) -> Result<Option<TranscriptRecord>, StoreError>;
    fn list_transcripts_for_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<TranscriptRecord>, StoreError>;
}

pub trait SessionInboxRepository {
    fn put_session_inbox_event(&self, record: &SessionInboxEventRecord) -> Result<(), StoreError>;
    fn get_session_inbox_event(
        &self,
        id: &str,
    ) -> Result<Option<SessionInboxEventRecord>, StoreError>;
    fn list_session_inbox_events_for_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<SessionInboxEventRecord>, StoreError>;
    fn list_queued_session_inbox_events_for_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<SessionInboxEventRecord>, StoreError>;
    fn list_queued_session_inbox_events(&self) -> Result<Vec<SessionInboxEventRecord>, StoreError>;
}

pub trait ContextSummaryRepository {
    fn put_context_summary(&self, record: &ContextSummaryRecord) -> Result<(), StoreError>;
    fn get_context_summary(
        &self,
        session_id: &str,
    ) -> Result<Option<ContextSummaryRecord>, StoreError>;
}

pub trait ContextOffloadRepository {
    fn put_context_offload(
        &self,
        record: &ContextOffloadRecord,
        payloads: &[ContextOffloadPayload],
    ) -> Result<(), StoreError>;
    fn get_context_offload(
        &self,
        session_id: &str,
    ) -> Result<Option<ContextOffloadRecord>, StoreError>;
    fn get_context_offload_payload(
        &self,
        artifact_id: &str,
    ) -> Result<Option<ContextOffloadPayload>, StoreError>;
}

pub trait PlanRepository {
    fn put_plan(&self, record: &PlanRecord) -> Result<(), StoreError>;
    fn get_plan(&self, session_id: &str) -> Result<Option<PlanRecord>, StoreError>;
}

pub trait ArtifactRepository {
    fn put_artifact(&self, record: &ArtifactRecord) -> Result<(), StoreError>;
    fn get_artifact(&self, id: &str) -> Result<Option<ArtifactRecord>, StoreError>;
}
