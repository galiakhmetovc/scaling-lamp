package memory

import (
	"context"
	"database/sql"
	"fmt"
	"strconv"
	"strings"
	"sync"
	"time"
)

type PostgresStore struct {
	db         *sql.DB
	embedder   Embedder
	vectorDims int
	vectorOnce sync.Once
	vectorOK   bool
	vectorErr  error
}

type PostgresStoreOption func(*PostgresStore)

func WithEmbedder(embedder Embedder) PostgresStoreOption {
	return func(s *PostgresStore) {
		s.embedder = embedder
	}
}

func WithVectorDimensions(dims int) PostgresStoreOption {
	return func(s *PostgresStore) {
		if dims > 0 {
			s.vectorDims = dims
		}
	}
}

func NewPostgresStore(db *sql.DB, opts ...PostgresStoreOption) *PostgresStore {
	store := &PostgresStore{db: db, vectorDims: 768}
	for _, opt := range opts {
		opt(store)
	}
	return store
}

func (s *PostgresStore) ensureSchema(ctx context.Context) error {
	const schema = `
CREATE TABLE IF NOT EXISTS memory_documents (
  doc_key TEXT PRIMARY KEY,
  scope TEXT NOT NULL,
  chat_id BIGINT NOT NULL DEFAULT 0,
  session_id TEXT NOT NULL DEFAULT '',
  kind TEXT NOT NULL,
  title TEXT NOT NULL,
  body TEXT NOT NULL,
  source TEXT NOT NULL DEFAULT '',
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_memory_documents_scope_chat_session
  ON memory_documents(scope, chat_id, session_id);
CREATE INDEX IF NOT EXISTS idx_memory_documents_kind
  ON memory_documents(kind);
CREATE INDEX IF NOT EXISTS idx_memory_documents_updated_at
  ON memory_documents(updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_memory_documents_fts
  ON memory_documents USING gin (to_tsvector('simple', coalesce(title, '') || ' ' || coalesce(body, '')));
`
	if _, err := s.db.ExecContext(ctx, schema); err != nil {
		return err
	}
	s.ensureVectorSchema(ctx)
	return nil
}

func (s *PostgresStore) ensureVectorSchema(ctx context.Context) {
	if s.embedder == nil || s.vectorDims <= 0 {
		return
	}
	s.vectorOnce.Do(func() {
		if _, err := s.db.ExecContext(ctx, `CREATE EXTENSION IF NOT EXISTS vector`); err != nil {
			exists, existsErr := s.vectorExtensionExists(ctx)
			if existsErr != nil || !exists {
				s.vectorErr = err
				return
			}
		}
		if _, err := s.db.ExecContext(ctx, fmt.Sprintf(`ALTER TABLE memory_documents ADD COLUMN IF NOT EXISTS embedding vector(%d)`, s.vectorDims)); err != nil {
			s.vectorErr = err
			return
		}
		if _, err := s.db.ExecContext(ctx, `CREATE INDEX IF NOT EXISTS idx_memory_documents_embedding_hnsw ON memory_documents USING hnsw (embedding vector_cosine_ops)`); err != nil {
			s.vectorErr = err
			return
		}
		s.vectorOK = true
	})
}

func (s *PostgresStore) vectorExtensionExists(ctx context.Context) (bool, error) {
	var exists bool
	err := s.db.QueryRowContext(ctx, `SELECT EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'vector')`).Scan(&exists)
	return exists, err
}

