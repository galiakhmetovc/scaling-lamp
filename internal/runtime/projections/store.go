package projections

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"time"
)

type Store interface {
	Save([]Projection) error
	Load([]Projection) error
}

type JSONFileStore struct {
	path string
}

func NewJSONFileStore(path string) (*JSONFileStore, error) {
	if path == "" {
		return nil, fmt.Errorf("projection store path is empty")
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return nil, fmt.Errorf("mkdir projection store dir: %w", err)
	}
	return &JSONFileStore{path: path}, nil
}

func (s *JSONFileStore) Save(projectionSet []Projection) error {
	raw := make(map[string]json.RawMessage, len(projectionSet))
	for _, projection := range projectionSet {
		encoded, err := json.Marshal(projection.SnapshotValue())
		if err != nil {
			return fmt.Errorf("marshal projection %q snapshot: %w", projection.ID(), err)
		}
		raw[projection.ID()] = encoded
	}
	body, err := json.MarshalIndent(raw, "", "  ")
	if err != nil {
		return fmt.Errorf("marshal projection store body: %w", err)
	}
	if err := os.WriteFile(s.path, body, 0o644); err != nil {
		return fmt.Errorf("write projection store: %w", err)
	}
	return nil
}

func (s *JSONFileStore) Load(projectionSet []Projection) error {
	body, err := os.ReadFile(s.path)
	if err != nil {
		if os.IsNotExist(err) {
			return nil
		}
		return fmt.Errorf("read projection store: %w", err)
	}

	var raw map[string]json.RawMessage
	if err := json.Unmarshal(body, &raw); err != nil {
		if recoverErr := s.quarantineCorruptStore(body); recoverErr != nil {
			return fmt.Errorf("decode projection store: %w (quarantine failed: %v)", err, recoverErr)
		}
		return nil
	}

	for _, projection := range projectionSet {
		snapshot, ok := raw[projection.ID()]
		if !ok {
			continue
		}
		if err := projection.RestoreSnapshot(snapshot); err != nil {
			return fmt.Errorf("restore projection %q: %w", projection.ID(), err)
		}
	}
	return nil
}

func (s *JSONFileStore) quarantineCorruptStore(body []byte) error {
	corruptPath := fmt.Sprintf("%s.corrupt-%d", s.path, time.Now().UTC().UnixNano())
	if err := os.WriteFile(corruptPath, body, 0o644); err != nil {
		return fmt.Errorf("write corrupt projection backup: %w", err)
	}
	if err := os.Remove(s.path); err != nil && !os.IsNotExist(err) {
		return fmt.Errorf("remove corrupt projection store: %w", err)
	}
	return nil
}
