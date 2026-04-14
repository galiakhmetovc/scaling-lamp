package runtime

import (
	"context"
	"database/sql"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	_ "modernc.org/sqlite"
	"teamd/internal/approvals"
)

type SQLiteStore struct {
	db *sql.DB
}

func NewSQLiteStore(path string) (*SQLiteStore, error) {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return nil, err
	}
	db, err := sql.Open("sqlite", path)
	if err != nil {
		return nil, err
	}
	db.SetMaxOpenConns(1)
	store := &SQLiteStore{db: db}
	if err := store.ensureSchema(context.Background()); err != nil {
		_ = db.Close()
		return nil, err
	}
	return store, nil
}

func (s *SQLiteStore) ensureSchema(ctx context.Context) error {
	if _, err := s.db.ExecContext(ctx, `PRAGMA journal_mode=WAL;`); err != nil {
		return err
	}
	if _, err := s.db.ExecContext(ctx, `PRAGMA busy_timeout=5000;`); err != nil {
		return err
	}
	const schema = `
CREATE TABLE IF NOT EXISTS runtime_runs (
  run_id TEXT PRIMARY KEY,
  chat_id INTEGER NOT NULL,
  session_id TEXT NOT NULL,
  query TEXT NOT NULL,
  final_response TEXT NOT NULL DEFAULT '',
  prompt_budget TEXT NOT NULL DEFAULT '{}',
  status TEXT NOT NULL,
  started_at TEXT NOT NULL,
  ended_at TEXT NOT NULL DEFAULT '',
  failure_reason TEXT NOT NULL DEFAULT '',
  cancel_requested INTEGER NOT NULL DEFAULT 0,
  policy_snapshot TEXT NOT NULL DEFAULT '{}'
);
CREATE TABLE IF NOT EXISTS runtime_jobs (
  job_id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  owner_run_id TEXT NOT NULL DEFAULT '',
  owner_worker_id TEXT NOT NULL DEFAULT '',
  chat_id INTEGER NOT NULL,
  session_id TEXT NOT NULL,
  command TEXT NOT NULL,
  args TEXT NOT NULL DEFAULT '[]',
  cwd TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL,
  started_at TEXT NOT NULL,
  ended_at TEXT NOT NULL DEFAULT '',
  exit_code INTEGER NULL,
  failure_reason TEXT NOT NULL DEFAULT '',
  cancel_requested INTEGER NOT NULL DEFAULT 0,
  policy_snapshot TEXT NOT NULL DEFAULT '{}'
);
CREATE TABLE IF NOT EXISTS runtime_job_logs (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  job_id TEXT NOT NULL,
  stream TEXT NOT NULL,
  content TEXT NOT NULL,
  created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS runtime_workers (
  worker_id TEXT PRIMARY KEY,
  parent_chat_id INTEGER NOT NULL,
  parent_session_id TEXT NOT NULL,
  worker_chat_id INTEGER NOT NULL UNIQUE,
  worker_session_id TEXT NOT NULL,
  status TEXT NOT NULL,
  last_run_id TEXT NOT NULL DEFAULT '',
  last_error TEXT NOT NULL DEFAULT '',
  process_pid INTEGER NOT NULL DEFAULT 0,
  process_state TEXT NOT NULL DEFAULT 'stopped',
  process_started_at TEXT NOT NULL DEFAULT '',
  process_last_heartbeat_at TEXT NOT NULL DEFAULT '',
  process_exited_at TEXT NOT NULL DEFAULT '',
  process_exit_reason TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  last_message_at TEXT NOT NULL DEFAULT '',
  closed_at TEXT NOT NULL DEFAULT '',
  policy_snapshot TEXT NOT NULL DEFAULT '{}'
);
CREATE TABLE IF NOT EXISTS runtime_worker_handoffs (
  worker_id TEXT PRIMARY KEY,
  last_run_id TEXT NOT NULL DEFAULT '',
  summary TEXT NOT NULL DEFAULT '',
  artifacts TEXT NOT NULL DEFAULT '[]',
  promoted_facts TEXT NOT NULL DEFAULT '[]',
  open_questions TEXT NOT NULL DEFAULT '[]',
  recommended_next_step TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS runtime_events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  entity_type TEXT NOT NULL,
  entity_id TEXT NOT NULL,
  chat_id INTEGER NOT NULL DEFAULT 0,
  session_id TEXT NOT NULL DEFAULT '',
  run_id TEXT NOT NULL DEFAULT '',
  kind TEXT NOT NULL,
  payload TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS runtime_plans (
  plan_id TEXT PRIMARY KEY,
  owner_type TEXT NOT NULL,
  owner_id TEXT NOT NULL,
  title TEXT NOT NULL,
  notes TEXT NOT NULL DEFAULT '[]',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS runtime_plan_items (
  plan_id TEXT NOT NULL,
  item_id TEXT NOT NULL,
  content TEXT NOT NULL,
  status TEXT NOT NULL,
  position INTEGER NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (plan_id, item_id)
);
CREATE TABLE IF NOT EXISTS runtime_checkpoints (
  chat_id INTEGER NOT NULL,
  session_id TEXT NOT NULL,
  originating_intent TEXT NOT NULL,
  what_happened TEXT NOT NULL,
  what_matters_now TEXT NOT NULL,
  archive_refs TEXT NOT NULL DEFAULT '[]',
  artifact_refs TEXT NOT NULL DEFAULT '[]',
  updated_at TEXT NOT NULL,
  PRIMARY KEY (chat_id, session_id)
);
CREATE TABLE IF NOT EXISTS runtime_continuity (
  chat_id INTEGER NOT NULL,
  session_id TEXT NOT NULL,
  user_goal TEXT NOT NULL,
  current_state TEXT NOT NULL,
  resolved_facts TEXT NOT NULL DEFAULT '[]',
  unresolved_items TEXT NOT NULL DEFAULT '[]',
  archive_refs TEXT NOT NULL DEFAULT '[]',
  artifact_refs TEXT NOT NULL DEFAULT '[]',
  updated_at TEXT NOT NULL,
  PRIMARY KEY (chat_id, session_id)
);
CREATE TABLE IF NOT EXISTS runtime_session_head (
  chat_id INTEGER NOT NULL,
  session_id TEXT NOT NULL,
  last_completed_run_id TEXT NOT NULL DEFAULT '',
  current_goal TEXT NOT NULL DEFAULT '',
  last_result_summary TEXT NOT NULL DEFAULT '',
  current_plan_id TEXT NOT NULL DEFAULT '',
  current_plan_title TEXT NOT NULL DEFAULT '',
  current_plan_items TEXT NOT NULL DEFAULT '[]',
  resolved_entities TEXT NOT NULL DEFAULT '[]',
  recent_artifact_refs TEXT NOT NULL DEFAULT '[]',
  open_loops TEXT NOT NULL DEFAULT '[]',
  current_project TEXT NOT NULL DEFAULT '',
  updated_at TEXT NOT NULL,
  PRIMARY KEY (chat_id, session_id)
);
CREATE TABLE IF NOT EXISTS runtime_processed_updates (
  chat_id INTEGER NOT NULL,
  update_id INTEGER NOT NULL,
  processed_at TEXT NOT NULL,
  PRIMARY KEY (chat_id, update_id)
);
CREATE TABLE IF NOT EXISTS runtime_session_overrides (
  session_id TEXT PRIMARY KEY,
  runtime_config TEXT NOT NULL DEFAULT '{}',
  memory_policy TEXT NOT NULL DEFAULT '{}',
  action_policy TEXT NOT NULL DEFAULT '{}',
  updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS runtime_approvals (
  approval_id TEXT PRIMARY KEY,
  worker_id TEXT NOT NULL,
  session_id TEXT NOT NULL,
  payload TEXT NOT NULL,
  status TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  reason TEXT NOT NULL DEFAULT '',
  target_type TEXT NOT NULL DEFAULT '',
  target_id TEXT NOT NULL DEFAULT '',
  requested_at TEXT NOT NULL DEFAULT '',
  decided_at TEXT NOT NULL DEFAULT '',
  decision_update_id TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS runtime_approval_callbacks (
  update_id TEXT PRIMARY KEY,
  approval_id TEXT NOT NULL,
  worker_id TEXT NOT NULL,
  session_id TEXT NOT NULL,
  payload TEXT NOT NULL,
  status TEXT NOT NULL,
  handled_at TEXT NOT NULL,
  reason TEXT NOT NULL DEFAULT '',
  target_type TEXT NOT NULL DEFAULT '',
  target_id TEXT NOT NULL DEFAULT '',
  requested_at TEXT NOT NULL DEFAULT '',
  decided_at TEXT NOT NULL DEFAULT '',
  decision_update_id TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS runtime_approval_continuations (
  approval_id TEXT PRIMARY KEY,
  run_id TEXT NOT NULL,
  chat_id INTEGER NOT NULL,
  session_id TEXT NOT NULL,
  query TEXT NOT NULL,
  tool_call_id TEXT NOT NULL,
  tool_name TEXT NOT NULL,
  tool_arguments TEXT NOT NULL DEFAULT '{}',
  requested_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS runtime_timeout_decisions (
  run_id TEXT PRIMARY KEY,
  chat_id INTEGER NOT NULL,
  session_id TEXT NOT NULL,
  status TEXT NOT NULL,
  failure_reason TEXT NOT NULL DEFAULT '',
  requested_at TEXT NOT NULL,
  resolved_at TEXT NOT NULL DEFAULT '',
  auto_continue_deadline TEXT NOT NULL DEFAULT '',
  auto_continue_used INTEGER NOT NULL DEFAULT 0,
  round_index INTEGER NOT NULL DEFAULT 0
);`
	_, err := s.db.ExecContext(ctx, schema)
	if err != nil {
		return err
	}
	for _, stmt := range []string{
		`ALTER TABLE runtime_runs ADD COLUMN final_response TEXT NOT NULL DEFAULT ''`,
		`ALTER TABLE runtime_runs ADD COLUMN prompt_budget TEXT NOT NULL DEFAULT '{}'`,
		`ALTER TABLE runtime_runs ADD COLUMN policy_snapshot TEXT NOT NULL DEFAULT '{}'`,
		`ALTER TABLE runtime_jobs ADD COLUMN policy_snapshot TEXT NOT NULL DEFAULT '{}'`,
		`ALTER TABLE runtime_workers ADD COLUMN policy_snapshot TEXT NOT NULL DEFAULT '{}'`,
		`ALTER TABLE runtime_workers ADD COLUMN process_pid INTEGER NOT NULL DEFAULT 0`,
		`ALTER TABLE runtime_workers ADD COLUMN process_state TEXT NOT NULL DEFAULT 'stopped'`,
		`ALTER TABLE runtime_workers ADD COLUMN process_started_at TEXT NOT NULL DEFAULT ''`,
		`ALTER TABLE runtime_workers ADD COLUMN process_last_heartbeat_at TEXT NOT NULL DEFAULT ''`,
		`ALTER TABLE runtime_workers ADD COLUMN process_exited_at TEXT NOT NULL DEFAULT ''`,
		`ALTER TABLE runtime_workers ADD COLUMN process_exit_reason TEXT NOT NULL DEFAULT ''`,
		`ALTER TABLE runtime_approvals ADD COLUMN reason TEXT NOT NULL DEFAULT ''`,
		`ALTER TABLE runtime_approvals ADD COLUMN target_type TEXT NOT NULL DEFAULT ''`,
		`ALTER TABLE runtime_approvals ADD COLUMN target_id TEXT NOT NULL DEFAULT ''`,
		`ALTER TABLE runtime_approvals ADD COLUMN requested_at TEXT NOT NULL DEFAULT ''`,
		`ALTER TABLE runtime_approvals ADD COLUMN decided_at TEXT NOT NULL DEFAULT ''`,
		`ALTER TABLE runtime_approvals ADD COLUMN decision_update_id TEXT NOT NULL DEFAULT ''`,
		`ALTER TABLE runtime_approval_callbacks ADD COLUMN reason TEXT NOT NULL DEFAULT ''`,
		`ALTER TABLE runtime_approval_callbacks ADD COLUMN target_type TEXT NOT NULL DEFAULT ''`,
		`ALTER TABLE runtime_approval_callbacks ADD COLUMN target_id TEXT NOT NULL DEFAULT ''`,
		`ALTER TABLE runtime_approval_callbacks ADD COLUMN requested_at TEXT NOT NULL DEFAULT ''`,
		`ALTER TABLE runtime_approval_callbacks ADD COLUMN decided_at TEXT NOT NULL DEFAULT ''`,
		`ALTER TABLE runtime_approval_callbacks ADD COLUMN decision_update_id TEXT NOT NULL DEFAULT ''`,
		`ALTER TABLE runtime_checkpoints ADD COLUMN archive_refs TEXT NOT NULL DEFAULT '[]'`,
		`ALTER TABLE runtime_checkpoints ADD COLUMN artifact_refs TEXT NOT NULL DEFAULT '[]'`,
		`ALTER TABLE runtime_continuity ADD COLUMN archive_refs TEXT NOT NULL DEFAULT '[]'`,
		`ALTER TABLE runtime_continuity ADD COLUMN artifact_refs TEXT NOT NULL DEFAULT '[]'`,
		`ALTER TABLE runtime_session_head ADD COLUMN current_plan_id TEXT NOT NULL DEFAULT ''`,
		`ALTER TABLE runtime_session_head ADD COLUMN current_plan_title TEXT NOT NULL DEFAULT ''`,
		`ALTER TABLE runtime_session_head ADD COLUMN current_plan_items TEXT NOT NULL DEFAULT '[]'`,
	} {
		if _, err := s.db.ExecContext(ctx, stmt); err != nil && !strings.Contains(err.Error(), "duplicate column name") {
			return err
		}
	}
	return nil
}

