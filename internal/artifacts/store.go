package artifacts

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"sync"
	"time"
)

type Artifact struct {
	Ref       string    `json:"ref"`
	Name      string    `json:"name"`
	OwnerType string    `json:"owner_type"`
	OwnerID   string    `json:"owner_id"`
	Payload   []byte    `json:"-"`
	CreatedAt time.Time `json:"created_at"`
}

type SearchQuery struct {
	OwnerType string
	OwnerID   string
	Query     string
	Limit     int
	Global    bool
}

type Store interface {
	Save(ownerType, ownerID, name string, payload []byte) (string, error)
	Get(ref string) (Artifact, bool, error)
	Search(query SearchQuery) ([]Artifact, error)
}

type InMemoryStore struct {
	data  map[string]Artifact
	order []string
}

type FilesystemStore struct {
	root string
	mu   sync.Mutex
}

type artifactMeta struct {
	Ref       string    `json:"ref"`
	Name      string    `json:"name"`
	OwnerType string    `json:"owner_type"`
	OwnerID   string    `json:"owner_id"`
	CreatedAt time.Time `json:"created_at"`
}

func NewInMemoryStore() *InMemoryStore {
	return &InMemoryStore{data: map[string]Artifact{}}
}

func NewFilesystemStore(root string) (*FilesystemStore, error) {
	root = strings.TrimSpace(root)
	if root == "" {
		return nil, fmt.Errorf("artifact root is required")
	}
	if err := os.MkdirAll(root, 0o755); err != nil {
		return nil, err
	}
	return &FilesystemStore{root: root}, nil
}

func (s *InMemoryStore) Save(ownerType, ownerID, name string, payload []byte) (string, error) {
	ref := fmt.Sprintf("artifact://%s", name)
	if s.data == nil {
		s.data = map[string]Artifact{}
	}
	if _, exists := s.data[ref]; !exists {
		s.order = append(s.order, ref)
	}
	s.data[ref] = Artifact{
		Ref:       ref,
		Name:      name,
		OwnerType: strings.TrimSpace(ownerType),
		OwnerID:   strings.TrimSpace(ownerID),
		Payload:   append([]byte(nil), payload...),
		CreatedAt: time.Now().UTC(),
	}
	return ref, nil
}

func (s *FilesystemStore) Save(ownerType, ownerID, name string, payload []byte) (string, error) {
	s.mu.Lock()
	defer s.mu.Unlock()

	now := time.Now().UTC()
	key := fmt.Sprintf("%d-%s", now.UnixNano(), sanitizeName(name))
	ref := "artifact://" + key
	meta := artifactMeta{
		Ref:       ref,
		Name:      name,
		OwnerType: strings.TrimSpace(ownerType),
		OwnerID:   strings.TrimSpace(ownerID),
		CreatedAt: now,
	}
	if err := os.WriteFile(s.metaPath(key), mustJSON(meta), 0o644); err != nil {
		return "", err
	}
	if err := os.WriteFile(s.payloadPath(key), payload, 0o644); err != nil {
		return "", err
	}
	return ref, nil
}

func (s *InMemoryStore) Get(ref string) (Artifact, bool, error) {
	item, ok := s.data[ref]
	if !ok {
		return Artifact{}, false, nil
	}
	item.Payload = append([]byte(nil), item.Payload...)
	return item, true, nil
}

func (s *FilesystemStore) Get(ref string) (Artifact, bool, error) {
	key, ok := artifactKey(ref)
	if !ok {
		return Artifact{}, false, nil
	}
	meta, ok, err := s.readMeta(key)
	if err != nil || !ok {
		return Artifact{}, ok, err
	}
	payload, err := os.ReadFile(s.payloadPath(key))
	if err != nil {
		if os.IsNotExist(err) {
			return Artifact{}, false, nil
		}
		return Artifact{}, false, err
	}
	return Artifact{
		Ref:       meta.Ref,
		Name:      meta.Name,
		OwnerType: meta.OwnerType,
		OwnerID:   meta.OwnerID,
		Payload:   payload,
		CreatedAt: meta.CreatedAt,
	}, true, nil
}

