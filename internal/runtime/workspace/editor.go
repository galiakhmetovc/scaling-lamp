package workspace

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"
)

type EditorBuffer struct {
	SessionID string
	Path      string
	Content   string
	Dirty     bool
}

type WorkspaceEditorManager struct {
	mu      sync.Mutex
	root    string
	buffers map[string]EditorBuffer
}

func NewWorkspaceEditorManager(root string) (*WorkspaceEditorManager, error) {
	trimmed := strings.TrimSpace(root)
	if trimmed == "" {
		return nil, fmt.Errorf("editor root is empty")
	}
	absRoot, err := filepath.Abs(trimmed)
	if err != nil {
		return nil, err
	}
	info, err := os.Stat(absRoot)
	if err != nil {
		return nil, err
	}
	if !info.IsDir() {
		return nil, fmt.Errorf("workspace root %q is not a directory", absRoot)
	}
	return &WorkspaceEditorManager{
		root:    absRoot,
		buffers: map[string]EditorBuffer{},
	}, nil
}

func (m *WorkspaceEditorManager) Open(sessionID, relPath string) (EditorBuffer, error) {
	normalized, err := m.normalize(relPath)
	if err != nil {
		return EditorBuffer{}, err
	}
	key := m.bufferKey(sessionID, normalized)

	m.mu.Lock()
	defer m.mu.Unlock()

	if buf, ok := m.buffers[key]; ok {
		return buf, nil
	}
	content, err := os.ReadFile(m.absPath(normalized))
	if err != nil {
		return EditorBuffer{}, err
	}
	buf := EditorBuffer{
		SessionID: sessionID,
		Path:      normalized,
		Content:   string(content),
		Dirty:     false,
	}
	m.buffers[key] = buf
	return buf, nil
}

func (m *WorkspaceEditorManager) Update(sessionID, relPath, content string) (EditorBuffer, error) {
	normalized, err := m.normalize(relPath)
	if err != nil {
		return EditorBuffer{}, err
	}
	key := m.bufferKey(sessionID, normalized)

	m.mu.Lock()
	defer m.mu.Unlock()

	buf, ok := m.buffers[key]
	if !ok {
		if _, err := os.ReadFile(m.absPath(normalized)); err != nil {
			return EditorBuffer{}, err
		}
		buf = EditorBuffer{SessionID: sessionID, Path: normalized}
	}
	buf.SessionID = sessionID
	buf.Path = normalized
	buf.Content = content
	buf.Dirty = true
	m.buffers[key] = buf
	return buf, nil
}

func (m *WorkspaceEditorManager) Save(sessionID, relPath string) (EditorBuffer, error) {
	normalized, err := m.normalize(relPath)
	if err != nil {
		return EditorBuffer{}, err
	}
	key := m.bufferKey(sessionID, normalized)

	m.mu.Lock()
	defer m.mu.Unlock()

	buf, ok := m.buffers[key]
	if !ok {
		var openErr error
		buf, openErr = m.openLocked(sessionID, normalized)
		if openErr != nil {
			return EditorBuffer{}, openErr
		}
	}
	if err := os.WriteFile(m.absPath(normalized), []byte(buf.Content), 0o644); err != nil {
		return EditorBuffer{}, err
	}
	buf.Dirty = false
	m.buffers[key] = buf
	return buf, nil
}

func (m *WorkspaceEditorManager) Current(sessionID, relPath string) (EditorBuffer, bool) {
	normalized, err := m.normalize(relPath)
	if err != nil {
		return EditorBuffer{}, false
	}
	m.mu.Lock()
	defer m.mu.Unlock()
	buf, ok := m.buffers[m.bufferKey(sessionID, normalized)]
	return buf, ok
}

func (m *WorkspaceEditorManager) normalize(relPath string) (string, error) {
	if m == nil {
		return "", fmt.Errorf("workspace editor manager is nil")
	}
	trimmed := strings.TrimSpace(relPath)
	if trimmed == "" {
		return "", fmt.Errorf("path is empty")
	}
	if filepath.IsAbs(trimmed) {
		return "", fmt.Errorf("path %q escapes workspace root", relPath)
	}
	cleaned := filepath.Clean(filepath.FromSlash(trimmed))
	joined := filepath.Join(m.root, cleaned)
	rel, err := filepath.Rel(m.root, joined)
	if err != nil {
		return "", err
	}
	rel = filepath.ToSlash(filepath.Clean(rel))
	if rel == ".." || strings.HasPrefix(rel, "../") {
		return "", fmt.Errorf("path %q escapes workspace root", relPath)
	}
	if rel == "." {
		return "", fmt.Errorf("path is empty")
	}
	return rel, nil
}

func (m *WorkspaceEditorManager) bufferKey(sessionID, relPath string) string {
	return sessionID + "\x00" + relPath
}

func (m *WorkspaceEditorManager) absPath(relPath string) string {
	if relPath == "." {
		return m.root
	}
	return filepath.Join(m.root, filepath.FromSlash(relPath))
}

func (m *WorkspaceEditorManager) openLocked(sessionID, relPath string) (EditorBuffer, error) {
	content, err := os.ReadFile(m.absPath(relPath))
	if err != nil {
		return EditorBuffer{}, err
	}
	buf := EditorBuffer{
		SessionID: sessionID,
		Path:      relPath,
		Content:   string(content),
	}
	m.buffers[m.bufferKey(sessionID, relPath)] = buf
	return buf, nil
}
