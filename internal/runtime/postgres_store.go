package runtime

import (
	"context"
	"database/sql"
	"encoding/json"
	"fmt"
	"strings"
	"time"

	"teamd/internal/approvals"
)

type PostgresStore struct {
	db *sql.DB
}

func NewPostgresStore(db *sql.DB) *PostgresStore {
	return &PostgresStore{db: db}
}

func (s *PostgresStore) ensureSchema(ctx context.Context) error {
	const schema = `
CREATE TABLE IF NOT EXISTS runtime_runs (
  run_id TEXT PRIMARY KEY,
  chat_id BIGINT NOT NULL,
  session_id TEXT NOT NULL,
  query TEXT NOT NULL,
  final_response TEXT NOT NULL DEFAULT '',
  prompt_budget JSONB NOT NULL DEFAULT '{}'::jsonb,
  status TEXT NOT NULL,
  started_at TIMESTAMPTZ NOT NULL,
  ended_at TIMESTAMPTZ NULL,
  failure_reason TEXT NOT NULL DEFAULT '',
  cancel_requested BOOLEAN NOT NULL DEFAULT FALSE,
  policy_snapshot JSONB NOT NULL DEFAULT '{}'::jsonb
);
CREATE TABLE IF NOT EXISTS runtime_jobs (
  job_id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  owner_run_id TEXT NOT NULL DEFAULT '',
  owner_worker_id TEXT NOT NULL DEFAULT '',
  chat_id BIGINT NOT NULL,
  session_id TEXT NOT NULL,
  command TEXT NOT NULL,
  args JSONB NOT NULL DEFAULT '[]'::jsonb,
  cwd TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL,
  started_at TIMESTAMPTZ NOT NULL,
  ended_at TIMESTAMPTZ NULL,
  exit_code INTEGER NULL,
  failure_reason TEXT NOT NULL DEFAULT '',
  cancel_requested BOOLEAN NOT NULL DEFAULT FALSE,
  policy_snapshot JSONB NOT NULL DEFAULT '{}'::jsonb
);
CREATE TABLE IF NOT EXISTS runtime_job_logs (
  id BIGSERIAL PRIMARY KEY,
  job_id TEXT NOT NULL,
  stream TEXT NOT NULL,
  content TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE TABLE IF NOT EXISTS runtime_workers (
  worker_id TEXT PRIMARY KEY,
  parent_chat_id BIGINT NOT NULL,
  parent_session_id TEXT NOT NULL,
  worker_chat_id BIGINT NOT NULL UNIQUE,
  worker_session_id TEXT NOT NULL,
  status TEXT NOT NULL,
  last_run_id TEXT NOT NULL DEFAULT '',
  last_error TEXT NOT NULL DEFAULT '',
  process_pid INTEGER NOT NULL DEFAULT 0,
  process_state TEXT NOT NULL DEFAULT 'stopped',
  process_started_at TIMESTAMPTZ NULL,
  process_last_heartbeat_at TIMESTAMPTZ NULL,
  process_exited_at TIMESTAMPTZ NULL,
  process_exit_reason TEXT NOT NULL DEFAULT '',
  created_at TIMESTAMPTZ NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL,
  last_message_at TIMESTAMPTZ NULL,
  closed_at TIMESTAMPTZ NULL,
  policy_snapshot JSONB NOT NULL DEFAULT '{}'::jsonb
);
CREATE TABLE IF NOT EXISTS runtime_worker_handoffs (
  worker_id TEXT PRIMARY KEY,
  last_run_id TEXT NOT NULL DEFAULT '',
  summary TEXT NOT NULL DEFAULT '',
  artifacts JSONB NOT NULL DEFAULT '[]'::jsonb,
  promoted_facts JSONB NOT NULL DEFAULT '[]'::jsonb,
  open_questions JSONB NOT NULL DEFAULT '[]'::jsonb,
  recommended_next_step TEXT NOT NULL DEFAULT '',
  created_at TIMESTAMPTZ NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL
);
CREATE TABLE IF NOT EXISTS runtime_events (
  id BIGSERIAL PRIMARY KEY,
  entity_type TEXT NOT NULL,
  entity_id TEXT NOT NULL,
  chat_id BIGINT NOT NULL DEFAULT 0,
  session_id TEXT NOT NULL DEFAULT '',
  run_id TEXT NOT NULL DEFAULT '',
  kind TEXT NOT NULL,
  payload JSONB NOT NULL DEFAULT '{}'::jsonb,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE TABLE IF NOT EXISTS runtime_plans (
  plan_id TEXT PRIMARY KEY,
  owner_type TEXT NOT NULL,
  owner_id TEXT NOT NULL,
  title TEXT NOT NULL,
  notes JSONB NOT NULL DEFAULT '[]'::jsonb,
  created_at TIMESTAMPTZ NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL
);
CREATE TABLE IF NOT EXISTS runtime_plan_items (
  plan_id TEXT NOT NULL,
  item_id TEXT NOT NULL,
  content TEXT NOT NULL,
  status TEXT NOT NULL,
  position INTEGER NOT NULL,
  created_at TIMESTAMPTZ NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL,
  PRIMARY KEY (plan_id, item_id)
);
CREATE TABLE IF NOT EXISTS runtime_checkpoints (
  chat_id BIGINT NOT NULL,
  session_id TEXT NOT NULL,
  originating_intent TEXT NOT NULL,
  what_happened TEXT NOT NULL,
  what_matters_now TEXT NOT NULL,
  archive_refs JSONB NOT NULL DEFAULT '[]'::jsonb,
  artifact_refs JSONB NOT NULL DEFAULT '[]'::jsonb,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (chat_id, session_id)
);
CREATE TABLE IF NOT EXISTS runtime_continuity (
  chat_id BIGINT NOT NULL,
  session_id TEXT NOT NULL,
  user_goal TEXT NOT NULL,
  current_state TEXT NOT NULL,
  resolved_facts JSONB NOT NULL DEFAULT '[]'::jsonb,
  unresolved_items JSONB NOT NULL DEFAULT '[]'::jsonb,
  archive_refs JSONB NOT NULL DEFAULT '[]'::jsonb,
  artifact_refs JSONB NOT NULL DEFAULT '[]'::jsonb,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (chat_id, session_id)
);
CREATE TABLE IF NOT EXISTS runtime_session_head (
  chat_id BIGINT NOT NULL,
  session_id TEXT NOT NULL,
  last_completed_run_id TEXT NOT NULL DEFAULT '',
  current_goal TEXT NOT NULL DEFAULT '',
  last_result_summary TEXT NOT NULL DEFAULT '',
  current_plan_id TEXT NOT NULL DEFAULT '',
  current_plan_title TEXT NOT NULL DEFAULT '',
  current_plan_items JSONB NOT NULL DEFAULT '[]'::jsonb,
  resolved_entities JSONB NOT NULL DEFAULT '[]'::jsonb,
  recent_artifact_refs JSONB NOT NULL DEFAULT '[]'::jsonb,
  open_loops JSONB NOT NULL DEFAULT '[]'::jsonb,
  current_project TEXT NOT NULL DEFAULT '',
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (chat_id, session_id)
);
CREATE TABLE IF NOT EXISTS runtime_processed_updates (
  chat_id BIGINT NOT NULL,
  update_id BIGINT NOT NULL,
  processed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (chat_id, update_id)
);
CREATE TABLE IF NOT EXISTS runtime_session_overrides (
  session_id TEXT PRIMARY KEY,
  runtime_config JSONB NOT NULL DEFAULT '{}'::jsonb,
  memory_policy JSONB NOT NULL DEFAULT '{}'::jsonb,
  action_policy JSONB NOT NULL DEFAULT '{}'::jsonb,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE TABLE IF NOT EXISTS runtime_approvals (
  approval_id TEXT PRIMARY KEY,
  worker_id TEXT NOT NULL,
  session_id TEXT NOT NULL,
  payload TEXT NOT NULL,
  status TEXT NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  reason TEXT NOT NULL DEFAULT '',
  target_type TEXT NOT NULL DEFAULT '',
  target_id TEXT NOT NULL DEFAULT '',
  requested_at TIMESTAMPTZ NULL,
  decided_at TIMESTAMPTZ NULL,
  decision_update_id TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS runtime_approval_callbacks (
  update_id TEXT PRIMARY KEY,
  approval_id TEXT NOT NULL,
  worker_id TEXT NOT NULL,
  session_id TEXT NOT NULL,
  payload TEXT NOT NULL,
  status TEXT NOT NULL,
  handled_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  reason TEXT NOT NULL DEFAULT '',
  target_type TEXT NOT NULL DEFAULT '',
  target_id TEXT NOT NULL DEFAULT '',
  requested_at TIMESTAMPTZ NULL,
  decided_at TIMESTAMPTZ NULL,
  decision_update_id TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS runtime_approval_continuations (
  approval_id TEXT PRIMARY KEY,
  run_id TEXT NOT NULL,
  chat_id BIGINT NOT NULL,
  session_id TEXT NOT NULL,
  query TEXT NOT NULL,
  tool_call_id TEXT NOT NULL,
  tool_name TEXT NOT NULL,
  tool_arguments JSONB NOT NULL DEFAULT '{}'::jsonb,
  requested_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE TABLE IF NOT EXISTS runtime_timeout_decisions (
  run_id TEXT PRIMARY KEY,
  chat_id BIGINT NOT NULL,
  session_id TEXT NOT NULL,
  status TEXT NOT NULL,
  failure_reason TEXT NOT NULL DEFAULT '',
  requested_at TIMESTAMPTZ NOT NULL,
  resolved_at TIMESTAMPTZ NULL,
  auto_continue_deadline TIMESTAMPTZ NULL,
  auto_continue_used BOOLEAN NOT NULL DEFAULT FALSE,
  round_index INTEGER NOT NULL DEFAULT 0
);
`
	if _, err := s.db.ExecContext(ctx, schema); err != nil {
		return err
	}
	_, err := s.db.ExecContext(ctx, `
ALTER TABLE runtime_runs ADD COLUMN IF NOT EXISTS policy_snapshot JSONB NOT NULL DEFAULT '{}'::jsonb;
ALTER TABLE runtime_runs ADD COLUMN IF NOT EXISTS final_response TEXT NOT NULL DEFAULT '';
ALTER TABLE runtime_runs ADD COLUMN IF NOT EXISTS prompt_budget JSONB NOT NULL DEFAULT '{}'::jsonb;
ALTER TABLE runtime_jobs ADD COLUMN IF NOT EXISTS policy_snapshot JSONB NOT NULL DEFAULT '{}'::jsonb;
ALTER TABLE runtime_workers ADD COLUMN IF NOT EXISTS policy_snapshot JSONB NOT NULL DEFAULT '{}'::jsonb;
ALTER TABLE runtime_workers ADD COLUMN IF NOT EXISTS process_pid INTEGER NOT NULL DEFAULT 0;
ALTER TABLE runtime_workers ADD COLUMN IF NOT EXISTS process_state TEXT NOT NULL DEFAULT 'stopped';
ALTER TABLE runtime_workers ADD COLUMN IF NOT EXISTS process_started_at TIMESTAMPTZ NULL;
ALTER TABLE runtime_workers ADD COLUMN IF NOT EXISTS process_last_heartbeat_at TIMESTAMPTZ NULL;
ALTER TABLE runtime_workers ADD COLUMN IF NOT EXISTS process_exited_at TIMESTAMPTZ NULL;
ALTER TABLE runtime_workers ADD COLUMN IF NOT EXISTS process_exit_reason TEXT NOT NULL DEFAULT '';
ALTER TABLE runtime_approvals ADD COLUMN IF NOT EXISTS reason TEXT NOT NULL DEFAULT '';
ALTER TABLE runtime_approvals ADD COLUMN IF NOT EXISTS target_type TEXT NOT NULL DEFAULT '';
ALTER TABLE runtime_approvals ADD COLUMN IF NOT EXISTS target_id TEXT NOT NULL DEFAULT '';
ALTER TABLE runtime_approvals ADD COLUMN IF NOT EXISTS requested_at TIMESTAMPTZ NULL;
ALTER TABLE runtime_approvals ADD COLUMN IF NOT EXISTS decided_at TIMESTAMPTZ NULL;
ALTER TABLE runtime_approvals ADD COLUMN IF NOT EXISTS decision_update_id TEXT NOT NULL DEFAULT '';
ALTER TABLE runtime_approval_callbacks ADD COLUMN IF NOT EXISTS reason TEXT NOT NULL DEFAULT '';
ALTER TABLE runtime_approval_callbacks ADD COLUMN IF NOT EXISTS target_type TEXT NOT NULL DEFAULT '';
ALTER TABLE runtime_approval_callbacks ADD COLUMN IF NOT EXISTS target_id TEXT NOT NULL DEFAULT '';
ALTER TABLE runtime_approval_callbacks ADD COLUMN IF NOT EXISTS requested_at TIMESTAMPTZ NULL;
ALTER TABLE runtime_approval_callbacks ADD COLUMN IF NOT EXISTS decided_at TIMESTAMPTZ NULL;
ALTER TABLE runtime_approval_callbacks ADD COLUMN IF NOT EXISTS decision_update_id TEXT NOT NULL DEFAULT '';
ALTER TABLE runtime_checkpoints ADD COLUMN IF NOT EXISTS archive_refs JSONB NOT NULL DEFAULT '[]'::jsonb;
ALTER TABLE runtime_checkpoints ADD COLUMN IF NOT EXISTS artifact_refs JSONB NOT NULL DEFAULT '[]'::jsonb;
ALTER TABLE runtime_continuity ADD COLUMN IF NOT EXISTS archive_refs JSONB NOT NULL DEFAULT '[]'::jsonb;
ALTER TABLE runtime_continuity ADD COLUMN IF NOT EXISTS artifact_refs JSONB NOT NULL DEFAULT '[]'::jsonb;
ALTER TABLE runtime_session_head ADD COLUMN IF NOT EXISTS resolved_entities JSONB NOT NULL DEFAULT '[]'::jsonb;
ALTER TABLE runtime_session_head ADD COLUMN IF NOT EXISTS recent_artifact_refs JSONB NOT NULL DEFAULT '[]'::jsonb;
ALTER TABLE runtime_session_head ADD COLUMN IF NOT EXISTS open_loops JSONB NOT NULL DEFAULT '[]'::jsonb;
ALTER TABLE runtime_session_head ADD COLUMN IF NOT EXISTS current_plan_id TEXT NOT NULL DEFAULT '';
ALTER TABLE runtime_session_head ADD COLUMN IF NOT EXISTS current_plan_title TEXT NOT NULL DEFAULT '';
ALTER TABLE runtime_session_head ADD COLUMN IF NOT EXISTS current_plan_items JSONB NOT NULL DEFAULT '[]'::jsonb;
`)
	return err
}