func (s *InMemoryStore) Search(query SearchQuery) ([]Artifact, error) {
	limit := normalizeLimit(query.Limit)
	needle := strings.ToLower(strings.TrimSpace(query.Query))
	out := make([]Artifact, 0, limit)
	for i := len(s.order) - 1; i >= 0; i-- {
		item := s.data[s.order[i]]
		if !matchesScope(item, query) {
			continue
		}
		if needle != "" && !artifactMatches(item, needle) {
			continue
		}
		item.Payload = append([]byte(nil), item.Payload...)
		out = append(out, item)
		if len(out) == limit {
			break
		}
	}
	return out, nil
}

func (s *FilesystemStore) Search(query SearchQuery) ([]Artifact, error) {
	entries, err := os.ReadDir(s.root)
	if err != nil {
		return nil, err
	}
	needle := strings.ToLower(strings.TrimSpace(query.Query))
	items := make([]Artifact, 0, len(entries))
	for _, entry := range entries {
		if entry.IsDir() || filepath.Ext(entry.Name()) != ".json" {
			continue
		}
		key := strings.TrimSuffix(entry.Name(), ".json")
		meta, ok, err := s.readMeta(key)
		if err != nil || !ok {
			return nil, err
		}
		item := Artifact{
			Ref:       meta.Ref,
			Name:      meta.Name,
			OwnerType: meta.OwnerType,
			OwnerID:   meta.OwnerID,
			CreatedAt: meta.CreatedAt,
		}
		if !matchesScope(item, query) {
			continue
		}
		if needle != "" {
			payload, err := os.ReadFile(s.payloadPath(key))
			if err != nil {
				return nil, err
			}
			item.Payload = payload
			if !artifactMatches(item, needle) {
				continue
			}
		}
		items = append(items, item)
	}
	sort.Slice(items, func(i, j int) bool {
		return items[i].CreatedAt.After(items[j].CreatedAt)
	})
	limit := normalizeLimit(query.Limit)
	if len(items) > limit {
		items = items[:limit]
	}
	return items, nil
}

func (s *FilesystemStore) metaPath(key string) string {
	return filepath.Join(s.root, key+".json")
}

func (s *FilesystemStore) payloadPath(key string) string {
	return filepath.Join(s.root, key+".bin")
}

func (s *FilesystemStore) readMeta(key string) (artifactMeta, bool, error) {
	data, err := os.ReadFile(s.metaPath(key))
	if err != nil {
		if os.IsNotExist(err) {
			return artifactMeta{}, false, nil
		}
		return artifactMeta{}, false, err
	}
	var meta artifactMeta
	if err := json.Unmarshal(data, &meta); err != nil {
		return artifactMeta{}, false, err
	}
	return meta, true, nil
}

func artifactMatches(item Artifact, needle string) bool {
	if needle == "" {
		return true
	}
	if strings.Contains(strings.ToLower(item.Ref), needle) || strings.Contains(strings.ToLower(item.Name), needle) {
		return true
	}
	return strings.Contains(strings.ToLower(string(item.Payload)), needle)
}

func matchesScope(item Artifact, query SearchQuery) bool {
	if query.Global {
		return true
	}
	if ownerType := strings.TrimSpace(query.OwnerType); ownerType != "" && item.OwnerType != ownerType {
		return false
	}
	if ownerID := strings.TrimSpace(query.OwnerID); ownerID != "" && item.OwnerID != ownerID {
		return false
	}
	return true
}

func normalizeLimit(limit int) int {
	if limit <= 0 {
		return 20
	}
	return limit
}

func sanitizeName(name string) string {
	name = strings.TrimSpace(name)
	if name == "" {
		return "artifact"
	}
	replacer := strings.NewReplacer("/", "-", "\\", "-", " ", "-", ":", "-", "\n", "-")
	name = replacer.Replace(name)
	name = strings.Trim(name, "-.")
	if name == "" {
		return "artifact"
	}
	return name
}

func artifactKey(ref string) (string, bool) {
	const prefix = "artifact://"
	if !strings.HasPrefix(ref, prefix) {
		return "", false
	}
	key := strings.TrimSpace(strings.TrimPrefix(ref, prefix))
	if key == "" {
		return "", false
	}
	return key, true
}

func mustJSON(v any) []byte {
	data, err := json.Marshal(v)
	if err != nil {
		panic(err)
	}
	return data
}