func (s *PostgresStore) UpsertDocument(doc Document) error {
	ctx := context.Background()
	if err := s.ensureSchema(ctx); err != nil {
		return err
	}
	if strings.TrimSpace(doc.DocKey) == "" {
		doc.DocKey = strings.TrimSpace(doc.Kind + ":" + doc.SessionID + ":" + doc.Title)
	}
	if doc.UpdatedAt.IsZero() {
		doc.UpdatedAt = time.Now().UTC()
	}
	var embeddingLiteral any
	if s.vectorOK && s.embedder != nil {
		vector, err := s.embedder.Embed(ctx, embeddingInput(doc))
		if err == nil && len(vector) == s.vectorDims {
			embeddingLiteral = vectorLiteral(vector)
		}
	}
	var err error
	if s.vectorOK {
		_, err = s.db.ExecContext(ctx, `
INSERT INTO memory_documents (doc_key, scope, chat_id, session_id, kind, title, body, source, updated_at, embedding)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10::vector)
ON CONFLICT(doc_key) DO UPDATE SET
  scope=excluded.scope,
  chat_id=excluded.chat_id,
  session_id=excluded.session_id,
  kind=excluded.kind,
  title=excluded.title,
  body=excluded.body,
  source=excluded.source,
  updated_at=excluded.updated_at,
  embedding=excluded.embedding
`, doc.DocKey, string(doc.Scope), doc.ChatID, doc.SessionID, doc.Kind, doc.Title, doc.Body, doc.Source, doc.UpdatedAt.UTC(), embeddingLiteral)
		return err
	}
	_, err = s.db.ExecContext(ctx, `
INSERT INTO memory_documents (doc_key, scope, chat_id, session_id, kind, title, body, source, updated_at)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
ON CONFLICT(doc_key) DO UPDATE SET
  scope=excluded.scope,
  chat_id=excluded.chat_id,
  session_id=excluded.session_id,
  kind=excluded.kind,
  title=excluded.title,
  body=excluded.body,
  source=excluded.source,
  updated_at=excluded.updated_at
`, doc.DocKey, string(doc.Scope), doc.ChatID, doc.SessionID, doc.Kind, doc.Title, doc.Body, doc.Source, doc.UpdatedAt.UTC())
	return err
}

func (s *PostgresStore) Search(q RecallQuery) ([]RecallItem, error) {
	ctx := context.Background()
	if err := s.ensureSchema(ctx); err != nil {
		return nil, err
	}
	kinds := NormalizeRecallKinds(q.Kinds)
	limit := q.Limit
	if limit <= 0 {
		limit = 3
	}
	if s.vectorOK && s.embedder != nil && strings.TrimSpace(q.Text) != "" {
		if vector, err := s.embedder.Embed(ctx, strings.TrimSpace(q.Text)); err == nil && len(vector) == s.vectorDims {
			rows, err := s.db.QueryContext(ctx, `
SELECT
  doc_key,
  kind,
  title,
  body,
  1 - (embedding <=> $1::vector) AS score
FROM memory_documents
WHERE
  (
    scope = 'global'
    OR (chat_id = $2 AND (session_id = '' OR session_id = $3))
  )
  AND (
    cardinality($5::text[]) = 0
    OR lower(kind) = ANY($5::text[])
  )
  AND embedding IS NOT NULL
ORDER BY embedding <=> $1::vector, updated_at DESC
LIMIT $4
`, vectorLiteral(vector), q.ChatID, q.SessionID, limit, textArrayLiteral(kinds))
			if err == nil {
				defer rows.Close()
				var out []RecallItem
				for rows.Next() {
					var item RecallItem
					if err := rows.Scan(&item.DocKey, &item.Kind, &item.Title, &item.Body, &item.Score); err != nil {
						return nil, err
					}
					out = append(out, item)
				}
				if err := rows.Err(); err != nil {
					return nil, err
				}
				if len(out) > 0 {
					return out, nil
				}
			}
		}
	}
	rows, err := s.db.QueryContext(ctx, `
SELECT
  doc_key,
  kind,
  title,
  body,
  (
    CASE
      WHEN $1 = '' THEN 1.0
      ELSE ts_rank(
        to_tsvector('simple', coalesce(title, '') || ' ' || coalesce(body, '')),
        websearch_to_tsquery('simple', $1)
      )
    END
  ) AS score
FROM memory_documents
WHERE
  (
    scope = 'global'
    OR (chat_id = $2 AND (session_id = '' OR session_id = $3))
  )
  AND (
    cardinality($5::text[]) = 0
    OR lower(kind) = ANY($5::text[])
  )
  AND (
    $1 = ''
    OR title ILIKE '%' || $1 || '%'
    OR body ILIKE '%' || $1 || '%'
    OR to_tsvector('simple', coalesce(title, '') || ' ' || coalesce(body, '')) @@ websearch_to_tsquery('simple', $1)
  )
ORDER BY score DESC, updated_at DESC
LIMIT $4
`, strings.TrimSpace(q.Text), q.ChatID, q.SessionID, limit, textArrayLiteral(kinds))
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var out []RecallItem
	for rows.Next() {
		var item RecallItem
		if err := rows.Scan(&item.DocKey, &item.Kind, &item.Title, &item.Body, &item.Score); err != nil {
			return nil, err
		}
		out = append(out, item)
	}
	return out, rows.Err()
}

