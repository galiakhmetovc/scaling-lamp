package filesystem

import (
	"encoding/json"
	"fmt"
	"io/fs"
	"os"
	"path"
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
		return jsonString(map[string]any{"status": "ok", "tool": toolName, "path": path, "entry_count": len(out), "entries": out}), nil
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
	case "fs_read_lines":
		rawPath, err := stringValue(args, "path")
		if err != nil {
			return "", err
		}
		path, err := e.resolveReadPath(contract.Scope, rawPath)
		if err != nil {
			return "", err
		}
		startLine, err := intValue(args, "start_line")
		if err != nil {
			return "", err
		}
		endLine, err := intValue(args, "end_line")
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
		lines, _ := splitLines(string(data))
		startIdx, endIdx, err := lineRange(len(lines), startLine, endLine)
		if err != nil {
			return "", err
		}
		out := make([]map[string]any, 0, endIdx-startIdx+1)
		for i := startIdx; i <= endIdx; i++ {
			out = append(out, map[string]any{
				"line": i + 1,
				"text": lines[i],
			})
		}
		return jsonString(map[string]any{
			"status":     "ok",
			"tool":       toolName,
			"path":       path,
			"start_line": startLine,
			"end_line":   endLine,
			"lines":      out,
		}), nil
	case "fs_search_text":
		rawPath, err := stringValue(args, "path")
		if err != nil {
			return "", err
		}
		path, err := e.resolveReadPath(contract.Scope, rawPath)
		if err != nil {
			return "", err
		}
		query, err := stringValue(args, "query")
		if err != nil {
			return "", err
		}
		limit, err := optionalIntValue(args, "limit")
		if err != nil {
			return "", err
		}
		if limit <= 0 {
			limit = 50
		}
		data, err := os.ReadFile(path)
		if err != nil {
			return "", fmt.Errorf("read file: %w", err)
		}
		if contract.IO.Enabled && contract.IO.Strategy == "bounded_text_io" && contract.IO.Params.MaxReadBytes > 0 && len(data) > contract.IO.Params.MaxReadBytes {
			return "", fmt.Errorf("read content exceeds max_read_bytes")
		}
		lines, _ := splitLines(string(data))
		matches := make([]map[string]any, 0, limit)
		for i, line := range lines {
			if !strings.Contains(line, query) {
				continue
			}
			matches = append(matches, map[string]any{
				"line": i + 1,
				"text": line,
			})
			if len(matches) >= limit {
				break
			}
		}
		return jsonString(map[string]any{
			"status":  "ok",
			"tool":    toolName,
			"path":    path,
			"query":   query,
			"matches": matches,
		}), nil
	case "fs_find_in_files":
		query, err := stringValue(args, "query")
		if err != nil {
			return "", err
		}
		globPattern, _ := optionalStringValue(args, "glob")
		limit, err := optionalIntValue(args, "limit")
		if err != nil {
			return "", err
		}
		return e.findInFiles(contract, toolName, query, globPattern, limit)
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
		mode, _ := optionalStringValue(args, "mode")
		if mode == "" {
			mode = "upsert"
		}
		if mode != "create" && mode != "overwrite" && mode != "upsert" {
			return "", fmt.Errorf("mode must be one of create, overwrite, upsert")
		}
		if contract.IO.Enabled && contract.IO.Strategy == "bounded_text_io" && contract.IO.Params.MaxWriteBytes > 0 && len(content) > contract.IO.Params.MaxWriteBytes {
			return "", fmt.Errorf("write content exceeds max_write_bytes")
		}
		if !allowWrites(contract.Mutation) {
			return "", fmt.Errorf("filesystem writes are denied by mutation policy")
		}
		_, statErr := os.Stat(path)
		exists := statErr == nil
		if statErr != nil && !os.IsNotExist(statErr) {
			return "", fmt.Errorf("stat write target: %w", statErr)
		}
		switch mode {
		case "create":
			if exists {
				return "", fmt.Errorf("write target already exists")
			}
		case "overwrite":
			if !exists {
				return "", fmt.Errorf("write target does not exist")
			}
		}
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			return "", fmt.Errorf("create parent dirs: %w", err)
		}
		if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
			return "", fmt.Errorf("write file: %w", err)
		}
		return jsonString(map[string]any{
			"status":      "ok",
			"tool":        toolName,
			"path":        path,
			"mode":        mode,
			"bytes":       len(content),
			"changed":     true,
			"created":     !exists,
			"overwritten": exists,
		}), nil
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
	case "fs_replace_lines":
		rawPath, err := stringValue(args, "path")
		if err != nil {
			return "", err
		}
		path, err := e.resolveWritePath(contract.Scope, rawPath)
		if err != nil {
			return "", err
		}
		startLine, err := intValue(args, "start_line")
		if err != nil {
			return "", err
		}
		endLine, err := intValue(args, "end_line")
		if err != nil {
			return "", err
		}
		content, err := stringValue(args, "content")
		if err != nil {
			return "", err
		}
		if !allowWrites(contract.Mutation) {
			return "", fmt.Errorf("filesystem writes are denied by mutation policy")
		}
		data, err := os.ReadFile(path)
		if err != nil {
			return "", fmt.Errorf("read file for replace: %w", err)
		}
		lines, trailingNewline := splitLines(string(data))
		startIdx, endIdx, err := lineRange(len(lines), startLine, endLine)
		if err != nil {
			return "", err
		}
		replacementLines, replacementTrailing := splitLines(content)
		updatedLines := append([]string{}, lines[:startIdx]...)
		updatedLines = append(updatedLines, replacementLines...)
		updatedLines = append(updatedLines, lines[endIdx+1:]...)
		updated := joinLines(updatedLines, trailingNewline || replacementTrailing)
		if contract.IO.Enabled && contract.IO.Strategy == "bounded_text_io" && contract.IO.Params.MaxWriteBytes > 0 && len(updated) > contract.IO.Params.MaxWriteBytes {
			return "", fmt.Errorf("patched content exceeds max_write_bytes")
		}
		if err := os.WriteFile(path, []byte(updated), 0o644); err != nil {
			return "", fmt.Errorf("write replaced file: %w", err)
		}
		return jsonString(map[string]any{
			"status":     "ok",
			"tool":       toolName,
			"path":       path,
			"start_line": startLine,
			"end_line":   endLine,
			"changed":    true,
		}), nil
	case "fs_insert_text":
		rawPath, err := stringValue(args, "path")
		if err != nil {
			return "", err
		}
		path, err := e.resolveWritePath(contract.Scope, rawPath)
		if err != nil {
			return "", err
		}
		position, err := stringValue(args, "position")
		if err != nil {
			return "", err
		}
		content, err := stringValue(args, "content")
		if err != nil {
			return "", err
		}
		if !allowWrites(contract.Mutation) {
			return "", fmt.Errorf("filesystem writes are denied by mutation policy")
		}
		data, err := os.ReadFile(path)
		if err != nil {
			return "", fmt.Errorf("read file for insert: %w", err)
		}
		lines, trailingNewline := splitLines(string(data))
		insertLines, insertTrailing := splitLines(content)
		line, err := optionalIntValue(args, "line")
		if err != nil {
			return "", err
		}
		insertIdx, err := insertIndex(len(lines), line, position)
		if err != nil {
			return "", err
		}
		updatedLines := append([]string{}, lines[:insertIdx]...)
		updatedLines = append(updatedLines, insertLines...)
		updatedLines = append(updatedLines, lines[insertIdx:]...)
		updated := joinLines(updatedLines, trailingNewline || insertTrailing)
		if contract.IO.Enabled && contract.IO.Strategy == "bounded_text_io" && contract.IO.Params.MaxWriteBytes > 0 && len(updated) > contract.IO.Params.MaxWriteBytes {
			return "", fmt.Errorf("patched content exceeds max_write_bytes")
		}
		if err := os.WriteFile(path, []byte(updated), 0o644); err != nil {
			return "", fmt.Errorf("write inserted file: %w", err)
		}
		return jsonString(map[string]any{
			"status":   "ok",
			"tool":     toolName,
			"path":     path,
			"position": position,
			"line":     line,
			"changed":  true,
		}), nil
	case "fs_replace_in_line":
		rawPath, err := stringValue(args, "path")
		if err != nil {
			return "", err
		}
		path, err := e.resolveWritePath(contract.Scope, rawPath)
		if err != nil {
			return "", err
		}
		line, err := intValue(args, "line")
		if err != nil {
			return "", err
		}
		search, hasSearch := optionalStringValue(args, "search")
		replace, hasReplace := optionalStringValue(args, "replace")
		content, hasContent := optionalStringValue(args, "content")
		if !allowWrites(contract.Mutation) {
			return "", fmt.Errorf("filesystem writes are denied by mutation policy")
		}
		data, err := os.ReadFile(path)
		if err != nil {
			return "", fmt.Errorf("read file for line replace: %w", err)
		}
		lines, trailingNewline := splitLines(string(data))
		startIdx, endIdx, err := lineRange(len(lines), line, line)
		if err != nil {
			return "", err
		}
		current := lines[startIdx]
		switch {
		case hasSearch:
			if !hasReplace {
				return "", fmt.Errorf("replace is required when search is set")
			}
			updated := strings.Replace(current, search, replace, 1)
			if updated == current {
				return "", fmt.Errorf("search text not found in line")
			}
			lines[startIdx] = updated
		case hasContent:
			lines[startIdx] = content
		default:
			return "", fmt.Errorf("either search+replace or content is required")
		}
		updated := joinLines(lines, trailingNewline)
		if contract.IO.Enabled && contract.IO.Strategy == "bounded_text_io" && contract.IO.Params.MaxWriteBytes > 0 && len(updated) > contract.IO.Params.MaxWriteBytes {
			return "", fmt.Errorf("patched content exceeds max_write_bytes")
		}
		if err := os.WriteFile(path, []byte(updated), 0o644); err != nil {
			return "", fmt.Errorf("write line-replaced file: %w", err)
		}
		return jsonString(map[string]any{
			"status":    "ok",
			"tool":      toolName,
			"path":      path,
			"line":      line,
			"changed":   true,
			"start_line": startIdx + 1,
			"end_line":   endIdx + 1,
		}), nil
	case "fs_replace_in_files":
		query, err := stringValue(args, "query")
		if err != nil {
			return "", err
		}
		replace, err := stringValue(args, "replace")
		if err != nil {
			return "", err
		}
		globPattern, _ := optionalStringValue(args, "glob")
		limit, err := optionalIntValue(args, "limit")
		if err != nil {
			return "", err
		}
		return e.replaceInFiles(contract, toolName, query, replace, globPattern, limit)
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
		if contract.Mutation.Strategy != "trash_only_delete" && !allowMove(contract.Mutation) {
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

func optionalStringValue(args map[string]any, key string) (string, bool) {
	value, ok := args[key]
	if !ok || value == nil {
		return "", false
	}
	text, ok := value.(string)
	if !ok {
		return "", false
	}
	return text, true
}

func intValue(args map[string]any, key string) (int, error) {
	value, ok := args[key]
	if !ok {
		return 0, fmt.Errorf("missing required argument %q", key)
	}
	switch typed := value.(type) {
	case int:
		return typed, nil
	case int64:
		return int(typed), nil
	case float64:
		return int(typed), nil
	default:
		return 0, fmt.Errorf("argument %q must be an integer", key)
	}
}

func optionalIntValue(args map[string]any, key string) (int, error) {
	value, ok := args[key]
	if !ok || value == nil {
		return 0, nil
	}
	switch typed := value.(type) {
	case int:
		return typed, nil
	case int64:
		return int(typed), nil
	case float64:
		return int(typed), nil
	default:
		return 0, fmt.Errorf("argument %q must be an integer", key)
	}
}

func splitLines(content string) ([]string, bool) {
	trailingNewline := strings.HasSuffix(content, "\n")
	content = strings.TrimSuffix(content, "\n")
	if content == "" {
		if trailingNewline {
			return []string{""}, true
		}
		return []string{}, false
	}
	return strings.Split(content, "\n"), trailingNewline
}

func joinLines(lines []string, trailingNewline bool) string {
	if len(lines) == 0 {
		if trailingNewline {
			return "\n"
		}
		return ""
	}
	body := strings.Join(lines, "\n")
	if trailingNewline {
		return body + "\n"
	}
	return body
}

func lineRange(totalLines, startLine, endLine int) (int, int, error) {
	if startLine <= 0 {
		return 0, 0, fmt.Errorf("start_line must be >= 1")
	}
	if endLine < startLine {
		return 0, 0, fmt.Errorf("start_line must be <= end_line")
	}
	if totalLines == 0 {
		return 0, 0, fmt.Errorf("file has no lines")
	}
	if endLine > totalLines {
		return 0, 0, fmt.Errorf("end_line exceeds file length")
	}
	return startLine - 1, endLine - 1, nil
}

func insertIndex(totalLines, line int, position string) (int, error) {
	switch position {
	case "prepend":
		return 0, nil
	case "append":
		return totalLines, nil
	case "before":
		if line <= 0 {
			return 0, fmt.Errorf("line must be >= 1 for before insertion")
		}
		if line > totalLines {
			return 0, fmt.Errorf("line exceeds file length")
		}
		return line - 1, nil
	case "after":
		if line <= 0 {
			return 0, fmt.Errorf("line must be >= 1 for after insertion")
		}
		if line > totalLines {
			return 0, fmt.Errorf("line exceeds file length")
		}
		return line, nil
	default:
		return 0, fmt.Errorf("unsupported insert position %q", position)
	}
}

func (e *Executor) findInFiles(contract contracts.FilesystemExecutionContract, toolName, query, globPattern string, limit int) (string, error) {
	root, err := rootPath(contract.Scope)
	if err != nil {
		return "", err
	}
	if limit <= 0 {
		limit = contract.IO.Params.MaxSearchHits
	}
	if limit <= 0 {
		limit = 50
	}
	maxFiles := contract.IO.Params.MaxSearchFiles
	if maxFiles <= 0 {
		maxFiles = 50
	}
	matches := make([]map[string]any, 0, limit)
	filesVisited := 0
	err = filepath.WalkDir(root, func(current string, entry fs.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		if entry.IsDir() {
			return nil
		}
		rel, err := filepath.Rel(root, current)
		if err != nil {
			return err
		}
		if !e.readAllowed(contract.Scope, current) {
			return nil
		}
		if !matchesGlob(rel, globPattern) {
			return nil
		}
		filesVisited++
		if filesVisited > maxFiles {
			return fmt.Errorf("search exceeds max_search_files")
		}
		data, err := os.ReadFile(current)
		if err != nil {
			return err
		}
		if contract.IO.Enabled && contract.IO.Strategy == "bounded_text_io" && contract.IO.Params.MaxReadBytes > 0 && len(data) > contract.IO.Params.MaxReadBytes {
			return nil
		}
		lines, _ := splitLines(string(data))
		for i, line := range lines {
			if !strings.Contains(line, query) {
				continue
			}
			matches = append(matches, map[string]any{
				"path": current,
				"line": i + 1,
				"text": line,
			})
			if len(matches) >= limit {
				return fs.SkipAll
			}
		}
		return nil
	})
	if err != nil && err != fs.SkipAll {
		return "", err
	}
	return jsonString(map[string]any{
		"status":       "ok",
		"tool":         toolName,
		"query":        query,
		"glob":         globPattern,
		"matches":      matches,
		"files_scanned": filesVisited,
	}), nil
}

func (e *Executor) replaceInFiles(contract contracts.FilesystemExecutionContract, toolName, query, replace, globPattern string, limit int) (string, error) {
	if !allowWrites(contract.Mutation) {
		return "", fmt.Errorf("filesystem writes are denied by mutation policy")
	}
	root, err := rootPath(contract.Scope)
	if err != nil {
		return "", err
	}
	maxFiles := contract.IO.Params.MaxReplaceFiles
	if maxFiles <= 0 {
		maxFiles = 20
	}
	maxHits := contract.IO.Params.MaxReplaceHits
	if maxHits <= 0 {
		maxHits = 50
	}
	if limit > 0 && limit < maxHits {
		maxHits = limit
	}
	changedFiles := make([]map[string]any, 0)
	filesChanged := 0
	replaceHits := 0
	err = filepath.WalkDir(root, func(current string, entry fs.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		if entry.IsDir() {
			return nil
		}
		if !e.readAllowed(contract.Scope, current) || !e.writeAllowed(contract.Scope, current) {
			return nil
		}
		rel, err := filepath.Rel(root, current)
		if err != nil {
			return err
		}
		if !matchesGlob(rel, globPattern) {
			return nil
		}
		data, err := os.ReadFile(current)
		if err != nil {
			return err
		}
		if contract.IO.Enabled && contract.IO.Strategy == "bounded_text_io" && contract.IO.Params.MaxReadBytes > 0 && len(data) > contract.IO.Params.MaxReadBytes {
			return nil
		}
		lines, trailingNewline := splitLines(string(data))
		fileHits := 0
		for i, line := range lines {
			if !strings.Contains(line, query) {
				continue
			}
			if replaceHits >= maxHits {
				break
			}
			lines[i] = strings.Replace(line, query, replace, 1)
			fileHits++
			replaceHits++
		}
		if fileHits == 0 {
			return nil
		}
		filesChanged++
		if filesChanged > maxFiles {
			return fmt.Errorf("replace exceeds max_replace_files")
		}
		updated := joinLines(lines, trailingNewline)
		if contract.IO.Enabled && contract.IO.Strategy == "bounded_text_io" && contract.IO.Params.MaxWriteBytes > 0 && len(updated) > contract.IO.Params.MaxWriteBytes {
			return fmt.Errorf("patched content exceeds max_write_bytes")
		}
		if err := os.WriteFile(current, []byte(updated), 0o644); err != nil {
			return err
		}
		changedFiles = append(changedFiles, map[string]any{
			"path": current,
			"hits": fileHits,
		})
		if replaceHits >= maxHits {
			return fs.SkipAll
		}
		return nil
	})
	if err != nil && err != fs.SkipAll {
		return "", err
	}
	return jsonString(map[string]any{
		"status":        "ok",
		"tool":          toolName,
		"query":         query,
		"replace":       replace,
		"glob":          globPattern,
		"changed_files": filesChanged,
		"replace_hits":  replaceHits,
		"files":         changedFiles,
	}), nil
}

func matchesGlob(rel, globPattern string) bool {
	if globPattern == "" {
		return true
	}
	rel = filepath.ToSlash(rel)
	if ok, _ := path.Match(globPattern, rel); ok {
		return true
	}
	if strings.Contains(globPattern, "**/") {
		alt := strings.ReplaceAll(globPattern, "**/", "")
		ok, _ := path.Match(alt, rel)
		return ok
	}
	return false
}

func (e *Executor) readAllowed(policy contracts.FilesystemScopePolicy, target string) bool {
	root, err := rootPath(policy)
	if err != nil {
		return false
	}
	if !within(root, target) {
		return false
	}
	if len(policy.Params.ReadSubpaths) == 0 {
		return true
	}
	for _, sub := range policy.Params.ReadSubpaths {
		if within(filepath.Join(root, sub), target) {
			return true
		}
	}
	return false
}

func (e *Executor) writeAllowed(policy contracts.FilesystemScopePolicy, target string) bool {
	root, err := rootPath(policy)
	if err != nil {
		return false
	}
	if !within(root, target) {
		return false
	}
	if len(policy.Params.WriteSubpaths) == 0 {
		return true
	}
	for _, sub := range policy.Params.WriteSubpaths {
		if within(filepath.Join(root, sub), target) {
			return true
		}
	}
	return false
}

func jsonString(value any) string {
	data, _ := json.Marshal(value)
	return string(data)
}
