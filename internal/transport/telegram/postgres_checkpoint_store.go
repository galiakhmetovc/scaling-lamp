package telegram

import (
	"context"
	"database/sql"
	"encoding/json"
	"fmt"

	"teamd/internal/worker"
)

func (s *PostgresStore) Checkpoint(chatID int64) (worker.Checkpoint, bool, error) {
	ctx := context.Background()
	if err := s.ensureSchema(ctx); err != nil {
		return worker.Checkpoint{}, false, err
	}
	session, err := s.ActiveSession(chatID)
	if err != nil {
		return worker.Checkpoint{}, false, err
	}

	var checkpoint worker.Checkpoint
	var unresolved []byte
	var nextActions []byte
	var archiveRefs []byte
	var sourceArtifacts []byte
	err = s.db.QueryRowContext(ctx, `
SELECT compaction_method, what_happened, what_matters_now, unresolved_items, next_actions, archive_refs, source_artifacts
FROM telegram_session_checkpoints
WHERE chat_id = $1 AND session_key = $2
`, chatID, session).Scan(
		&checkpoint.CompactionMethod,
		&checkpoint.WhatHappened,
		&checkpoint.WhatMattersNow,
		&unresolved,
		&nextActions,
		&archiveRefs,
		&sourceArtifacts,
	)
	if err == sql.ErrNoRows {
		return worker.Checkpoint{}, false, nil
	}
	if err != nil {
		return worker.Checkpoint{}, false, err
	}
	checkpoint.SessionID = fmt.Sprintf("telegram:%d/%s", chatID, session)
	if err := json.Unmarshal(unresolved, &checkpoint.UnresolvedItems); err != nil {
		return worker.Checkpoint{}, false, err
	}
	if err := json.Unmarshal(nextActions, &checkpoint.NextActions); err != nil {
		return worker.Checkpoint{}, false, err
	}
	if err := json.Unmarshal(archiveRefs, &checkpoint.ArchiveRefs); err != nil {
		return worker.Checkpoint{}, false, err
	}
	if err := json.Unmarshal(sourceArtifacts, &checkpoint.SourceArtifacts); err != nil {
		return worker.Checkpoint{}, false, err
	}
	return checkpoint, true, nil
}

func (s *PostgresStore) SaveCheckpoint(chatID int64, checkpoint worker.Checkpoint) error {
	ctx := context.Background()
	if err := s.ensureSchema(ctx); err != nil {
		return err
	}
	checkpoint = sanitizeCheckpoint(checkpoint)
	session, err := s.ActiveSession(chatID)
	if err != nil {
		return err
	}

	unresolved, err := json.Marshal(checkpoint.UnresolvedItems)
	if err != nil {
		return err
	}
	nextActions, err := json.Marshal(checkpoint.NextActions)
	if err != nil {
		return err
	}
	archiveRefs, err := json.Marshal(checkpoint.ArchiveRefs)
	if err != nil {
		return err
	}
	sourceArtifacts, err := json.Marshal(checkpoint.SourceArtifacts)
	if err != nil {
		return err
	}

	_, err = s.db.ExecContext(ctx, `
INSERT INTO telegram_session_checkpoints (
  chat_id, session_key, compaction_method, what_happened, what_matters_now, unresolved_items, next_actions, archive_refs, source_artifacts
) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
ON CONFLICT (chat_id, session_key) DO UPDATE SET
  compaction_method = EXCLUDED.compaction_method,
  what_happened = EXCLUDED.what_happened,
  what_matters_now = EXCLUDED.what_matters_now,
  unresolved_items = EXCLUDED.unresolved_items,
  next_actions = EXCLUDED.next_actions,
  archive_refs = EXCLUDED.archive_refs,
  source_artifacts = EXCLUDED.source_artifacts,
  updated_at = NOW()
`, chatID, session, checkpoint.CompactionMethod, checkpoint.WhatHappened, checkpoint.WhatMattersNow, unresolved, nextActions, archiveRefs, sourceArtifacts)
	return err
}