func (s *PostgresStore) SaveRun(run RunRecord) error {
	if err := s.ensureSchema(context.Background()); err != nil {
		return err
	}
	snapshotJSON, err := json.Marshal(NormalizePolicySnapshot(run.PolicySnapshot))
	if err != nil {
		return err
	}
	promptBudgetJSON, err := json.Marshal(run.PromptBudget)
	if err != nil {
		return err
	}
	_, err = s.db.ExecContext(context.Background(), `
INSERT INTO runtime_runs (run_id, chat_id, session_id, query, final_response, prompt_budget, status, started_at, ended_at, failure_reason, cancel_requested, policy_snapshot)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
ON CONFLICT(run_id) DO UPDATE SET
  chat_id=excluded.chat_id,
  session_id=excluded.session_id,
  query=excluded.query,
  final_response=excluded.final_response,
  prompt_budget=excluded.prompt_budget,
  status=excluded.status,
  started_at=excluded.started_at,
  ended_at=excluded.ended_at,
  failure_reason=excluded.failure_reason,
  cancel_requested=excluded.cancel_requested,
  policy_snapshot=excluded.policy_snapshot
`, run.RunID, run.ChatID, run.SessionID, run.Query, run.FinalResponse, promptBudgetJSON, string(run.Status), run.StartedAt.UTC(), run.EndedAt, run.FailureReason, run.CancelRequested, snapshotJSON)
	return err
}

func (s *PostgresStore) SavePlan(plan PlanRecord) error {
	if err := s.ensureSchema(context.Background()); err != nil {
		return err
	}
	notesJSON, err := json.Marshal(plan.Notes)
	if err != nil {
		return err
	}
	tx, err := s.db.BeginTx(context.Background(), nil)
	if err != nil {
		return err
	}
	defer func() { _ = tx.Rollback() }()
	_, err = tx.ExecContext(context.Background(), `
INSERT INTO runtime_plans (plan_id, owner_type, owner_id, title, notes, created_at, updated_at)
VALUES ($1, $2, $3, $4, $5, $6, $7)
ON CONFLICT(plan_id) DO UPDATE SET
  owner_type=excluded.owner_type,
  owner_id=excluded.owner_id,
  title=excluded.title,
  notes=excluded.notes,
  created_at=excluded.created_at,
  updated_at=excluded.updated_at
`, plan.PlanID, plan.OwnerType, plan.OwnerID, plan.Title, notesJSON, plan.CreatedAt.UTC(), plan.UpdatedAt.UTC())
	if err != nil {
		return err
	}
	if _, err := tx.ExecContext(context.Background(), `DELETE FROM runtime_plan_items WHERE plan_id = $1`, plan.PlanID); err != nil {
		return err
	}
	for _, item := range plan.Items {
		_, err = tx.ExecContext(context.Background(), `
INSERT INTO runtime_plan_items (plan_id, item_id, content, status, position, created_at, updated_at)
VALUES ($1, $2, $3, $4, $5, $6, $7)
`, plan.PlanID, item.ItemID, item.Content, string(item.Status), item.Position, item.CreatedAt.UTC(), item.UpdatedAt.UTC())
		if err != nil {
			return err
		}
	}
	return tx.Commit()
}

