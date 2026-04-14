# State Machines

This page is the quick map for the runtime lifecycle primitives in `teamD`.

The canonical status strings live in code:

- [internal/runtime/store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/store.go)
- [internal/approvals/service.go](/home/admin/AI-AGENT/data/projects/teamD/internal/approvals/service.go)
- [internal/runtime/types.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/types.go)

## Run

Run state tracks one user request from start to terminal result.

Current runtime states:

- `queued`
- `running`
- `waiting_approval`
- `completed`
- `failed`
- `cancelled`

Current transitions:

- `queued -> running`
  - accepted runs are persisted as `running`
  - see [internal/runtime/run_manager.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/run_manager.go)
- `running -> waiting_approval`
  - a tool call needs an approval gate
  - the runtime store persists `waiting_approval`
  - see [docs/agent/approvals.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/approvals.md)
- `running -> completed`
  - the provider returns final text and the run finishes cleanly
- `running -> failed`
  - the execution path returns an error
- `running -> cancelled`
  - `/cancel` requests stop the active context
- `waiting_approval -> running`
  - approval is granted and execution resumes
- `waiting_approval -> failed`
  - approval is rejected, expired, or otherwise cannot resume
- `running|waiting_approval -> cancelled`
  - cancel wins even if the run is blocked on an approval wait

Important detail:

- `queued` exists in the enum and recovery SQL
- the current `RunManager` writes `running` on accepted start
- restart recovery can move interrupted `queued` or `running` runs to `failed`

Relevant events:

- `run.started`
- `run.cancel_requested`
- `run.cancelled`
- `run.failed`
- `run.completed`

Code paths:

- [internal/runtime/run_manager.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/run_manager.go)
- [internal/runtime/active_registry.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/active_registry.go)
- [internal/runtime/runtime_api.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/runtime_api.go)

## Approval

Approval state is restart-safe and keyed by approval id.

States:

- `pending`
- `approved`
- `rejected`
- `expired`
- `canceled`

Transitions:

- `pending -> approved`
  - user accepts the approval
- `pending -> rejected`
  - user rejects the approval
- `pending -> expired`
  - the approval is no longer valid
- `pending -> canceled`
  - the underlying work is canceled before a decision arrives
- `approved|rejected|expired|canceled`
  - terminal states

The approval service also keeps callback idempotency, so the same callback update does not apply twice.

Where approval touches run state:

- a pending approval usually coincides with a run in `waiting_approval`
- once approved, the saved continuation resumes the same execution path

Relevant events and storage:

- approval records in the approvals store
- approval continuation records in the runtime store
- `waiting_approval` on the run record while approval is pending

Code paths:

- [internal/approvals/service.go](/home/admin/AI-AGENT/data/projects/teamD/internal/approvals/service.go)
- [internal/runtime/store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/store.go)
- [internal/transport/telegram/approval_resume.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/approval_resume.go)

## Worker

Worker state tracks a managed local subagent.

States:

- `idle`
- `running`
- `waiting_approval`
- `failed`
- `closed`

Transitions:

- `idle -> running`
  - `worker message` starts a detached worker run
- `running -> waiting_approval`
  - the worker run blocks on an approval
- `running -> idle`
  - the worker run completes successfully and no new active run remains
- `running -> failed`
  - the worker run fails
- `waiting_approval -> idle`
  - approval resolves and the run finishes cleanly
- `waiting_approval -> failed`
  - approval is rejected or the continuation fails
- `idle|running|waiting_approval -> closed`
  - the worker is explicitly closed
- `closed ->` no further messages accepted

Important detail:

- worker status is derived from the last run when possible
- `worker.approval_requested` is emitted when a worker hits an approval wait
- `worker.handoff_created` is emitted after the worker run ends and a handoff is assembled

Code paths:

- [internal/runtime/workers_service.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/workers_service.go)
- [internal/runtime/types.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/types.go)
- [internal/worker/lifecycle.go](/home/admin/AI-AGENT/data/projects/teamD/internal/worker/lifecycle.go)

## Job

Job state tracks detached background command execution.

States:

- `queued`
- `running`
- `completed`
- `failed`
- `cancelled`

Transitions:

- `queued -> running`
  - the command starts successfully
- `queued -> failed`
  - the command cannot start
- `running -> completed`
  - the process exits with success
- `running -> failed`
  - the process exits non-zero or execution fails
- `running -> cancelled`
  - cancel is requested and the context exits
- `queued|running -> failed`
  - restart recovery marks interrupted jobs as failed

Relevant events:

- `job.created`
- `job.started`
- `job.completed`
- `job.failed`
- `job.cancel_requested`
- `job.cancelled`

Code paths:

- [internal/runtime/jobs_service.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/jobs_service.go)
- [internal/runtime/types.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/types.go)
- [internal/runtime/store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/store.go)

## Where To Look When Debugging

- Run status looks wrong: check `internal/runtime/run_manager.go`, then the run record and `runtime_events`.
- Approval is stuck: check `internal/approvals/service.go`, `runtime_approvals`, and the saved continuation records.
- Worker says `waiting_approval`: check `internal/runtime/workers_service.go` and the worker event stream for `worker.approval_requested`.
- Job never finishes: check `internal/runtime/jobs_service.go`, `runtime_job_logs`, and `runtime_jobs.cancel_requested`.
- Status looks stale after restart: check recovery paths in `internal/runtime/sqlite_store.go` and `internal/runtime/postgres_store.go`.
- Need the transport-facing view: check `docs/agent/runtime-api-walkthrough.md` and `docs/agent/operator-chat.md`.