func (s *SQLiteStore) SaveRun(run RunRecord) error {
	snapshotJSON, err := json.Marshal(NormalizePolicySnapshot(run.PolicySnapshot))
	if err != nil {
		return err
	}
	promptBudgetJSON, err := json.Marshal(run.PromptBudget)
	if err != nil {
		return err
	}
	endedAt := ""
	if run.EndedAt != nil {
		endedAt = run.EndedAt.UTC().Format(time.RFC3339Nano)
	}
	_, err = s.db.ExecContext(context.Background(), `
INSERT INTO runtime_runs (run_id, chat_id, session_id, query, final_response, prompt_budget, status, started_at, ended_at, failure_reason, cancel_requested, policy_snapshot)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
`, run.RunID, run.ChatID, run.SessionID, run.Query, run.FinalResponse, string(promptBudgetJSON), string(run.Status), run.StartedAt.UTC().Format(time.RFC3339Nano), endedAt, run.FailureReason, boolToInt(run.CancelRequested), string(snapshotJSON))
	return err
}

func (s *SQLiteStore) SavePlan(plan PlanRecord) error {
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
VALUES (?, ?, ?, ?, ?, ?, ?)
ON CONFLICT(plan_id) DO UPDATE SET
  owner_type=excluded.owner_type,
  owner_id=excluded.owner_id,
  title=excluded.title,
  notes=excluded.notes,
  created_at=excluded.created_at,
  updated_at=excluded.updated_at
`, plan.PlanID, plan.OwnerType, plan.OwnerID, plan.Title, string(notesJSON), plan.CreatedAt.UTC().Format(time.RFC3339Nano), plan.UpdatedAt.UTC().Format(time.RFC3339Nano))
	if err != nil {
		return err
	}
	if _, err := tx.ExecContext(context.Background(), `DELETE FROM runtime_plan_items WHERE plan_id = ?`, plan.PlanID); err != nil {
		return err
	}
	for _, item := range plan.Items {
		_, err = tx.ExecContext(context.Background(), `
INSERT INTO runtime_plan_items (plan_id, item_id, content, status, position, created_at, updated_at)
VALUES (?, ?, ?, ?, ?, ?, ?)
`, plan.PlanID, item.ItemID, item.Content, string(item.Status), item.Position, item.CreatedAt.UTC().Format(time.RFC3339Nano), item.UpdatedAt.UTC().Format(time.RFC3339Nano))
		if err != nil {
			return err
		}
	}
	return tx.Commit()
}

func (s *SQLiteStore) SaveWorkerHandoff(handoff WorkerHandoff) error {
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
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
ON CONFLICT(worker_id) DO UPDATE SET
  last_run_id=excluded.last_run_id,
  summary=excluded.summary,
  artifacts=excluded.artifacts,
  promoted_facts=excluded.promoted_facts,
  open_questions=excluded.open_questions,
  recommended_next_step=excluded.recommended_next_step,
  created_at=excluded.created_at,
  updated_at=excluded.updated_at
`, handoff.WorkerID, handoff.LastRunID, handoff.Summary, string(artifactsJSON), string(promotedFactsJSON), string(openQuestionsJSON), handoff.RecommendedNextStep, handoff.CreatedAt.UTC().Format(time.RFC3339Nano), handoff.UpdatedAt.UTC().Format(time.RFC3339Nano))
	return err
}