func (s *PostgresStore) SaveWorkerHandoff(handoff WorkerHandoff) error {
	if err := s.ensureSchema(context.Background()); err != nil {
		return err
	}
	artifactsJSON, err := json.Marshal(handoff.Artifacts)
	if err != nil {
		return err
	}
	promotedFactsJSON, err := json.Marshal(handoff.PromotedFacts)
	if err != nil {
		return err
	}
	openQuestionsJSON, err := json.Marshal(handoff.OpenQuestions)
	if err != nil {
		return err
	}
	_, err = s.db.ExecContext(context.Background(), `
INSERT INTO runtime_worker_handoffs (worker_id, last_run_id, summary, artifacts, promoted_facts, open_questions, recommended_next_step, created_at, updated_at)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
ON CONFLICT(worker_id) DO UPDATE SET
  last_run_id=excluded.last_run_id,
  summary=excluded.summary,
  artifacts=excluded.artifacts,
  promoted_facts=excluded.promoted_facts,
  open_questions=excluded.open_questions,
  recommended_next_step=excluded.recommended_next_step,
  created_at=excluded.created_at,
  updated_at=excluded.updated_at
`, handoff.WorkerID, handoff.LastRunID, handoff.Summary, artifactsJSON, promotedFactsJSON, openQuestionsJSON, handoff.RecommendedNextStep, handoff.CreatedAt.UTC(), handoff.UpdatedAt.UTC())
	return err
}

func (s *PostgresStore) WorkerHandoff(workerID string) (WorkerHandoff, bool, error) {
	if err := s.ensureSchema(context.Background()); err != nil {
		return WorkerHandoff{}, false, err
	}
	var (
		out               WorkerHandoff
		artifactsJSON     []byte
		promotedFactsJSON []byte
		openQuestionsJSON []byte
	)
	err := s.db.QueryRowContext(context.Background(), `
SELECT worker_id, last_run_id, summary, artifacts, promoted_facts, open_questions, recommended_next_step, created_at, updated_at
FROM runtime_worker_handoffs WHERE worker_id = $1
`, workerID).Scan(&out.WorkerID, &out.LastRunID, &out.Summary, &artifactsJSON, &promotedFactsJSON, &openQuestionsJSON, &out.RecommendedNextStep, &out.CreatedAt, &out.UpdatedAt)
	if err == sql.ErrNoRows {
		return WorkerHandoff{}, false, nil
	}
	if err != nil {
		return WorkerHandoff{}, false, err
	}
	_ = json.Unmarshal(artifactsJSON, &out.Artifacts)
	_ = json.Unmarshal(promotedFactsJSON, &out.PromotedFacts)
	_ = json.Unmarshal(openQuestionsJSON, &out.OpenQuestions)
	return out, true, nil
}

func (s *PostgresStore) Plan(planID string) (PlanRecord, bool, error) {
	if err := s.ensureSchema(context.Background()); err != nil {
		return PlanRecord{}, false, err
	}
	var (
		out       PlanRecord
		notesJSON []byte
	)
	err := s.db.QueryRowContext(context.Background(), `
SELECT plan_id, owner_type, owner_id, title, notes, created_at, updated_at
FROM runtime_plans WHERE plan_id = $1
`, planID).Scan(&out.PlanID, &out.OwnerType, &out.OwnerID, &out.Title, &notesJSON, &out.CreatedAt, &out.UpdatedAt)
	if err == sql.ErrNoRows {
		return PlanRecord{}, false, nil
	}
	if err != nil {
		return PlanRecord{}, false, err
	}
	_ = json.Unmarshal(notesJSON, &out.Notes)
	items, err := s.planItems(planID)
	if err != nil {
		return PlanRecord{}, false, err
	}
	out.Items = items
	return out, true, nil
}

