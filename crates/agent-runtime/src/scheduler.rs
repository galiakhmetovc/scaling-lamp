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
mod tests;