func (s *SQLiteStore) WorkerHandoff(workerID string) (WorkerHandoff, bool, error) {
	var (
		out               WorkerHandoff
		artifactsJSON     string
		promotedFactsJSON string
		openQuestionsJSON string
		createdAt         string
		updatedAt         string
	)
	err := s.db.QueryRowContext(context.Background(), `
SELECT worker_id, last_run_id, summary, artifacts, promoted_facts, open_questions, recommended_next_step, created_at, updated_at
FROM runtime_worker_handoffs WHERE worker_id = ?
`, workerID).Scan(&out.WorkerID, &out.LastRunID, &out.Summary, &artifactsJSON, &promotedFactsJSON, &openQuestionsJSON, &out.RecommendedNextStep, &createdAt, &updatedAt)
	if err == sql.ErrNoRows {
		return WorkerHandoff{}, false, nil
	}
	if err != nil {
		return WorkerHandoff{}, false, err
	}
	_ = json.Unmarshal([]byte(artifactsJSON), &out.Artifacts)
	_ = json.Unmarshal([]byte(promotedFactsJSON), &out.PromotedFacts)
	_ = json.Unmarshal([]byte(openQuestionsJSON), &out.OpenQuestions)
	if parsed, err := time.Parse(time.RFC3339Nano, createdAt); err == nil {
		out.CreatedAt = parsed
	}
	if parsed, err := time.Parse(time.RFC3339Nano, updatedAt); err == nil {
		out.UpdatedAt = parsed
	}
	return out, true, nil
}

func (s *SQLiteStore) Plan(planID string) (PlanRecord, bool, error) {
	var (
		out       PlanRecord
		notesJSON string
		createdAt string
		updatedAt string
	)
	err := s.db.QueryRowContext(context.Background(), `
SELECT plan_id, owner_type, owner_id, title, notes, created_at, updated_at
FROM runtime_plans WHERE plan_id = ?
`, planID).Scan(&out.PlanID, &out.OwnerType, &out.OwnerID, &out.Title, &notesJSON, &createdAt, &updatedAt)
	if err == sql.ErrNoRows {
		return PlanRecord{}, false, nil
	}
	if err != nil {
		return PlanRecord{}, false, err
	}
	_ = json.Unmarshal([]byte(notesJSON), &out.Notes)
	if parsed, err := time.Parse(time.RFC3339Nano, createdAt); err == nil {
		out.CreatedAt = parsed
	}
	if parsed, err := time.Parse(time.RFC3339Nano, updatedAt); err == nil {
		out.UpdatedAt = parsed
	}
	items, err := s.planItems(planID)
	if err != nil {
		return PlanRecord{}, false, err
	}
	out.Items = items
	return out, true, nil
}

func (s *SQLiteStore) ListPlans(query PlanQuery) ([]PlanRecord, error) {
	stmt := `
SELECT plan_id, owner_type, owner_id, title, notes, created_at, updated_at
FROM runtime_plans`
	args := []any{}
	where := []string{}
	if v := strings.TrimSpace(query.OwnerType); v != "" {
		where = append(where, "owner_type = ?")
		args = append(args, v)
	}
	if v := strings.TrimSpace(query.OwnerID); v != "" {
		where = append(where, "owner_id = ?")
		args = append(args, v)
	}
	if len(where) > 0 {
		stmt += " WHERE " + strings.Join(where, " AND ")
	}
	stmt += " ORDER BY updated_at DESC"
	if query.Limit > 0 {
		stmt += fmt.Sprintf(" LIMIT %d", query.Limit)
	}
	rows, err := s.db.QueryContext(context.Background(), stmt, args...)
	if err != nil {
		return nil, err
	}
	out := []PlanRecord{}
	for rows.Next() {
		var (
			item      PlanRecord
			notesJSON string
			createdAt string
			updatedAt string
		)
		if err := rows.Scan(&item.PlanID, &item.OwnerType, &item.OwnerID, &item.Title, &notesJSON, &createdAt, &updatedAt); err != nil {
			return nil, err
		}
		_ = json.Unmarshal([]byte(notesJSON), &item.Notes)
		if parsed, err := time.Parse(time.RFC3339Nano, createdAt); err == nil {
			item.CreatedAt = parsed
		}
		if parsed, err := time.Parse(time.RFC3339Nano, updatedAt); err == nil {
			item.UpdatedAt = parsed
		}
		out = append(out, item)
	}
	if err := rows.Close(); err != nil {
		return nil, err
	}
	if err := rows.Err(); err != nil {
		return nil, err
	}
	for i := range out {
		items, err := s.planItems(out[i].PlanID)
		if err != nil {
			return nil, err
		}
		out[i].Items = items
	}
	return out, nil
}

func (s *SQLiteStore) planItems(planID string) ([]PlanItem, error) {
	rows, err := s.db.QueryContext(context.Background(), `
SELECT item_id, content, status, position, created_at, updated_at
FROM runtime_plan_items
WHERE plan_id = ?
ORDER BY position ASC
`, planID)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := []PlanItem{}
	for rows.Next() {
		var (
			item      PlanItem
			status    string
			createdAt string
			updatedAt string
		)
		if err := rows.Scan(&item.ItemID, &item.Content, &status, &item.Position, &createdAt, &updatedAt); err != nil {
			return nil, err
		}
		item.Status = PlanItemStatus(status)
		if parsed, err := time.Parse(time.RFC3339Nano, createdAt); err == nil {
			item.CreatedAt = parsed
		}
		if parsed, err := time.Parse(time.RFC3339Nano, updatedAt); err == nil {
			item.UpdatedAt = parsed
		}
		out = append(out, item)
	}
	return out, rows.Err()
}

func (s *SQLiteStore) MarkCancelRequested(runID string) error {
	_, err := s.db.ExecContext(context.Background(), `UPDATE runtime_runs SET cancel_requested = 1 WHERE run_id = ?`, runID)
	return err
}

func (s *SQLiteStore) Run(runID string) (RunRecord, bool, error) {
	var (
		run          RunRecord
		status       string
		startedAt    string
		endedAt      string
		cancelInt    int
		promptBudget string
		snapshotJSON string
	)
	err := s.db.QueryRowContext(context.Background(), `
SELECT run_id, chat_id, session_id, query, final_response, prompt_budget, status, started_at, ended_at, failure_reason, cancel_requested, policy_snapshot
FROM runtime_runs
WHERE run_id = ?
`, runID).Scan(&run.RunID, &run.ChatID, &run.SessionID, &run.Query, &run.FinalResponse, &promptBudget, &status, &startedAt, &endedAt, &run.FailureReason, &cancelInt, &snapshotJSON)
	if err == sql.ErrNoRows {
		return RunRecord{}, false, nil
	}
	if err != nil {
		return RunRecord{}, false, err
	}
	run.Status = RunStatus(status)
	run.CancelRequested = cancelInt == 1
	if parsed, err := time.Parse(time.RFC3339Nano, startedAt); err == nil {
		run.StartedAt = parsed
	}
	if endedAt != "" {
		if parsed, err := time.Parse(time.RFC3339Nano, endedAt); err == nil {
			run.EndedAt = &parsed
		}
	}
	_ = json.Unmarshal([]byte(promptBudget), &run.PromptBudget)
	_ = json.Unmarshal([]byte(snapshotJSON), &run.PolicySnapshot)
	return run, true, nil
}