func (s *PostgresStore) ListPlans(query PlanQuery) ([]PlanRecord, error) {
	if err := s.ensureSchema(context.Background()); err != nil {
		return nil, err
	}
	var (
		where []string
		args  []any
	)
	next := func(v any) string {
		args = append(args, v)
		return fmt.Sprintf("$%d", len(args))
	}
	if v := strings.TrimSpace(query.OwnerType); v != "" {
		where = append(where, "owner_type = "+next(v))
	}
	if v := strings.TrimSpace(query.OwnerID); v != "" {
		where = append(where, "owner_id = "+next(v))
	}
	stmt := `SELECT plan_id, owner_type, owner_id, title, notes, created_at, updated_at FROM runtime_plans`
	if len(where) > 0 {
		stmt += " WHERE " + strings.Join(where, " AND ")
	}
	stmt += " ORDER BY updated_at DESC"
	if query.Limit > 0 {
		stmt += " LIMIT " + next(query.Limit)
	}
	rows, err := s.db.QueryContext(context.Background(), stmt, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := []PlanRecord{}
	for rows.Next() {
		var (
			item      PlanRecord
			notesJSON []byte
		)
		if err := rows.Scan(&item.PlanID, &item.OwnerType, &item.OwnerID, &item.Title, &notesJSON, &item.CreatedAt, &item.UpdatedAt); err != nil {
			return nil, err
		}
		_ = json.Unmarshal(notesJSON, &item.Notes)
		items, err := s.planItems(item.PlanID)
		if err != nil {
			return nil, err
		}
		item.Items = items
		out = append(out, item)
	}
	return out, rows.Err()
}

func (s *PostgresStore) planItems(planID string) ([]PlanItem, error) {
	rows, err := s.db.QueryContext(context.Background(), `
SELECT item_id, content, status, position, created_at, updated_at
FROM runtime_plan_items
WHERE plan_id = $1
ORDER BY position ASC
`, planID)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := []PlanItem{}
	for rows.Next() {
		var (
			item   PlanItem
			status string
		)
		if err := rows.Scan(&item.ItemID, &item.Content, &status, &item.Position, &item.CreatedAt, &item.UpdatedAt); err != nil {
			return nil, err
		}
		item.Status = PlanItemStatus(status)
		out = append(out, item)
	}
	return out, rows.Err()
}

func (s *PostgresStore) MarkCancelRequested(runID string) error {
	if err := s.ensureSchema(context.Background()); err != nil {
		return err
	}
	_, err := s.db.ExecContext(context.Background(), `UPDATE runtime_runs SET cancel_requested = TRUE WHERE run_id = $1`, runID)
	return err
}

func (s *PostgresStore) Run(runID string) (RunRecord, bool, error) {
	if err := s.ensureSchema(context.Background()); err != nil {
		return RunRecord{}, false, err
	}
	var out RunRecord
	var endedAt sql.NullTime
	var promptBudgetJSON []byte
	var snapshotJSON []byte
	err := s.db.QueryRowContext(context.Background(), `
SELECT run_id, chat_id, session_id, query, final_response, prompt_budget, status, started_at, ended_at, failure_reason, cancel_requested, policy_snapshot
FROM runtime_runs WHERE run_id = $1
`, runID).Scan(&out.RunID, &out.ChatID, &out.SessionID, &out.Query, &out.FinalResponse, &promptBudgetJSON, &out.Status, &out.StartedAt, &endedAt, &out.FailureReason, &out.CancelRequested, &snapshotJSON)
	if err == sql.ErrNoRows {
		return RunRecord{}, false, nil
	}
	if err != nil {
		return RunRecord{}, false, err
	}
	if endedAt.Valid {
		out.EndedAt = &endedAt.Time
	}
	_ = json.Unmarshal(promptBudgetJSON, &out.PromptBudget)
	_ = json.Unmarshal(snapshotJSON, &out.PolicySnapshot)
	return out, true, nil
}

func (s *PostgresStore) ListRuns(query RunQuery) ([]RunRecord, error) {
	if err := s.ensureSchema(context.Background()); err != nil {
		return nil, err
	}
	var (
		where []string
		args  []any
	)
	next := func(v any) string {
		args = append(args, v)
		return fmt.Sprintf("$%d", len(args))
	}
	if query.HasChatID {
		where = append(where, "chat_id = "+next(query.ChatID))
	}
	if sessionID := strings.TrimSpace(query.SessionID); sessionID != "" {
		where = append(where, "session_id = "+next(sessionID))
	}
	if query.HasStatus {
		where = append(where, "status = "+next(string(query.Status)))
	}
	stmt := `
SELECT run_id, chat_id, session_id, query, final_response, prompt_budget, status, started_at, ended_at, failure_reason, cancel_requested, policy_snapshot
FROM runtime_runs`
	if len(where) > 0 {
		stmt += " WHERE " + strings.Join(where, " AND ")
	}
	stmt += " ORDER BY started_at DESC"
	if query.Limit > 0 {
		stmt += " LIMIT " + next(query.Limit)
	}
	rows, err := s.db.QueryContext(context.Background(), stmt, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := []RunRecord{}
	for rows.Next() {
		var item RunRecord
		var endedAt sql.NullTime
		var promptBudgetJSON []byte
		var snapshotJSON []byte
		if err := rows.Scan(&item.RunID, &item.ChatID, &item.SessionID, &item.Query, &item.FinalResponse, &promptBudgetJSON, &item.Status, &item.StartedAt, &endedAt, &item.FailureReason, &item.CancelRequested, &snapshotJSON); err != nil {
			return nil, err
		}
		if endedAt.Valid {
			item.EndedAt = &endedAt.Time
		}
		_ = json.Unmarshal(promptBudgetJSON, &item.PromptBudget)
		_ = json.Unmarshal(snapshotJSON, &item.PolicySnapshot)
		out = append(out, item)
	}
	return out, rows.Err()
}

func (s *PostgresStore) ListSessions(query SessionQuery) ([]SessionRecord, error) {
	if err := s.ensureSchema(context.Background()); err != nil {
		return nil, err
	}
	var args []any
	next := func(v any) string {
		args = append(args, v)
		return fmt.Sprintf("$%d", len(args))
	}
	runSelect := "SELECT session_id, started_at AS activity_at, FALSE AS has_overrides FROM runtime_runs"
	overrideSelect := "SELECT session_id, updated_at AS activity_at, TRUE AS has_overrides FROM runtime_session_overrides"
	if query.HasChatID {
		runSelect += " WHERE chat_id = " + next(query.ChatID)
		overrideSelect += " WHERE session_id LIKE " + next(fmt.Sprintf("%d:%%", query.ChatID))
	}
	stmt := `
SELECT session_id, MAX(activity_at) AS last_activity, BOOL_OR(has_overrides) AS has_overrides
FROM (` + runSelect + ` UNION ALL ` + overrideSelect + `) AS runtime_sessions
GROUP BY session_id
ORDER BY last_activity DESC`
	if query.Limit > 0 {
		stmt += " LIMIT " + next(query.Limit)
	}
	rows, err := s.db.QueryContext(context.Background(), stmt, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := []SessionRecord{}
	for rows.Next() {
		var item SessionRecord
		if err := rows.Scan(&item.SessionID, &item.LastActivityAt, &item.HasOverrides); err != nil {
			return nil, err
		}
		out = append(out, item)
	}
	return out, rows.Err()
}

func (s *PostgresStore) SaveJob(job JobRecord) error {
	if err := s.ensureSchema(context.Background()); err != nil {
		return err
	}
	argsJSON, err := json.Marshal(job.Args)
	if err != nil {
		return err
	}
	snapshotJSON, err := json.Marshal(NormalizePolicySnapshot(job.PolicySnapshot))
	if err != nil {
		return err
	}
	_, err = s.db.ExecContext(context.Background(), `
INSERT INTO runtime_jobs (job_id, kind, owner_run_id, owner_worker_id, chat_id, session_id, command, args, cwd, status, started_at, ended_at, exit_code, failure_reason, cancel_requested, policy_snapshot)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
ON CONFLICT(job_id) DO UPDATE SET
  kind=excluded.kind,
  owner_run_id=excluded.owner_run_id,
  owner_worker_id=excluded.owner_worker_id,
  chat_id=excluded.chat_id,
  session_id=excluded.session_id,
  command=excluded.command,
  args=excluded.args,
  cwd=excluded.cwd,
  status=excluded.status,
  started_at=excluded.started_at,
  ended_at=excluded.ended_at,
  exit_code=excluded.exit_code,
  failure_reason=excluded.failure_reason,
  cancel_requested=excluded.cancel_requested,
  policy_snapshot=excluded.policy_snapshot
`, job.JobID, job.Kind, job.OwnerRunID, job.OwnerWorkerID, job.ChatID, job.SessionID, job.Command, argsJSON, job.Cwd, string(job.Status), job.StartedAt.UTC(), job.EndedAt, job.ExitCode, job.FailureReason, job.CancelRequested, snapshotJSON)
	return err
}

func (s *PostgresStore) Job(jobID string) (JobRecord, bool, error) {
	if err := s.ensureSchema(context.Background()); err != nil {
		return JobRecord{}, false, err
	}
	var (
		out          JobRecord
		argsJSON     []byte
		snapshotJSON []byte
		endedAt      sql.NullTime
		exitCode     sql.NullInt32
	)
	err := s.db.QueryRowContext(context.Background(), `
SELECT job_id, kind, owner_run_id, owner_worker_id, chat_id, session_id, command, args, cwd, status, started_at, ended_at, exit_code, failure_reason, cancel_requested, policy_snapshot
FROM runtime_jobs WHERE job_id = $1
`, jobID).Scan(&out.JobID, &out.Kind, &out.OwnerRunID, &out.OwnerWorkerID, &out.ChatID, &out.SessionID, &out.Command, &argsJSON, &out.Cwd, &out.Status, &out.StartedAt, &endedAt, &exitCode, &out.FailureReason, &out.CancelRequested, &snapshotJSON)
	if err == sql.ErrNoRows {
		return JobRecord{}, false, nil
	}
	if err != nil {
		return JobRecord{}, false, err
	}
	_ = json.Unmarshal(argsJSON, &out.Args)
	if endedAt.Valid {
		out.EndedAt = &endedAt.Time
	}
	if exitCode.Valid {
		code := int(exitCode.Int32)
		out.ExitCode = &code
	}
	_ = json.Unmarshal(snapshotJSON, &out.PolicySnapshot)
	return out, true, nil
}

func (s *PostgresStore) ListJobs(limit int) ([]JobRecord, error) {
	if err := s.ensureSchema(context.Background()); err != nil {
		return nil, err
	}
	stmt := `
SELECT job_id, kind, owner_run_id, owner_worker_id, chat_id, session_id, command, args, cwd, status, started_at, ended_at, exit_code, failure_reason, cancel_requested, policy_snapshot
FROM runtime_jobs
ORDER BY started_at DESC`
	args := []any{}
	if limit > 0 {
		stmt += fmt.Sprintf(" LIMIT $%d", 1)
		args = append(args, limit)
	}
	rows, err := s.db.QueryContext(context.Background(), stmt, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := []JobRecord{}
	for rows.Next() {
		var (
			item         JobRecord
			argsJSON     []byte
			snapshotJSON []byte
			endedAt      sql.NullTime
			exitCode     sql.NullInt32
		)
		if err := rows.Scan(&item.JobID, &item.Kind, &item.OwnerRunID, &item.OwnerWorkerID, &item.ChatID, &item.SessionID, &item.Command, &argsJSON, &item.Cwd, &item.Status, &item.StartedAt, &endedAt, &exitCode, &item.FailureReason, &item.CancelRequested, &snapshotJSON); err != nil {
			return nil, err
		}
		_ = json.Unmarshal(argsJSON, &item.Args)
		if endedAt.Valid {
			item.EndedAt = &endedAt.Time
		}
		if exitCode.Valid {
			code := int(exitCode.Int32)
			item.ExitCode = &code
		}
		_ = json.Unmarshal(snapshotJSON, &item.PolicySnapshot)
		out = append(out, item)
	}
	return out, rows.Err()
}

func (s *PostgresStore) MarkJobCancelRequested(jobID string) error {
	if err := s.ensureSchema(context.Background()); err != nil {
		return err
	}
	_, err := s.db.ExecContext(context.Background(), `UPDATE runtime_jobs SET cancel_requested = TRUE WHERE job_id = $1`, jobID)
	return err
}

func (s *PostgresStore) SaveJobLog(chunk JobLogChunk) error {
	if err := s.ensureSchema(context.Background()); err != nil {
		return err
	}
	var err error
	_, err = s.db.ExecContext(context.Background(), `
	INSERT INTO runtime_job_logs (job_id, stream, content, created_at)
	VALUES ($1, $2, $3, $4)
	`, chunk.JobID, chunk.Stream, chunk.Content, chunk.CreatedAt.UTC())
	return err
}

func (s *PostgresStore) JobLogs(query JobLogQuery) ([]JobLogChunk, error) {
	if err := s.ensureSchema(context.Background()); err != nil {
		return nil, err
	}
	var (
		where []string
		args  []any
	)
	next := func(v any) string {
		args = append(args, v)
		return fmt.Sprintf("$%d", len(args))
	}
	where = append(where, "job_id = "+next(query.JobID))
	if stream := strings.TrimSpace(query.Stream); stream != "" {
		where = append(where, "stream = "+next(stream))
	}
	if query.AfterID > 0 {
		where = append(where, "id > "+next(query.AfterID))
	}
	stmt := `
SELECT id, job_id, stream, content, created_at
FROM runtime_job_logs
WHERE ` + strings.Join(where, " AND ") + `
ORDER BY id ASC`
	if query.Limit > 0 {
		stmt += " LIMIT " + next(query.Limit)
	}
	rows, err := s.db.QueryContext(context.Background(), stmt, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := []JobLogChunk{}
	for rows.Next() {
		var item JobLogChunk
		if err := rows.Scan(&item.ID, &item.JobID, &item.Stream, &item.Content, &item.CreatedAt); err != nil {
			return nil, err
		}
		out = append(out, item)
	}
	return out, rows.Err()
}

func (s *PostgresStore) RecoverInterruptedJobs(reason string) (int, error) {
	if err := s.ensureSchema(context.Background()); err != nil {
		return 0, err
	}
	result, err := s.db.ExecContext(context.Background(), `
UPDATE runtime_jobs
SET status = $1, failure_reason = $2, ended_at = NOW()
WHERE status IN ($3, $4)
`, string(JobFailed), reason, string(JobQueued), string(JobRunning))
	if err != nil {
		return 0, err
	}
	n, err := result.RowsAffected()
	return int(n), err
}

func (s *PostgresStore) SaveWorker(worker WorkerRecord) error {
	if err := s.ensureSchema(context.Background()); err != nil {
		return err
	}
	snapshotJSON, err := json.Marshal(NormalizePolicySnapshot(worker.PolicySnapshot))
	if err != nil {
		return err
	}
	var processStartedAt, processHeartbeatAt, processExitedAt *time.Time
	if worker.Process.StartedAt != nil {
		t := worker.Process.StartedAt.UTC()
		processStartedAt = &t
	}
	if worker.Process.LastHeartbeatAt != nil {
		t := worker.Process.LastHeartbeatAt.UTC()
		processHeartbeatAt = &t
	}
	if worker.Process.ExitedAt != nil {
		t := worker.Process.ExitedAt.UTC()
		processExitedAt = &t
	}
	_, err = s.db.ExecContext(context.Background(), `
INSERT INTO runtime_workers (worker_id, parent_chat_id, parent_session_id, worker_chat_id, worker_session_id, status, last_run_id, last_error, process_pid, process_state, process_started_at, process_last_heartbeat_at, process_exited_at, process_exit_reason, created_at, updated_at, last_message_at, closed_at, policy_snapshot)
VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19)
ON CONFLICT(worker_id) DO UPDATE SET
  parent_chat_id=excluded.parent_chat_id,
  parent_session_id=excluded.parent_session_id,
  worker_chat_id=excluded.worker_chat_id,
  worker_session_id=excluded.worker_session_id,
  status=excluded.status,
  last_run_id=excluded.last_run_id,
  last_error=excluded.last_error,
  process_pid=excluded.process_pid,
  process_state=excluded.process_state,
  process_started_at=excluded.process_started_at,
  process_last_heartbeat_at=excluded.process_last_heartbeat_at,
  process_exited_at=excluded.process_exited_at,
  process_exit_reason=excluded.process_exit_reason,
  created_at=excluded.created_at,
  updated_at=excluded.updated_at,
  last_message_at=excluded.last_message_at,
  closed_at=excluded.closed_at,
  policy_snapshot=excluded.policy_snapshot
`, worker.WorkerID, worker.ParentChatID, worker.ParentSessionID, worker.WorkerChatID, worker.WorkerSessionID, string(worker.Status), worker.LastRunID, worker.LastError, worker.Process.PID, string(worker.Process.State), processStartedAt, processHeartbeatAt, processExitedAt, worker.Process.ExitReason, worker.CreatedAt.UTC(), worker.UpdatedAt.UTC(), worker.LastMessageAt, worker.ClosedAt, snapshotJSON)
	return err
}

func (s *PostgresStore) Worker(workerID string) (WorkerRecord, bool, error) {
	if err := s.ensureSchema(context.Background()); err != nil {
		return WorkerRecord{}, false, err
	}
	var (
		item          WorkerRecord
		status        string
		processState  string
		lastMessageAt sql.NullTime
		closedAt      sql.NullTime
		processStartedAt sql.NullTime
		processHeartbeatAt sql.NullTime
		processExitedAt sql.NullTime
		processExitReason string
		snapshotJSON  []byte
	)
	err := s.db.QueryRowContext(context.Background(), `
SELECT worker_id, parent_chat_id, parent_session_id, worker_chat_id, worker_session_id, status, last_run_id, last_error, process_pid, process_state, process_started_at, process_last_heartbeat_at, process_exited_at, process_exit_reason, created_at, updated_at, last_message_at, closed_at, policy_snapshot
FROM runtime_workers WHERE worker_id = $1
`, workerID).Scan(&item.WorkerID, &item.ParentChatID, &item.ParentSessionID, &item.WorkerChatID, &item.WorkerSessionID, &status, &item.LastRunID, &item.LastError, &item.Process.PID, &processState, &processStartedAt, &processHeartbeatAt, &processExitedAt, &processExitReason, &item.CreatedAt, &item.UpdatedAt, &lastMessageAt, &closedAt, &snapshotJSON)
	if err == sql.ErrNoRows {
		return WorkerRecord{}, false, nil
	}
	if err != nil {
		return WorkerRecord{}, false, err
	}
	item.Status = WorkerStatus(status)
	item.Process.State = WorkerProcessState(processState)
	item.Process.ExitReason = processExitReason
	if processStartedAt.Valid {
		item.Process.StartedAt = &processStartedAt.Time
	}
	if processHeartbeatAt.Valid {
		item.Process.LastHeartbeatAt = &processHeartbeatAt.Time
	}
	if processExitedAt.Valid {
		item.Process.ExitedAt = &processExitedAt.Time
	}
	if lastMessageAt.Valid {
		item.LastMessageAt = &lastMessageAt.Time
	}
	if closedAt.Valid {
		item.ClosedAt = &closedAt.Time
	}
	_ = json.Unmarshal(snapshotJSON, &item.PolicySnapshot)
	return item, true, nil
}

func (s *PostgresStore) ListWorkers(query WorkerQuery) ([]WorkerRecord, error) {
	if err := s.ensureSchema(context.Background()); err != nil {
		return nil, err
	}
	var (
		where []string
		args  []any
	)
	if query.HasParentChatID {
		args = append(args, query.ParentChatID)
		where = append(where, fmt.Sprintf("parent_chat_id = $%d", len(args)))
	}
	stmt := `
SELECT worker_id, parent_chat_id, parent_session_id, worker_chat_id, worker_session_id, status, last_run_id, last_error, process_pid, process_state, process_started_at, process_last_heartbeat_at, process_exited_at, process_exit_reason, created_at, updated_at, last_message_at, closed_at, policy_snapshot
FROM runtime_workers`
	if len(where) > 0 {
		stmt += " WHERE " + strings.Join(where, " AND ")
	}
	stmt += " ORDER BY created_at DESC"
	if query.Limit > 0 {
		args = append(args, query.Limit)
		stmt += fmt.Sprintf(" LIMIT $%d", len(args))
	}
	rows, err := s.db.QueryContext(context.Background(), stmt, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := []WorkerRecord{}
	for rows.Next() {
		var (
			item          WorkerRecord
			status        string
			processState  string
			lastMessageAt sql.NullTime
			closedAt      sql.NullTime
			processStartedAt sql.NullTime
			processHeartbeatAt sql.NullTime
			processExitedAt sql.NullTime
			processExitReason string
			snapshotJSON  []byte
		)
		if err := rows.Scan(&item.WorkerID, &item.ParentChatID, &item.ParentSessionID, &item.WorkerChatID, &item.WorkerSessionID, &status, &item.LastRunID, &item.LastError, &item.Process.PID, &processState, &processStartedAt, &processHeartbeatAt, &processExitedAt, &processExitReason, &item.CreatedAt, &item.UpdatedAt, &lastMessageAt, &closedAt, &snapshotJSON); err != nil {
			return nil, err
		}
		item.Status = WorkerStatus(status)
		item.Process.State = WorkerProcessState(processState)
		item.Process.ExitReason = processExitReason
		if processStartedAt.Valid {
			item.Process.StartedAt = &processStartedAt.Time
		}
		if processHeartbeatAt.Valid {
			item.Process.LastHeartbeatAt = &processHeartbeatAt.Time
		}
		if processExitedAt.Valid {
			item.Process.ExitedAt = &processExitedAt.Time
		}
		if lastMessageAt.Valid {
			item.LastMessageAt = &lastMessageAt.Time
		}
		if closedAt.Valid {
			item.ClosedAt = &closedAt.Time
		}
		_ = json.Unmarshal(snapshotJSON, &item.PolicySnapshot)
		out = append(out, item)
	}
	return out, rows.Err()
}

func (s *PostgresStore) RecoverInterruptedWorkers(reason string) (int, error) {
	if err := s.ensureSchema(context.Background()); err != nil {
		return 0, err
	}
	result, err := s.db.ExecContext(context.Background(), `
UPDATE runtime_workers
SET process_state = $1, process_exit_reason = $2, process_exited_at = NOW(), updated_at = NOW()
WHERE process_state IN ($3, $4)
`, string(WorkerProcessFailed), reason, string(WorkerProcessStarting), string(WorkerProcessRunning))
	if err != nil {
		return 0, err
	}
	count, err := result.RowsAffected()
	if err != nil {
		return 0, err
	}
	return int(count), nil
}

func (s *PostgresStore) SaveEvent(event RuntimeEvent) error {
	if err := s.ensureSchema(context.Background()); err != nil {
		return err
	}
	payload := []byte("{}")
	if len(event.Payload) > 0 {
		payload = event.Payload
	}
	_, err := s.db.ExecContext(context.Background(), `
INSERT INTO runtime_events (entity_type, entity_id, chat_id, session_id, run_id, kind, payload, created_at)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
`, event.EntityType, event.EntityID, event.ChatID, event.SessionID, event.RunID, event.Kind, payload, event.CreatedAt.UTC())
	return err
}

func (s *PostgresStore) ListEvents(query EventQuery) ([]RuntimeEvent, error) {
	if err := s.ensureSchema(context.Background()); err != nil {
		return nil, err
	}
	var (
		where []string
		args  []any
	)
	next := func(v any) string {
		args = append(args, v)
		return fmt.Sprintf("$%d", len(args))
	}
	if entityType := strings.TrimSpace(query.EntityType); entityType != "" {
		where = append(where, "entity_type = "+next(entityType))
	}
	if entityID := strings.TrimSpace(query.EntityID); entityID != "" {
		where = append(where, "entity_id = "+next(entityID))
	}
	if runID := strings.TrimSpace(query.RunID); runID != "" {
		where = append(where, "run_id = "+next(runID))
	}
	if sessionID := strings.TrimSpace(query.SessionID); sessionID != "" {
		where = append(where, "session_id = "+next(sessionID))
	}
	if query.AfterID > 0 {
		where = append(where, "id > "+next(query.AfterID))
	}
	stmt := `
SELECT id, entity_type, entity_id, chat_id, session_id, run_id, kind, payload, created_at
FROM runtime_events`
	if len(where) > 0 {
		stmt += " WHERE " + strings.Join(where, " AND ")
	}
	stmt += " ORDER BY id ASC"
	if query.Limit > 0 {
		stmt += " LIMIT " + next(query.Limit)
	}
	rows, err := s.db.QueryContext(context.Background(), stmt, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := []RuntimeEvent{}
	for rows.Next() {
		var item RuntimeEvent
		if err := rows.Scan(&item.ID, &item.EntityType, &item.EntityID, &item.ChatID, &item.SessionID, &item.RunID, &item.Kind, &item.Payload, &item.CreatedAt); err != nil {
			return nil, err
		}
		out = append(out, item)
	}
	return out, rows.Err()
}

func (s *PostgresStore) RecoverInterruptedRuns(reason string) (int, error) {
	if err := s.ensureSchema(context.Background()); err != nil {
		return 0, err
	}
	result, err := s.db.ExecContext(context.Background(), `
UPDATE runtime_runs
SET status = $1, failure_reason = $2, ended_at = NOW()
WHERE status IN ($3, $4)
`, string(StatusFailed), reason, string(StatusQueued), string(StatusRunning))
	if err != nil {
		return 0, err
	}
	n, err := result.RowsAffected()
	return int(n), err
}

func (s *PostgresStore) SaveCheckpoint(checkpoint Checkpoint) error {
	if err := s.ensureSchema(context.Background()); err != nil {
		return err
	}
	archiveRefs, err := json.Marshal(checkpoint.ArchiveRefs)
	if err != nil {
		return err
	}
	artifactRefs, err := json.Marshal(checkpoint.ArtifactRefs)
	if err != nil {
		return err
	}
	_, err = s.db.ExecContext(context.Background(), `
INSERT INTO runtime_checkpoints (chat_id, session_id, originating_intent, what_happened, what_matters_now, archive_refs, artifact_refs, updated_at)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
ON CONFLICT(chat_id, session_id) DO UPDATE SET
  originating_intent=excluded.originating_intent,
  what_happened=excluded.what_happened,
  what_matters_now=excluded.what_matters_now,
  archive_refs=excluded.archive_refs,
  artifact_refs=excluded.artifact_refs,
  updated_at=excluded.updated_at
`, checkpoint.ChatID, checkpoint.SessionID, checkpoint.OriginatingIntent, checkpoint.WhatHappened, checkpoint.WhatMattersNow, archiveRefs, artifactRefs, checkpoint.UpdatedAt.UTC())
	return err
}

func (s *PostgresStore) Checkpoint(chatID int64, sessionID string) (Checkpoint, bool, error) {
	if err := s.ensureSchema(context.Background()); err != nil {
		return Checkpoint{}, false, err
	}
	var (
		out          Checkpoint
		archiveRefs  []byte
		artifactRefs []byte
	)
	err := s.db.QueryRowContext(context.Background(), `
SELECT chat_id, session_id, originating_intent, what_happened, what_matters_now, archive_refs, artifact_refs, updated_at
FROM runtime_checkpoints WHERE chat_id = $1 AND session_id = $2
`, chatID, sessionID).Scan(&out.ChatID, &out.SessionID, &out.OriginatingIntent, &out.WhatHappened, &out.WhatMattersNow, &archiveRefs, &artifactRefs, &out.UpdatedAt)
	if err == sql.ErrNoRows {
		return Checkpoint{}, false, nil
	}
	if err != nil {
		return Checkpoint{}, false, err
	}
	_ = json.Unmarshal(archiveRefs, &out.ArchiveRefs)
	_ = json.Unmarshal(artifactRefs, &out.ArtifactRefs)
	return out, true, nil
}

func (s *PostgresStore) SaveContinuity(continuity Continuity) error {
	if err := s.ensureSchema(context.Background()); err != nil {
		return err
	}
	resolved, err := json.Marshal(continuity.ResolvedFacts)
	if err != nil {
		return err
	}
	unresolved, err := json.Marshal(continuity.UnresolvedItems)
	if err != nil {
		return err
	}
	archiveRefs, err := json.Marshal(continuity.ArchiveRefs)
	if err != nil {
		return err
	}
	artifactRefs, err := json.Marshal(continuity.ArtifactRefs)
	if err != nil {
		return err
	}
	_, err = s.db.ExecContext(context.Background(), `
INSERT INTO runtime_continuity (chat_id, session_id, user_goal, current_state, resolved_facts, unresolved_items, archive_refs, artifact_refs, updated_at)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
ON CONFLICT(chat_id, session_id) DO UPDATE SET
  user_goal=excluded.user_goal,
  current_state=excluded.current_state,
  resolved_facts=excluded.resolved_facts,
  unresolved_items=excluded.unresolved_items,
  archive_refs=excluded.archive_refs,
  artifact_refs=excluded.artifact_refs,
  updated_at=excluded.updated_at
`, continuity.ChatID, continuity.SessionID, continuity.UserGoal, continuity.CurrentState, resolved, unresolved, archiveRefs, artifactRefs, continuity.UpdatedAt.UTC())
	return err
}

func (s *PostgresStore) Continuity(chatID int64, sessionID string) (Continuity, bool, error) {
	if err := s.ensureSchema(context.Background()); err != nil {
		return Continuity{}, false, err
	}
	var (
		out          Continuity
		resolved     []byte
		unresolved   []byte
		archiveRefs  []byte
		artifactRefs []byte
	)
	err := s.db.QueryRowContext(context.Background(), `
SELECT chat_id, session_id, user_goal, current_state, resolved_facts, unresolved_items, archive_refs, artifact_refs, updated_at
FROM runtime_continuity WHERE chat_id = $1 AND session_id = $2
`, chatID, sessionID).Scan(&out.ChatID, &out.SessionID, &out.UserGoal, &out.CurrentState, &resolved, &unresolved, &archiveRefs, &artifactRefs, &out.UpdatedAt)
	if err == sql.ErrNoRows {
		return Continuity{}, false, nil
	}
	if err != nil {
		return Continuity{}, false, err
	}
	_ = json.Unmarshal(resolved, &out.ResolvedFacts)
	_ = json.Unmarshal(unresolved, &out.UnresolvedItems)
	_ = json.Unmarshal(archiveRefs, &out.ArchiveRefs)
	_ = json.Unmarshal(artifactRefs, &out.ArtifactRefs)
	return out, true, nil
}

func (s *PostgresStore) SaveSessionHead(head SessionHead) error {
	if err := s.ensureSchema(context.Background()); err != nil {
		return err
	}
	planItems, err := json.Marshal(head.CurrentPlanItems)
	if err != nil {
		return err
	}
	resolved, err := json.Marshal(head.ResolvedEntities)
	if err != nil {
		return err
	}
	artifacts, err := json.Marshal(head.RecentArtifactRefs)
	if err != nil {
		return err
	}
	openLoops, err := json.Marshal(head.OpenLoops)
	if err != nil {
		return err
	}
	_, err = s.db.ExecContext(context.Background(), `
INSERT INTO runtime_session_head (chat_id, session_id, last_completed_run_id, current_goal, last_result_summary, current_plan_id, current_plan_title, current_plan_items, resolved_entities, recent_artifact_refs, open_loops, current_project, updated_at)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
ON CONFLICT(chat_id, session_id) DO UPDATE SET
  last_completed_run_id=excluded.last_completed_run_id,
  current_goal=excluded.current_goal,
  last_result_summary=excluded.last_result_summary,
  current_plan_id=excluded.current_plan_id,
  current_plan_title=excluded.current_plan_title,
  current_plan_items=excluded.current_plan_items,
  resolved_entities=excluded.resolved_entities,
  recent_artifact_refs=excluded.recent_artifact_refs,
  open_loops=excluded.open_loops,
  current_project=excluded.current_project,
  updated_at=excluded.updated_at
`, head.ChatID, head.SessionID, head.LastCompletedRunID, head.CurrentGoal, head.LastResultSummary, head.CurrentPlanID, head.CurrentPlanTitle, planItems, resolved, artifacts, openLoops, head.CurrentProject, head.UpdatedAt.UTC())
	return err
}

func (s *PostgresStore) SessionHead(chatID int64, sessionID string) (SessionHead, bool, error) {
	if err := s.ensureSchema(context.Background()); err != nil {
		return SessionHead{}, false, err
	}
	var (
		out       SessionHead
		planItems []byte
		resolved  []byte
		artifacts []byte
		openLoops []byte
	)
	err := s.db.QueryRowContext(context.Background(), `
SELECT chat_id, session_id, last_completed_run_id, current_goal, last_result_summary, current_plan_id, current_plan_title, current_plan_items, resolved_entities, recent_artifact_refs, open_loops, current_project, updated_at
FROM runtime_session_head WHERE chat_id = $1 AND session_id = $2
`, chatID, sessionID).Scan(&out.ChatID, &out.SessionID, &out.LastCompletedRunID, &out.CurrentGoal, &out.LastResultSummary, &out.CurrentPlanID, &out.CurrentPlanTitle, &planItems, &resolved, &artifacts, &openLoops, &out.CurrentProject, &out.UpdatedAt)
	if err == sql.ErrNoRows {
		return SessionHead{}, false, nil
	}
	if err != nil {
		return SessionHead{}, false, err
	}
	_ = json.Unmarshal(planItems, &out.CurrentPlanItems)
	_ = json.Unmarshal(resolved, &out.ResolvedEntities)
	_ = json.Unmarshal(artifacts, &out.RecentArtifactRefs)
	_ = json.Unmarshal(openLoops, &out.OpenLoops)
	return out, true, nil
}

func (s *PostgresStore) TryMarkUpdate(chatID int64, updateID int64) (bool, error) {
	if err := s.ensureSchema(context.Background()); err != nil {
		return false, err
	}
	result, err := s.db.ExecContext(context.Background(), `
INSERT INTO runtime_processed_updates (chat_id, update_id, processed_at)
VALUES ($1, $2, NOW())
ON CONFLICT DO NOTHING
`, chatID, updateID)
	if err != nil {
		return false, err
	}
	n, err := result.RowsAffected()
	return n > 0, err
}

func (s *PostgresStore) SaveSessionOverrides(overrides SessionOverrides) error {
	if err := s.ensureSchema(context.Background()); err != nil {
		return err
	}
	runtimeJSON, err := json.Marshal(overrides.Runtime)
	if err != nil {
		return err
	}
	memoryJSON, err := json.Marshal(overrides.MemoryPolicy)
	if err != nil {
		return err
	}
	actionJSON, err := json.Marshal(overrides.ActionPolicy)
	if err != nil {
		return err
	}
	_, err = s.db.ExecContext(context.Background(), `
INSERT INTO runtime_session_overrides (session_id, runtime_config, memory_policy, action_policy, updated_at)
VALUES ($1, $2, $3, $4, $5)
ON CONFLICT(session_id) DO UPDATE SET
  runtime_config=excluded.runtime_config,
  memory_policy=excluded.memory_policy,
  action_policy=excluded.action_policy,
  updated_at=excluded.updated_at
`, overrides.SessionID, runtimeJSON, memoryJSON, actionJSON, overrides.UpdatedAt.UTC())
	return err
}

func (s *PostgresStore) SessionOverrides(sessionID string) (SessionOverrides, bool, error) {
	if err := s.ensureSchema(context.Background()); err != nil {
		return SessionOverrides{}, false, err
	}
	var (
		out         SessionOverrides
		runtimeJSON []byte
		memoryJSON  []byte
		actionJSON  []byte
	)
	err := s.db.QueryRowContext(context.Background(), `
SELECT session_id, runtime_config, memory_policy, action_policy, updated_at
FROM runtime_session_overrides
WHERE session_id = $1
`, sessionID).Scan(&out.SessionID, &runtimeJSON, &memoryJSON, &actionJSON, &out.UpdatedAt)
	if err == sql.ErrNoRows {
		return SessionOverrides{}, false, nil
	}
	if err != nil {
		return SessionOverrides{}, false, err
	}
	_ = json.Unmarshal(runtimeJSON, &out.Runtime)
	_ = json.Unmarshal(memoryJSON, &out.MemoryPolicy)
	_ = json.Unmarshal(actionJSON, &out.ActionPolicy)
	return out, true, nil
}

func (s *PostgresStore) ClearSessionOverrides(sessionID string) error {
	if err := s.ensureSchema(context.Background()); err != nil {
		return err
	}
	_, err := s.db.ExecContext(context.Background(), `DELETE FROM runtime_session_overrides WHERE session_id = $1`, sessionID)
	return err
}

func (s *PostgresStore) SaveApproval(record approvals.Record) error {
	if err := s.ensureSchema(context.Background()); err != nil {
		return err
	}
	var requestedAt any
	if !record.RequestedAt.IsZero() {
		requestedAt = record.RequestedAt.UTC()
	}
	var decidedAt any
	if record.DecidedAt != nil {
		decidedAt = record.DecidedAt.UTC()
	}
	_, err := s.db.ExecContext(context.Background(), `
INSERT INTO runtime_approvals (approval_id, worker_id, session_id, payload, status, updated_at, reason, target_type, target_id, requested_at, decided_at, decision_update_id)
VALUES ($1, $2, $3, $4, $5, NOW(), $6, $7, $8, $9, $10, $11)
ON CONFLICT(approval_id) DO UPDATE SET
  worker_id=excluded.worker_id,
  session_id=excluded.session_id,
  payload=excluded.payload,
  status=excluded.status,
  updated_at=excluded.updated_at,
  reason=excluded.reason,
  target_type=excluded.target_type,
  target_id=excluded.target_id,
  requested_at=excluded.requested_at,
  decided_at=excluded.decided_at,
  decision_update_id=excluded.decision_update_id
`, record.ID, record.WorkerID, record.SessionID, record.Payload, string(record.Status), record.Reason, record.TargetType, record.TargetID, requestedAt, decidedAt, record.DecisionUpdateID)
	return err
}

func (s *PostgresStore) Approval(id string) (approvals.Record, bool, error) {
	if err := s.ensureSchema(context.Background()); err != nil {
		return approvals.Record{}, false, err
	}
	var record approvals.Record
	var status string
	var requestedAt sql.NullTime
	var decidedAt sql.NullTime
	err := s.db.QueryRowContext(context.Background(), `
SELECT approval_id, worker_id, session_id, payload, status, reason, target_type, target_id, requested_at, decided_at, decision_update_id
FROM runtime_approvals WHERE approval_id = $1
`, id).Scan(&record.ID, &record.WorkerID, &record.SessionID, &record.Payload, &status, &record.Reason, &record.TargetType, &record.TargetID, &requestedAt, &decidedAt, &record.DecisionUpdateID)
	if err == sql.ErrNoRows {
		return approvals.Record{}, false, nil
	}
	if err != nil {
		return approvals.Record{}, false, err
	}
	record.Status = approvals.Status(status)
	if requestedAt.Valid {
		record.RequestedAt = requestedAt.Time
	}
	if decidedAt.Valid {
		record.DecidedAt = &decidedAt.Time
	}
	return record, true, nil
}

func (s *PostgresStore) PendingApprovals(sessionID string) ([]approvals.Record, error) {
	if err := s.ensureSchema(context.Background()); err != nil {
		return nil, err
	}
	rows, err := s.db.QueryContext(context.Background(), `
SELECT approval_id, worker_id, session_id, payload, status, reason, target_type, target_id, requested_at, decided_at, decision_update_id
FROM runtime_approvals
WHERE session_id = $1 AND status = $2
ORDER BY approval_id
`, sessionID, string(approvals.StatusPending))
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := make([]approvals.Record, 0)
	for rows.Next() {
		var record approvals.Record
		var status string
		var requestedAt sql.NullTime
		var decidedAt sql.NullTime
		if err := rows.Scan(&record.ID, &record.WorkerID, &record.SessionID, &record.Payload, &status, &record.Reason, &record.TargetType, &record.TargetID, &requestedAt, &decidedAt, &record.DecisionUpdateID); err != nil {
			return nil, err
		}
		record.Status = approvals.Status(status)
		if requestedAt.Valid {
			record.RequestedAt = requestedAt.Time
		}
		if decidedAt.Valid {
			record.DecidedAt = &decidedAt.Time
		}
		out = append(out, record)
	}
	return out, rows.Err()
}

func (s *PostgresStore) SaveHandledApprovalCallback(updateID string, record approvals.Record) error {
	if err := s.ensureSchema(context.Background()); err != nil {
		return err
	}
	var requestedAt any
	if !record.RequestedAt.IsZero() {
		requestedAt = record.RequestedAt.UTC()
	}
	var decidedAt any
	if record.DecidedAt != nil {
		decidedAt = record.DecidedAt.UTC()
	}
	_, err := s.db.ExecContext(context.Background(), `
INSERT INTO runtime_approval_callbacks (update_id, approval_id, worker_id, session_id, payload, status, handled_at, reason, target_type, target_id, requested_at, decided_at, decision_update_id)
VALUES ($1, $2, $3, $4, $5, $6, NOW(), $7, $8, $9, $10, $11, $12)
ON CONFLICT(update_id) DO UPDATE SET
  approval_id=excluded.approval_id,
  worker_id=excluded.worker_id,
  session_id=excluded.session_id,
  payload=excluded.payload,
  status=excluded.status,
  handled_at=excluded.handled_at,
  reason=excluded.reason,
  target_type=excluded.target_type,
  target_id=excluded.target_id,
  requested_at=excluded.requested_at,
  decided_at=excluded.decided_at,
  decision_update_id=excluded.decision_update_id
`, updateID, record.ID, record.WorkerID, record.SessionID, record.Payload, string(record.Status), record.Reason, record.TargetType, record.TargetID, requestedAt, decidedAt, record.DecisionUpdateID)
	return err
}

func (s *PostgresStore) HandledApprovalCallback(updateID string) (approvals.Record, bool, error) {
	if err := s.ensureSchema(context.Background()); err != nil {
		return approvals.Record{}, false, err
	}
	var record approvals.Record
	var status string
	var requestedAt sql.NullTime
	var decidedAt sql.NullTime
	err := s.db.QueryRowContext(context.Background(), `
SELECT approval_id, worker_id, session_id, payload, status, reason, target_type, target_id, requested_at, decided_at, decision_update_id
FROM runtime_approval_callbacks WHERE update_id = $1
`, updateID).Scan(&record.ID, &record.WorkerID, &record.SessionID, &record.Payload, &status, &record.Reason, &record.TargetType, &record.TargetID, &requestedAt, &decidedAt, &record.DecisionUpdateID)
	if err == sql.ErrNoRows {
		return approvals.Record{}, false, nil
	}
	if err != nil {
		return approvals.Record{}, false, err
	}
	record.Status = approvals.Status(status)
	if requestedAt.Valid {
		record.RequestedAt = requestedAt.Time
	}
	if decidedAt.Valid {
		record.DecidedAt = &decidedAt.Time
	}
	return record, true, nil
}

func (s *PostgresStore) SaveApprovalContinuation(cont ApprovalContinuation) error {
	if err := s.ensureSchema(context.Background()); err != nil {
		return err
	}
	argsJSON, err := json.Marshal(cont.ToolArguments)
	if err != nil {
		return err
	}
	_, err = s.db.ExecContext(context.Background(), `
INSERT INTO runtime_approval_continuations (approval_id, run_id, chat_id, session_id, query, tool_call_id, tool_name, tool_arguments, requested_at)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
ON CONFLICT(approval_id) DO UPDATE SET
  run_id=excluded.run_id,
  chat_id=excluded.chat_id,
  session_id=excluded.session_id,
  query=excluded.query,
  tool_call_id=excluded.tool_call_id,
  tool_name=excluded.tool_name,
  tool_arguments=excluded.tool_arguments,
  requested_at=excluded.requested_at
`, cont.ApprovalID, cont.RunID, cont.ChatID, cont.SessionID, cont.Query, cont.ToolCallID, cont.ToolName, argsJSON, cont.RequestedAt.UTC())
	return err
}

func (s *PostgresStore) ApprovalContinuation(id string) (ApprovalContinuation, bool, error) {
	if err := s.ensureSchema(context.Background()); err != nil {
		return ApprovalContinuation{}, false, err
	}
	var (
		out      ApprovalContinuation
		argsJSON []byte
	)
	err := s.db.QueryRowContext(context.Background(), `
SELECT approval_id, run_id, chat_id, session_id, query, tool_call_id, tool_name, tool_arguments, requested_at
FROM runtime_approval_continuations WHERE approval_id = $1
`, id).Scan(&out.ApprovalID, &out.RunID, &out.ChatID, &out.SessionID, &out.Query, &out.ToolCallID, &out.ToolName, &argsJSON, &out.RequestedAt)
	if err == sql.ErrNoRows {
		return ApprovalContinuation{}, false, nil
	}
	if err != nil {
		return ApprovalContinuation{}, false, err
	}
	_ = json.Unmarshal(argsJSON, &out.ToolArguments)
	return out, true, nil
}

func (s *PostgresStore) DeleteApprovalContinuation(id string) error {
	if err := s.ensureSchema(context.Background()); err != nil {
		return err
	}
	_, err := s.db.ExecContext(context.Background(), `DELETE FROM runtime_approval_continuations WHERE approval_id = $1`, id)
	return err
}

func (s *PostgresStore) SaveTimeoutDecision(record TimeoutDecisionRecord) error {
	if err := s.ensureSchema(context.Background()); err != nil {
		return err
	}
	_, err := s.db.ExecContext(context.Background(), `
INSERT INTO runtime_timeout_decisions (run_id, chat_id, session_id, status, failure_reason, requested_at, resolved_at, auto_continue_deadline, auto_continue_used, round_index)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
ON CONFLICT(run_id) DO UPDATE SET
  chat_id=excluded.chat_id,
  session_id=excluded.session_id,
  status=excluded.status,
  failure_reason=excluded.failure_reason,
  requested_at=excluded.requested_at,
  resolved_at=excluded.resolved_at,
  auto_continue_deadline=excluded.auto_continue_deadline,
  auto_continue_used=excluded.auto_continue_used,
  round_index=excluded.round_index
`, record.RunID, record.ChatID, record.SessionID, string(record.Status), record.FailureReason, record.RequestedAt.UTC(), record.ResolvedAt, record.AutoContinueDeadline, record.AutoContinueUsed, record.RoundIndex)
	return err
}

func (s *PostgresStore) TimeoutDecision(runID string) (TimeoutDecisionRecord, bool, error) {
	if err := s.ensureSchema(context.Background()); err != nil {
		return TimeoutDecisionRecord{}, false, err
	}
	var (
		out       TimeoutDecisionRecord
		status    string
		resolved  sql.NullTime
		deadline  sql.NullTime
	)
	err := s.db.QueryRowContext(context.Background(), `
SELECT run_id, chat_id, session_id, status, failure_reason, requested_at, resolved_at, auto_continue_deadline, auto_continue_used, round_index
FROM runtime_timeout_decisions WHERE run_id = $1
`, runID).Scan(&out.RunID, &out.ChatID, &out.SessionID, &status, &out.FailureReason, &out.RequestedAt, &resolved, &deadline, &out.AutoContinueUsed, &out.RoundIndex)
	if err == sql.ErrNoRows {
		return TimeoutDecisionRecord{}, false, nil
	}
	if err != nil {
		return TimeoutDecisionRecord{}, false, err
	}
	out.Status = TimeoutDecisionStatus(status)
	if resolved.Valid {
		out.ResolvedAt = &resolved.Time
	}
	if deadline.Valid {
		out.AutoContinueDeadline = &deadline.Time
	}
	return out, true, nil
}

func (s *PostgresStore) DeleteTimeoutDecision(runID string) error {
	if err := s.ensureSchema(context.Background()); err != nil {
		return err
	}
	_, err := s.db.ExecContext(context.Background(), `DELETE FROM runtime_timeout_decisions WHERE run_id = $1`, runID)
	return err
}
