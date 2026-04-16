package artifacts

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"
)

const refPrefix = "artifact://"

type Record struct {
	ID        string    `json:"id"`
	Ref       string    `json:"ref"`
	ToolName  string    `json:"tool_name"`
	CreatedAt time.Time `json:"created_at"`
	Path      string    `json:"path"`
	SizeBytes int       `json:"size_bytes"`
	SizeChars int       `json:"size_chars"`
	Preview   string    `json:"preview"`
}

type SearchResult struct {
	Ref       string `json:"ref"`
	ToolName  string `json:"tool_name"`
	SizeChars int    `json:"size_chars"`
	Preview   string `json:"preview"`
}

type Store struct {
	rootPath string
}

func NewStore(rootPath string) (*Store, error) {
	if rootPath == "" {
		return nil, fmt.Errorf("artifact store root path is empty")
	}
	if err := os.MkdirAll(rootPath, 0o755); err != nil {
		return nil, fmt.Errorf("mkdir artifact root: %w", err)
	}
	return &Store{rootPath: rootPath}, nil
}

func (s *Store) Write(ctx context.Context, toolName, content string, previewChars int) (Record, error) {
	if err := ctx.Err(); err != nil {
		return Record{}, err
	}
	if s == nil {
		return Record{}, fmt.Errorf("artifact store is nil")
	}
	id := fmt.Sprintf("%d", time.Now().UTC().UnixNano())
	record := Record{
		ID:        id,
		Ref:       refPrefix + id,
		ToolName:  toolName,
		CreatedAt: time.Now().UTC(),
		Path:      s.contentPath(id),
		SizeBytes: len([]byte(content)),
		SizeChars: len(content),
		Preview:   preview(content, previewChars),
	}
	if err := os.WriteFile(record.Path, []byte(content), 0o644); err != nil {
		return Record{}, fmt.Errorf("write artifact content: %w", err)
	}
	body, err := json.MarshalIndent(record, "", "  ")
	if err != nil {
		return Record{}, fmt.Errorf("encode artifact metadata: %w", err)
	}
	if err := os.WriteFile(s.metadataPath(id), body, 0o644); err != nil {
		return Record{}, fmt.Errorf("write artifact metadata: %w", err)
	}
	return record, nil
}

func (s *Store) Read(ctx context.Context, ref string) (Record, string, error) {
	if err := ctx.Err(); err != nil {
		return Record{}, "", err
	}
	if s == nil {
		return Record{}, "", fmt.Errorf("artifact store is nil")
	}
	id, err := artifactID(ref)
	if err != nil {
		return Record{}, "", err
	}
	record, err := s.readRecord(id)
	if err != nil {
		return Record{}, "", err
	}
	body, err := os.ReadFile(s.contentPath(id))
	if err != nil {
		return Record{}, "", fmt.Errorf("read artifact content: %w", err)
	}
	return record, string(body), nil
}

func (s *Store) Search(ctx context.Context, query string, limit int) ([]SearchResult, error) {
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	if s == nil {
		return nil, fmt.Errorf("artifact store is nil")
	}
	if query == "" {
		return nil, fmt.Errorf("artifact search query is empty")
	}
	if limit <= 0 {
		limit = 10
	}
	entries, err := os.ReadDir(s.rootPath)
	if err != nil {
		return nil, fmt.Errorf("read artifact root: %w", err)
	}
	query = strings.ToLower(query)
	results := make([]SearchResult, 0, limit)
	for _, entry := range entries {
		if entry.IsDir() || !strings.HasSuffix(entry.Name(), ".json") {
			continue
		}
		id := strings.TrimSuffix(entry.Name(), ".json")
		record, err := s.readRecord(id)
		if err != nil {
			return nil, err
		}
		content, err := os.ReadFile(s.contentPath(id))
		if err != nil {
			return nil, fmt.Errorf("read artifact content: %w", err)
		}
		haystack := strings.ToLower(record.ToolName + "\n" + record.Preview + "\n" + string(content))
		if !strings.Contains(haystack, query) {
			continue
		}
		results = append(results, SearchResult{
			Ref:       record.Ref,
			ToolName:  record.ToolName,
			SizeChars: record.SizeChars,
			Preview:   record.Preview,
		})
		if len(results) >= limit {
			break
		}
	}
	sort.Slice(results, func(i, j int) bool { return results[i].Ref < results[j].Ref })
	return results, nil
}

func (s *Store) contentPath(id string) string {
	return filepath.Join(s.rootPath, id+".txt")
}

func (s *Store) metadataPath(id string) string {
	return filepath.Join(s.rootPath, id+".json")
}

func (s *Store) readRecord(id string) (Record, error) {
	body, err := os.ReadFile(s.metadataPath(id))
	if err != nil {
		return Record{}, fmt.Errorf("read artifact metadata: %w", err)
	}
	var record Record
	if err := json.Unmarshal(body, &record); err != nil {
		return Record{}, fmt.Errorf("decode artifact metadata: %w", err)
	}
	return record, nil
}

func artifactID(ref string) (string, error) {
	if !strings.HasPrefix(ref, refPrefix) {
		return "", fmt.Errorf("artifact ref %q must start with %q", ref, refPrefix)
	}
	id := strings.TrimPrefix(ref, refPrefix)
	if id == "" {
		return "", fmt.Errorf("artifact ref %q is missing id", ref)
	}
	return id, nil
}

func preview(content string, limit int) string {
	if limit <= 0 || len(content) <= limit {
		return content
	}
	head := limit / 2
	tail := limit - head
	if tail > len(content) {
		tail = len(content)
	}
	return content[:head] + "\n...[offloaded]...\n" + content[len(content)-tail:]
}
