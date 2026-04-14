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
	case "fs_list":
		rawPath, err := stringValue(args, "path")
		if err != nil {
			return "", err
		}
		path, err := e.resolveReadPath(contract.Scope, rawPath)
		if err != nil {
			return "", err
		}
		entries, err := os.ReadDir(path)
		if err != nil {
			return "", fmt.Errorf("read dir: %w", err)
		}
		out := make([]map[string]any, 0, len(entries))
		for _, entry := range entries {
			out = append(out, map[string]any{
				"name":   entry.Name(),
				"is_dir": entry.IsDir(),
			})
		}
		return jsonString(map[string]any{"status": "ok", "tool": toolName, "path": path, "entries": out}), nil
	case "fs_read_text":
		rawPath, err := stringValue(args, "path")
		if err != nil {
			return "", err
		}
		path, err := e.resolveReadPath(contract.Scope, rawPath)
		if err != nil {
			return "", err
		}
		data, err := os.ReadFile(path)
		if err != nil {
			return "", fmt.Errorf("read file: %w", err)
		}
		if contract.IO.Enabled && contract.IO.Strategy == "bounded_text_io" && contract.IO.Params.MaxReadBytes > 0 && len(data) > contract.IO.Params.MaxReadBytes {
			return "", fmt.Errorf("read content exceeds max_read_bytes")
		}
		return jsonString(map[string]any{"status": "ok", "tool": toolName, "path": path, "content": string(data), "bytes": len(data)}), nil
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
	case "fs_patch_text":
		rawPath, err := stringValue(args, "path")
		if err != nil {
			return "", err
		}
		path, err := e.resolveWritePath(contract.Scope, rawPath)
		if err != nil {
			return "", err
		}
		search, err := stringValue(args, "search")
		if err != nil {
			return "", err
		}
		replace, err := stringValue(args, "replace")
		if err != nil {
			return "", err
		}
		if !allowWrites(contract.Mutation) {
			return "", fmt.Errorf("filesystem writes are denied by mutation policy")
		}
		data, err := os.ReadFile(path)
		if err != nil {
			return "", fmt.Errorf("read file for patch: %w", err)
		}
		updated := strings.Replace(string(data), search, replace, 1)
		if updated == string(data) {
			return "", fmt.Errorf("search text not found in file")
		}
		if contract.IO.Enabled && contract.IO.Strategy == "bounded_text_io" && contract.IO.Params.MaxWriteBytes > 0 && len(updated) > contract.IO.Params.MaxWriteBytes {
			return "", fmt.Errorf("patched content exceeds max_write_bytes")
		}
		if err := os.WriteFile(path, []byte(updated), 0o644); err != nil {
			return "", fmt.Errorf("write patched file: %w", err)
		}
		return jsonString(map[string]any{"status": "ok", "tool": toolName, "path": path, "changed": true}), nil
	case "fs_mkdir":
		rawPath, err := stringValue(args, "path")
		if err != nil {
			return "", err
		}
		path, err := e.resolveWritePath(contract.Scope, rawPath)
		if err != nil {
			return "", err
		}
		if !allowMkdir(contract.Mutation) {
			return "", fmt.Errorf("filesystem mkdir is denied by mutation policy")
		}
		if err := os.MkdirAll(path, 0o755); err != nil {
			return "", fmt.Errorf("mkdir: %w", err)
		}
		return jsonString(map[string]any{"status": "ok", "tool": toolName, "path": path, "changed": true}), nil
	case "fs_move":
		rawSrc, err := stringValue(args, "src")
		if err != nil {
			return "", err
		}
		rawDest, err := stringValue(args, "dest")
		if err != nil {
			return "", err
		}
		src, err := e.resolveWritePath(contract.Scope, rawSrc)
		if err != nil {
			return "", err
		}
		dest, err := e.resolveWritePath(contract.Scope, rawDest)
		if err != nil {
			return "", err
		}
		if !allowMove(contract.Mutation) {
			return "", fmt.Errorf("filesystem move is denied by mutation policy")
		}
		if err := os.MkdirAll(filepath.Dir(dest), 0o755); err != nil {
			return "", fmt.Errorf("create move destination dirs: %w", err)
		}
		if err := os.Rename(src, dest); err != nil {
			return "", fmt.Errorf("move path: %w", err)
		}
		return jsonString(map[string]any{"status": "ok", "tool": toolName, "src": src, "dest": dest, "changed": true}), nil
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

func (e *Executor) resolveReadPath(policy contracts.FilesystemScopePolicy, raw string) (string, error) {
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
	if len(policy.Params.ReadSubpaths) > 0 {
		allowed := false
		for _, sub := range policy.Params.ReadSubpaths {
			if within(filepath.Join(root, sub), target) {
				allowed = true
				break
			}
		}
		if !allowed {
			return "", fmt.Errorf("path %q is outside read scope", raw)
		}
	}
	return target, nil
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

func allowMove(policy contracts.FilesystemMutationPolicy) bool {
	if !policy.Enabled {
		return true
	}
	if policy.Strategy != "allow_writes" {
		return false
	}
	return policy.Params.AllowMove
}

func allowMkdir(policy contracts.FilesystemMutationPolicy) bool {
	if !policy.Enabled {
		return true
	}
	if policy.Strategy != "allow_writes" {
		return false
	}
	return policy.Params.AllowMkdir
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