func (s *PostgresStore) Get(docKey string) (Document, bool, error) {
	ctx := context.Background()
	if err := s.ensureSchema(ctx); err != nil {
		return Document{}, false, err
	}
	row := s.db.QueryRowContext(ctx, `
SELECT doc_key, scope, chat_id, session_id, kind, title, body, source, updated_at
FROM memory_documents
WHERE doc_key = $1
`, strings.TrimSpace(docKey))
	var doc Document
	var scope string
	if err := row.Scan(&doc.DocKey, &scope, &doc.ChatID, &doc.SessionID, &doc.Kind, &doc.Title, &doc.Body, &doc.Source, &doc.UpdatedAt); err != nil {
		if err == sql.ErrNoRows {
			return Document{}, false, nil
		}
		return Document{}, false, err
	}
	doc.Scope = Scope(scope)
	return doc, true, nil
}

func (s *PostgresStore) BackfillMissingEmbeddings(ctx context.Context, limit int) error {
	if err := s.ensureSchema(ctx); err != nil {
		return err
	}
	if !s.vectorOK || s.embedder == nil {
		return nil
	}
	if limit <= 0 {
		limit = 128
	}
	rows, err := s.db.QueryContext(ctx, `
SELECT doc_key, title, body
FROM memory_documents
WHERE embedding IS NULL
ORDER BY updated_at DESC
LIMIT $1
`, limit)
	if err != nil {
		return err
	}
	defer rows.Close()

	type rowDoc struct {
		key   string
		title string
		body  string
	}
	var docs []rowDoc
	for rows.Next() {
		var doc rowDoc
		if err := rows.Scan(&doc.key, &doc.title, &doc.body); err != nil {
			return err
		}
		docs = append(docs, doc)
	}
	if err := rows.Err(); err != nil {
		return err
	}
	for _, doc := range docs {
		vector, err := s.embedder.Embed(ctx, embeddingInput(Document{Title: doc.title, Body: doc.body}))
		if err != nil || len(vector) != s.vectorDims {
			continue
		}
		if _, err := s.db.ExecContext(ctx, `UPDATE memory_documents SET embedding = $2::vector WHERE doc_key = $1`, doc.key, vectorLiteral(vector)); err != nil {
			return err
		}
	}
	return nil
}

func embeddingInput(doc Document) string {
	title := strings.TrimSpace(doc.Title)
	body := strings.TrimSpace(doc.Body)
	switch {
	case title == "":
		return body
	case body == "":
		return title
	default:
		return title + "\n\n" + body
	}
}

func vectorLiteral(vector []float32) string {
	var b strings.Builder
	b.Grow(len(vector) * 10)
	b.WriteByte('[')
	for i, value := range vector {
		if i > 0 {
			b.WriteByte(',')
		}
		b.WriteString(strconv.FormatFloat(float64(value), 'f', -1, 32))
	}
	b.WriteByte(']')
	return b.String()
}

func textArrayLiteral(values []string) string {
	if len(values) == 0 {
		return "{}"
	}
	parts := make([]string, 0, len(values))
	for _, value := range values {
		value = strings.ReplaceAll(value, `\`, `\\`)
		value = strings.ReplaceAll(value, `"`, `\"`)
		parts = append(parts, `"`+value+`"`)
	}
	return "{" + strings.Join(parts, ",") + "}"
}
