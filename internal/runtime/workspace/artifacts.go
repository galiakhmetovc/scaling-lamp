package workspace

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"

	"teamd/internal/artifacts"
)

type ArtifactListItem struct {
	Ref       string    `json:"ref"`
	ToolName  string    `json:"tool_name"`
	CreatedAt time.Time `json:"created_at"`
	SizeChars int       `json:"size_chars"`
	SizeBytes int       `json:"size_bytes"`
	Preview   string    `json:"preview"`
}

type ArtifactSnapshot struct {
	SessionID   string             `json:"session_id"`
	RootPath    string             `json:"root_path"`
	Items       []ArtifactListItem `json:"items"`
	SelectedRef string             `json:"selected_ref"`
	Content     string             `json:"content"`
}

type WorkspaceArtifactsManager struct {
	root  string
	store *artifacts.Store
}

func NewWorkspaceArtifactsManager(root string) (*WorkspaceArtifactsManager, error) {
	trimmed := strings.TrimSpace(root)
	if trimmed == "" {
		return nil, fmt.Errorf("artifact root is empty")
	}
	absRoot, err := filepath.Abs(trimmed)
	if err != nil {
		return nil, err
	}
	store, err := artifacts.NewStore(absRoot)
	if err != nil {
		return nil, err
	}
	return &WorkspaceArtifactsManager{root: absRoot, store: store}, nil
}

func (m *WorkspaceArtifactsManager) Snapshot(sessionID string) (ArtifactSnapshot, error) {
	items, err := m.listItems()
	if err != nil {
		return ArtifactSnapshot{}, err
	}
	out := ArtifactSnapshot{
		SessionID: sessionID,
		RootPath:  m.root,
		Items:     items,
	}
	if len(items) == 0 {
		return out, nil
	}
	return m.Open(sessionID, items[0].Ref)
}

func (m *WorkspaceArtifactsManager) Open(sessionID, artifactRef string) (ArtifactSnapshot, error) {
	items, err := m.listItems()
	if err != nil {
		return ArtifactSnapshot{}, err
	}
	out := ArtifactSnapshot{
		SessionID:   sessionID,
		RootPath:    m.root,
		Items:       items,
		SelectedRef: artifactRef,
	}
	if strings.TrimSpace(artifactRef) == "" {
		return out, nil
	}
	_, content, err := m.store.Read(context.Background(), artifactRef)
	if err != nil {
		return ArtifactSnapshot{}, err
	}
	out.Content = content
	return out, nil
}

func (m *WorkspaceArtifactsManager) listItems() ([]ArtifactListItem, error) {
	if m == nil || m.store == nil {
		return nil, fmt.Errorf("workspace artifacts manager is nil")
	}
	entries, err := os.ReadDir(m.root)
	if err != nil {
		return nil, err
	}
	items := make([]ArtifactListItem, 0, len(entries))
	for _, entry := range entries {
		if entry.IsDir() || !strings.HasSuffix(entry.Name(), ".json") {
			continue
		}
		body, err := os.ReadFile(filepath.Join(m.root, entry.Name()))
		if err != nil {
			return nil, err
		}
		var record artifacts.Record
		if err := json.Unmarshal(body, &record); err != nil {
			return nil, err
		}
		items = append(items, ArtifactListItem{
			Ref:       record.Ref,
			ToolName:  record.ToolName,
			CreatedAt: record.CreatedAt,
			SizeChars: record.SizeChars,
			SizeBytes: record.SizeBytes,
			Preview:   record.Preview,
		})
	}
	sort.Slice(items, func(i, j int) bool {
		if items[i].CreatedAt.Equal(items[j].CreatedAt) {
			return items[i].Ref > items[j].Ref
		}
		return items[i].CreatedAt.After(items[j].CreatedAt)
	})
	return items, nil
}
