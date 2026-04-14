package mesh

import (
	"context"
	"database/sql"
	"encoding/json"
	"time"

	_ "github.com/jackc/pgx/v5/stdlib"
)

type PostgresRegistry struct {
	db             *sql.DB
	staleThreshold time.Duration
}

func NewPostgresRegistry(db *sql.DB, staleThreshold time.Duration) *PostgresRegistry {
	if staleThreshold <= 0 {
		staleThreshold = 2 * time.Minute
	}
	return &PostgresRegistry{
		db:             db,
		staleThreshold: staleThreshold,
	}
}

func (r *PostgresRegistry) ensureSchema(ctx context.Context) error {
	const schema = `
CREATE TABLE IF NOT EXISTS mesh_agents (
  agent_id TEXT PRIMARY KEY,
  addr TEXT NOT NULL,
  model TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'idle',
  started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  last_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  metadata JSONB NOT NULL DEFAULT '{}'::jsonb
);
CREATE TABLE IF NOT EXISTS mesh_agent_scores (
  agent_id TEXT NOT NULL,
  task_class TEXT NOT NULL,
  tasks_seen INTEGER NOT NULL DEFAULT 0,
  tasks_won INTEGER NOT NULL DEFAULT 0,
  success_count INTEGER NOT NULL DEFAULT 0,
  failure_count INTEGER NOT NULL DEFAULT 0,
  avg_latency_ms BIGINT NOT NULL DEFAULT 0,
  last_score_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (agent_id, task_class)
);
CREATE INDEX IF NOT EXISTS idx_mesh_agents_last_seen ON mesh_agents(last_seen_at);
`
	_, err := r.db.ExecContext(ctx, schema)
	return err
}

func (r *PostgresRegistry) Register(ctx context.Context, peer PeerDescriptor) error {
	if err := r.ensureSchema(ctx); err != nil {
		return err
	}
	metadata, err := json.Marshal(peer.Metadata)
	if err != nil {
		return err
	}
	startedAt := peer.StartedAt
	if startedAt.IsZero() {
		startedAt = time.Now().UTC()
	}
	lastSeenAt := peer.LastSeenAt
	if lastSeenAt.IsZero() {
		lastSeenAt = time.Now().UTC()
	}
	_, err = r.db.ExecContext(ctx, `
INSERT INTO mesh_agents (agent_id, addr, model, status, started_at, last_seen_at, metadata)
VALUES ($1, $2, $3, $4, $5, $6, $7)
ON CONFLICT (agent_id) DO UPDATE SET
  addr = EXCLUDED.addr,
  model = EXCLUDED.model,
  status = EXCLUDED.status,
  started_at = EXCLUDED.started_at,
  last_seen_at = EXCLUDED.last_seen_at,
  metadata = EXCLUDED.metadata
`, peer.AgentID, peer.Addr, peer.Model, peer.Status, startedAt, lastSeenAt, metadata)
	return err
}

func (r *PostgresRegistry) Heartbeat(ctx context.Context, agentID string, at time.Time) error {
	if err := r.ensureSchema(ctx); err != nil {
		return err
	}
	if at.IsZero() {
		at = time.Now().UTC()
	}
	_, err := r.db.ExecContext(ctx, `UPDATE mesh_agents SET last_seen_at = $2 WHERE agent_id = $1`, agentID, at)
	return err
}

func (r *PostgresRegistry) ListOnline(ctx context.Context) ([]PeerDescriptor, error) {
	if err := r.ensureSchema(ctx); err != nil {
		return nil, err
	}
	cutoff := time.Now().UTC().Add(-r.staleThreshold)
	rows, err := r.db.QueryContext(ctx, `
SELECT agent_id, addr, model, status, started_at, last_seen_at, metadata
FROM mesh_agents
WHERE last_seen_at >= $1
ORDER BY last_seen_at DESC
`, cutoff)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var out []PeerDescriptor
	for rows.Next() {
		var peer PeerDescriptor
		var metadata []byte
		if err := rows.Scan(&peer.AgentID, &peer.Addr, &peer.Model, &peer.Status, &peer.StartedAt, &peer.LastSeenAt, &metadata); err != nil {
			return nil, err
		}
		if len(metadata) > 0 {
			if err := json.Unmarshal(metadata, &peer.Metadata); err != nil {
				return nil, err
			}
		}
		out = append(out, peer)
	}
	if err := rows.Err(); err != nil {
		return nil, err
	}
	return out, nil
}

func (r *PostgresRegistry) ListScores(ctx context.Context, taskClass string) ([]ScoreRecord, error) {
	if err := r.ensureSchema(ctx); err != nil {
		return nil, err
	}
	rows, err := r.db.QueryContext(ctx, `
SELECT agent_id, task_class, tasks_seen, tasks_won, success_count, failure_count, avg_latency_ms, last_score_at
FROM mesh_agent_scores
WHERE task_class = $1
ORDER BY tasks_won DESC, success_count DESC, avg_latency_ms ASC, agent_id ASC
`, taskClass)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var out []ScoreRecord
	for rows.Next() {
		var score ScoreRecord
		if err := rows.Scan(
			&score.AgentID,
			&score.TaskClass,
			&score.TasksSeen,
			&score.TasksWon,
			&score.SuccessCount,
			&score.FailureCount,
			&score.AvgLatencyMS,
			&score.LastScoreAt,
		); err != nil {
			return nil, err
		}
		out = append(out, score)
	}
	if err := rows.Err(); err != nil {
		return nil, err
	}
	return out, nil
}

func (r *PostgresRegistry) RecordScore(ctx context.Context, score ScoreRecord) error {
	if err := r.ensureSchema(ctx); err != nil {
		return err
	}
	lastScoreAt := score.LastScoreAt
	if lastScoreAt.IsZero() {
		lastScoreAt = time.Now().UTC()
	}
	_, err := r.db.ExecContext(ctx, `
INSERT INTO mesh_agent_scores (
  agent_id, task_class, tasks_seen, tasks_won, success_count, failure_count, avg_latency_ms, last_score_at
)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
ON CONFLICT (agent_id, task_class) DO UPDATE SET
  tasks_seen = EXCLUDED.tasks_seen,
  tasks_won = EXCLUDED.tasks_won,
  success_count = EXCLUDED.success_count,
  failure_count = EXCLUDED.failure_count,
  avg_latency_ms = EXCLUDED.avg_latency_ms,
  last_score_at = EXCLUDED.last_score_at
`, score.AgentID, score.TaskClass, score.TasksSeen, score.TasksWon, score.SuccessCount, score.FailureCount, score.AvgLatencyMS, lastScoreAt)
	return err
}
