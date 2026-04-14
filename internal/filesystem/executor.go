package filesystem

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	"teamd/internal/contracts"
)

type Executor struct{}

func NewExecutor() *Executor {
	return &Executor{}
}

func (e *Executor) Execute(contract contracts.FilesystemExecutionContract, toolName string, args map[string]any) (string, error) {
	if e == nil {
		return "", fmt.Errorf("filesystem executor is nil")
	}
	switch toolName {
	case "fs_write_text":
		rawPath, err := stringValue(args, "path")
		if err != nil {
			return "", err
		}
		path, err := e.resolveWritePath(contract.Scope, rawPath)
		if err != nil {
			return "", err
		}
		content, err := stringValue(args, "content")
		if err != nil {
			return "", err
		}
		if contract.IO.Enabled && contract.IO.Strategy == "bounded_text_io" && contract.IO.Params.MaxWriteBytes > 0 && len(content) > contract.IO.Params.MaxWriteBytes {
			return "", fmt.Errorf("write content exceeds max_write_bytes")
		}
		if !allowWrites(contract.Mutation) {
			return "", fmt.Errorf("filesystem writes are denied by mutation policy")
		}
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			return "", fmt.Errorf("create parent dirs: %w", err)
		}
		if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
			return "", fmt.Errorf("write file: %w", err)
		}
		return jsonString(map[string]any{"status": "ok", "tool": toolName, "path": path, "bytes": len(content), "changed": true}), nil
	case "fs_trash":
		rawPath, err := stringValue(args, "path")
		if err != nil {
			return "", err
		}
		path, err := e.resolveWritePath(contract.Scope, rawPath)
		if err != nil {
			return "", err
		}
		if contract.Mutation.Strategy != "trash_only_delete" {
			return "", fmt.Errorf("filesystem trash is denied by mutation policy")
		}
		root, err := rootPath(contract.Scope)
		if err != nil {
			return "", err
		}
		trashDir := contract.Mutation.Params.TrashDir
		if trashDir == "" {
			trashDir = ".trash"
		}
		trashRoot := filepath.Join(root, trashDir)
		if err := os.MkdirAll(trashRoot, 0o755); err != nil {
			return "", fmt.Errorf("create trash dir: %w", err)
		}
		target := filepath.Join(trashRoot, fmt.Sprintf("%d-%s", time.Now().UTC().UnixNano(), filepath.Base(path)))
		if err := os.Rename(path, target); err != nil {
			return "", fmt.Errorf("move to trash: %w", err)
		}
		return jsonString(map[string]any{"status": "ok", "tool": toolName, "path": path, "trashed_to": target}), nil
	default:
		return "", fmt.Errorf("filesystem tool %q is not implemented", toolName)
	}
}

func (e *Executor) resolveWritePath(policy contracts.FilesystemScopePolicy, raw string) (string, error) {
	if raw == "" {
		return "", fmt.Errorf("path is required")
	}
	root, err := rootPath(policy)
	if err != nil {
		return "", err
	}
	target := raw
	if !filepath.IsAbs(target) {
		target = filepath.Join(root, target)
	}
	target = filepath.Clean(target)
	if !within(root, target) {
		return "", fmt.Errorf("path %q escapes filesystem scope", raw)
	}
	if len(policy.Params.WriteSubpaths) > 0 {
		allowed := false
		for _, sub := range policy.Params.WriteSubpaths {
			if within(filepath.Join(root, sub), target) {
				allowed = true
				break
			}
		}
		if !allowed {
			return "", fmt.Errorf("path %q is outside write scope", raw)
		}
	}
	return target, nil
}

func rootPath(policy contracts.FilesystemScopePolicy) (string, error) {
	root := policy.Params.RootPath
	if root == "" {
		root = "."
	}
	abs, err := filepath.Abs(root)
	if err != nil {
		return "", fmt.Errorf("resolve root path: %w", err)
	}
	return abs, nil
}

func within(root, target string) bool {
	rel, err := filepath.Rel(root, target)
	if err != nil {
		return false
	}
	return rel != ".." && !strings.HasPrefix(rel, ".."+string(filepath.Separator))
}

func allowWrites(policy contracts.FilesystemMutationPolicy) bool {
	if !policy.Enabled {
		return true
	}
	if policy.Strategy != "allow_writes" {
		return false
	}
	return policy.Params.AllowWrite
}

func stringValue(args map[string]any, key string) (string, error) {
	value, ok := args[key]
	if !ok {
		return "", fmt.Errorf("missing required argument %q", key)
	}
	text, ok := value.(string)
	if !ok || text == "" {
		return "", fmt.Errorf("argument %q must be a non-empty string", key)
	}
	return text, nil
}

func jsonString(value any) string {
	data, _ := json.Marshal(value)
	return string(data)
}
