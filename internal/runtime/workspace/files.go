package workspace

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"sync"
	"time"
)

type FileNode struct {
	Path           string    `json:"path"`
	Name           string    `json:"name"`
	IsDir          bool      `json:"is_dir"`
	Size           int64     `json:"size"`
	ModTime        time.Time `json:"mod_time"`
	Expanded       bool      `json:"expanded"`
	ChildrenLoaded bool      `json:"children_loaded"`
}

type FileTreeSnapshot struct {
	SessionID string     `json:"session_id"`
	RootPath  string     `json:"root_path"`
	Items     []FileNode `json:"items"`
}

type WorkspaceFilesManager struct {
	mu   sync.RWMutex
	root string
}

func NewWorkspaceFilesManager(root string) (*WorkspaceFilesManager, error) {
	trimmed := strings.TrimSpace(root)
	if trimmed == "" {
		trimmed = "."
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
	return &WorkspaceFilesManager{root: absRoot}, nil
}

func (m *WorkspaceFilesManager) Snapshot(sessionID string) (FileTreeSnapshot, error) {
	if sessionID == "" {
		return FileTreeSnapshot{}, fmt.Errorf("session id is empty")
	}
	items, err := m.listDir(".")
	if err != nil {
		return FileTreeSnapshot{}, err
	}
	return FileTreeSnapshot{SessionID: sessionID, RootPath: m.root, Items: items}, nil
}

func (m *WorkspaceFilesManager) Expand(sessionID, relPath string) (FileTreeSnapshot, error) {
	if sessionID == "" {
		return FileTreeSnapshot{}, fmt.Errorf("session id is empty")
	}
	normalized, err := m.Normalize(relPath)
	if err != nil {
		return FileTreeSnapshot{}, err
	}
	items, err := m.listDir(".")
	if err != nil {
		return FileTreeSnapshot{}, err
	}
	if normalized != "." {
		expanded, err := m.nodeForRel(normalized)
		if err != nil {
			return FileTreeSnapshot{}, err
		}
		if !expanded.IsDir {
			return FileTreeSnapshot{}, fmt.Errorf("%q is not a directory", relPath)
		}
		children, err := m.listDir(normalized)
		if err != nil {
			return FileTreeSnapshot{}, err
		}
		items = insertExpandedNode(items, normalized, expanded, children)
	}
	return FileTreeSnapshot{SessionID: sessionID, RootPath: m.root, Items: items}, nil
}

func (m *WorkspaceFilesManager) Stat(relPath string) (FileNode, error) {
	normalized, err := m.Normalize(relPath)
	if err != nil {
		return FileNode{}, err
	}
	return m.nodeForRel(normalized)
}

func (m *WorkspaceFilesManager) Normalize(relPath string) (string, error) {
	if m == nil {
		return "", fmt.Errorf("workspace files manager is nil")
	}
	trimmed := strings.TrimSpace(relPath)
	if trimmed == "" || trimmed == "." {
		return ".", nil
	}
	if filepath.IsAbs(trimmed) {
		return "", fmt.Errorf("path %q escapes workspace root", relPath)
	}
	cleaned := filepath.Clean(filepath.FromSlash(trimmed))
	if cleaned == "." {
		return ".", nil
	}
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
		return ".", nil
	}
	return rel, nil
}

func (m *WorkspaceFilesManager) listDir(relPath string) ([]FileNode, error) {
	normalized, err := m.Normalize(relPath)
	if err != nil {
		return nil, err
	}
	dirPath := m.root
	if normalized != "." {
		dirPath = filepath.Join(m.root, filepath.FromSlash(normalized))
	}
	entries, err := os.ReadDir(dirPath)
	if err != nil {
		return nil, err
	}
	out := make([]FileNode, 0, len(entries))
	for _, entry := range entries {
		node, err := m.nodeFromEntry(normalized, entry)
		if err != nil {
			return nil, err
		}
		out = append(out, node)
	}
	sortNodes(out)
	return out, nil
}

func (m *WorkspaceFilesManager) nodeForRel(relPath string) (FileNode, error) {
	normalized, err := m.Normalize(relPath)
	if err != nil {
		return FileNode{}, err
	}
	absPath := m.root
	if normalized != "." {
		absPath = filepath.Join(m.root, filepath.FromSlash(normalized))
	}
	info, err := os.Stat(absPath)
	if err != nil {
		return FileNode{}, err
	}
	name := filepath.Base(absPath)
	if normalized == "." {
		name = filepath.Base(m.root)
	}
	return FileNode{
		Path:    normalized,
		Name:    name,
		IsDir:   info.IsDir(),
		Size:    info.Size(),
		ModTime: info.ModTime(),
	}, nil
}

func (m *WorkspaceFilesManager) nodeFromEntry(parentRel string, entry os.DirEntry) (FileNode, error) {
	info, err := entry.Info()
	if err != nil {
		return FileNode{}, err
	}
	rel := entry.Name()
	if parentRel != "." {
		rel = filepath.ToSlash(filepath.Join(parentRel, entry.Name()))
	}
	return FileNode{
		Path:    rel,
		Name:    entry.Name(),
		IsDir:   entry.IsDir(),
		Size:    info.Size(),
		ModTime: info.ModTime(),
	}, nil
}

func insertExpandedNode(items []FileNode, expandedPath string, expanded FileNode, children []FileNode) []FileNode {
	out := make([]FileNode, 0, len(items)+len(children))
	for _, item := range items {
		if item.Path == expandedPath {
			item.Expanded = true
			item.ChildrenLoaded = true
			out = append(out, item)
			out = append(out, children...)
			continue
		}
		out = append(out, item)
	}
	return out
}

func sortNodes(items []FileNode) {
	sort.SliceStable(items, func(i, j int) bool {
		if items[i].IsDir != items[j].IsDir {
			return items[i].IsDir
		}
		if items[i].Name == items[j].Name {
			return items[i].Path < items[j].Path
		}
		return items[i].Name < items[j].Name
	})
}
