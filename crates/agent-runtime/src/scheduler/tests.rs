use super::{
    AutonomyBudget, MissionVerificationSummary, SupervisorAction, SupervisorLoop, SupervisorPolicy,
    SupervisorTickInput,
};
use crate::mission::{
    JobExecutionInput, JobKind, JobSpec, JobStatus, MissionExecutionIntent, MissionSchedule,
    MissionSpec, MissionStatus,
};
use crate::run::{RunSnapshot, RunStatus};
use crate::verification::VerificationStatus;

#[test]
fn due_scheduled_mission_is_queued_for_an_autonomous_turn() {
    let supervisor = SupervisorLoop::new(SupervisorPolicy::default(), AutonomyBudget::new(1, 1));
    let mission = scheduled_mission("mission-1", MissionStatus::Ready, Some(60), Some(3600));

    let tick = supervisor.tick(SupervisorTickInput {
        now: 60,
        missions: &[mission],
        jobs: &[],
        runs: &[],
        verifications: &[],
    });

    assert_eq!(
        tick.actions,
        vec![SupervisorAction::QueueJob(Box::new(JobSpec::mission_turn(
            "mission-1-mission-turn-60",
            "session-1",
            "mission-1",
            None,
            None,
            "Ship the runtime",
            60,
        )))]
    );
    assert_eq!(tick.budget_remaining, 0);
}

#[test]
fn budget_exhaustion_defers_due_missions_instead_of_queueing_them() {
    let supervisor = SupervisorLoop::new(SupervisorPolicy::default(), AutonomyBudget::new(0, 1));
    let mission = scheduled_mission("mission-1", MissionStatus::Ready, Some(60), Some(3600));

    let tick = supervisor.tick(SupervisorTickInput {
        now: 60,
        missions: &[mission],
        jobs: &[],
        runs: &[],
        verifications: &[],
    });

    assert_eq!(
        tick.actions,
        vec![SupervisorAction::DeferMission {
            mission_id: "mission-1".to_string(),
            reason: "autonomy budget exhausted".to_string(),
        }]
    );
}

#[test]
fn delegate_jobs_require_approval_before_dispatch() {
    let supervisor = SupervisorLoop::new(SupervisorPolicy::default(), AutonomyBudget::new(1, 1));
    let job = JobSpec {
        id: "job-1".to_string(),
        session_id: "session-1".to_string(),
        mission_id: Some("mission-1".to_string()),
        run_id: None,
        parent_job_id: None,
        kind: JobKind::Delegate,
        status: JobStatus::Queued,
        input: JobExecutionInput::Delegate {
            label: "worker-a".to_string(),
            goal: "inspect the runtime".to_string(),
            bounded_context: vec!["src/runtime.rs".to_string()],
            write_scope: crate::delegation::DelegateWriteScope {
                allowed_paths: vec!["src".to_string()],
            },
            expected_output: "summary".to_string(),
            owner: "local-child".to_string(),
        },
        result: None,
        error: None,
        created_at: 10,
        updated_at: 10,
        started_at: None,
        finished_at: None,
        attempt_count: 0,
        max_attempts: 1,
        lease_owner: None,
        lease_expires_at: None,
        heartbeat_at: None,
        cancel_requested_at: None,
        last_progress_message: None,
        callback: None,
        callback_sent_at: None,
    };

    let tick = supervisor.tick(SupervisorTickInput {
        now: 60,
        missions: &[],
        jobs: &[job],
        runs: &[],
        verifications: &[],
    });

    assert_eq!(
        tick.actions,
        vec![SupervisorAction::RequestApproval {
            job_id: "job-1".to_string(),
            reason: "delegate jobs require operator approval".to_string(),
        }]
    );
}

#[test]
fn running_missions_need_passing_verification_before_completion() {
    let supervisor = SupervisorLoop::new(SupervisorPolicy::default(), AutonomyBudget::new(1, 1));
    let mission = scheduled_mission("mission-1", MissionStatus::Running, None, None);

    let tick = supervisor.tick(SupervisorTickInput {
        now: 60,
        missions: &[mission],
        jobs: &[],
        runs: &[],
        verifications: &[MissionVerificationSummary {
            mission_id: "mission-1".to_string(),
            status: VerificationStatus::NeedsReview,
            missing_required_checks: vec!["test".to_string()],
            open_risks: vec!["risk-1".to_string()],
        }],
    });

    assert_eq!(
        tick.actions,
        vec![SupervisorAction::DeferMission {
            mission_id: "mission-1".to_string(),
            reason: "verification is not yet passing".to_string(),
        }]
    );
}

#[test]
fn passing_verification_allows_running_missions_to_complete() {
    let supervisor = SupervisorLoop::new(SupervisorPolicy::default(), AutonomyBudget::new(1, 1));
    let mission = scheduled_mission("mission-1", MissionStatus::Running, None, None);

    let tick = supervisor.tick(SupervisorTickInput {
        now: 60,
        missions: &[mission],
        jobs: &[],
        runs: &[RunSnapshot {
            id: "run-1".to_string(),
            session_id: "session-1".to_string(),
            mission_id: Some("mission-1".to_string()),
            status: RunStatus::Completed,
            started_at: 10,
            updated_at: 20,
            finished_at: Some(20),
            error: None,
            result: Some("done".to_string()),
            ..RunSnapshot::default()
        }],
        verifications: &[MissionVerificationSummary {
            mission_id: "mission-1".to_string(),
            status: VerificationStatus::Passed,
            missing_required_checks: Vec::new(),
            open_risks: Vec::new(),
        }],
    });

    assert_eq!(
        tick.actions,
        vec![SupervisorAction::CompleteMission {
            mission_id: "mission-1".to_string(),
        }]
    );
}

fn scheduled_mission(
    id: &str,
    status: MissionStatus,
    not_before: Option<i64>,
    interval_seconds: Option<u64>,
) -> MissionSpec {
    MissionSpec {
        id: id.to_string(),
        session_id: "session-1".to_string(),
        objective: "Ship the runtime".to_string(),
        status,
        execution_intent: if interval_seconds.is_some() {
            MissionExecutionIntent::Scheduled
        } else {
            MissionExecutionIntent::Autonomous
        },
        schedule: MissionSchedule {
            not_before,
            interval_seconds,
        },
        acceptance_criteria: Vec::new(),
        created_at: 1,
        updated_at: 1,
        completed_at: None,
    }
}