func (s *SQLiteStore) ListRuns(query RunQuery) ([]RunRecord, error) {
	where := []string{}
	args := []any{}
	if query.HasChatID {
		where = append(where, "chat_id = ?")
		args = append(args, query.ChatID)
	}
	if sessionID := strings.TrimSpace(query.SessionID); sessionID != "" {
		where = append(where, "session_id = ?")
		args = append(args, sessionID)
	}
	if query.HasStatus {
		where = append(where, "status = ?")
		args = append(args, string(query.Status))
	}
	stmt := `
SELECT run_id, chat_id, session_id, query, final_response, prompt_budget, status, started_at, ended_at, failure_reason, cancel_requested, policy_snapshot
FROM runtime_runs`
	if len(where) > 0 {
		stmt += " WHERE " + strings.Join(where, " AND ")
	}
	stmt += " ORDER BY started_at DESC"
	if query.Limit > 0 {
		stmt += fmt.Sprintf(" LIMIT %d", query.Limit)
	}
	rows, err := s.db.QueryContext(context.Background(), stmt, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := []RunRecord{}
	for rows.Next() {
		var (
			item         RunRecord
			status       string
			startedAt    string
			endedAt      string
			cancelInt    int
			promptBudget string
			snapshotJSON string
		)
		if err := rows.Scan(&item.RunID, &item.ChatID, &item.SessionID, &item.Query, &item.FinalResponse, &promptBudget, &status, &startedAt, &endedAt, &item.FailureReason, &cancelInt, &snapshotJSON); err != nil {
			return nil, err
		}
		item.Status = RunStatus(status)
		item.CancelRequested = cancelInt == 1
		if parsed, err := time.Parse(time.RFC3339Nano, startedAt); err == nil {
			item.StartedAt = parsed
		}
		if endedAt != "" {
			if parsed, err := time.Parse(time.RFC3339Nano, endedAt); err == nil {
				item.EndedAt = &parsed
			}
		}
		_ = json.Unmarshal([]byte(promptBudget), &item.PromptBudget)
		_ = json.Unmarshal([]byte(snapshotJSON), &item.PolicySnapshot)
		out = append(out, item)
	}
	return out, rows.Err()
}

func (s *SQLiteStore) ListSessions(query SessionQuery) ([]SessionRecord, error) {
	runSelect := "SELECT session_id, started_at AS activity_at, 0 AS has_overrides FROM runtime_runs"
	overrideSelect := "SELECT session_id, updated_at AS activity_at, 1 AS has_overrides FROM runtime_session_overrides"
	args := []any{}
	if query.HasChatID {
		runSelect += " WHERE chat_id = ?"
		args = append(args, query.ChatID)
		overrideSelect += " WHERE session_id LIKE ?"
		args = append(args, fmt.Sprintf("%d:%%", query.ChatID))
	}
	stmt := `
SELECT session_id, MAX(activity_at) AS last_activity, MAX(has_overrides) AS has_overrides
FROM (` + runSelect + ` UNION ALL ` + overrideSelect + `) AS runtime_sessions
GROUP BY session_id
ORDER BY last_activity DESC`
	if query.Limit > 0 {
		stmt += fmt.Sprintf(" LIMIT %d", query.Limit)
	}
	rows, err := s.db.QueryContext(context.Background(), stmt, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := []SessionRecord{}
	for rows.Next() {
		var (
			item         SessionRecord
			lastActivity string
			hasOverrides int
		)
		if err := rows.Scan(&item.SessionID, &lastActivity, &hasOverrides); err != nil {
			return nil, err
		}
		if parsed, err := time.Parse(time.RFC3339Nano, lastActivity); err == nil {
			item.LastActivityAt = parsed
		}
		item.HasOverrides = hasOverrides == 1
		out = append(out, item)
	}
	return out, rows.Err()
}

func (s *SQLiteStore) SaveJob(job JobRecord) error {
	argsJSON, err := json.Marshal(job.Args)
	if err != nil {
		return err
	}
	snapshotJSON, err := json.Marshal(NormalizePolicySnapshot(job.PolicySnapshot))
	if err != nil {
		return err
	}
	endedAt := ""
	if job.EndedAt != nil {
		endedAt = job.EndedAt.UTC().Format(time.RFC3339Nano)
	}
	_, err = s.db.ExecContext(context.Background(), `
INSERT INTO runtime_jobs (job_id, kind, owner_run_id, owner_worker_id, chat_id, session_id, command, args, cwd, status, started_at, ended_at, exit_code, failure_reason, cancel_requested, policy_snapshot)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
`, job.JobID, job.Kind, job.OwnerRunID, job.OwnerWorkerID, job.ChatID, job.SessionID, job.Command, string(argsJSON), job.Cwd, string(job.Status), job.StartedAt.UTC().Format(time.RFC3339Nano), endedAt, job.ExitCode, job.FailureReason, boolToInt(job.CancelRequested), string(snapshotJSON))
	return err
}

func (s *SQLiteStore) Job(jobID string) (JobRecord, bool, error) {
	var (
		job          JobRecord
		status       string
		argsJSON     string
		startedAt    string
		endedAt      string
		exitCode     sql.NullInt64
		cancelInt    int
		snapshotJSON string
	)
	err := s.db.QueryRowContext(context.Background(), `
SELECT job_id, kind, owner_run_id, owner_worker_id, chat_id, session_id, command, args, cwd, status, started_at, ended_at, exit_code, failure_reason, cancel_requested, policy_snapshot
FROM runtime_jobs WHERE job_id = ?
`, jobID).Scan(&job.JobID, &job.Kind, &job.OwnerRunID, &job.OwnerWorkerID, &job.ChatID, &job.SessionID, &job.Command, &argsJSON, &job.Cwd, &status, &startedAt, &endedAt, &exitCode, &job.FailureReason, &cancelInt, &snapshotJSON)
	if err == sql.ErrNoRows {
		return JobRecord{}, false, nil
	}
	if err != nil {
		return JobRecord{}, false, err
	}
	job.Status = JobStatus(status)
	job.CancelRequested = cancelInt == 1
	_ = json.Unmarshal([]byte(argsJSON), &job.Args)
	if parsed, err := time.Parse(time.RFC3339Nano, startedAt); err == nil {
		job.StartedAt = parsed
	}
	if endedAt != "" {
		if parsed, err := time.Parse(time.RFC3339Nano, endedAt); err == nil {
			job.EndedAt = &parsed
		}
	}
	if exitCode.Valid {
		code := int(exitCode.Int64)
		job.ExitCode = &code
	}
	_ = json.Unmarshal([]byte(snapshotJSON), &job.PolicySnapshot)
	return job, true, nil
}

func (s *SQLiteStore) ListJobs(limit int) ([]JobRecord, error) {
	stmt := `
SELECT job_id, kind, owner_run_id, owner_worker_id, chat_id, session_id, command, args, cwd, status, started_at, ended_at, exit_code, failure_reason, cancel_requested, policy_snapshot
FROM runtime_jobs
ORDER BY started_at DESC`
	if limit > 0 {
		stmt += fmt.Sprintf(" LIMIT %d", limit)
	}
	rows, err := s.db.QueryContext(context.Background(), stmt)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := []JobRecord{}
	for rows.Next() {
		var (
			job          JobRecord
			status       string
			argsJSON     string
			startedAt    string
			endedAt      string
			exitCode     sql.NullInt64
			cancelInt    int
			snapshotJSON string
		)
		if err := rows.Scan(&job.JobID, &job.Kind, &job.OwnerRunID, &job.OwnerWorkerID, &job.ChatID, &job.SessionID, &job.Command, &argsJSON, &job.Cwd, &status, &startedAt, &endedAt, &exitCode, &job.FailureReason, &cancelInt, &snapshotJSON); err != nil {
			return nil, err
		}
		job.Status = JobStatus(status)
		job.CancelRequested = cancelInt == 1
		_ = json.Unmarshal([]byte(argsJSON), &job.Args)
		if parsed, err := time.Parse(time.RFC3339Nano, startedAt); err == nil {
			job.StartedAt = parsed
		}
		if endedAt != "" {
			if parsed, err := time.Parse(time.RFC3339Nano, endedAt); err == nil {
				job.EndedAt = &parsed
			}
		}
		if exitCode.Valid {
			code := int(exitCode.Int64)
			job.ExitCode = &code
		}
		_ = json.Unmarshal([]byte(snapshotJSON), &job.PolicySnapshot)
		out = append(out, job)
	}
	return out, rows.Err()
}

func (s *SQLiteStore) MarkJobCancelRequested(jobID string) error {
	_, err := s.db.ExecContext(context.Background(), `UPDATE runtime_jobs SET cancel_requested = 1 WHERE job_id = ?`, jobID)
	return err
}

func (s *SQLiteStore) SaveJobLog(chunk JobLogChunk) error {
	_, err := s.db.ExecContext(context.Background(), `
INSERT INTO runtime_job_logs (job_id, stream, content, created_at)
VALUES (?, ?, ?, ?)
`, chunk.JobID, chunk.Stream, chunk.Content, chunk.CreatedAt.UTC().Format(time.RFC3339Nano))
	return err
}

func (s *SQLiteStore) JobLogs(query JobLogQuery) ([]JobLogChunk, error) {
	where := []string{"job_id = ?"}
	args := []any{query.JobID}
	if stream := strings.TrimSpace(query.Stream); stream != "" {
		where = append(where, "stream = ?")
		args = append(args, stream)
	}
	if query.AfterID > 0 {
		where = append(where, "id > ?")
		args = append(args, query.AfterID)
	}
	stmt := `
SELECT id, job_id, stream, content, created_at
FROM runtime_job_logs
WHERE ` + strings.Join(where, " AND ") + `
ORDER BY id ASC`
	if query.Limit > 0 {
		stmt += fmt.Sprintf(" LIMIT %d", query.Limit)
	}
	rows, err := s.db.QueryContext(context.Background(), stmt, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := []JobLogChunk{}
	for rows.Next() {
		var (
			chunk     JobLogChunk
			createdAt string
		)
		if err := rows.Scan(&chunk.ID, &chunk.JobID, &chunk.Stream, &chunk.Content, &createdAt); err != nil {
			return nil, err
		}
		if parsed, err := time.Parse(time.RFC3339Nano, createdAt); err == nil {
			chunk.CreatedAt = parsed
		}
		out = append(out, chunk)
	}
	return out, rows.Err()
}

func (s *SQLiteStore) RecoverInterruptedJobs(reason string) (int, error) {
	now := time.Now().UTC().Format(time.RFC3339Nano)
	result, err := s.db.ExecContext(context.Background(), `
UPDATE runtime_jobs
SET status = ?, failure_reason = ?, ended_at = ?
WHERE status IN (?, ?)
`, string(JobFailed), reason, now, string(JobQueued), string(JobRunning))
	if err != nil {
		return 0, err
	}
	count, err := result.RowsAffected()
	if err != nil {
		return 0, err
	}
	return int(count), nil
}

func (s *SQLiteStore) SaveWorker(worker WorkerRecord) error {
	snapshotJSON, err := json.Marshal(NormalizePolicySnapshot(worker.PolicySnapshot))
	if err != nil {
		return err
	}
	lastMessageAt := ""
	if worker.LastMessageAt != nil {
		lastMessageAt = worker.LastMessageAt.UTC().Format(time.RFC3339Nano)
	}
	closedAt := ""
	if worker.ClosedAt != nil {
		closedAt = worker.ClosedAt.UTC().Format(time.RFC3339Nano)
	}
	processStartedAt := ""
	if worker.Process.StartedAt != nil {
		processStartedAt = worker.Process.StartedAt.UTC().Format(time.RFC3339Nano)
	}
	processHeartbeatAt := ""
	if worker.Process.LastHeartbeatAt != nil {
		processHeartbeatAt = worker.Process.LastHeartbeatAt.UTC().Format(time.RFC3339Nano)
	}
	processExitedAt := ""
	if worker.Process.ExitedAt != nil {
		processExitedAt = worker.Process.ExitedAt.UTC().Format(time.RFC3339Nano)
	}
	_, err = s.db.ExecContext(context.Background(), `
INSERT INTO runtime_workers (worker_id, parent_chat_id, parent_session_id, worker_chat_id, worker_session_id, status, last_run_id, last_error, process_pid, process_state, process_started_at, process_last_heartbeat_at, process_exited_at, process_exit_reason, created_at, updated_at, last_message_at, closed_at, policy_snapshot)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
`, worker.WorkerID, worker.ParentChatID, worker.ParentSessionID, worker.WorkerChatID, worker.WorkerSessionID, string(worker.Status), worker.LastRunID, worker.LastError, worker.Process.PID, string(worker.Process.State), processStartedAt, processHeartbeatAt, processExitedAt, worker.Process.ExitReason, worker.CreatedAt.UTC().Format(time.RFC3339Nano), worker.UpdatedAt.UTC().Format(time.RFC3339Nano), lastMessageAt, closedAt, string(snapshotJSON))
	return err
}

func (s *SQLiteStore) Worker(workerID string) (WorkerRecord, bool, error) {
	var (
		item          WorkerRecord
		status        string
		processState  string
		createdAt     string
		updatedAt     string
		processStartedAt   string
		processHeartbeatAt string
		processExitedAt    string
		lastMessageAt string
		closedAt      string
		processExitReason string
		snapshotJSON  string
	)
	err := s.db.QueryRowContext(context.Background(), `
SELECT worker_id, parent_chat_id, parent_session_id, worker_chat_id, worker_session_id, status, last_run_id, last_error, process_pid, process_state, process_started_at, process_last_heartbeat_at, process_exited_at, process_exit_reason, created_at, updated_at, last_message_at, closed_at, policy_snapshot
FROM runtime_workers WHERE worker_id = ?
`, workerID).Scan(&item.WorkerID, &item.ParentChatID, &item.ParentSessionID, &item.WorkerChatID, &item.WorkerSessionID, &status, &item.LastRunID, &item.LastError, &item.Process.PID, &processState, &processStartedAt, &processHeartbeatAt, &processExitedAt, &processExitReason, &createdAt, &updatedAt, &lastMessageAt, &closedAt, &snapshotJSON)
	if err == sql.ErrNoRows {
		return WorkerRecord{}, false, nil
	}
	if err != nil {
		return WorkerRecord{}, false, err
	}
	item.Status = WorkerStatus(status)
	item.Process.State = WorkerProcessState(processState)
	item.Process.ExitReason = processExitReason
	if parsed, err := time.Parse(time.RFC3339Nano, createdAt); err == nil {
		item.CreatedAt = parsed
	}
	if parsed, err := time.Parse(time.RFC3339Nano, updatedAt); err == nil {
		item.UpdatedAt = parsed
	}
	if processStartedAt != "" {
		if parsed, err := time.Parse(time.RFC3339Nano, processStartedAt); err == nil {
			item.Process.StartedAt = &parsed
		}
	}
	if processHeartbeatAt != "" {
		if parsed, err := time.Parse(time.RFC3339Nano, processHeartbeatAt); err == nil {
			item.Process.LastHeartbeatAt = &parsed
		}
	}
	if processExitedAt != "" {
		if parsed, err := time.Parse(time.RFC3339Nano, processExitedAt); err == nil {
			item.Process.ExitedAt = &parsed
		}
	}
	if lastMessageAt != "" {
		if parsed, err := time.Parse(time.RFC3339Nano, lastMessageAt); err == nil {
			item.LastMessageAt = &parsed
		}
	}
	if closedAt != "" {
		if parsed, err := time.Parse(time.RFC3339Nano, closedAt); err == nil {
			item.ClosedAt = &parsed
		}
	}
	_ = json.Unmarshal([]byte(snapshotJSON), &item.PolicySnapshot)
	return item, true, nil
}

func (s *SQLiteStore) ListWorkers(query WorkerQuery) ([]WorkerRecord, error) {
	stmt := `
SELECT worker_id, parent_chat_id, parent_session_id, worker_chat_id, worker_session_id, status, last_run_id, last_error, created_at, updated_at, last_message_at, closed_at, policy_snapshot
FROM runtime_workers`
	args := []any{}
	if query.HasParentChatID {
		stmt += " WHERE parent_chat_id = ?"
		args = append(args, query.ParentChatID)
	}
	stmt += " ORDER BY created_at DESC"
	if query.Limit > 0 {
		stmt += fmt.Sprintf(" LIMIT %d", query.Limit)
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
			createdAt     string
			updatedAt     string
			processStartedAt   string
			processHeartbeatAt string
			processExitedAt    string
			lastMessageAt string
			closedAt      string
			processExitReason string
			snapshotJSON  string
		)
		if err := rows.Scan(&item.WorkerID, &item.ParentChatID, &item.ParentSessionID, &item.WorkerChatID, &item.WorkerSessionID, &status, &item.LastRunID, &item.LastError, &item.Process.PID, &processState, &processStartedAt, &processHeartbeatAt, &processExitedAt, &processExitReason, &createdAt, &updatedAt, &lastMessageAt, &closedAt, &snapshotJSON); err != nil {
			return nil, err
		}
		item.Status = WorkerStatus(status)
		item.Process.State = WorkerProcessState(processState)
		item.Process.ExitReason = processExitReason
		if parsed, err := time.Parse(time.RFC3339Nano, createdAt); err == nil {
			item.CreatedAt = parsed
		}
		if parsed, err := time.Parse(time.RFC3339Nano, updatedAt); err == nil {
			item.UpdatedAt = parsed
		}
		if processStartedAt != "" {
			if parsed, err := time.Parse(time.RFC3339Nano, processStartedAt); err == nil {
				item.Process.StartedAt = &parsed
			}
		}
		if processHeartbeatAt != "" {
			if parsed, err := time.Parse(time.RFC3339Nano, processHeartbeatAt); err == nil {
				item.Process.LastHeartbeatAt = &parsed
			}
		}
		if processExitedAt != "" {
			if parsed, err := time.Parse(time.RFC3339Nano, processExitedAt); err == nil {
				item.Process.ExitedAt = &parsed
			}
		}
		if lastMessageAt != "" {
			if parsed, err := time.Parse(time.RFC3339Nano, lastMessageAt); err == nil {
				item.LastMessageAt = &parsed
			}
		}
		if closedAt != "" {
			if parsed, err := time.Parse(time.RFC3339Nano, closedAt); err == nil {
				item.ClosedAt = &parsed
			}
		}
		_ = json.Unmarshal([]byte(snapshotJSON), &item.PolicySnapshot)
		out = append(out, item)
	}
	return out, rows.Err()
}

func (s *SQLiteStore) RecoverInterruptedWorkers(reason string) (int, error) {
	now := time.Now().UTC().Format(time.RFC3339Nano)
	result, err := s.db.ExecContext(context.Background(), `
UPDATE runtime_workers
SET process_state = ?, process_exit_reason = ?, process_exited_at = ?, updated_at = ?
WHERE process_state IN (?, ?)
`, string(WorkerProcessFailed), reason, now, now, string(WorkerProcessStarting), string(WorkerProcessRunning))
	if err != nil {
		return 0, err
	}
	count, err := result.RowsAffected()
	if err != nil {
		return 0, err
	}
	return int(count), nil
}

func (s *SQLiteStore) SaveEvent(event RuntimeEvent) error {
	payload := "{}"
	if len(event.Payload) > 0 {
		payload = string(event.Payload)
	}
	_, err := s.db.ExecContext(context.Background(), `
INSERT INTO runtime_events (entity_type, entity_id, chat_id, session_id, run_id, kind, payload, created_at)
VALUES (?, ?, ?, ?, ?, ?, ?, ?)
`, event.EntityType, event.EntityID, event.ChatID, event.SessionID, event.RunID, event.Kind, payload, event.CreatedAt.UTC().Format(time.RFC3339Nano))
	return err
}

func (s *SQLiteStore) ListEvents(query EventQuery) ([]RuntimeEvent, error) {
	where := []string{}
	args := []any{}
	if entityType := strings.TrimSpace(query.EntityType); entityType != "" {
		where = append(where, "entity_type = ?")
		args = append(args, entityType)
	}
	if entityID := strings.TrimSpace(query.EntityID); entityID != "" {
		where = append(where, "entity_id = ?")
		args = append(args, entityID)
	}
	if runID := strings.TrimSpace(query.RunID); runID != "" {
		where = append(where, "run_id = ?")
		args = append(args, runID)
	}
	if sessionID := strings.TrimSpace(query.SessionID); sessionID != "" {
		where = append(where, "session_id = ?")
		args = append(args, sessionID)
	}
	if query.AfterID > 0 {
		where = append(where, "id > ?")
		args = append(args, query.AfterID)
	}
	stmt := `
SELECT id, entity_type, entity_id, chat_id, session_id, run_id, kind, payload, created_at
FROM runtime_events`
	if len(where) > 0 {
		stmt += " WHERE " + strings.Join(where, " AND ")
	}
	stmt += " ORDER BY id ASC"
	if query.Limit > 0 {
		stmt += fmt.Sprintf(" LIMIT %d", query.Limit)
	}
	rows, err := s.db.QueryContext(context.Background(), stmt, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := []RuntimeEvent{}
	for rows.Next() {
		var (
			item      RuntimeEvent
			payload   string
			createdAt string
		)
		if err := rows.Scan(&item.ID, &item.EntityType, &item.EntityID, &item.ChatID, &item.SessionID, &item.RunID, &item.Kind, &payload, &createdAt); err != nil {
			return nil, err
		}
		item.Payload = json.RawMessage(payload)
		if parsed, err := time.Parse(time.RFC3339Nano, createdAt); err == nil {
			item.CreatedAt = parsed
		}
		out = append(out, item)
	}
	return out, rows.Err()
}

func (s *SQLiteStore) RecoverInterruptedRuns(reason string) (int, error) {
	now := time.Now().UTC().Format(time.RFC3339Nano)
	result, err := s.db.ExecContext(context.Background(), `
UPDATE runtime_runs
SET status = ?, failure_reason = ?, ended_at = ?
WHERE status IN (?, ?)
`, string(StatusFailed), reason, now, string(StatusQueued), string(StatusRunning))
	if err != nil {
		return 0, err
	}
	count, err := result.RowsAffected()
	if err != nil {
		return 0, err
	}
	return int(count), nil
}

func (s *SQLiteStore) SaveCheckpoint(checkpoint Checkpoint) error {
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
VALUES (?, ?, ?, ?, ?, ?, ?, ?)
ON CONFLICT(chat_id, session_id) DO UPDATE SET
  originating_intent=excluded.originating_intent,
  what_happened=excluded.what_happened,
  what_matters_now=excluded.what_matters_now,
  archive_refs=excluded.archive_refs,
  artifact_refs=excluded.artifact_refs,
  updated_at=excluded.updated_at
`, checkpoint.ChatID, checkpoint.SessionID, checkpoint.OriginatingIntent, checkpoint.WhatHappened, checkpoint.WhatMattersNow, string(archiveRefs), string(artifactRefs), checkpoint.UpdatedAt.UTC().Format(time.RFC3339Nano))
	return err
}

func (s *SQLiteStore) Checkpoint(chatID int64, sessionID string) (Checkpoint, bool, error) {
	var (
		out          Checkpoint
		archiveRefs  string
		artifactRefs string
		updatedAt    string
	)
	err := s.db.QueryRowContext(context.Background(), `
SELECT chat_id, session_id, originating_intent, what_happened, what_matters_now, archive_refs, artifact_refs, updated_at
FROM runtime_checkpoints
WHERE chat_id = ? AND session_id = ?
`, chatID, sessionID).Scan(&out.ChatID, &out.SessionID, &out.OriginatingIntent, &out.WhatHappened, &out.WhatMattersNow, &archiveRefs, &artifactRefs, &updatedAt)
	if err == sql.ErrNoRows {
		return Checkpoint{}, false, nil
	}
	if err != nil {
		return Checkpoint{}, false, err
	}
	_ = json.Unmarshal([]byte(archiveRefs), &out.ArchiveRefs)
	_ = json.Unmarshal([]byte(artifactRefs), &out.ArtifactRefs)
	if parsed, err := time.Parse(time.RFC3339Nano, updatedAt); err == nil {
		out.UpdatedAt = parsed
	}
	return out, true, nil
}

func (s *SQLiteStore) SaveContinuity(continuity Continuity) error {
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
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
ON CONFLICT(chat_id, session_id) DO UPDATE SET
  user_goal=excluded.user_goal,
  current_state=excluded.current_state,
  resolved_facts=excluded.resolved_facts,
  unresolved_items=excluded.unresolved_items,
  archive_refs=excluded.archive_refs,
  artifact_refs=excluded.artifact_refs,
  updated_at=excluded.updated_at
`, continuity.ChatID, continuity.SessionID, continuity.UserGoal, continuity.CurrentState, string(resolved), string(unresolved), string(archiveRefs), string(artifactRefs), continuity.UpdatedAt.UTC().Format(time.RFC3339Nano))
	return err
}

func (s *SQLiteStore) Continuity(chatID int64, sessionID string) (Continuity, bool, error) {
	var (
		out          Continuity
		resolved     string
		unresolved   string
		archiveRefs  string
		artifactRefs string
		updatedAt    string
	)
	err := s.db.QueryRowContext(context.Background(), `
SELECT chat_id, session_id, user_goal, current_state, resolved_facts, unresolved_items, archive_refs, artifact_refs, updated_at
FROM runtime_continuity
WHERE chat_id = ? AND session_id = ?
`, chatID, sessionID).Scan(&out.ChatID, &out.SessionID, &out.UserGoal, &out.CurrentState, &resolved, &unresolved, &archiveRefs, &artifactRefs, &updatedAt)
	if err == sql.ErrNoRows {
		return Continuity{}, false, nil
	}
	if err != nil {
		return Continuity{}, false, err
	}
	_ = json.Unmarshal([]byte(resolved), &out.ResolvedFacts)
	_ = json.Unmarshal([]byte(unresolved), &out.UnresolvedItems)
	_ = json.Unmarshal([]byte(archiveRefs), &out.ArchiveRefs)
	_ = json.Unmarshal([]byte(artifactRefs), &out.ArtifactRefs)
	if parsed, err := time.Parse(time.RFC3339Nano, updatedAt); err == nil {
		out.UpdatedAt = parsed
	}
	return out, true, nil
}

func (s *SQLiteStore) SaveSessionHead(head SessionHead) error {
	planItems, err := json.Marshal(head.CurrentPlanItems)
	if err != nil {
		return err
	}
	resolved, err := json.Marshal(head.ResolvedEntities)
	if err != nil {
		return err
	}
	artifactRefs, err := json.Marshal(head.RecentArtifactRefs)
	if err != nil {
		return err
	}
	openLoops, err := json.Marshal(head.OpenLoops)
	if err != nil {
		return err
	}
	_, err = s.db.ExecContext(context.Background(), `
INSERT INTO runtime_session_head (chat_id, session_id, last_completed_run_id, current_goal, last_result_summary, current_plan_id, current_plan_title, current_plan_items, resolved_entities, recent_artifact_refs, open_loops, current_project, updated_at)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
`, head.ChatID, head.SessionID, head.LastCompletedRunID, head.CurrentGoal, head.LastResultSummary, head.CurrentPlanID, head.CurrentPlanTitle, string(planItems), string(resolved), string(artifactRefs), string(openLoops), head.CurrentProject, head.UpdatedAt.UTC().Format(time.RFC3339Nano))
	return err
}

func (s *SQLiteStore) SessionHead(chatID int64, sessionID string) (SessionHead, bool, error) {
	var (
		out          SessionHead
		planItems    string
		resolved     string
		artifactRefs string
		openLoops    string
		updatedAt    string
	)
	err := s.db.QueryRowContext(context.Background(), `
SELECT chat_id, session_id, last_completed_run_id, current_goal, last_result_summary, current_plan_id, current_plan_title, current_plan_items, resolved_entities, recent_artifact_refs, open_loops, current_project, updated_at
FROM runtime_session_head
WHERE chat_id = ? AND session_id = ?
`, chatID, sessionID).Scan(&out.ChatID, &out.SessionID, &out.LastCompletedRunID, &out.CurrentGoal, &out.LastResultSummary, &out.CurrentPlanID, &out.CurrentPlanTitle, &planItems, &resolved, &artifactRefs, &openLoops, &out.CurrentProject, &updatedAt)
	if err == sql.ErrNoRows {
		return SessionHead{}, false, nil
	}
	if err != nil {
		return SessionHead{}, false, err
	}
	_ = json.Unmarshal([]byte(planItems), &out.CurrentPlanItems)
	_ = json.Unmarshal([]byte(resolved), &out.ResolvedEntities)
	_ = json.Unmarshal([]byte(artifactRefs), &out.RecentArtifactRefs)
	_ = json.Unmarshal([]byte(openLoops), &out.OpenLoops)
	if parsed, err := time.Parse(time.RFC3339Nano, updatedAt); err == nil {
		out.UpdatedAt = parsed
	}
	return out, true, nil
}

func (s *SQLiteStore) TryMarkUpdate(chatID int64, updateID int64) (bool, error) {
	result, err := s.db.ExecContext(context.Background(), `
INSERT OR IGNORE INTO runtime_processed_updates (chat_id, update_id, processed_at)
VALUES (?, ?, ?)
`, chatID, updateID, time.Now().UTC().Format(time.RFC3339Nano))
	if err != nil {
		return false, err
	}
	rows, err := result.RowsAffected()
	if err != nil {
		return false, err
	}
	return rows > 0, nil
}

func (s *SQLiteStore) SaveSessionOverrides(overrides SessionOverrides) error {
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
VALUES (?, ?, ?, ?, ?)
ON CONFLICT(session_id) DO UPDATE SET
  runtime_config=excluded.runtime_config,
  memory_policy=excluded.memory_policy,
  action_policy=excluded.action_policy,
  updated_at=excluded.updated_at
`, overrides.SessionID, string(runtimeJSON), string(memoryJSON), string(actionJSON), overrides.UpdatedAt.UTC().Format(time.RFC3339Nano))
	return err
}

func (s *SQLiteStore) SessionOverrides(sessionID string) (SessionOverrides, bool, error) {
	var (
		out         SessionOverrides
		runtimeJSON string
		memoryJSON  string
		actionJSON  string
		updatedAt   string
	)
	err := s.db.QueryRowContext(context.Background(), `
SELECT session_id, runtime_config, memory_policy, action_policy, updated_at
FROM runtime_session_overrides
WHERE session_id = ?
`, sessionID).Scan(&out.SessionID, &runtimeJSON, &memoryJSON, &actionJSON, &updatedAt)
	if err == sql.ErrNoRows {
		return SessionOverrides{}, false, nil
	}
	if err != nil {
		return SessionOverrides{}, false, err
	}
	_ = json.Unmarshal([]byte(runtimeJSON), &out.Runtime)
	_ = json.Unmarshal([]byte(memoryJSON), &out.MemoryPolicy)
	_ = json.Unmarshal([]byte(actionJSON), &out.ActionPolicy)
	if parsed, err := time.Parse(time.RFC3339Nano, updatedAt); err == nil {
		out.UpdatedAt = parsed
	}
	return out, true, nil
}

func (s *SQLiteStore) ClearSessionOverrides(sessionID string) error {
	_, err := s.db.ExecContext(context.Background(), `DELETE FROM runtime_session_overrides WHERE session_id = ?`, sessionID)
	return err
}

func (s *SQLiteStore) SaveApproval(record approvals.Record) error {
	requestedAt := ""
	if !record.RequestedAt.IsZero() {
		requestedAt = record.RequestedAt.UTC().Format(time.RFC3339Nano)
	}
	decidedAt := ""
	if record.DecidedAt != nil {
		decidedAt = record.DecidedAt.UTC().Format(time.RFC3339Nano)
	}
	_, err := s.db.ExecContext(context.Background(), `
INSERT INTO runtime_approvals (approval_id, worker_id, session_id, payload, status, updated_at, reason, target_type, target_id, requested_at, decided_at, decision_update_id)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
`, record.ID, record.WorkerID, record.SessionID, record.Payload, string(record.Status), time.Now().UTC().Format(time.RFC3339Nano), record.Reason, record.TargetType, record.TargetID, requestedAt, decidedAt, record.DecisionUpdateID)
	return err
}

func (s *SQLiteStore) Approval(id string) (approvals.Record, bool, error) {
	var record approvals.Record
	var status string
	var requestedAt string
	var decidedAt string
	err := s.db.QueryRowContext(context.Background(), `
SELECT approval_id, worker_id, session_id, payload, status, reason, target_type, target_id, requested_at, decided_at, decision_update_id
FROM runtime_approvals WHERE approval_id = ?
`, id).Scan(&record.ID, &record.WorkerID, &record.SessionID, &record.Payload, &status, &record.Reason, &record.TargetType, &record.TargetID, &requestedAt, &decidedAt, &record.DecisionUpdateID)
	if err == sql.ErrNoRows {
		return approvals.Record{}, false, nil
	}
	if err != nil {
		return approvals.Record{}, false, err
	}
	record.Status = approvals.Status(status)
	if requestedAt != "" {
		if parsed, err := time.Parse(time.RFC3339Nano, requestedAt); err == nil {
			record.RequestedAt = parsed
		}
	}
	if decidedAt != "" {
		if parsed, err := time.Parse(time.RFC3339Nano, decidedAt); err == nil {
			record.DecidedAt = &parsed
		}
	}
	return record, true, nil
}

func (s *SQLiteStore) PendingApprovals(sessionID string) ([]approvals.Record, error) {
	rows, err := s.db.QueryContext(context.Background(), `
SELECT approval_id, worker_id, session_id, payload, status, reason, target_type, target_id, requested_at, decided_at, decision_update_id
FROM runtime_approvals
WHERE session_id = ? AND status = ?
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
		var requestedAt string
		var decidedAt string
		if err := rows.Scan(&record.ID, &record.WorkerID, &record.SessionID, &record.Payload, &status, &record.Reason, &record.TargetType, &record.TargetID, &requestedAt, &decidedAt, &record.DecisionUpdateID); err != nil {
			return nil, err
		}
		record.Status = approvals.Status(status)
		if requestedAt != "" {
			if parsed, err := time.Parse(time.RFC3339Nano, requestedAt); err == nil {
				record.RequestedAt = parsed
			}
		}
		if decidedAt != "" {
			if parsed, err := time.Parse(time.RFC3339Nano, decidedAt); err == nil {
				record.DecidedAt = &parsed
			}
		}
		out = append(out, record)
	}
	return out, rows.Err()
}

func (s *SQLiteStore) SaveHandledApprovalCallback(updateID string, record approvals.Record) error {
	requestedAt := ""
	if !record.RequestedAt.IsZero() {
		requestedAt = record.RequestedAt.UTC().Format(time.RFC3339Nano)
	}
	decidedAt := ""
	if record.DecidedAt != nil {
		decidedAt = record.DecidedAt.UTC().Format(time.RFC3339Nano)
	}
	_, err := s.db.ExecContext(context.Background(), `
INSERT INTO runtime_approval_callbacks (update_id, approval_id, worker_id, session_id, payload, status, handled_at, reason, target_type, target_id, requested_at, decided_at, decision_update_id)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
`, updateID, record.ID, record.WorkerID, record.SessionID, record.Payload, string(record.Status), time.Now().UTC().Format(time.RFC3339Nano), record.Reason, record.TargetType, record.TargetID, requestedAt, decidedAt, record.DecisionUpdateID)
	return err
}

func (s *SQLiteStore) HandledApprovalCallback(updateID string) (approvals.Record, bool, error) {
	var record approvals.Record
	var status string
	var requestedAt string
	var decidedAt string
	err := s.db.QueryRowContext(context.Background(), `
SELECT approval_id, worker_id, session_id, payload, status, reason, target_type, target_id, requested_at, decided_at, decision_update_id
FROM runtime_approval_callbacks WHERE update_id = ?
`, updateID).Scan(&record.ID, &record.WorkerID, &record.SessionID, &record.Payload, &status, &record.Reason, &record.TargetType, &record.TargetID, &requestedAt, &decidedAt, &record.DecisionUpdateID)
	if err == sql.ErrNoRows {
		return approvals.Record{}, false, nil
	}
	if err != nil {
		return approvals.Record{}, false, err
	}
	record.Status = approvals.Status(status)
	if requestedAt != "" {
		if parsed, err := time.Parse(time.RFC3339Nano, requestedAt); err == nil {
			record.RequestedAt = parsed
		}
	}
	if decidedAt != "" {
		if parsed, err := time.Parse(time.RFC3339Nano, decidedAt); err == nil {
			record.DecidedAt = &parsed
		}
	}
	return record, true, nil
}

func (s *SQLiteStore) SaveApprovalContinuation(cont ApprovalContinuation) error {
	argsJSON, err := json.Marshal(cont.ToolArguments)
	if err != nil {
		return err
	}
	_, err = s.db.ExecContext(context.Background(), `
INSERT INTO runtime_approval_continuations (approval_id, run_id, chat_id, session_id, query, tool_call_id, tool_name, tool_arguments, requested_at)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
ON CONFLICT(approval_id) DO UPDATE SET
  run_id=excluded.run_id,
  chat_id=excluded.chat_id,
  session_id=excluded.session_id,
  query=excluded.query,
  tool_call_id=excluded.tool_call_id,
  tool_name=excluded.tool_name,
  tool_arguments=excluded.tool_arguments,
  requested_at=excluded.requested_at
`, cont.ApprovalID, cont.RunID, cont.ChatID, cont.SessionID, cont.Query, cont.ToolCallID, cont.ToolName, string(argsJSON), cont.RequestedAt.UTC().Format(time.RFC3339Nano))
	return err
}

func (s *SQLiteStore) ApprovalContinuation(id string) (ApprovalContinuation, bool, error) {
	var (
		out       ApprovalContinuation
		argsJSON  string
		requested string
	)
	err := s.db.QueryRowContext(context.Background(), `
SELECT approval_id, run_id, chat_id, session_id, query, tool_call_id, tool_name, tool_arguments, requested_at
FROM runtime_approval_continuations WHERE approval_id = ?
`, id).Scan(&out.ApprovalID, &out.RunID, &out.ChatID, &out.SessionID, &out.Query, &out.ToolCallID, &out.ToolName, &argsJSON, &requested)
	if err == sql.ErrNoRows {
		return ApprovalContinuation{}, false, nil
	}
	if err != nil {
		return ApprovalContinuation{}, false, err
	}
	_ = json.Unmarshal([]byte(argsJSON), &out.ToolArguments)
	if parsed, err := time.Parse(time.RFC3339Nano, requested); err == nil {
		out.RequestedAt = parsed
	}
	return out, true, nil
}

func (s *SQLiteStore) DeleteApprovalContinuation(id string) error {
	_, err := s.db.ExecContext(context.Background(), `DELETE FROM runtime_approval_continuations WHERE approval_id = ?`, id)
	return err
}

func (s *SQLiteStore) SaveTimeoutDecision(record TimeoutDecisionRecord) error {
	var resolvedAt string
	if record.ResolvedAt != nil {
		resolvedAt = record.ResolvedAt.UTC().Format(time.RFC3339Nano)
	}
	var deadline string
	if record.AutoContinueDeadline != nil {
		deadline = record.AutoContinueDeadline.UTC().Format(time.RFC3339Nano)
	}
	_, err := s.db.ExecContext(context.Background(), `
INSERT INTO runtime_timeout_decisions (run_id, chat_id, session_id, status, failure_reason, requested_at, resolved_at, auto_continue_deadline, auto_continue_used, round_index)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
`, record.RunID, record.ChatID, record.SessionID, string(record.Status), record.FailureReason, record.RequestedAt.UTC().Format(time.RFC3339Nano), resolvedAt, deadline, boolToInt(record.AutoContinueUsed), record.RoundIndex)
	return err
}

func (s *SQLiteStore) TimeoutDecision(runID string) (TimeoutDecisionRecord, bool, error) {
	var (
		out         TimeoutDecisionRecord
		status      string
		requestedAt string
		resolvedAt  string
		deadline    string
		autoUsed    int
	)
	err := s.db.QueryRowContext(context.Background(), `
SELECT run_id, chat_id, session_id, status, failure_reason, requested_at, resolved_at, auto_continue_deadline, auto_continue_used, round_index
FROM runtime_timeout_decisions WHERE run_id = ?
`, runID).Scan(&out.RunID, &out.ChatID, &out.SessionID, &status, &out.FailureReason, &requestedAt, &resolvedAt, &deadline, &autoUsed, &out.RoundIndex)
	if err == sql.ErrNoRows {
		return TimeoutDecisionRecord{}, false, nil
	}
	if err != nil {
		return TimeoutDecisionRecord{}, false, err
	}
	out.Status = TimeoutDecisionStatus(status)
	out.AutoContinueUsed = autoUsed != 0
	if parsed, err := time.Parse(time.RFC3339Nano, requestedAt); err == nil {
		out.RequestedAt = parsed
	}
	if resolvedAt != "" {
		if parsed, err := time.Parse(time.RFC3339Nano, resolvedAt); err == nil {
			out.ResolvedAt = &parsed
		}
	}
	if deadline != "" {
		if parsed, err := time.Parse(time.RFC3339Nano, deadline); err == nil {
			out.AutoContinueDeadline = &parsed
		}
	}
	return out, true, nil
}

func (s *SQLiteStore) DeleteTimeoutDecision(runID string) error {
	_, err := s.db.ExecContext(context.Background(), `DELETE FROM runtime_timeout_decisions WHERE run_id = ?`, runID)
	return err
}

func boolToInt(v bool) int {
	if v {
		return 1
	}
	return 0
}
