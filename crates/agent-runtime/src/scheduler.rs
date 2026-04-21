use crate::mission::{
    JobKind, JobSpec, JobStatus, MissionExecutionIntent, MissionSpec, MissionStatus,
};
use crate::run::{RunSnapshot, RunStatus};
use crate::verification::VerificationStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutonomyBudget {
    pub max_jobs_per_tick: usize,
    pub max_running_runs: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupervisorPolicy {
    pub allow_assisted_wakeup: bool,
    pub require_delegate_approval: bool,
    pub require_verification_for_completion: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissionVerificationSummary {
    pub mission_id: String,
    pub status: VerificationStatus,
    pub missing_required_checks: Vec<String>,
    pub open_risks: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SupervisorLoop {
    policy: SupervisorPolicy,
    budget: AutonomyBudget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupervisorTickInput<'a> {
    pub now: i64,
    pub missions: &'a [MissionSpec],
    pub jobs: &'a [JobSpec],
    pub runs: &'a [RunSnapshot],
    pub verifications: &'a [MissionVerificationSummary],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupervisorTick {
    pub actions: Vec<SupervisorAction>,
    pub budget_remaining: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupervisorAction {
    QueueJob(Box<JobSpec>),
    DispatchJob { job_id: String, kind: JobKind },
    RequestApproval { job_id: String, reason: String },
    DeferMission { mission_id: String, reason: String },
    CompleteMission { mission_id: String },
}

impl AutonomyBudget {
    pub fn new(max_jobs_per_tick: usize, max_running_runs: usize) -> Self {
        Self {
            max_jobs_per_tick,
            max_running_runs,
        }
    }
}

impl Default for SupervisorPolicy {
    fn default() -> Self {
        Self {
            allow_assisted_wakeup: false,
            require_delegate_approval: true,
            require_verification_for_completion: true,
        }
    }
}

impl Default for AutonomyBudget {
    fn default() -> Self {
        Self {
            max_jobs_per_tick: 1,
            max_running_runs: 1,
        }
    }
}

impl SupervisorLoop {
    pub fn new(policy: SupervisorPolicy, budget: AutonomyBudget) -> Self {
        Self { policy, budget }
    }

    pub fn tick(&self, input: SupervisorTickInput<'_>) -> SupervisorTick {
        let mut actions = Vec::new();
        let mut budget_remaining = self.budget.max_jobs_per_tick;
        let running_runs = count_active_runs(input.runs);

        for mission in input
            .missions
            .iter()
            .filter(|mission| mission.status == MissionStatus::Running)
        {
            if mission.schedule.interval_seconds.is_some()
                || mission_has_open_jobs(mission, input.jobs)
            {
                continue;
            }

            if let Some(summary) = input
                .verifications
                .iter()
                .find(|summary| summary.mission_id == mission.id)
            {
                if self.policy.require_verification_for_completion
                    && summary.status != VerificationStatus::Passed
                {
                    actions.push(SupervisorAction::DeferMission {
                        mission_id: mission.id.clone(),
                        reason: "verification is not yet passing".to_string(),
                    });
                    continue;
                }
            } else if self.policy.require_verification_for_completion {
                actions.push(SupervisorAction::DeferMission {
                    mission_id: mission.id.clone(),
                    reason: "verification is not yet passing".to_string(),
                });
                continue;
            }

            if mission_has_terminal_run(mission, input.runs) {
                actions.push(SupervisorAction::CompleteMission {
                    mission_id: mission.id.clone(),
                });
            }
        }

        let mut dispatched_jobs = 0usize;
        for job in input
            .jobs
            .iter()
            .filter(|job| job.status == JobStatus::Queued)
        {
            if self.policy.require_delegate_approval && job.kind == JobKind::Delegate {
                actions.push(SupervisorAction::RequestApproval {
                    job_id: job.id.clone(),
                    reason: "delegate jobs require operator approval".to_string(),
                });
                continue;
            }

            if budget_remaining == 0
                || running_runs.saturating_add(dispatched_jobs) >= self.budget.max_running_runs
            {
                break;
            }

            actions.push(SupervisorAction::DispatchJob {
                job_id: job.id.clone(),
                kind: job.kind,
            });
            budget_remaining -= 1;
            dispatched_jobs += 1;
        }

        for mission in input.missions {
            if !self.mission_is_due(mission, input.jobs, input.now) {
                continue;
            }

            if budget_remaining == 0
                || running_runs.saturating_add(dispatched_jobs) >= self.budget.max_running_runs
            {
                actions.push(SupervisorAction::DeferMission {
                    mission_id: mission.id.clone(),
                    reason: "autonomy budget exhausted".to_string(),
                });
                continue;
            }

            actions.push(SupervisorAction::QueueJob(Box::new(JobSpec::mission_turn(
                format!("{}-mission-turn-{}", mission.id, input.now),
                mission.session_id.clone(),
                mission.id.clone(),
                None,
                None,
                mission.objective.clone(),
                input.now,
            ))));
            budget_remaining -= 1;
            dispatched_jobs += 1;
        }

        SupervisorTick {
            actions,
            budget_remaining,
        }
    }

    fn mission_is_due(&self, mission: &MissionSpec, jobs: &[JobSpec], now: i64) -> bool {
        if mission.status != MissionStatus::Ready {
            return false;
        }

        match mission.execution_intent {
            MissionExecutionIntent::Assisted => {
                self.policy.allow_assisted_wakeup
                    && mission
                        .schedule
                        .not_before
                        .is_none_or(|not_before| now >= not_before)
            }
            MissionExecutionIntent::Autonomous => {
                mission
                    .schedule
                    .not_before
                    .is_none_or(|not_before| now >= not_before)
                    && jobs
                        .iter()
                        .all(|job| job.mission_id.as_deref() != Some(mission.id.as_str()))
            }
            MissionExecutionIntent::Scheduled => {
                if mission
                    .schedule
                    .not_before
                    .is_some_and(|not_before| now < not_before)
                {
                    return false;
                }

                if jobs.iter().any(|job| {
                    job.mission_id.as_deref() == Some(mission.id.as_str())
                        && job.kind == JobKind::MissionTurn
                        && matches!(
                            job.status,
                            JobStatus::Queued | JobStatus::Running | JobStatus::Blocked
                        )
                }) {
                    return false;
                }

                match latest_finished_mission_turn_at(mission, jobs) {
                    Some(last_finished_at) => mission
                        .schedule
                        .interval_seconds
                        .is_some_and(|interval| now - last_finished_at >= interval as i64),
                    None => true,
                }
            }
        }
    }
}

fn count_active_runs(runs: &[RunSnapshot]) -> usize {
    runs.iter()
        .filter(|run| {
            matches!(
                run.status,
                RunStatus::Queued
                    | RunStatus::Running
                    | RunStatus::WaitingApproval
                    | RunStatus::WaitingProcess
                    | RunStatus::WaitingDelegate
                    | RunStatus::Resuming
            )
        })
        .count()
}

fn mission_has_open_jobs(mission: &MissionSpec, jobs: &[JobSpec]) -> bool {
    jobs.iter().any(|job| {
        job.mission_id.as_deref() == Some(mission.id.as_str())
            && matches!(
                job.status,
                JobStatus::Queued | JobStatus::Running | JobStatus::Blocked
            )
    })
}

fn mission_has_terminal_run(mission: &MissionSpec, runs: &[RunSnapshot]) -> bool {
    runs.iter().any(|run| {
        run.mission_id.as_deref() == Some(mission.id.as_str()) && run.status.is_terminal()
    })
}

fn latest_finished_mission_turn_at(mission: &MissionSpec, jobs: &[JobSpec]) -> Option<i64> {
    jobs.iter()
        .filter(|job| {
            job.mission_id.as_deref() == Some(mission.id.as_str())
                && job.kind == JobKind::MissionTurn
        })
        .filter_map(|job| job.finished_at.or(Some(job.updated_at)))
        .max()
}

#[cfg(test)]
mod tests {
    use super::{
        AutonomyBudget, MissionVerificationSummary, SupervisorAction, SupervisorLoop,
        SupervisorPolicy, SupervisorTickInput,
    };
    use crate::mission::{
        JobExecutionInput, JobKind, JobSpec, JobStatus, MissionExecutionIntent, MissionSchedule,
        MissionSpec, MissionStatus,
    };
    use crate::run::{RunSnapshot, RunStatus};
    use crate::verification::VerificationStatus;

    #[test]
    fn due_scheduled_mission_is_queued_for_an_autonomous_turn() {
        let supervisor =
            SupervisorLoop::new(SupervisorPolicy::default(), AutonomyBudget::new(1, 1));
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
        let supervisor =
            SupervisorLoop::new(SupervisorPolicy::default(), AutonomyBudget::new(0, 1));
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
        let supervisor =
            SupervisorLoop::new(SupervisorPolicy::default(), AutonomyBudget::new(1, 1));
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
        let supervisor =
            SupervisorLoop::new(SupervisorPolicy::default(), AutonomyBudget::new(1, 1));
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
        let supervisor =
            SupervisorLoop::new(SupervisorPolicy::default(), AutonomyBudget::new(1, 1));
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
}
