package api

import (
	"bufio"
	"context"
	"crypto/subtle"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"time"

	"teamd/internal/approvals"
	"teamd/internal/artifacts"
	"teamd/internal/llmtrace"
	"teamd/internal/memory"
	"teamd/internal/provider"
	"teamd/internal/runtime"
	"teamd/internal/vfs"
)

type Server struct {
	core           runtime.AgentCore
	runtime        *runtime.API
	runtimeConfig  provider.RequestConfig
	memoryPolicy   runtime.MemoryPolicy
	actionPolicy   runtime.ActionPolicy
	memory         memory.Store
	artifacts      artifacts.Store
	runner         RunStarter
	jobs           JobRunner
	workers        WorkerRunner
	sessionActions SessionActionRunner
	rawProvider    provider.Provider
	operatorToken  string
	toolCatalog    ToolCatalog
	toolExecutor   RawToolExecutor
	previewer      ProviderPreviewer
	sessionHeads   SessionHeadStore
	rawSessionLogDir string
	rawVFSRootDir  string
	mux            *http.ServeMux
}

func nowUTC() time.Time {
	return time.Now().UTC()
}

func (s *Server) policySnapshot(sessionID string) runtime.PolicySnapshot {
	if s.core != nil {
		summary, err := s.core.RuntimeSummary(sessionID)
		if err == nil {
			return runtime.NormalizePolicySnapshot(runtime.PolicySnapshot{
				Runtime:      summary.Runtime,
				MemoryPolicy: summary.MemoryPolicy,
				ActionPolicy: summary.ActionPolicy,
			})
		}
	}
	if s.runtime == nil {
		return runtime.NormalizePolicySnapshot(runtime.PolicySnapshot{
			Runtime:      s.runtimeConfig,
			MemoryPolicy: s.memoryPolicy,
			ActionPolicy: s.actionPolicy,
		})
	}
	summary, err := s.runtime.RuntimeSummary(sessionID, s.runtimeConfig, s.memoryPolicy, s.actionPolicy)
	if err != nil {
		return runtime.NormalizePolicySnapshot(runtime.PolicySnapshot{
			Runtime:      s.runtimeConfig,
			MemoryPolicy: s.memoryPolicy,
			ActionPolicy: s.actionPolicy,
		})
	}
	return runtime.NormalizePolicySnapshot(runtime.PolicySnapshot{
		Runtime:      summary.Runtime,
		MemoryPolicy: summary.MemoryPolicy,
		ActionPolicy: summary.ActionPolicy,
	})
}

func mergePolicySnapshotRuntime(snapshot runtime.PolicySnapshot, override provider.RequestConfig) runtime.PolicySnapshot {
	snapshot.Runtime = runtime.MergeRequestConfig(snapshot.Runtime, override)
	return runtime.NormalizePolicySnapshot(snapshot)
}

type RunStarter interface {
	StartDetached(ctx context.Context, req runtime.StartRunRequest) (runtime.RunView, bool, error)
}

type JobRunner interface {
	StartDetached(ctx context.Context, req runtime.JobStartRequest) (runtime.JobView, error)
	Job(jobID string) (runtime.JobView, bool, error)
	List(limit int) ([]runtime.JobView, error)
	Logs(query runtime.JobLogQuery) ([]runtime.JobLogChunk, error)
	Cancel(jobID string) (bool, error)
}

type WorkerRunner interface {
	Spawn(ctx context.Context, req runtime.WorkerSpawnRequest) (runtime.WorkerView, error)
	Message(ctx context.Context, workerID string, req runtime.WorkerMessageRequest) (runtime.WorkerView, error)
	Wait(workerID string, afterCursor int, afterEventID int64, eventLimit int) (runtime.WorkerWaitResult, bool, error)
	Handoff(workerID string) (runtime.WorkerHandoff, bool, error)
	Close(workerID string) (runtime.WorkerView, bool, error)
	Worker(workerID string) (runtime.WorkerView, bool, error)
	List(query runtime.WorkerQuery) ([]runtime.WorkerView, error)
}

type SessionActionRunner interface {
	Execute(chatID int64, req runtime.SessionActionRequest) (runtime.SessionActionResult, error)
}

type ToolCatalog interface {
	DebugToolCatalog(role string) ([]provider.ToolDefinition, error)
}

type RawToolExecutor interface {
	ExecuteApprovedTool(ctx context.Context, chatID int64, allowedTools []string, call provider.ToolCall) (string, error)
}

type ProviderPreviewer interface {
	DebugProviderPreview(ctx context.Context, chatID int64, sessionID, query string, runtimeConfig provider.RequestConfig, profile *runtime.DebugExecutionProfile) (provider.PromptRequest, runtime.PromptBudgetMetrics, error)
}

type SessionHeadStore interface {
	SaveSessionHead(runtime.SessionHead) error
	SessionHead(chatID int64, sessionID string) (runtime.SessionHead, bool, error)
}

func normalizeDebugToolName(name string) string {
	name = strings.TrimSpace(strings.ToLower(name))
	name = strings.ReplaceAll(name, ".", "_")
	name = strings.ReplaceAll(name, "-", "_")
	return name
}

func resolveDebugTools(catalog []provider.ToolDefinition, selected []string) []provider.ToolDefinition {
	if len(selected) == 0 || len(catalog) == 0 {
		return nil
	}
	allowed := map[string]struct{}{}
	for _, name := range selected {
		if normalized := normalizeDebugToolName(name); normalized != "" {
			allowed[normalized] = struct{}{}
		}
	}
	if len(allowed) == 0 {
		return nil
	}
	out := make([]provider.ToolDefinition, 0, len(allowed))
	for _, item := range catalog {
		if _, ok := allowed[normalizeDebugToolName(item.Name)]; ok {
			out = append(out, item)
		}
	}
	return out
}

type rawSessionLogEntry struct {
	Timestamp           time.Time               `json:"timestamp"`
	Kind                string                  `json:"kind"`
	ChatID              int64                   `json:"chat_id"`
	SessionID           string                  `json:"session_id,omitempty"`
	Query               string                  `json:"query,omitempty"`
	SystemPrompt        string                  `json:"system_prompt,omitempty"`
	IncludeSystemPrompt bool                    `json:"include_system_prompt,omitempty"`
	SelectedTools       []string                `json:"selected_tools,omitempty"`
	Request             provider.PromptRequest  `json:"request,omitempty"`
	Response            provider.PromptResponse `json:"response,omitempty"`
	Trace               llmtrace.CallTrace      `json:"trace,omitempty"`
	ToolCall            *provider.ToolCall      `json:"tool_call,omitempty"`
	ToolOutput          string                  `json:"tool_output,omitempty"`
	ToolError           string                  `json:"tool_error,omitempty"`
}

func formatToolExecutionError(err error) string {
	if err == nil {
		return ""
	}
	return "tool execution error: " + strings.TrimSpace(err.Error())
}

func (s *Server) appendRawSessionLog(sessionID string, entry rawSessionLogEntry) (string, error) {
	root := strings.TrimSpace(s.rawSessionLogDir)
	if root == "" {
		return "", nil
	}
	if strings.TrimSpace(sessionID) == "" {
		sessionID = fmt.Sprintf("chat-%d", entry.ChatID)
	}
	if err := os.MkdirAll(root, 0o755); err != nil {
		return "", err
	}
	dir := filepath.Join(root, sanitizeLogFilename(sessionID))
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return "", err
	}
	path := filepath.Join(dir, "session.jsonl")
	file, err := os.OpenFile(path, os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0o644)
	if err != nil {
		return "", err
	}
	defer file.Close()
	writer := bufio.NewWriter(file)
	body, err := json.Marshal(entry)
	if err != nil {
		return "", err
	}
	if _, err := writer.Write(append(body, '\n')); err != nil {
		return "", err
	}
	if err := writer.Flush(); err != nil {
		return "", err
	}
	return path, nil
}

func (s *Server) rawSessionFiles() ([]string, error) {
	root := strings.TrimSpace(s.rawSessionLogDir)
	if root == "" {
		return nil, nil
	}
	entries, err := os.ReadDir(root)
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return nil, nil
		}
		return nil, err
	}
	out := make([]string, 0, len(entries))
	for _, entry := range entries {
		if !entry.IsDir() {
			continue
		}
		path := filepath.Join(root, entry.Name(), "session.jsonl")
		if _, err := os.Stat(path); err == nil {
			out = append(out, path)
		}
	}
	sort.Strings(out)
	return out, nil
}

func readRawSessionLog(path string) ([]rawSessionLogEntry, error) {
	file, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer file.Close()
	var out []rawSessionLogEntry
	scanner := bufio.NewScanner(file)
	const maxCap = 8 << 20
	buf := make([]byte, 0, 64*1024)
	scanner.Buffer(buf, maxCap)
	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())
		if line == "" {
			continue
		}
		var entry rawSessionLogEntry
		if err := json.Unmarshal([]byte(line), &entry); err != nil {
			return nil, err
		}
		out = append(out, entry)
	}
	if err := scanner.Err(); err != nil {
		return nil, err
	}
	return out, nil
}

func normalizeProviderMessages(messages []provider.Message) []provider.Message {
	out := make([]provider.Message, 0, len(messages))
	for _, message := range messages {
		if strings.EqualFold(strings.TrimSpace(message.Role), "system") {
			continue
		}
		out = append(out, provider.Message{
			Role:       message.Role,
			Content:    message.Content,
			Name:       message.Name,
			ToolCallID: message.ToolCallID,
			ToolCalls:  append([]provider.ToolCall(nil), message.ToolCalls...),
		})
	}
	return out
}

func (s *Server) listRawSessionStates(chatID int64, hasChatID bool) ([]runtime.SessionState, error) {
	files, err := s.rawSessionFiles()
	if err != nil {
		return nil, err
	}
	type rawMeta struct {
		chatID int64
		last   time.Time
	}
	seen := map[string]rawMeta{}
	for _, path := range files {
		entries, err := readRawSessionLog(path)
		if err != nil || len(entries) == 0 {
			continue
		}
		for _, entry := range entries {
			if strings.TrimSpace(entry.SessionID) == "" {
				continue
			}
			if hasChatID && entry.ChatID != chatID {
				continue
			}
			meta, ok := seen[entry.SessionID]
			if !ok || entry.Timestamp.After(meta.last) {
				seen[entry.SessionID] = rawMeta{chatID: entry.ChatID, last: entry.Timestamp}
			}
		}
	}
	out := make([]runtime.SessionState, 0, len(seen))
	for sessionID, meta := range seen {
		out = append(out, runtime.SessionState{
			SessionID:      sessionID,
			ChatID:         meta.chatID,
			LastActivityAt: meta.last,
		})
	}
	sort.Slice(out, func(i, j int) bool { return out[i].LastActivityAt.After(out[j].LastActivityAt) })
	return out, nil
}

func mergeSessionStates(primary []runtime.SessionState, extra []runtime.SessionState, limit int) []runtime.SessionState {
	seen := map[string]runtime.SessionState{}
	for _, item := range primary {
		seen[item.SessionID] = item
	}
	for _, item := range extra {
		if existing, ok := seen[item.SessionID]; ok {
			if existing.LastActivityAt.Before(item.LastActivityAt) {
				existing.LastActivityAt = item.LastActivityAt
				seen[item.SessionID] = existing
			}
			continue
		}
		seen[item.SessionID] = item
	}
	out := make([]runtime.SessionState, 0, len(seen))
	for _, item := range seen {
		out = append(out, item)
	}
	sort.Slice(out, func(i, j int) bool { return out[i].LastActivityAt.After(out[j].LastActivityAt) })
	if limit > 0 && len(out) > limit {
		out = out[:limit]
	}
	return out
}

func (s *Server) loadRawConversation(sessionID string) (DebugRawConversationResponse, bool, error) {
	files, err := s.rawSessionFiles()
	if err != nil {
		return DebugRawConversationResponse{}, false, err
	}
	for _, path := range files {
		entries, err := readRawSessionLog(path)
		if err != nil || len(entries) == 0 {
			continue
		}
		turns := make([]DebugRawConversationTurn, 0)
		var messages []provider.Message
		found := false
		for _, entry := range entries {
			if entry.SessionID != sessionID {
				continue
			}
			found = true
			if entry.Kind != "provider_turn" {
				continue
			}
			turns = append(turns, DebugRawConversationTurn{
				Query:               entry.Query,
				Request:             entry.Request,
				Response:            entry.Response,
				Trace:               entry.Trace,
				LogPath:             path,
				SystemPrompt:        entry.SystemPrompt,
				IncludeSystemPrompt: entry.IncludeSystemPrompt,
			})
			messages = normalizeProviderMessages(entry.Request.Messages)
			messages = append(messages, provider.Message{
				Role:      "assistant",
				Content:   entry.Response.Text,
				ToolCalls: append([]provider.ToolCall(nil), entry.Response.ToolCalls...),
			})
		}
		if found {
			return DebugRawConversationResponse{
				SessionID: sessionID,
				Messages:  messages,
				Turns:     turns,
			}, true, nil
		}
	}
	return DebugRawConversationResponse{}, false, nil
}

func sanitizeLogFilename(s string) string {
	s = strings.TrimSpace(s)
	if s == "" {
		return "session"
	}
	return strings.NewReplacer("/", "-", "\\", "-", " ", "-", ":", "-", "\n", "-", "\t", "-").Replace(s)
}

const rawToolOffloadThresholdChars = 1200

func offloadedToolMessage(path, content string) string {
	preview := strings.TrimSpace(content)
	preview = strings.ReplaceAll(preview, "\r\n", "\n")
	preview = strings.ReplaceAll(preview, "\n", " ")
	preview = strings.Join(strings.Fields(preview), " ")
	if len(preview) > 240 {
		preview = preview[:240] + "..."
	}
	return fmt.Sprintf("Artifact offloaded to %s\nsize: %d chars\npreview: %s", path, len(content), preview)
}

func (s *Server) offloadOldToolMessages(chatID int64, sessionID string, messages []provider.Message) ([]provider.Message, []string, error) {
	if len(messages) == 0 || strings.TrimSpace(s.rawVFSRootDir) == "" {
		return append([]provider.Message(nil), messages...), nil, nil
	}
	out := append([]provider.Message(nil), messages...)
	root, _, err := s.sessionVFS(chatID, sessionID)
	if err != nil {
		return nil, nil, err
	}
	refs := make([]string, 0)
	for i := range out {
		msg := out[i]
		if !strings.EqualFold(msg.Role, "tool") || i == len(out)-1 {
			continue
		}
		content := msg.Content
		if len(content) < rawToolOffloadThresholdChars {
			continue
		}
		if strings.Contains(content, "Artifact offloaded to .agent/memory/") {
			continue
		}
		relPath := filepath.ToSlash(filepath.Join(".agent", "memory", fmt.Sprintf("tool-%d-%03d.txt", nowUTC().UnixNano(), i)))
		if err := root.WriteFile(relPath, []byte(content)); err != nil {
			return nil, nil, err
		}
		msg.Content = offloadedToolMessage(relPath, content)
		out[i] = msg
		refs = append(refs, relPath)
	}
	return out, refs, nil
}

func mergeArtifactRefs(existing, next []string) []string {
	seen := map[string]struct{}{}
	out := make([]string, 0, len(existing)+len(next))
	for _, item := range append(append([]string(nil), existing...), next...) {
		item = strings.TrimSpace(item)
		if item == "" {
			continue
		}
		if _, ok := seen[item]; ok {
			continue
		}
		seen[item] = struct{}{}
		out = append(out, item)
	}
	return out
}

func rawSessionChatID(sessionID string, fallback int64) int64 {
	parts := strings.SplitN(strings.TrimSpace(sessionID), ":", 2)
	if len(parts) == 2 {
		if chatID, err := strconv.ParseInt(parts[0], 10, 64); err == nil {
			return chatID
		}
	}
	return fallback
}

func (s *Server) updateRawSessionHeadArtifacts(chatID int64, sessionID string, refs []string) error {
	if s.sessionHeads == nil || strings.TrimSpace(sessionID) == "" || len(refs) == 0 {
		return nil
	}
	chatID = rawSessionChatID(sessionID, chatID)
	head, _, err := s.sessionHeads.SessionHead(chatID, sessionID)
	if err != nil {
		return err
	}
	head.ChatID = chatID
	head.SessionID = sessionID
	head.RecentArtifactRefs = mergeArtifactRefs(head.RecentArtifactRefs, refs)
	head.UpdatedAt = nowUTC()
	return s.sessionHeads.SaveSessionHead(head)
}

func (s *Server) rawVFSTools() []provider.ToolDefinition {
	return []provider.ToolDefinition{
		{
			Name:        "vfs_path",
			Description: "Resolve one relative path inside the virtual filesystem and report its absolute location under the VFS root.",
			Parameters: map[string]any{
				"type": "object",
				"properties": map[string]any{
					"path": map[string]any{"type": "string"},
				},
			},
		},
		{
			Name:        "vfs_tree",
			Description: "Show the recursive directory tree inside the virtual filesystem root.",
			Parameters: map[string]any{
				"type": "object",
				"properties": map[string]any{
					"path": map[string]any{"type": "string"},
				},
			},
		},
		{
			Name:        "vfs_grep",
			Description: "Search files in the virtual filesystem root by regex pattern.",
			Parameters: map[string]any{
				"type": "object",
				"properties": map[string]any{
					"path":    map[string]any{"type": "string"},
					"pattern": map[string]any{"type": "string"},
				},
				"required": []string{"pattern"},
			},
		},
		{
			Name:        "vfs_read_file",
			Description: "Read one file from the virtual filesystem with bounded size.",
			Parameters: map[string]any{
				"type": "object",
				"properties": map[string]any{
					"path": map[string]any{"type": "string"},
				},
				"required": []string{"path"},
			},
		},
		{
			Name:        "vfs_read_lines",
			Description: "Read a specific line range from a file in the virtual filesystem.",
			Parameters: map[string]any{
				"type": "object",
				"properties": map[string]any{
					"path":       map[string]any{"type": "string"},
					"start_line": map[string]any{"type": "integer"},
					"end_line":   map[string]any{"type": "integer"},
				},
				"required": []string{"path", "start_line", "end_line"},
			},
		},
		{
			Name:        "vfs_write_file",
			Description: "Write or replace one file inside the virtual filesystem.",
			Parameters: map[string]any{
				"type": "object",
				"properties": map[string]any{
					"path":    map[string]any{"type": "string"},
					"content": map[string]any{"type": "string"},
				},
				"required": []string{"path", "content"},
			},
		},
		{
			Name:        "vfs_patch",
			Description: "Patch a file inside the virtual filesystem by replacing one exact text fragment with another and returning a diff.",
			Parameters: map[string]any{
				"type": "object",
				"properties": map[string]any{
					"path":        map[string]any{"type": "string"},
					"old":         map[string]any{"type": "string"},
					"new":         map[string]any{"type": "string"},
					"replace_all": map[string]any{"type": "boolean"},
				},
				"required": []string{"path", "old", "new"},
			},
		},
		{
			Name:        "vfs_mkdir",
			Description: "Create a directory inside the virtual filesystem.",
			Parameters: map[string]any{
				"type": "object",
				"properties": map[string]any{
					"path": map[string]any{"type": "string"},
				},
				"required": []string{"path"},
			},
		},
		{
			Name:        "vfs_touch",
			Description: "Create an empty file or update mtime inside the virtual filesystem.",
			Parameters: map[string]any{
				"type": "object",
				"properties": map[string]any{
					"path": map[string]any{"type": "string"},
				},
				"required": []string{"path"},
			},
		},
		{
			Name:        "vfs_diff",
			Description: "Produce a unified diff for two files or one file versus inline content inside the virtual filesystem.",
			Parameters: map[string]any{
				"type": "object",
				"properties": map[string]any{
					"left_path":     map[string]any{"type": "string"},
					"right_path":    map[string]any{"type": "string"},
					"right_content": map[string]any{"type": "string"},
				},
				"required": []string{"left_path"},
			},
		},
	}
}

func (s *Server) combinedDebugToolCatalog(role string) ([]provider.ToolDefinition, error) {
	var out []provider.ToolDefinition
	if s.toolCatalog != nil {
		items, err := s.toolCatalog.DebugToolCatalog(role)
		if err != nil {
			return nil, err
		}
		out = append(out, items...)
	}
	if strings.TrimSpace(s.rawVFSRootDir) != "" {
		out = append(out, s.rawVFSTools()...)
	}
	return out, nil
}

func (s *Server) sessionVFS(chatID int64, sessionID string) (*vfs.Root, string, error) {
	base := strings.TrimSpace(s.rawVFSRootDir)
	if base == "" {
		return nil, "", fmt.Errorf("raw vfs root is not configured")
	}
	label := strings.TrimSpace(sessionID)
	if label == "" {
		label = fmt.Sprintf("chat-%d", chatID)
	}
	rootPath := filepath.Join(base, sanitizeLogFilename(label))
	root, err := vfs.New(rootPath)
	if err != nil {
		return nil, "", err
	}
	return root, rootPath, nil
}

func (s *Server) executeRawVFSTool(chatID int64, sessionID string, call provider.ToolCall) (string, error) {
	root, rootPath, err := s.sessionVFS(chatID, sessionID)
	if err != nil {
		return "", err
	}
	argString := func(key string, required bool) (string, error) {
		value, _ := call.Arguments[key]
		text, _ := value.(string)
		text = strings.TrimSpace(text)
		if required && text == "" {
			return "", fmt.Errorf("%s is required", key)
		}
		return text, nil
	}
	argInt := func(key string) (int, error) {
		value, ok := call.Arguments[key]
		if !ok {
			return 0, fmt.Errorf("%s is required", key)
		}
		switch n := value.(type) {
		case float64:
			return int(n), nil
		case int:
			return n, nil
		case string:
			parsed, err := strconv.Atoi(strings.TrimSpace(n))
			if err != nil {
				return 0, fmt.Errorf("invalid %s", key)
			}
			return parsed, nil
		default:
			return 0, fmt.Errorf("invalid %s", key)
		}
	}
	argBool := func(key string) bool {
		value, ok := call.Arguments[key]
		if !ok {
			return false
		}
		switch v := value.(type) {
		case bool:
			return v
		case string:
			return strings.EqualFold(strings.TrimSpace(v), "true")
		default:
			return false
		}
	}
	switch call.Name {
	case "vfs_path":
		path, _ := argString("path", false)
		info, err := root.Path(path)
		if err != nil {
			return "", err
		}
		out := []string{
			"vfs_root: " + info.Root,
			"relative_path: " + info.RelativePath,
			"absolute_path: " + info.AbsolutePath,
			fmt.Sprintf("exists: %t", info.Exists),
		}
		if info.Exists {
			out = append(out, fmt.Sprintf("is_dir: %t", info.IsDir))
			out = append(out, fmt.Sprintf("size: %d", info.Size))
		}
		return strings.Join(out, "\n"), nil
	case "vfs_tree":
		path, _ := argString("path", false)
		entries, err := root.Tree(path)
		if err != nil {
			return "", err
		}
		lines := []string{"vfs_root: " + rootPath}
		for _, entry := range entries {
			prefix := strings.Repeat("  ", entry.Depth)
			suffix := ""
			if entry.IsDir {
				suffix = "/"
			}
			lines = append(lines, prefix+entry.Path+suffix)
		}
		if len(entries) == 0 {
			lines = append(lines, ".")
		}
		return strings.Join(lines, "\n"), nil
	case "vfs_grep":
		path, _ := argString("path", false)
		pattern, err := argString("pattern", true)
		if err != nil {
			return "", err
		}
		hits, err := root.Search(path, pattern)
		if err != nil {
			return "", err
		}
		lines := []string{"vfs_root: " + rootPath}
		for _, hit := range hits {
			lines = append(lines, fmt.Sprintf("%s:%d:%d: %s", hit.Path, hit.Line, hit.Column, hit.Snippet))
		}
		if len(hits) == 0 {
			lines = append(lines, "no matches")
		}
		return strings.Join(lines, "\n"), nil
	case "vfs_read_file":
		path, err := argString("path", true)
		if err != nil {
			return "", err
		}
		file, err := root.ReadFile(path)
		if err != nil {
			return "", err
		}
		return fmt.Sprintf("path: %s\nsize: %d\nlines: %d\n\n%s", file.Path, file.Size, file.Lines, string(file.Content)), nil
	case "vfs_read_lines":
		path, err := argString("path", true)
		if err != nil {
			return "", err
		}
		start, err := argInt("start_line")
		if err != nil {
			return "", err
		}
		end, err := argInt("end_line")
		if err != nil {
			return "", err
		}
		lines, err := root.ReadLines(path, start, end)
		if err != nil {
			return "", err
		}
		out := []string{fmt.Sprintf("path: %s", path), fmt.Sprintf("range: %d-%d", start, end), ""}
		for idx, line := range lines {
			out = append(out, fmt.Sprintf("%d: %s", start+idx, line))
		}
		return strings.Join(out, "\n"), nil
	case "vfs_write_file":
		path, err := argString("path", true)
		if err != nil {
			return "", err
		}
		content, err := argString("content", false)
		if err != nil {
			return "", err
		}
		if err := root.WriteFile(path, []byte(content)); err != nil {
			return "", err
		}
		return "wrote " + path, nil
	case "vfs_patch":
		path, err := argString("path", true)
		if err != nil {
			return "", err
		}
		oldText, err := argString("old", true)
		if err != nil {
			return "", err
		}
		newText, err := argString("new", false)
		if err != nil {
			return "", err
		}
		result, err := root.PatchReplace(path, oldText, newText, argBool("replace_all"))
		if err != nil {
			return "", err
		}
		return fmt.Sprintf("patched %s\nreplacements: %d\n\n%s", result.Path, result.Replacements, result.Diff), nil
	case "vfs_mkdir":
		path, err := argString("path", true)
		if err != nil {
			return "", err
		}
		if err := root.Mkdir(path); err != nil {
			return "", err
		}
		return "created directory " + path, nil
	case "vfs_touch":
		path, err := argString("path", true)
		if err != nil {
			return "", err
		}
		if err := root.Touch(path); err != nil {
			return "", err
		}
		return "touched " + path, nil
	case "vfs_diff":
		leftPath, err := argString("left_path", true)
		if err != nil {
			return "", err
		}
		rightPath, _ := argString("right_path", false)
		rightContent, _ := argString("right_content", false)
		if rightPath != "" {
			return root.UnifiedDiffFiles(leftPath, rightPath)
		}
		return root.UnifiedDiffContent(leftPath, []byte(rightContent))
	default:
		return "", fmt.Errorf("unsupported raw vfs tool: %s", call.Name)
	}
}

func NewServer(rt *runtime.API, mem memory.Store, artifactStore artifacts.Store, runner RunStarter, jobs JobRunner, workers WorkerRunner, runtimeConfig provider.RequestConfig, memoryPolicy runtime.MemoryPolicy, actionPolicy runtime.ActionPolicy) *Server {
	s := &Server{
		runtime:       rt,
		runtimeConfig: runtimeConfig,
		memoryPolicy:  runtime.NormalizeMemoryPolicy(memoryPolicy),
		actionPolicy:  runtime.NormalizeActionPolicy(actionPolicy),
		memory:        mem,
		artifacts:     artifactStore,
		runner:        runner,
		jobs:          jobs,
		workers:       workers,
		mux:           http.NewServeMux(),
	}
	s.routes()
	return s
}

func (s *Server) WithCore(core runtime.AgentCore) *Server {
	s.core = core
	return s
}

func (s *Server) Handler() http.Handler {
	if strings.TrimSpace(s.operatorToken) == "" {
		return s.mux
	}
	return http.HandlerFunc(s.handleWithOperatorAuth)
}

func (s *Server) WithOperatorToken(token string) *Server {
	s.operatorToken = strings.TrimSpace(token)
	return s
}

func (s *Server) WithSessionActions(actions SessionActionRunner) *Server {
	s.sessionActions = actions
	return s
}

func (s *Server) WithRawProvider(p provider.Provider) *Server {
	s.rawProvider = p
	return s
}

func (s *Server) WithToolCatalog(catalog ToolCatalog) *Server {
	s.toolCatalog = catalog
	return s
}

func (s *Server) WithToolExecutor(executor RawToolExecutor) *Server {
	s.toolExecutor = executor
	return s
}

func (s *Server) WithProviderPreviewer(previewer ProviderPreviewer) *Server {
	s.previewer = previewer
	return s
}

func (s *Server) WithSessionHeadStore(store SessionHeadStore) *Server {
	s.sessionHeads = store
	return s
}

func (s *Server) WithRawSessionLogDir(dir string) *Server {
	s.rawSessionLogDir = strings.TrimSpace(dir)
	return s
}

func (s *Server) WithRawVFSRootDir(dir string) *Server {
	s.rawVFSRootDir = strings.TrimSpace(dir)
	return s
}

func (s *Server) handleWithOperatorAuth(w http.ResponseWriter, r *http.Request) {
	if s.isOperatorExempt(r) || s.isAuthorizedOperator(r) {
		s.mux.ServeHTTP(w, r)
		return
	}
	w.Header().Set("WWW-Authenticate", `Bearer realm="teamd-runtime"`)
	writeJSON(w, http.StatusUnauthorized, NewErrorResponse("unauthorized", "operator token required"))
}

func (s *Server) isOperatorExempt(r *http.Request) bool {
	if r.Method == http.MethodGet && r.URL.Path == "/api/runtime" {
		return true
	}
	if r.Method == http.MethodGet && (r.URL.Path == "/debug/test-bench" || strings.HasPrefix(r.URL.Path, "/debug/assets/")) {
		return true
	}
	return false
}

func (s *Server) isAuthorizedOperator(r *http.Request) bool {
	token := strings.TrimSpace(s.operatorToken)
	if token == "" {
		return true
	}
	auth := strings.TrimSpace(r.Header.Get("Authorization"))
	if !strings.HasPrefix(auth, "Bearer ") {
		return false
	}
	provided := strings.TrimSpace(strings.TrimPrefix(auth, "Bearer "))
	if provided == "" {
		return false
	}
	return subtle.ConstantTimeCompare([]byte(provided), []byte(token)) == 1
}

func (s *Server) routes() {
	s.mux.HandleFunc("/debug/test-bench", s.handleDebugShell)
	s.mux.HandleFunc("/debug/assets/app.js", s.handleDebugShellAsset)
	s.mux.HandleFunc("/debug/assets/styles.css", s.handleDebugShellAsset)
	s.mux.HandleFunc("/api/runtime", s.handleRuntime)
	s.mux.HandleFunc("/api/runtime/sessions/", s.handleRuntime)
	s.mux.HandleFunc("/api/session-actions", s.handleSessionActions)
	s.mux.HandleFunc("/api/sessions", s.handleSessions)
	s.mux.HandleFunc("/api/sessions/", s.handleSessions)
	s.mux.HandleFunc("/api/control/", s.handleControl)
	s.mux.HandleFunc("/api/debug/raw-network", s.handleDebug)
	s.mux.HandleFunc("/api/debug/raw-tool-exec", s.handleDebug)
	s.mux.HandleFunc("/api/debug/raw-conversations/", s.handleDebug)
	s.mux.HandleFunc("/api/debug/tools", s.handleDebug)
	s.mux.HandleFunc("/api/debug/sessions/", s.handleDebug)
	s.mux.HandleFunc("/api/debug/runs/", s.handleDebug)
	s.mux.HandleFunc("/api/events", s.handleEvents)
	s.mux.HandleFunc("/api/events/stream", s.handleEventStream)
	s.mux.HandleFunc("/api/plans", s.handlePlans)
	s.mux.HandleFunc("/api/plans/", s.handlePlans)
	s.mux.HandleFunc("/api/approvals", s.handleApprovals)
	s.mux.HandleFunc("/api/approvals/", s.handleApprovalAction)
	s.mux.HandleFunc("/api/memory/search", s.handleMemorySearch)
	s.mux.HandleFunc("/api/memory/", s.handleMemoryRead)
	s.mux.HandleFunc("/api/artifacts/", s.handleArtifacts)
	s.mux.HandleFunc("/api/jobs", s.handleJobs)
	s.mux.HandleFunc("/api/jobs/", s.handleJobs)
	s.mux.HandleFunc("/api/workers", s.handleWorkers)
	s.mux.HandleFunc("/api/workers/", s.handleWorkers)
	s.mux.HandleFunc("/api/runs", s.handleRuns)
	s.mux.HandleFunc("/api/runs/", s.handleRunByID)
}

func (s *Server) handleDebugShell(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		writeJSON(w, http.StatusMethodNotAllowed, NewErrorResponse("method_not_allowed", "method not allowed"))
		return
	}
	w.Header().Set("Cache-Control", "no-store")
	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	tokenBootstrap := "null"
	if token := strings.TrimSpace(s.operatorToken); token != "" {
		tokenBootstrap = strconv.Quote(token)
	}
	body := strings.Replace(debugShellHTML, "__OPERATOR_TOKEN__", tokenBootstrap, 1)
	body = strings.Replace(body, "__RUNTIME_DEFAULTS__", s.runtimeDefaultsBootstrap(), 1)
	_, _ = w.Write([]byte(body))
}

func (s *Server) runtimeDefaultsBootstrap() string {
	raw, err := json.Marshal(s.runtimeConfig)
	if err != nil {
		return "{}"
	}
	return string(raw)
}

func (s *Server) handleDebugShellAsset(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		writeJSON(w, http.StatusMethodNotAllowed, NewErrorResponse("method_not_allowed", "method not allowed"))
		return
	}
	w.Header().Set("Cache-Control", "no-store")
	switch r.URL.Path {
	case "/debug/assets/app.js":
		w.Header().Set("Content-Type", "application/javascript; charset=utf-8")
		_, _ = w.Write([]byte(debugShellJS))
	case "/debug/assets/styles.css":
		w.Header().Set("Content-Type", "text/css; charset=utf-8")
		_, _ = w.Write([]byte(debugShellCSS))
	default:
		http.NotFound(w, r)
	}
}

const debugShellHTML = `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>teamD Web Session Test Bench</title>
  <link rel="icon" href="data:,">
  <link rel="stylesheet" href="/debug/assets/styles.css">
</head>
<body>
  <script>window.__TEAMD_OPERATOR_TOKEN__ = __OPERATOR_TOKEN__;</script>
  <script>window.__TEAMD_RUNTIME_DEFAULTS__ = __RUNTIME_DEFAULTS__;</script>
  <div class="shell">
    <aside class="pane pane-sessions">
      <div class="pane-header">
        <div class="header-stack">
          <h1>teamD Web Session Test Bench</h1>
          <p class="header-copy">Local session lab for raw provider behavior, compaction, recall, artifacts, and prompt provenance.</p>
        </div>
        <span class="status-pill">local test bench</span>
      </div>
      <div class="rail-scroll">
        <section class="rail-section">
          <div class="section-head">
            <h3>Session Setup</h3>
            <p>Choose the local chat container, inspection mode, and active session.</p>
          </div>
          <div class="rail-grid rail-grid-top">
            <label class="field">
              <span>Chat ID</span>
              <input id="chat-id-input" type="number" value="1001" min="1">
            </label>
            <label class="field">
              <span>Flow</span>
              <input id="mode-display" type="text" value="Raw Conversation" readonly>
            </label>
          </div>
          <div class="button-row">
            <button id="reload-sessions" type="button">Reload</button>
          </div>
          <form id="new-session-form" class="session-create-form">
            <input id="new-session-name" type="text" placeholder="new session name" autocomplete="off">
            <button type="submit">New Session</button>
          </form>
        </section>
        <section class="rail-section">
          <div class="section-head">
            <h3>Provider Runtime</h3>
            <p>Per-request provider overrides for raw probes and isolated session runs.</p>
          </div>
          <div class="subsection">
            <h4>Model</h4>
            <div class="rail-grid">
              <label class="field field-full">
                <span>Model code</span>
                <input id="model-select" type="text" value="glm-5-turbo" autocomplete="off">
              </label>
            </div>
          </div>
          <div class="subsection">
            <h4>Thinking</h4>
            <div class="rail-grid">
              <label class="field">
                <span>Reasoning</span>
                <select id="reasoning-mode-select">
                  <option value="">default</option>
                  <option value="enabled">enabled</option>
                  <option value="disabled">disabled</option>
                </select>
              </label>
              <label class="field field-toggle">
                <span>clear_thinking</span>
                <small>Provider flag. Off keeps prior reasoning blocks when the provider supports them. On asks for a clean reasoning chain.</small>
                <input id="clear-thinking-input" type="checkbox">
              </label>
            </div>
          </div>
          <div class="subsection">
            <h4>Sampling</h4>
            <div class="rail-grid">
              <label class="field">
                <span>Temperature</span>
                <input id="temperature-input" type="number" min="0" max="2" step="0.01" placeholder="default">
              </label>
              <label class="field">
                <span>Top P</span>
                <input id="top-p-input" type="number" min="0" max="1" step="0.01" placeholder="default">
              </label>
              <label class="field">
                <span>Max Tokens</span>
                <input id="max-tokens-input" type="number" min="1" step="1" placeholder="default">
              </label>
              <label class="field field-toggle">
                <span>Do Sample</span>
                <small>Allow temperature and top-p sampling.</small>
                <input id="do-sample-input" type="checkbox">
              </label>
            </div>
          </div>
          <div class="subsection">
            <h4>Response Shape</h4>
            <div class="rail-grid">
              <label class="field">
                <span>Response Format</span>
                <select id="response-format-select">
                  <option value="">default</option>
                  <option value="text">text</option>
                  <option value="json_object">json_object</option>
                </select>
              </label>
            </div>
          </div>
        </section>
        <section class="rail-section">
          <div class="section-head">
            <h3>Tools</h3>
            <p>Selected tools are attached directly to the raw conversation provider payload.</p>
          </div>
          <div class="subsection">
            <h4>System Prompt</h4>
            <div class="rail-grid rail-grid-context">
              <label class="field field-toggle">
                <span>include system prompt</span>
                <small>Prepend one raw system message to each provider request in this conversation.</small>
                <input id="include-system-prompt-input" type="checkbox">
              </label>
              <label class="field field-full">
                <span>System Prompt</span>
                <textarea id="system-prompt-input" rows="6" placeholder="Optional raw system prompt"></textarea>
              </label>
            </div>
          </div>
          <div class="subsection">
            <h4>Tool Execution</h4>
            <div class="rail-grid rail-grid-context">
              <label class="field field-toggle">
                <span>auto-approve tools</span>
                <small>When the model requests tools, execute allowed calls immediately and send the follow-up round automatically.</small>
                <input id="auto-approve-tools-input" type="checkbox">
              </label>
              <label class="field field-toggle">
                <span>offload old tool outputs</span>
                <small>Save older large tool results into VFS .agent/memory and keep only artifact previews in the provider payload.</small>
                <input id="offload-old-tools-input" type="checkbox">
              </label>
            </div>
          </div>
          <div id="tool-selection-summary" class="selection-summary empty">No tools selected.</div>
          <details id="tool-picker-shell" class="tool-picker-shell">
            <summary>Tool Picker</summary>
            <div id="tool-picker" class="tool-picker empty">Tool catalog loads here.</div>
          </details>
        </section>
      </div>
      <div id="sessions-list" class="list empty">Sessions load here</div>
    </aside>
    <main class="pane pane-chat">
      <div class="pane-header">
        <h2>Chat</h2>
        <span id="selected-session" class="status-pill">no session selected</span>
      </div>
      <div id="request-preview" class="panel empty">Request Preview appears here.</div>
      <div id="chat-transcript" class="panel empty">Select a session to inspect transcript and runtime state.</div>
      <div id="pending-tool-banner" class="panel pending-tool-banner hidden"></div>
      <form id="chat-form" class="composer">
        <input id="chat-input" type="text" placeholder="Send a test message" autocomplete="off">
        <span id="submit-status" class="composer-status">idle</span>
        <button type="submit">Send</button>
      </form>
    </main>
    <section class="pane pane-inspector">
      <div class="pane-header">
        <h2>Timeline & Inspector</h2>
      </div>
      <div id="timeline" class="panel empty">Transcript timeline, SessionHead, recall, compaction, and artifacts appear here.</div>
      <div id="inspector" class="panel empty">Prompt layers and budget provenance appear here.</div>
    </section>
  </div>
  <script src="/debug/assets/app.js"></script>
</body>
</html>`

const debugShellJS = `const state = {
  operatorToken: window.__TEAMD_OPERATOR_TOKEN__ || null,
  runtimeDefaults: window.__TEAMD_RUNTIME_DEFAULTS__ || {},
  chatID: 1001,
  sessions: [],
  selectedSession: null,
  sessionState: null,
  rawConversations: {},
  toolCatalog: [],
  selectedTools: [],
  manualToolLocks: {},
  submitStatus: { kind: "idle", message: "idle" },
};

function escapeHTML(value) {
  return String(value ?? "").replace(/[&<>"]/g, (ch) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[ch]));
}

function apiURL(path, params = {}) {
  const url = new URL(path, window.location.origin);
  Object.entries(params).forEach(([key, value]) => {
    if (value !== undefined && value !== null && value !== "") {
      url.searchParams.set(key, String(value));
    }
  });
  return url.toString();
}

async function apiJSON(path, options = {}) {
  const headers = {
    "Content-Type": "application/json",
    ...(options.headers || {}),
  };
  if (state.operatorToken) {
    headers.Authorization = "Bearer " + state.operatorToken;
  }
  const response = await fetch(path, {
    headers,
    ...options,
  });
  if (!response.ok) {
    const text = await response.text();
    throw new Error(text || ("HTTP " + response.status));
  }
  return response.json();
}

function writeRequestConfigToForm(config) {
  const current = config || {};
  const modelEl = document.getElementById("model-select");
  const reasoningEl = document.getElementById("reasoning-mode-select");
  const clearThinkingEl = document.getElementById("clear-thinking-input");
  const temperatureEl = document.getElementById("temperature-input");
  const topPEl = document.getElementById("top-p-input");
  const maxTokensEl = document.getElementById("max-tokens-input");
  const doSampleEl = document.getElementById("do-sample-input");
  const responseFormatEl = document.getElementById("response-format-select");
  if (modelEl) modelEl.value = current.Model || current.model || "";
  if (reasoningEl) reasoningEl.value = current.ReasoningMode || current.reasoning_mode || "";
  if (clearThinkingEl) clearThinkingEl.checked = Boolean(current.ClearThinking !== undefined ? current.ClearThinking : current.clear_thinking);
  if (temperatureEl) temperatureEl.value = current.Temperature !== undefined ? String(current.Temperature) : (current.temperature !== undefined ? String(current.temperature) : "");
  if (topPEl) topPEl.value = current.TopP !== undefined ? String(current.TopP) : (current.top_p !== undefined ? String(current.top_p) : "");
  if (maxTokensEl) maxTokensEl.value = current.MaxTokens !== undefined ? String(current.MaxTokens) : (current.max_tokens !== undefined ? String(current.max_tokens) : "");
  if (doSampleEl) doSampleEl.checked = Boolean(current.DoSample !== undefined ? current.DoSample : current.do_sample);
  if (responseFormatEl) responseFormatEl.value = current.ResponseFormat || current.response_format || "";
}

function readRequestConfigFromForm() {
  const model = (document.getElementById("model-select")?.value || "").trim();
  const reasoningMode = (document.getElementById("reasoning-mode-select")?.value || "").trim();
  const clearThinking = document.getElementById("clear-thinking-input")?.checked || false;
  const temperatureRaw = (document.getElementById("temperature-input")?.value || "").trim();
  const topPRaw = (document.getElementById("top-p-input")?.value || "").trim();
  const maxTokensRaw = (document.getElementById("max-tokens-input")?.value || "").trim();
  const doSample = document.getElementById("do-sample-input")?.checked || false;
  const responseFormat = (document.getElementById("response-format-select")?.value || "").trim();
  const config = {};
  if (model) config.model = model;
  if (reasoningMode) config.reasoning_mode = reasoningMode;
  config.clear_thinking = clearThinking;
  if (temperatureRaw !== "") config.temperature = Number(temperatureRaw);
  if (topPRaw !== "") config.top_p = Number(topPRaw);
  if (maxTokensRaw !== "") config.max_tokens = Number(maxTokensRaw);
  config.do_sample = doSample;
  if (responseFormat) config.response_format = responseFormat;
  return config;
}

async function loadToolCatalog() {
  try {
    const response = await apiJSON(apiURL("/api/debug/tools", { role: "telegram" }));
    state.toolCatalog = (response && response.items) || [];
  } catch (error) {
    console.error(error);
    state.toolCatalog = [];
  }
  renderToolPicker();
  renderRequestPreview();
}

function selectedToolDefinitions() {
  if (!state.selectedTools.length) return [];
  const selected = new Set(state.selectedTools.map((item) => String(item || "").trim()));
  return (state.toolCatalog || []).filter((item) => selected.has(item.Name || item.name || ""));
}

function offloadOldToolOutputsEnabled() {
  return document.getElementById("offload-old-tools-input")?.checked || false;
}

function normalizeProviderMessages(messages) {
  return (messages || []).filter((message) => String(message.Role || message.role || "").toLowerCase() !== "system").map((message) => ({
    role: message.Role || message.role || "",
    content: message.Content !== undefined ? message.Content : (message.content || ""),
    name: message.Name || message.name || "",
    tool_call_id: message.ToolCallID || message.tool_call_id || "",
    ToolCalls: message.ToolCalls || message.tool_calls || [],
  }));
}

function toggleSelectedTool(name) {
  const next = new Set(state.selectedTools);
  if (next.has(name)) {
    next.delete(name);
  } else {
    next.add(name);
  }
  state.selectedTools = Array.from(next);
  renderToolPicker();
  renderRequestPreview();
}

function renderSelectedToolsSummary() {
  const root = document.getElementById("tool-selection-summary");
  if (!root) return;
  if (!state.selectedTools.length) {
    root.className = "selection-summary empty";
    root.textContent = "No tools selected.";
    return;
  }
  root.className = "selection-summary";
  root.innerHTML = state.selectedTools.map((item) => '<span class="selection-pill">' + escapeHTML(item) + '</span>').join("");
}

function renderToolPicker() {
  const root = document.getElementById("tool-picker");
  if (!root) return;
  const items = state.toolCatalog || [];
  if (!items.length) {
    root.className = "tool-picker empty";
    root.textContent = "No tool catalog available.";
    renderSelectedToolsSummary();
    return;
  }
  const selected = new Set(state.selectedTools);
  root.className = "tool-picker";
  root.innerHTML = '<div class="tool-picker-head">Tool Picker</div>'
    + '<div class="tool-picker-grid">'
    + items.map((item) => {
      const toolName = item.name || item.Name || "";
      const toolDesc = item.description || item.Description || "no description";
      const active = selected.has(toolName) ? " active" : "";
      return '<label class="tool-toggle' + active + '">'
        + '<input type="checkbox" data-tool-name="' + escapeHTML(toolName) + '"' + (selected.has(toolName) ? ' checked' : '') + '>'
        + '<span class="tool-toggle-copy">'
        + '<span class="tool-chip-name">' + escapeHTML(toolName) + '</span>'
        + '<span class="tool-chip-desc">' + escapeHTML(toolDesc) + '</span>'
        + '</span>'
        + '</label>';
    }).join("")
    + '</div>';
  root.querySelectorAll("[data-tool-name]").forEach((button) => {
    button.addEventListener("change", () => {
      toggleSelectedTool(button.getAttribute("data-tool-name") || "");
    });
  });
  renderSelectedToolsSummary();
}

function setSubmitStatus(kind, message) {
  state.submitStatus = { kind, message };
  const root = document.getElementById("submit-status");
  if (!root) return;
  root.className = "composer-status " + kind;
  root.textContent = message;
}

function sessionLabel(item) {
  const active = item.LatestRun && item.LatestRun.Active ? " active" : "";
  const recent = item.Head && item.Head.LastResultSummary ? " recent-work" : "";
  const latestRun = item.LatestRun || {};
  return '<details class="session-entry' + active + recent + '"' + (state.selectedSession === item.SessionID ? ' open' : '') + '>'
    + '<summary data-session-id="' + escapeHTML(item.SessionID) + '">'
    + '<span class="session-summary-title">' + escapeHTML(item.SessionID) + '</span>'
    + '<span class="session-summary-meta">' + escapeHTML(latestRun.Status || "idle") + '</span>'
    + '</summary>'
    + '<div class="session-entry-body">'
    + '<div class="session-entry-goal">' + escapeHTML(item.Head && item.Head.CurrentGoal ? item.Head.CurrentGoal : "no current goal") + '</div>'
    + '<div class="session-entry-meta">last activity: ' + escapeHTML(item.LastActivityAt || "n/a") + '</div>'
    + '<div class="session-entry-meta">latest run: ' + escapeHTML(latestRun.RunID || "none") + '</div>'
    + '</div>'
    + '</details>';
}

function renderSessions() {
  const sessionsEl = document.getElementById("sessions-list");
  if (!sessionsEl) return;
  if (!state.sessions.length) {
    sessionsEl.classList.add("empty");
    sessionsEl.textContent = "No sessions yet for this chat.";
    return;
  }
  sessionsEl.classList.remove("empty");
  sessionsEl.innerHTML = state.sessions.map(sessionLabel).join("");
  sessionsEl.querySelectorAll("[data-session-id]").forEach((summary) => {
    summary.addEventListener("click", (event) => {
      event.preventDefault();
      const details = summary.closest("details");
      if (details) {
        details.open = true;
      }
      selectSession(summary.getAttribute("data-session-id"));
    });
  });
  renderRequestPreview();
}

function upsertSession(session) {
  if (!session || !session.SessionID) return;
  const idx = state.sessions.findIndex((item) => item.SessionID === session.SessionID);
  if (idx >= 0) {
    state.sessions[idx] = session;
  } else {
    state.sessions.unshift(session);
  }
}

function rawConversationHistory(sessionID) {
  return state.rawConversations[sessionID] || { messages: [], turns: [] };
}

function saveRawConversationHistory(sessionID, payload) {
  state.rawConversations[sessionID] = payload;
  try {
    window.localStorage.setItem("teamd.rawConversation." + sessionID, JSON.stringify(payload));
  } catch (_error) {}
}

function loadRawConversationHistory(sessionID) {
  if (!sessionID) return { messages: [], turns: [] };
  if (state.rawConversations[sessionID]) {
    return state.rawConversations[sessionID];
  }
  try {
    const raw = window.localStorage.getItem("teamd.rawConversation." + sessionID);
    const parsed = raw ? JSON.parse(raw) : { messages: [], turns: [] };
    if (Array.isArray(parsed)) {
      state.rawConversations[sessionID] = { messages: parsed, turns: [] };
      return state.rawConversations[sessionID];
    }
    if (parsed && typeof parsed === "object") {
      state.rawConversations[sessionID] = {
        messages: Array.isArray(parsed.messages) ? parsed.messages : [],
        turns: Array.isArray(parsed.turns) ? parsed.turns : [],
      };
      return state.rawConversations[sessionID];
    }
  } catch (_error) {}
  state.rawConversations[sessionID] = { messages: [], turns: [] };
  return state.rawConversations[sessionID];
}

function summarizeUsage(result) {
  const response = (result && result.response) || {};
  const rawResponse = parseJSONBody((result && result.trace && result.trace.provider_response_body) || "");
  const usage = rawResponse && rawResponse.usage ? rawResponse.usage : (response.Usage || {});
  return usage || {};
}

function renderUsageRows(usage) {
  const reasoningTokens = usage.completion_tokens_details && usage.completion_tokens_details.reasoning_tokens !== undefined
    ? usage.completion_tokens_details.reasoning_tokens
    : "";
  const cachedTokens = usage.prompt_tokens_details && usage.prompt_tokens_details.cached_tokens !== undefined
    ? usage.prompt_tokens_details.cached_tokens
    : (usage.cached_tokens !== undefined ? usage.cached_tokens : "");
  return renderKVList([
    ["prompt_tokens", usage.prompt_tokens],
    ["completion_tokens", usage.completion_tokens],
    ["reasoning_tokens", reasoningTokens],
    ["cached_tokens", cachedTokens],
    ["total_tokens", usage.total_tokens],
  ]);
}

function renderHTTPBlock(title, summary, body, open) {
  return '<details class="http-section"' + (open ? ' open' : '') + '>'
    + '<summary>' + escapeHTML(title) + '<span class="json-kind">' + escapeHTML(summary || "") + '</span></summary>'
    + body
    + '</details>';
}

function renderRawTraceRequest(trace) {
  return ''
    + '<div class="http-block"><div class="http-line-label">URL</div><pre>' + escapeHTML(trace.provider_url || "") + '</pre></div>'
    + renderHTTPBlock("Headers", "request", '<pre>' + escapeHTML(JSON.stringify(trace.provider_request_headers || {}, null, 2)) + '</pre>', false)
    + renderHTTPBlock("Body", "json", renderMaybeJSONTree(trace.provider_request_body || ""), true);
}

function renderRawTraceResponse(trace) {
  return ''
    + '<div class="http-block"><div class="http-line-label">Status</div><pre>' + escapeHTML(String(trace.provider_status_code || 0)) + '</pre></div>'
    + renderHTTPBlock("Headers", "response", '<pre>' + escapeHTML(JSON.stringify(trace.provider_response_headers || {}, null, 2)) + '</pre>', false)
    + renderHTTPBlock("Body", "json", renderMaybeJSONTree(trace.provider_response_body || ""), true);
}

function renderRawTurnDiff(current, previous, field) {
  if (!previous) {
    return '<div class="empty-note">No previous turn.</div>';
  }
  const diffs = [];
  collectDiffs(
    normalizeForDiff(current && current[field] ? current[field] : {}),
    normalizeForDiff(previous && previous[field] ? previous[field] : {}),
    "",
    diffs
  );
  if (!diffs.length) {
    return '<div class="empty-note">No diff.</div>';
  }
  return '<pre class="compact-pre">' + escapeHTML(JSON.stringify(diffs.slice(0, 50), null, 2)) + '</pre>';
}

function toolCallsForTurn(turn) {
  return (turn && turn.response && Array.isArray(turn.response.ToolCalls)) ? turn.response.ToolCalls : [];
}

function renderManualToolActions(turn, index) {
  const calls = toolCallsForTurn(turn);
  if (!calls.length) {
    return '';
  }
  return '<div class="manual-tools"><div class="inline-answer-label">Manual Tool Step</div>'
    + calls.map((call, callIndex) => {
      const key = String(index) + ':' + String(callIndex);
      const status = turn.manualSteps && turn.manualSteps[key] ? turn.manualSteps[key] : null;
      const summary = status
        ? '<span class="manual-tool-status">' + escapeHTML(status.status || 'done') + '</span>'
        : '';
      const disabled = status && (status.status === "running" || status.status === "completed" || status.status === "failed");
      return '<div class="manual-tool-row">'
        + '<button type="button" class="mode-button" data-manual-tool-index="' + escapeHTML(String(index)) + '" data-manual-tool-call="' + escapeHTML(String(callIndex)) + '"' + (disabled ? ' disabled' : '') + '>' + escapeHTML((status && status.status === "running") ? ("Running " + (call.Name || 'tool')) : ("Run " + (call.Name || 'tool'))) + '</button>'
        + summary
        + '</div>';
    }).join('')
    + '</div>';
}

function latestPendingToolCallBundle() {
  const sessionID = state.selectedSession || "";
  const conversation = rawConversationHistory(sessionID);
  const turns = conversation.turns || [];
  for (let turnIndex = turns.length - 1; turnIndex >= 0; turnIndex -= 1) {
    const turn = turns[turnIndex];
    const calls = toolCallsForTurn(turn);
    if (!calls.length) continue;
    const pending = calls.map((call, callIndex) => ({
      call,
      callIndex,
      key: String(turnIndex) + ':' + String(callIndex),
    })).filter((item) => {
      const status = turn.manualSteps && turn.manualSteps[item.key] ? turn.manualSteps[item.key].status : "";
      return status !== "completed" && status !== "failed";
    });
    if (pending.length) {
      return { turn, turnIndex, pending };
    }
  }
  return null;
}

function renderPendingToolBanner() {
  const root = document.getElementById("pending-tool-banner");
  if (!root) return;
  const bundle = latestPendingToolCallBundle();
  if (!bundle) {
    root.classList.add("hidden");
    root.innerHTML = "";
    return;
  }
  root.classList.remove("hidden");
  root.innerHTML = ''
    + '<div class="pending-tool-heading"><strong>Tool confirmation required</strong><span class="pending-tool-note">Model requested a tool call. Confirm execution here or inside the latest turn.</span></div>'
    + '<div class="pending-tool-actions">'
    + bundle.pending.map(({ call, callIndex }) => {
      const key = String(bundle.turnIndex) + ':' + String(callIndex);
      const status = bundle.turn.manualSteps && bundle.turn.manualSteps[key] ? bundle.turn.manualSteps[key] : null;
      const disabled = status && (status.status === "running" || status.status === "completed" || status.status === "failed");
      return '<div class="pending-tool-card">'
        + '<strong>' + escapeHTML(call.Name || 'tool') + '</strong>'
        + '<pre class="pending-tool-args">' + escapeHTML(JSON.stringify(call.Arguments || {}, null, 2)) + '</pre>'
        + '<button type="button" class="mode-button" data-manual-tool-index="' + escapeHTML(String(bundle.turnIndex)) + '" data-manual-tool-call="' + escapeHTML(String(callIndex)) + '"' + (disabled ? ' disabled' : '') + '>' + escapeHTML((status && status.status === "running") ? ("Запускается " + (call.Name || 'tool')) : ("Подтвердить запуск " + (call.Name || 'tool'))) + '</button>'
        + '</div>';
    }).join("")
    + '</div>';
}

function normalizeForDiff(value) {
  if (value === null || typeof value !== "object") {
    return value;
  }
  if (Array.isArray(value)) {
    return value.map(normalizeForDiff);
  }
  const out = {};
  for (const key of Object.keys(value).sort()) {
    out[key] = normalizeForDiff(value[key]);
  }
  return out;
}

function collectDiffs(left, right, path, out) {
  if (JSON.stringify(left) === JSON.stringify(right)) {
    return;
  }
  const leftObj = left && typeof left === "object";
  const rightObj = right && typeof right === "object";
  if (!leftObj || !rightObj || Array.isArray(left) !== Array.isArray(right)) {
    out.push({ path: path || "root", left, right });
    return;
  }
  if (Array.isArray(left) && Array.isArray(right)) {
    const length = Math.max(left.length, right.length);
    for (let i = 0; i < length; i++) {
      collectDiffs(left[i], right[i], (path || "root") + "[" + i + "]", out);
    }
    return;
  }
  const keys = new Set([...Object.keys(left), ...Object.keys(right)]);
  Array.from(keys).sort().forEach((key) => {
    collectDiffs(left[key], right[key], path ? path + "." + key : key, out);
  });
}

const DISPLAY_MESSAGE_LIMIT = 10;
const DISPLAY_TURN_LIMIT = 10;
const DISPLAY_MESSAGE_CHARS = 600;

function displayTrimText(text, maxChars) {
  const value = String(text || "");
  const limit = Number(maxChars || DISPLAY_MESSAGE_CHARS);
  if (value.length <= limit) {
    return value;
  }
  return value.slice(0, limit) + "… [display truncated]";
}

function messageContentValue(message) {
  if (!message || typeof message !== "object") return "";
  if (message.content !== undefined) return String(message.content || "");
  if (message.Content !== undefined) return String(message.Content || "");
  return "";
}

function displayMessagesPreview(messages) {
  const source = Array.isArray(messages) ? messages : [];
  const slice = source.slice(-DISPLAY_MESSAGE_LIMIT);
  return slice.map((message) => {
    const next = Object.assign({}, message);
    const content = messageContentValue(message);
    if (content) {
      next.content = displayTrimText(content, DISPLAY_MESSAGE_CHARS);
      delete next.Content;
    }
    return next;
  });
}

function displayTurnsPreview(turns) {
  const source = Array.isArray(turns) ? turns : [];
  return source.slice(-DISPLAY_TURN_LIMIT);
}

function renderTranscript(view) {
  const transcriptEl = document.getElementById("chat-transcript");
  if (!transcriptEl) return;
  const sessionID = state.selectedSession || "";
  const conversation = rawConversationHistory(sessionID);
  const turns = conversation.turns || [];
  if (!turns.length) {
    transcriptEl.classList.add("empty");
    transcriptEl.textContent = "Raw Conversation has no turns yet.";
    renderPendingToolBanner();
    return;
  }
  transcriptEl.classList.remove("empty");
  const displayTurns = displayTurnsPreview(turns);
  const turnOffset = Math.max(turns.length - displayTurns.length, 0);
  const toolbar = '<div class="mode-toolbar"><button type="button" class="mode-button" data-collapse-target=".raw-turn" data-collapse-value="collapse">Collapse Turns</button><button type="button" class="mode-button" data-collapse-target=".raw-turn" data-collapse-value="expand">Expand Turns</button></div>';
  const summary = '<div class="empty-note">Showing last ' + escapeHTML(String(displayTurns.length)) + ' of ' + escapeHTML(String(turns.length)) + ' turns for display.</div>';
  transcriptEl.innerHTML = displayTurns.map((turn, index) => {
    const response = turn.response || {};
    const trace = turn.trace || {};
    const usage = summarizeUsage(turn);
    const absoluteIndex = turnOffset + index;
    return '<details class="raw-turn event-card raw-turn-card"' + (index === displayTurns.length - 1 ? ' open' : '') + '>'
      + '<summary><strong>Turn #' + escapeHTML(String(absoluteIndex + 1)) + '</strong><span>' + escapeHTML(displayTrimText(response.Text || "no answer yet", 240)) + '</span></summary>'
      + '<div class="turn-label-row"><span>Prompt</span><span>' + escapeHTML(displayTrimText(turn.query || "", 240)) + '</span></div>'
      + '<div class="turn-label-row"><span>Status</span><span>' + escapeHTML(String(trace.provider_status_code || 0)) + ' · ' + escapeHTML(String(usage.total_tokens || 0)) + ' tokens</span></div>'
      + renderHTTPBlock("Request", trace.provider_url || "", renderRawTraceRequest(trace), false)
      + renderHTTPBlock("Response", String(trace.provider_status_code || 0), renderRawTraceResponse(trace), false)
      + renderHTTPBlock("Usage", String(usage.total_tokens || 0) + " tokens", renderUsageRows(usage), false)
      + renderHTTPBlock("Diff vs previous request", "json", renderRawTurnDiff(turn, turns[absoluteIndex - 1], "request"), false)
      + renderHTTPBlock("Diff vs previous response", "json", renderRawTurnDiff(turn, turns[absoluteIndex - 1], "response"), false)
      + renderManualToolActions(turn, absoluteIndex)
      + '</details>';
  }).join("");
  transcriptEl.innerHTML = toolbar + summary + transcriptEl.innerHTML;
  renderPendingToolBanner();
  bindManualToolActions();
}

function renderKVList(items) {
  const rows = items
    .filter((item) => item && item[1] !== undefined && item[1] !== null && item[1] !== "")
    .map(([label, value]) => '<div class="kv-row"><span class="kv-label">' + escapeHTML(label) + '</span><span class="kv-value">' + escapeHTML(value) + '</span></div>');
  if (!rows.length) {
    return '<div class="empty-note">No data.</div>';
  }
  return rows.join("");
}

function renderStringList(items) {
  const values = Array.isArray(items) ? items.filter((item) => item !== undefined && item !== null && String(item) !== "") : [];
  if (!values.length) {
    return '<div class="empty-note">No data.</div>';
  }
  return '<ul class="simple-list">' + values.map((item) => '<li>' + escapeHTML(String(item)) + '</li>').join("") + '</ul>';
}

function renderSessionHead(head) {
  if (!head) {
    return '<div class="empty-note">No SessionHead recorded for this session.</div>';
  }
  return ''
    + '<div class="section-card"><h3>Session Head</h3>'
    + renderKVList([
      ["last_completed_run_id", head.LastCompletedRunID || ""],
      ["current_goal", head.CurrentGoal || ""],
      ["last_result_summary", head.LastResultSummary || ""],
      ["current_project", head.CurrentProject || ""],
      ["current_plan_id", head.CurrentPlanID || ""],
      ["current_plan_title", head.CurrentPlanTitle || ""],
      ["updated_at", head.UpdatedAt || ""],
    ])
    + '</div>'
    + '<div class="section-card"><h3>Plan Items</h3>' + renderStringList(head.CurrentPlanItems || []) + '</div>'
    + '<div class="section-card"><h3>Resolved Entities</h3>' + renderStringList(head.ResolvedEntities || []) + '</div>'
    + '<div class="section-card"><h3>Recent Artifacts</h3>' + renderStringList(head.RecentArtifactRefs || []) + '</div>'
    + '<div class="section-card"><h3>Open Loops</h3>' + renderStringList(head.OpenLoops || []) + '</div>';
}

function renderJSONTreeNode(value, label, open) {
  const title = label ? '<span class="json-label">' + escapeHTML(label) + '</span>' : '<span class="json-label">value</span>';
  if (value === null || typeof value !== "object") {
    return '<div class="json-leaf">' + title + '<span class="json-scalar">' + escapeHTML(JSON.stringify(value)) + '</span></div>';
  }
  if (Array.isArray(value)) {
    const items = value.map((item, index) => renderJSONTreeNode(item, '[' + index + ']', false)).join("");
    return '<details class="json-node"' + (open ? ' open' : '') + '><summary>' + title + '<span class="json-kind">array(' + value.length + ')</span></summary>' + items + '</details>';
  }
  const entries = Object.entries(value);
  const children = entries.map(([key, child]) => renderJSONTreeNode(child, key, false)).join("");
  return '<details class="json-node"' + (open ? ' open' : '') + '><summary>' + title + '<span class="json-kind">object(' + entries.length + ')</span></summary>' + children + '</details>';
}

function renderMaybeJSONTree(body) {
  const text = String(body || "");
  try {
    const parsed = JSON.parse(text);
    return '<div class="json-tree">' + renderJSONTreeNode(parsed, 'root', true) + '</div>';
  } catch (_error) {
    return '<pre>' + escapeHTML(text) + '</pre>';
  }
}

function currentDraftQuery() {
  return (document.getElementById("chat-input")?.value || "").trim();
}

function currentSystemPrompt() {
  return (document.getElementById("system-prompt-input")?.value || "").trim();
}

function includeSystemPrompt() {
  return Boolean(document.getElementById("include-system-prompt-input")?.checked);
}

function autoApproveToolsEnabled() {
  return Boolean(document.getElementById("auto-approve-tools-input")?.checked);
}

function applyLocalToolOffloadPreview(messages) {
  const next = (messages || []).map((message) => Object.assign({}, message));
  next.forEach((message, index) => {
    if (index === next.length - 1) return;
    if (String(message.role || message.Role || "").toLowerCase() !== "tool") return;
    const content = String(message.content !== undefined ? message.content : (message.Content || ""));
    if (content.length < 1200) return;
    if (content.includes("Artifact offloaded to .agent/memory/")) return;
    const preview = content.replace(/\s+/g, " ").trim().slice(0, 240);
    message.content = "Artifact offloaded to .agent/memory/<on-send>.txt\nsize: " + content.length + " chars\npreview: " + preview + (content.length > 240 ? "..." : "");
  });
  return next;
}

function buildRequestPreview() {
  const query = currentDraftQuery();
  const config = readRequestConfigFromForm();
  const conversation = rawConversationHistory(state.selectedSession || "");
  const systemPrompt = currentSystemPrompt();
  const includeSystem = includeSystemPrompt();
  const messages = (conversation.messages || []).slice();
  if (query) {
    messages.push({ role: "user", content: query });
  }
  const providerMessages = offloadOldToolOutputsEnabled() ? applyLocalToolOffloadPreview(messages) : messages.slice();
  if (includeSystem && systemPrompt) {
    providerMessages.unshift({ role: "system", content: systemPrompt });
  }
  return {
    mode: "raw-conversation",
    endpoint: "/api/debug/raw-network",
    payload: {
      chat_id: state.chatID,
      session_id: state.selectedSession || "",
      system_prompt: systemPrompt,
      include_system_prompt: includeSystem,
      messages: providerMessages,
      tools: state.selectedTools.slice(),
      offload_old_tool_outputs: offloadOldToolOutputsEnabled(),
      config,
    },
    meta: {
      raw_messages: messages.length,
      auto_approve_tools: autoApproveToolsEnabled(),
      offload_old_tool_outputs: offloadOldToolOutputsEnabled(),
    },
  };
}

async function loadRequestPreview() {
  renderRequestPreview();
}

function renderRequestPreview() {
  const root = document.getElementById("request-preview");
  if (!root) return;
  const preview = buildRequestPreview();
  const displayPayload = Object.assign({}, preview.payload, {
    messages: displayMessagesPreview(preview.payload.messages),
  });
  root.classList.remove("empty");
  root.innerHTML = '<h3>Request Preview</h3>'
    + '<div class="section-card">'
    + renderKVList([
      ["mode", preview.mode],
      ["endpoint", preview.endpoint],
      ["selected_session", state.selectedSession || "none"],
      ["draft_query", currentDraftQuery() || "empty"],
      ["messages", String(preview.meta.raw_messages || 0)],
      ["system_prompt", includeSystemPrompt() ? (currentSystemPrompt() || "empty") : "disabled"],
      ["tools", state.selectedTools.join(", ") || "none"],
      ["auto_approve_tools", preview.meta.auto_approve_tools ? "on" : "off"],
      ["offload_old_tool_outputs", preview.meta.offload_old_tool_outputs ? "on" : "off"],
      ["display_messages", "last " + String(Math.min(DISPLAY_MESSAGE_LIMIT, preview.meta.raw_messages || 0)) + " (truncated)"],
    ])
    + '</div>'
    + '<div class="section-card"><h3>Selected Tool Definitions</h3>'
    + renderMaybeJSONTree(JSON.stringify(selectedToolDefinitions(), null, 2))
    + '</div>'
    + '<div class="section-card"><h3>Provider Payload</h3>'
    + '<div class="empty-note">Display-only preview: showing last ' + escapeHTML(String(Math.min(DISPLAY_MESSAGE_LIMIT, preview.meta.raw_messages || 0))) + ' messages with trimmed content. Real request is unchanged.</div>'
    + renderMaybeJSONTree(JSON.stringify(displayPayload, null, 2))
    + '</div>';
}

function parseJSONBody(body) {
  try {
    return JSON.parse(String(body || ""));
  } catch (_error) {
    return null;
  }
}

function toggleNestedDetails(root, open) {
  if (!root) return;
  root.querySelectorAll(".http-section, .json-node").forEach((details) => {
    details.open = open;
  });
}

function toggleTopLevelCollapse(selector, open) {
  document.querySelectorAll(selector).forEach((details) => {
    details.open = open;
    toggleNestedDetails(details, open);
  });
}

function bindCollapseToggles() {
  document.querySelectorAll("[data-collapse-target]").forEach((button) => {
    button.addEventListener("click", () => {
      const selector = button.getAttribute("data-collapse-target") || "";
      const open = button.getAttribute("data-collapse-value") === "expand";
      toggleTopLevelCollapse(selector, open);
    });
  });
}

async function runManualToolStep(turnIndex, callIndex) {
  const history = loadRawConversationHistory(state.selectedSession);
  const turn = history.turns[turnIndex];
  const call = toolCallsForTurn(turn)[callIndex];
  if (!turn || !call) {
    return;
  }
  const stepKey = String(turnIndex) + ':' + String(callIndex);
  if (state.manualToolLocks[stepKey]) {
    return;
  }
  state.manualToolLocks[stepKey] = true;
  const runningSteps = Object.assign({}, turn.manualSteps || {});
  runningSteps[stepKey] = Object.assign({}, runningSteps[stepKey] || {}, {
    status: "running",
    started_at: new Date().toISOString(),
  });
  history.turns[turnIndex] = Object.assign({}, turn, { manualSteps: runningSteps });
  saveRawConversationHistory(state.selectedSession, history);
  renderTranscript(null);
  renderInspector(null, null);
  renderRequestPreview();
  setSubmitStatus("running", "executing manual tool step");
  try {
    const toolResult = await apiJSON("/api/debug/raw-tool-exec", {
      method: "POST",
      body: JSON.stringify({
        chat_id: state.chatID,
        session_id: state.selectedSession,
        tools: state.selectedTools.slice(),
        call,
      }),
    });
    const nextMessages = (history.messages || []).concat([{
      role: "tool",
      tool_call_id: call.ID,
      content: toolResult.output || "",
      name: call.Name || "",
    }]);
    const config = readRequestConfigFromForm();
    const nextResult = await apiJSON("/api/debug/raw-network", {
      method: "POST",
      body: JSON.stringify({
        chat_id: state.chatID,
        session_id: state.selectedSession,
        system_prompt: currentSystemPrompt(),
        include_system_prompt: includeSystemPrompt(),
        messages: nextMessages,
        tools: state.selectedTools.slice(),
        offload_old_tool_outputs: offloadOldToolOutputsEnabled(),
        config,
      }),
    });
    const responseText = nextResult && nextResult.response ? nextResult.response.Text || "" : "";
    const updatedMessages = normalizeProviderMessages((nextResult.request && nextResult.request.Messages) || nextMessages).concat([{
      role: "assistant",
      content: responseText,
      ToolCalls: (nextResult.response && nextResult.response.ToolCalls) || [],
    }]);
    const manualSteps = Object.assign({}, history.turns[turnIndex].manualSteps || {});
    manualSteps[stepKey] = {
      status: toolResult && toolResult.success === false ? "failed" : "completed",
      output: toolResult.output || "",
      error: toolResult && toolResult.error_message ? toolResult.error_message : "",
      completed_at: new Date().toISOString(),
    };
    history.turns[turnIndex] = Object.assign({}, history.turns[turnIndex], { manualSteps });
    history.turns.push({
      query: "[manual tool step] " + (call.Name || "tool"),
      request: nextResult.request || {},
      response: nextResult.response || {},
      trace: nextResult.trace || {},
      logPath: nextResult.log_path || "",
      systemPrompt: currentSystemPrompt(),
      includeSystemPrompt: includeSystemPrompt(),
      config,
    });
    saveRawConversationHistory(state.selectedSession, {
      messages: updatedMessages,
      turns: history.turns,
    });
    if (toolResult && toolResult.success === false) {
      setSubmitStatus("success", "tool failed, error sent back to model");
    } else {
      setSubmitStatus("success", "tool executed and follow-up sent");
    }
    renderSessions();
    renderTranscript(null);
    renderInspector(null, null);
    renderRequestPreview();
    if (autoApproveToolsEnabled()) {
      await maybeAutoApproveLatestTurn();
    }
  } finally {
    delete state.manualToolLocks[stepKey];
  }
}

async function maybeAutoApproveLatestTurn() {
  const history = loadRawConversationHistory(state.selectedSession);
  const turns = history.turns || [];
  if (!turns.length) return;
  const turnIndex = turns.length - 1;
  const turn = turns[turnIndex];
  const calls = toolCallsForTurn(turn);
  for (let callIndex = 0; callIndex < calls.length; callIndex += 1) {
    const key = String(turnIndex) + ':' + String(callIndex);
    const status = turn.manualSteps && turn.manualSteps[key] ? turn.manualSteps[key] : null;
    if (!status || (status.status !== "running" && status.status !== "completed")) {
      await runManualToolStep(turnIndex, callIndex);
      return;
    }
  }
}

function bindManualToolActions() {
  document.querySelectorAll("[data-manual-tool-index]").forEach((button) => {
    button.addEventListener("click", async () => {
      try {
        await runManualToolStep(
          Number(button.getAttribute("data-manual-tool-index")),
          Number(button.getAttribute("data-manual-tool-call")),
        );
      } catch (error) {
        console.error(error);
        setSubmitStatus("error", error.message || "manual tool step failed");
      }
    });
  });
}

function renderInspector(_view, _provenance) {
  const timelineEl = document.getElementById("timeline");
  const inspectorEl = document.getElementById("inspector");
  const selectedEl = document.getElementById("selected-session");
  if (selectedEl) {
    selectedEl.textContent = state.selectedSession || "no session selected";
  }
  const conversation = rawConversationHistory(state.selectedSession || "");
  const turns = conversation.turns || [];
  const displayTurns = displayTurnsPreview(turns);
  const turnOffset = Math.max(turns.length - displayTurns.length, 0);
  const lastTurn = turns.length ? turns[turns.length - 1] : null;
  const sessionState = state.sessionState || null;
  const sessionHead = sessionState && sessionState.Head ? sessionState.Head : null;
  if (timelineEl) {
    timelineEl.classList.remove("empty");
    timelineEl.innerHTML = '<h3>Conversation Summary</h3>'
      + '<div class="section-card"><h3>Raw Conversation</h3>'
      + renderKVList([
        ["session", state.selectedSession || "n/a"],
        ["turns", turns.length],
        ["messages", (conversation.messages || []).length],
        ["display_turns", String(displayTurns.length) + " / " + String(turns.length)],
        ["latest_answer", lastTurn && lastTurn.response ? lastTurn.response.Text || "" : "none"],
        ["system_prompt", lastTurn && lastTurn.includeSystemPrompt ? (lastTurn.systemPrompt || "empty") : "disabled"],
        ["log_path", lastTurn && lastTurn.logPath ? lastTurn.logPath : "none"],
      ])
      + '</div>';
  }
  if (inspectorEl) {
    inspectorEl.classList.remove("empty");
    inspectorEl.innerHTML = '<h3>Run Metrics</h3>'
      + renderSessionHead(sessionHead)
      + (turns.length
        ? '<div class="empty-note">Showing metrics for last ' + escapeHTML(String(displayTurns.length)) + ' of ' + escapeHTML(String(turns.length)) + ' turns.</div>'
        + displayTurns.map((turn, index) => {
            const usage = summarizeUsage(turn);
            return '<div class="section-card"><h3>Turn #' + escapeHTML(String(turnOffset + index + 1)) + '</h3>'
              + renderUsageRows(usage)
              + '<div class="inline-answer"><div class="inline-answer-label">Provider</div>'
              + renderKVList([
                ["model", turn.response ? turn.response.Model || "" : ""],
                ["finish_reason", turn.response ? turn.response.FinishReason || "stop" : "stop"],
                ["status_code", turn.trace ? turn.trace.provider_status_code || 0 : 0],
                ["duration_ms", turn.trace ? turn.trace.duration_ms || 0 : 0],
              ])
              + '</div></div>';
          }).join("")
        : '<div class="empty-note">No raw conversation turns yet.</div>');
  }
  bindCollapseToggles();
}

async function loadSessions() {
  state.chatID = Number(document.getElementById("chat-id-input")?.value || 1001);
  const result = await apiJSON(apiURL("/api/sessions", { chat_id: state.chatID }));
  state.sessions = result.items || [];
  renderSessions();
  if (!state.selectedSession && state.sessions.length) {
    await selectSession(state.sessions[0].SessionID);
  }
}

async function selectSession(sessionID) {
  state.selectedSession = sessionID;
  state.sessionState = null;
  const history = loadRawConversationHistory(sessionID);
  try {
    const remote = await apiJSON("/api/debug/raw-conversations/" + encodeURIComponent(sessionID));
    if (remote && Array.isArray(remote.messages) && Array.isArray(remote.turns)) {
      const localTurns = Array.isArray(history.turns) ? history.turns.length : 0;
      const localMessages = Array.isArray(history.messages) ? history.messages.length : 0;
      if (remote.turns.length > localTurns || remote.messages.length > localMessages) {
        saveRawConversationHistory(sessionID, {
          messages: remote.messages,
          turns: remote.turns,
        });
      }
    }
  } catch (_error) {}
  try {
    const sessionResponse = await apiJSON(apiURL("/api/sessions/" + encodeURIComponent(sessionID), { chat_id: state.chatID }));
    if (sessionResponse && sessionResponse.session) {
      state.sessionState = sessionResponse.session;
    }
  } catch (_error) {}
  renderSessions();
  renderTranscript(null);
  renderInspector(null, null);
  loadRequestPreview().catch((error) => console.error(error));
}

async function createSession(event) {
  event.preventDefault();
  const input = document.getElementById("new-session-name");
  const name = input?.value.trim();
  if (!name) return;
  await apiJSON("/api/session-actions", {
    method: "POST",
    body: JSON.stringify({
      chat_id: state.chatID,
      action: "session.create",
      session_name: name,
    }),
  });
  if (input) input.value = "";
  const sessionID = state.chatID + ":" + name.toLowerCase();
  upsertSession({ SessionID: sessionID, LastActivityAt: "", LatestRun: {}, Head: {} });
  renderSessions();
  await selectSession(sessionID);
}

async function submitMessage(event) {
  event.preventDefault();
  const input = document.getElementById("chat-input");
  const submitButton = document.querySelector("#chat-form button[type='submit']");
  const query = input?.value.trim();
  if (!query) return;
  const config = readRequestConfigFromForm();
  try {
    if (submitButton) submitButton.disabled = true;
    setSubmitStatus("running", "sending raw conversation turn");
    if (!state.selectedSession) {
      setSubmitStatus("error", "select a session first");
      return;
    }
    const history = loadRawConversationHistory(state.selectedSession);
    const messages = (history.messages || []).concat([{ role: "user", content: query }]);
    const result = await apiJSON("/api/debug/raw-network", {
      method: "POST",
      body: JSON.stringify({
        chat_id: state.chatID,
        session_id: state.selectedSession,
        system_prompt: currentSystemPrompt(),
        include_system_prompt: includeSystemPrompt(),
        messages,
        tools: state.selectedTools.slice(),
        offload_old_tool_outputs: offloadOldToolOutputsEnabled(),
        config,
      }),
    });
    const responseText = result && result.response ? result.response.Text || "" : "";
    const updatedMessages = normalizeProviderMessages((result.request && result.request.Messages) || messages).concat([{
      role: "assistant",
      content: responseText,
      ToolCalls: (result.response && result.response.ToolCalls) || [],
    }]);
    const updatedTurns = (history.turns || []).concat([{
      query,
      request: result.request || {},
      response: result.response || {},
      trace: result.trace || {},
      logPath: result.log_path || "",
      systemPrompt: currentSystemPrompt(),
      includeSystemPrompt: includeSystemPrompt(),
      config,
    }]);
    saveRawConversationHistory(state.selectedSession, {
      messages: updatedMessages,
      turns: updatedTurns,
    });
    const toolCalls = (result.response && result.response.ToolCalls) || [];
    if (toolCalls.length) {
      setSubmitStatus("success", "tool call received, waiting for confirmation");
    } else {
      setSubmitStatus("success", "raw conversation turn stored");
    }
    renderSessions();
    renderTranscript(null);
    renderInspector(null, null);
    renderRequestPreview();
    if (toolCalls.length && autoApproveToolsEnabled()) {
      await maybeAutoApproveLatestTurn();
    }
  } finally {
    if (submitButton) submitButton.disabled = false;
  }
  if (input) input.value = "";
  loadRequestPreview().catch((error) => console.error(error));
}

document.addEventListener("DOMContentLoaded", () => {
  writeRequestConfigToForm(state.runtimeDefaults);
  setSubmitStatus("idle", "idle");
  loadToolCatalog().catch((error) => console.error(error));
  document.getElementById("reload-sessions")?.addEventListener("click", () => {
    loadSessions().catch((error) => console.error(error));
  });
  document.getElementById("new-session-form")?.addEventListener("submit", (event) => {
    createSession(event).catch((error) => console.error(error));
  });
  document.getElementById("chat-form")?.addEventListener("submit", (event) => {
    submitMessage(event).catch((error) => {
      console.error(error);
      setSubmitStatus("error", error.message || "submit failed");
    });
  });
  document.querySelectorAll("#chat-id-input, #chat-input, #model-select, #reasoning-mode-select, #clear-thinking-input, #temperature-input, #top-p-input, #max-tokens-input, #do-sample-input, #response-format-select, #system-prompt-input, #include-system-prompt-input, #auto-approve-tools-input, #offload-old-tools-input").forEach((el) => {
    el.addEventListener("input", () => loadRequestPreview().catch((error) => console.error(error)));
    el.addEventListener("change", () => loadRequestPreview().catch((error) => console.error(error)));
  });
  loadRequestPreview().catch((error) => console.error(error));
  loadSessions().catch((error) => console.error(error));
});`

const debugShellCSS = `:root {
  color-scheme: dark;
  --bg: #0b1117;
  --bg-deep: #071017;
  --panel: rgba(16, 24, 33, 0.94);
  --panel-alt: rgba(23, 35, 47, 0.9);
  --panel-soft: rgba(18, 29, 39, 0.82);
  --border: rgba(132, 170, 201, 0.22);
  --border-strong: rgba(154, 209, 255, 0.34);
  --text: #e7edf4;
  --muted: #9fb2c4;
  --accent: #9ad1ff;
  --accent-soft: rgba(154, 209, 255, 0.12);
  --shadow: 0 18px 45px rgba(2, 8, 14, 0.34);
}

* {
  box-sizing: border-box;
}

body {
  margin: 0;
  font-family: "IBM Plex Mono", ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", monospace;
  background:
    radial-gradient(circle at top left, rgba(83, 143, 209, 0.12), transparent 28%),
    linear-gradient(180deg, var(--bg-deep) 0%, var(--bg) 100%);
  color: var(--text);
  overflow: hidden;
}

.shell {
  height: 100vh;
  display: grid;
  grid-template-columns: 460px minmax(520px, 1fr) 320px;
  gap: 12px;
  padding: 12px;
  overflow: hidden;
}

.pane {
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: 18px;
  overflow: hidden;
  display: flex;
  flex-direction: column;
  min-height: 0;
  box-shadow: var(--shadow);
}

.pane-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  padding: 18px 18px 16px;
  border-bottom: 1px solid var(--border);
  background: linear-gradient(180deg, rgba(255, 255, 255, 0.03), rgba(255, 255, 255, 0.01));
}

.pane-header h1,
.pane-header h2 {
  margin: 0;
  font-size: 14px;
  line-height: 1.4;
}

.header-stack {
  display: grid;
  gap: 6px;
  min-width: 0;
}

.header-copy {
  margin: 0;
  max-width: 28ch;
  color: var(--muted);
  font-size: 12px;
  line-height: 1.5;
}

.status-pill {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 4px 8px;
  border-radius: 999px;
  border: 1px solid rgba(154, 209, 255, 0.25);
  color: var(--accent);
  font-size: 12px;
  background: rgba(154, 209, 255, 0.06);
}

.rail-scroll {
  flex: 1 1 auto;
  display: grid;
  gap: 12px;
  padding: 14px;
  min-height: 0;
  overflow: auto;
}

.rail-section {
  padding: 14px;
  border: 1px solid var(--border);
  border-radius: 14px;
  background: linear-gradient(180deg, var(--panel-alt), var(--panel-soft));
}

.section-head {
  display: grid;
  gap: 4px;
  margin-bottom: 12px;
}

.section-head h3,
.subsection h4 {
  margin: 0;
  font-size: 13px;
  color: var(--text);
}

.section-head p,
.subsection small {
  margin: 0;
  color: var(--muted);
  font-size: 11px;
  line-height: 1.45;
}

.subsection {
  display: grid;
  gap: 10px;
  padding-top: 10px;
  margin-top: 10px;
  border-top: 1px solid rgba(154, 209, 255, 0.08);
}

.subsection:first-of-type {
  padding-top: 0;
  margin-top: 0;
  border-top: 0;
}

.context-note {
  color: var(--muted);
  font-size: 11px;
  line-height: 1.45;
}

.rail-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 10px;
}

.rail-grid-context {
  grid-template-columns: 1fr;
}

.rail-grid-top {
  grid-template-columns: repeat(2, minmax(0, 1fr));
}

.button-row {
  display: flex;
  justify-content: flex-start;
  margin-top: 10px;
}

.session-create-form {
  display: grid;
  grid-template-columns: 1fr auto;
  gap: 10px;
  margin-top: 12px;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 6px;
  color: var(--muted);
  font-size: 12px;
}

.field textarea,
.field input,
.field select,
.composer input {
  width: 100%;
}

.field > span {
  color: var(--text);
}

.field-full {
  grid-column: 1 / -1;
}

.field-toggle {
  justify-content: space-between;
  min-height: 100%;
}

.field-toggle input[type="checkbox"] {
  width: 18px;
  height: 18px;
  margin-top: 2px;
  accent-color: #7cc2ff;
}

.field-checkline {
  flex-direction: row;
  align-items: center;
  justify-content: space-between;
  padding: 8px 10px;
  border: 1px solid rgba(154, 209, 255, 0.12);
  border-radius: 10px;
  background: rgba(255, 255, 255, 0.02);
}

.field-checkline input[type="checkbox"] {
  width: 18px;
  height: 18px;
  accent-color: #7cc2ff;
}

.field-stack textarea {
  min-height: 92px;
  resize: vertical;
  padding: 10px 12px;
  border-radius: 10px;
  border: 1px solid rgba(154, 209, 255, 0.18);
  background: rgba(10, 16, 22, 0.72);
  color: var(--text);
  font-family: inherit;
  font-size: 12px;
  line-height: 1.45;
}

.tool-picker {
  display: grid;
  gap: 10px;
  padding: 12px;
  border: 1px solid rgba(154, 209, 255, 0.14);
  border-radius: 12px;
  background: rgba(255, 255, 255, 0.02);
}

.tool-picker-shell {
  margin-top: 12px;
  border: 1px solid rgba(154, 209, 255, 0.12);
  border-radius: 12px;
  background: rgba(255, 255, 255, 0.02);
  overflow: hidden;
}

.tool-picker-shell > summary {
  cursor: pointer;
  padding: 10px 12px;
  color: var(--text);
}

.tool-picker-shell[open] > summary {
  border-bottom: 1px solid rgba(154, 209, 255, 0.08);
}

.tool-picker.empty {
  color: var(--muted);
  font-size: 12px;
}

.tool-picker-head {
  color: var(--text);
  font-size: 12px;
}

.tool-picker-grid {
  display: grid;
  grid-template-columns: 1fr;
  gap: 8px;
}

.tool-toggle {
  width: 100%;
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 10px;
  padding: 10px 12px;
  border-radius: 10px;
  border: 1px solid rgba(154, 209, 255, 0.14);
  background: rgba(8, 14, 20, 0.56);
  color: var(--text);
  text-align: left;
  cursor: pointer;
}

.tool-toggle input[type="checkbox"] {
  width: 18px;
  height: 18px;
  margin: 0;
  accent-color: #7cc2ff;
}

.tool-toggle.active {
  border-color: var(--border-strong);
  background: rgba(154, 209, 255, 0.12);
}

.tool-toggle-copy {
  display: grid;
  gap: 4px;
}

.tool-chip-name {
  font-size: 12px;
}

.tool-chip-desc {
  color: var(--muted);
  font-size: 11px;
  line-height: 1.4;
}

.list,
.panel {
  margin: 12px 14px 14px;
  padding: 14px;
  border: 1px solid rgba(154, 209, 255, 0.16);
  border-radius: 14px;
  background: var(--panel-alt);
  color: var(--muted);
}

.pane-chat .panel {
  flex: 1 1 auto;
  min-height: 0;
  overflow: auto;
}

.pane-sessions .list {
  flex: 1 1 auto;
  min-height: 0;
  overflow: auto;
}

.session-item {
  width: 100%;
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 4px;
  margin-bottom: 10px;
  padding: 10px 12px;
  border-radius: 12px;
  border: 1px solid rgba(154, 209, 255, 0.1);
  background: rgba(255, 255, 255, 0.02);
}

.session-item.active {
  border-color: var(--border-strong);
  background: rgba(154, 209, 255, 0.08);
}

.session-item.recent-work strong::after {
  content: " recent";
  color: var(--accent);
  font-size: 11px;
}

.session-entry {
  margin-bottom: 10px;
  border: 1px solid rgba(154, 209, 255, 0.1);
  border-radius: 12px;
  background: rgba(255, 255, 255, 0.02);
}

.session-entry > summary {
  list-style: none;
  cursor: pointer;
  display: flex;
  justify-content: space-between;
  gap: 10px;
  padding: 10px 12px;
}

.session-entry > summary::-webkit-details-marker {
  display: none;
}

.session-summary-title {
  color: var(--text);
}

.session-summary-meta,
.session-entry-meta {
  color: var(--muted);
  font-size: 11px;
}

.session-entry-body {
  display: grid;
  gap: 6px;
  padding: 0 12px 12px;
}

.session-entry-goal {
  color: var(--text);
  font-size: 12px;
}

.selection-summary {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  min-height: 38px;
  padding: 8px 10px;
  border-radius: 10px;
  border: 1px solid rgba(154, 209, 255, 0.12);
  background: rgba(255, 255, 255, 0.02);
}

.selection-summary.empty {
  color: var(--muted);
  font-size: 12px;
  align-items: center;
}

.selection-pill {
  padding: 4px 8px;
  border-radius: 999px;
  background: rgba(154, 209, 255, 0.12);
  color: var(--accent);
  font-size: 11px;
}

.pending-tool-banner {
  border-color: rgba(245, 158, 11, 0.4);
  background: linear-gradient(180deg, rgba(55, 34, 8, 0.92), rgba(26, 20, 11, 0.96));
  margin-top: 14px;
  flex: 0 0 auto;
}

.pending-tool-banner.hidden {
  display: none;
}

.pending-tool-heading {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  gap: 12px;
  margin-bottom: 10px;
}

.pending-tool-heading strong {
  color: #fde68a;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  font-size: 12px;
}

.pending-tool-note {
  color: #f8d99b;
  font-size: 12px;
}

.pending-tool-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
}

.pending-tool-card {
  min-width: 220px;
  max-width: 100%;
  padding: 12px;
  border-radius: 12px;
  border: 1px solid rgba(245, 158, 11, 0.18);
  background: rgba(15, 23, 42, 0.42);
}

.pending-tool-card strong {
  display: block;
  margin-bottom: 6px;
}

.pending-tool-args {
  margin: 0 0 10px;
  padding: 10px;
  border-radius: 10px;
  background: rgba(2, 6, 23, 0.7);
  color: #dbeafe;
  font-size: 12px;
  max-height: 160px;
  overflow: auto;
}

.sr-only {
  position: absolute;
  width: 1px;
  height: 1px;
  padding: 0;
  margin: -1px;
  overflow: hidden;
  clip: rect(0, 0, 0, 0);
  white-space: nowrap;
  border: 0;
}

.composer {
  display: grid;
  grid-template-columns: 1fr auto auto;
  gap: 10px;
  padding: 14px;
  border-top: 1px solid var(--border);
  flex: 0 0 auto;
}

.composer-status {
  align-self: center;
  font-size: 12px;
  color: var(--muted);
}

.composer-status.running {
  color: #f7c96b;
}

.composer-status.success {
  color: #8fd6a3;
}

.composer-status.error {
  color: #ff9f9f;
}

input,
button,
select {
  font: inherit;
  border-radius: 10px;
  border: 1px solid var(--border);
  padding: 10px 12px;
  background: rgba(8, 15, 22, 0.88);
  color: var(--text);
}

button {
  color: var(--accent);
  background: linear-gradient(180deg, rgba(16, 30, 42, 0.98), rgba(10, 18, 27, 0.95));
}

button:hover {
  border-color: var(--border-strong);
  background: linear-gradient(180deg, rgba(23, 43, 60, 0.98), rgba(11, 22, 32, 0.95));
}

.event-card {
  border: 1px solid rgba(154, 209, 255, 0.14);
  border-radius: 10px;
  padding: 12px;
  margin-bottom: 12px;
  background: rgba(255, 255, 255, 0.02);
}

.final-answer-card {
  border: 1px solid rgba(154, 209, 255, 0.32);
  border-radius: 10px;
  padding: 12px;
  margin-bottom: 12px;
  background: rgba(154, 209, 255, 0.08);
}

.event-card header {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  gap: 12px;
  margin-bottom: 8px;
  color: var(--muted);
}

.event-card pre,
#timeline pre,
#inspector pre {
  margin: 0;
  white-space: pre-wrap;
  word-break: break-word;
}

.http-block {
  margin-top: 12px;
}

.http-section {
  margin-top: 10px;
  border: 1px solid rgba(154, 209, 255, 0.12);
  border-radius: 10px;
  padding: 10px;
  background: rgba(255, 255, 255, 0.02);
}

.http-section > summary {
  cursor: pointer;
  display: flex;
  justify-content: space-between;
  gap: 12px;
  color: var(--text);
}

.raw-turn > summary,
.run-group > summary {
  cursor: pointer;
  display: flex;
  justify-content: space-between;
  gap: 12px;
  color: var(--text);
}

.http-line-label {
  margin-bottom: 8px;
  color: var(--accent);
  font-size: 12px;
}

.mode-toolbar {
  display: flex;
  gap: 8px;
  margin-bottom: 12px;
}

.mode-button.active {
  border-color: var(--accent);
  background: rgba(154, 209, 255, 0.12);
}

.section-card {
  border: 1px solid rgba(154, 209, 255, 0.14);
  border-radius: 10px;
  padding: 12px;
  margin-bottom: 12px;
  background: rgba(255, 255, 255, 0.02);
}

.pane-inspector .panel {
  min-height: 0;
  overflow: auto;
}

#request-preview {
  flex: 0 0 auto;
  max-height: 30vh;
}

#chat-transcript {
  min-height: 0;
}

#timeline {
  flex: 0 0 auto;
  max-height: 34vh;
}

#inspector {
  flex: 1 1 auto;
}

.section-card h3 {
  margin: 0 0 10px 0;
  font-size: 13px;
  color: var(--text);
}

.kv-row {
  display: grid;
  grid-template-columns: 140px minmax(0, 1fr);
  gap: 10px;
  padding: 6px 0;
  border-bottom: 1px solid rgba(154, 209, 255, 0.08);
}

.kv-row:last-child {
  border-bottom: 0;
}

.kv-label {
  color: var(--muted);
}

.kv-value {
  color: var(--text);
  word-break: break-word;
}

.summary-list {
  margin: 0;
  padding-left: 18px;
}

.summary-list li {
  margin-bottom: 6px;
}

.run-group {
  margin-bottom: 10px;
  padding: 8px;
  border: 1px solid rgba(154, 209, 255, 0.14);
  border-radius: 8px;
}

.run-group > summary {
  cursor: pointer;
  display: flex;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 10px;
}

.diff-toolbar {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
  margin-bottom: 12px;
}

.empty-note {
  color: var(--muted);
}

.turn-label-row {
  display: grid;
  grid-template-columns: 100px minmax(0, 1fr);
  gap: 10px;
  padding: 4px 0;
}

.turn-label-row span:first-child {
  color: var(--muted);
}

.compact-pre {
  max-height: 220px;
  overflow: auto;
  -webkit-overflow-scrolling: touch;
}

.json-tree {
  border: 1px solid rgba(154, 209, 255, 0.1);
  border-radius: 8px;
  padding: 10px;
  background: rgba(255, 255, 255, 0.01);
}

.json-node,
.json-leaf {
  margin-left: 12px;
}

.json-node > summary {
  cursor: pointer;
  list-style: none;
  margin-bottom: 6px;
}

.json-node > summary::-webkit-details-marker {
  display: none;
}

.json-label {
  color: var(--accent);
  margin-right: 8px;
}

.json-kind {
  color: var(--muted);
}

.json-scalar {
  color: var(--text);
  word-break: break-word;
}

.inline-answer {
  margin-top: 12px;
  padding-top: 12px;
  border-top: 1px solid rgba(154, 209, 255, 0.08);
}

.inline-answer-label {
  margin-bottom: 8px;
  color: var(--accent);
  font-size: 12px;
}

@media (max-width: 1100px) {
  body {
    overflow: auto;
  }
  .shell {
    grid-template-columns: 1fr;
    height: auto;
    min-height: 100dvh;
    overflow: visible;
    padding: 8px;
    gap: 8px;
  }
  .pane {
    overflow: visible;
    min-height: auto;
  }
  .pane-header {
    padding: 14px 14px 12px;
  }
  .rail-grid,
  .rail-grid-top,
  .session-create-form {
    grid-template-columns: 1fr;
  }
  .rail-scroll,
  .pane-chat .panel,
  .pane-sessions .list,
  .pane-inspector .panel,
  #request-preview,
  #timeline,
  #inspector {
    overflow: visible;
    max-height: none;
  }
  .rail-scroll,
  .pane-chat .panel,
  .pane-sessions .list,
  .pane-inspector .panel {
    flex: 0 0 auto;
  }
  .list,
  .panel {
    margin: 8px;
    padding: 10px;
  }
  .composer {
    grid-template-columns: 1fr;
    padding: 10px;
  }
  .pending-tool-heading,
  .raw-turn > summary,
  .run-group > summary,
  .session-entry > summary {
    display: grid;
    gap: 6px;
  }
  .turn-label-row,
  .kv-row,
  .diff-toolbar {
    grid-template-columns: 1fr;
  }
  .json-tree,
  .http-section,
  .section-card,
  .event-card {
    overflow: hidden;
  }
}`

func (s *Server) handleDebug(w http.ResponseWriter, r *http.Request) {
	if s.core == nil && s.runner == nil {
		if !((strings.HasPrefix(r.URL.Path, "/api/debug/raw-network") && s.rawProvider != nil) ||
			(strings.HasPrefix(r.URL.Path, "/api/debug/raw-tool-exec") && (s.toolExecutor != nil || strings.TrimSpace(s.rawVFSRootDir) != "")) ||
			(strings.HasPrefix(r.URL.Path, "/api/debug/raw-conversations/") && strings.TrimSpace(s.rawSessionLogDir) != "") ||
			(strings.HasPrefix(r.URL.Path, "/api/debug/sessions/") && strings.HasSuffix(r.URL.Path, "/provider-preview") && s.previewer != nil) ||
			(r.URL.Path == "/api/debug/tools" && (s.toolCatalog != nil || strings.TrimSpace(s.rawVFSRootDir) != ""))) {
			writeJSON(w, http.StatusServiceUnavailable, NewErrorResponse("runtime_unavailable", "agent core is not configured"))
			return
		}
	}
	switch {
	case r.URL.Path == "/api/debug/tools":
		if r.Method != http.MethodGet {
			writeJSON(w, http.StatusMethodNotAllowed, NewErrorResponse("method_not_allowed", "method not allowed"))
			return
		}
		if s.toolCatalog == nil && strings.TrimSpace(s.rawVFSRootDir) == "" {
			writeJSON(w, http.StatusServiceUnavailable, NewErrorResponse("runtime_unavailable", "tool catalog is not configured"))
			return
		}
		role := strings.TrimSpace(r.URL.Query().Get("role"))
		if role == "" {
			role = "telegram"
		}
		items, err := s.combinedDebugToolCatalog(role)
		if err != nil {
			writeJSON(w, http.StatusInternalServerError, NewRuntimeErrorResponse(err, "tool_catalog_error", err.Error()))
			return
		}
		writeJSON(w, http.StatusOK, ToolCatalogResponse{Items: items})
		return
	case r.URL.Path == "/api/debug/raw-network":
		if r.Method != http.MethodPost {
			writeJSON(w, http.StatusMethodNotAllowed, NewErrorResponse("method_not_allowed", "method not allowed"))
			return
		}
		if s.rawProvider == nil {
			writeJSON(w, http.StatusServiceUnavailable, NewErrorResponse("runtime_unavailable", "raw provider is not configured"))
			return
		}
		var req DebugRawNetworkRequest
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			writeJSON(w, http.StatusBadRequest, NewErrorResponse("bad_request", "invalid json body"))
			return
		}
		messages := append([]provider.Message(nil), req.Messages...)
		systemPrompt := strings.TrimSpace(req.SystemPrompt)
		if req.IncludeSystemPrompt && systemPrompt != "" {
			messages = append([]provider.Message{{Role: "system", Content: systemPrompt}}, messages...)
		}
		query := strings.TrimSpace(req.Query)
		if query != "" {
			hasTrailingUser := len(messages) > 0 && messages[len(messages)-1].Role == "user" && strings.TrimSpace(messages[len(messages)-1].Content) == query
			if !hasTrailingUser {
				messages = append(messages, provider.Message{Role: "user", Content: query})
			}
		}
		if len(messages) == 0 {
			writeJSON(w, http.StatusBadRequest, NewErrorResponse("bad_request", "query is required"))
			return
		}
		if query == "" && len(messages) > 0 {
			query = strings.TrimSpace(messages[len(messages)-1].Content)
		}
		if len(messages) == 0 {
			writeJSON(w, http.StatusBadRequest, NewErrorResponse("bad_request", "messages are required"))
			return
		}
		if req.OffloadOldToolOutputs {
			var offloadErr error
			var offloadedArtifactRefs []string
			messages, offloadedArtifactRefs, offloadErr = s.offloadOldToolMessages(req.ChatID, req.SessionID, messages)
			if offloadErr != nil {
				writeJSON(w, http.StatusInternalServerError, NewRuntimeErrorResponse(offloadErr, "raw_offload_error", offloadErr.Error()))
				return
			}
			if updateErr := s.updateRawSessionHeadArtifacts(req.ChatID, req.SessionID, offloadedArtifactRefs); updateErr != nil {
				writeJSON(w, http.StatusInternalServerError, NewRuntimeErrorResponse(updateErr, "raw_session_head_error", updateErr.Error()))
				return
			}
		}
		collector := llmtrace.NewCollector(llmtrace.RunMeta{
			RunID:     fmt.Sprintf("raw-%d", nowUTC().UnixNano()),
			ChatID:    req.ChatID,
			Query:     query,
			StartedAt: nowUTC(),
		})
		ctx := llmtrace.WithCollector(r.Context(), collector)
		var tools []provider.ToolDefinition
		if len(req.Tools) > 0 {
			if s.toolCatalog == nil && strings.TrimSpace(s.rawVFSRootDir) == "" {
				writeJSON(w, http.StatusServiceUnavailable, NewErrorResponse("runtime_unavailable", "tool catalog is not configured"))
				return
			}
			items, err := s.combinedDebugToolCatalog("telegram")
			if err != nil {
				writeJSON(w, http.StatusInternalServerError, NewRuntimeErrorResponse(err, "tool_catalog_error", err.Error()))
				return
			}
			tools = resolveDebugTools(items, req.Tools)
		}
		_, err := s.rawProvider.Generate(ctx, provider.PromptRequest{
			WorkerID: fmt.Sprintf("web-raw:%d", req.ChatID),
			Messages: messages,
			Tools:    tools,
			Config:   runtime.MergeRequestConfig(s.runtimeConfig, req.Config),
		})
		if err != nil {
			writeJSON(w, http.StatusBadRequest, NewRuntimeErrorResponse(err, "provider_error", err.Error()))
			return
		}
		trace := collector.Snapshot()
		var call llmtrace.CallTrace
		if len(trace.Calls) > 0 {
			call = trace.Calls[len(trace.Calls)-1]
		}
		logPath, err := s.appendRawSessionLog(req.SessionID, rawSessionLogEntry{
			Timestamp:           nowUTC(),
			Kind:                "provider_turn",
			ChatID:              req.ChatID,
			SessionID:           req.SessionID,
			Query:               query,
			SystemPrompt:        systemPrompt,
			IncludeSystemPrompt: req.IncludeSystemPrompt,
			SelectedTools:       append([]string(nil), req.Tools...),
			Request:             call.Request,
			Response:            call.ParsedResponse,
			Trace:               call,
		})
		if err != nil {
			writeJSON(w, http.StatusInternalServerError, NewRuntimeErrorResponse(err, "raw_session_log_error", err.Error()))
			return
		}
		writeJSON(w, http.StatusOK, DebugRawNetworkResponse{
			Request:  call.Request,
			Response: call.ParsedResponse,
			Trace:    call,
			LogPath:  logPath,
		})
		return
	case r.URL.Path == "/api/debug/raw-tool-exec":
		if r.Method != http.MethodPost {
			writeJSON(w, http.StatusMethodNotAllowed, NewErrorResponse("method_not_allowed", "method not allowed"))
			return
		}
		var req DebugRawToolExecRequest
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			writeJSON(w, http.StatusBadRequest, NewErrorResponse("bad_request", "invalid json body"))
			return
		}
		if strings.TrimSpace(req.Call.Name) == "" {
			writeJSON(w, http.StatusBadRequest, NewErrorResponse("bad_request", "tool call name is required"))
			return
		}
		var out string
		var err error
		if strings.HasPrefix(req.Call.Name, "vfs_") {
			out, err = s.executeRawVFSTool(req.ChatID, req.SessionID, req.Call)
		} else {
			if s.toolExecutor == nil {
				writeJSON(w, http.StatusServiceUnavailable, NewErrorResponse("runtime_unavailable", "raw tool executor is not configured"))
				return
			}
			out, err = s.toolExecutor.ExecuteApprovedTool(r.Context(), req.ChatID, req.Tools, req.Call)
		}
		if err != nil {
			out = formatToolExecutionError(err)
			logPath, logErr := s.appendRawSessionLog(req.SessionID, rawSessionLogEntry{
				Timestamp:  nowUTC(),
				Kind:       "tool_execution",
				ChatID:     req.ChatID,
				SessionID:  req.SessionID,
				ToolCall:   &req.Call,
				ToolOutput: out,
				ToolError:  err.Error(),
			})
			if logErr != nil {
				writeJSON(w, http.StatusInternalServerError, NewRuntimeErrorResponse(logErr, "raw_session_log_error", logErr.Error()))
				return
			}
			writeJSON(w, http.StatusOK, DebugRawToolExecResponse{
				Call:         req.Call,
				Output:       out,
				Success:      false,
				ErrorCode:    "tool_execution_error",
				ErrorMessage: err.Error(),
				LogPath:      logPath,
			})
			return
		}
		logPath, err := s.appendRawSessionLog(req.SessionID, rawSessionLogEntry{
			Timestamp:  nowUTC(),
			Kind:       "tool_execution",
			ChatID:     req.ChatID,
			SessionID:  req.SessionID,
			ToolCall:   &req.Call,
			ToolOutput: out,
		})
		if err != nil {
			writeJSON(w, http.StatusInternalServerError, NewRuntimeErrorResponse(err, "raw_session_log_error", err.Error()))
			return
		}
		writeJSON(w, http.StatusOK, DebugRawToolExecResponse{
			Call:    req.Call,
			Output:  out,
			Success: true,
			LogPath: logPath,
		})
		return
	case strings.HasPrefix(r.URL.Path, "/api/debug/raw-conversations/"):
		if r.Method != http.MethodGet {
			writeJSON(w, http.StatusMethodNotAllowed, NewErrorResponse("method_not_allowed", "method not allowed"))
			return
		}
		sessionID := strings.Trim(strings.TrimPrefix(r.URL.Path, "/api/debug/raw-conversations/"), "/")
		if sessionID == "" {
			writeJSON(w, http.StatusBadRequest, NewErrorResponse("bad_request", "missing session id"))
			return
		}
		payload, ok, err := s.loadRawConversation(sessionID)
		if err != nil {
			writeJSON(w, http.StatusInternalServerError, NewRuntimeErrorResponse(err, "raw_conversation_error", err.Error()))
			return
		}
		if !ok {
			writeJSON(w, http.StatusNotFound, NewErrorResponse("not_found", "raw conversation not found"))
			return
		}
		writeJSON(w, http.StatusOK, payload)
		return
	case strings.HasPrefix(r.URL.Path, "/api/debug/sessions/"):
		path := strings.Trim(strings.TrimPrefix(r.URL.Path, "/api/debug/sessions/"), "/")
		if path == "" {
			writeJSON(w, http.StatusBadRequest, NewErrorResponse("bad_request", "missing session id"))
			return
		}
		if strings.HasSuffix(path, "/provider-preview") {
			if r.Method != http.MethodPost {
				writeJSON(w, http.StatusMethodNotAllowed, NewErrorResponse("method_not_allowed", "method not allowed"))
				return
			}
			if s.previewer == nil {
				writeJSON(w, http.StatusServiceUnavailable, NewErrorResponse("runtime_unavailable", "provider previewer is not configured"))
				return
			}
			sessionID := strings.Trim(strings.TrimSuffix(path, "/provider-preview"), "/")
			if sessionID == "" {
				writeJSON(w, http.StatusBadRequest, NewErrorResponse("bad_request", "missing session id"))
				return
			}
			var req CreateRunRequest
			if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
				writeJSON(w, http.StatusBadRequest, NewErrorResponse("bad_request", "invalid json body"))
				return
			}
			promptReq, metrics, err := s.previewer.DebugProviderPreview(r.Context(), req.ChatID, sessionID, strings.TrimSpace(req.Query), mergePolicySnapshotRuntime(s.policySnapshot(sessionID), req.Config).Runtime, req.ContextInputs)
			if err != nil {
				writeJSON(w, http.StatusBadRequest, NewRuntimeErrorResponse(err, "runtime_error", err.Error()))
				return
			}
			writeJSON(w, http.StatusOK, DebugProviderPreviewResponse{
				Request: promptReq,
				Metrics: metrics,
			})
			return
		}
		if strings.HasSuffix(path, "/messages") {
			if r.Method != http.MethodPost {
				writeJSON(w, http.StatusMethodNotAllowed, NewErrorResponse("method_not_allowed", "method not allowed"))
				return
			}
			sessionID := strings.TrimSuffix(path, "/messages")
			sessionID = strings.Trim(sessionID, "/")
			if sessionID == "" {
				writeJSON(w, http.StatusBadRequest, NewErrorResponse("bad_request", "missing session id"))
				return
			}
			var req CreateRunRequest
			if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
				writeJSON(w, http.StatusBadRequest, NewErrorResponse("bad_request", "invalid json body"))
				return
			}
			startReq := runtime.StartRunRequest{
				RunID:          fmt.Sprintf("run-%d", nowUTC().UnixNano()),
				ChatID:         req.ChatID,
				SessionID:      sessionID,
				Query:          strings.TrimSpace(req.Query),
				PolicySnapshot: mergePolicySnapshotRuntime(s.policySnapshot(sessionID), req.Config),
				DebugProfile:   req.ContextInputs,
				Interactive:    false,
			}
			var (
				view runtime.RunView
				ok   bool
				err  error
			)
			if s.core != nil {
				view, ok, err = s.core.StartRunDetached(r.Context(), startReq)
			} else {
				view, ok, err = s.runner.StartDetached(r.Context(), startReq)
			}
			if err != nil {
				writeJSON(w, http.StatusBadRequest, NewRuntimeErrorResponse(err, "runtime_error", err.Error()))
				return
			}
			writeJSON(w, http.StatusAccepted, CreateRunResponse{
				RunID:    view.RunID,
				Accepted: ok,
				Run:      view,
			})
			return
		}
		if r.Method != http.MethodGet {
			writeJSON(w, http.StatusMethodNotAllowed, NewErrorResponse("method_not_allowed", "method not allowed"))
			return
		}
		sessionID := path
		chatID, err := parseRequiredChatID(r.URL.Query())
		if err != nil {
			writeJSON(w, http.StatusBadRequest, NewErrorResponse("bad_request", err.Error()))
			return
		}
		eventLimit := parsePositiveQueryInt(r.URL.Query(), "event_limit", 20)
		if s.core == nil {
			writeJSON(w, http.StatusServiceUnavailable, NewErrorResponse("runtime_unavailable", "agent core is not configured"))
			return
		}
		view, err := s.core.DebugSession(sessionID, chatID, eventLimit)
		if err != nil {
			writeJSON(w, http.StatusBadRequest, NewRuntimeErrorResponse(err, "runtime_error", err.Error()))
			return
		}
		writeJSON(w, http.StatusOK, DebugSessionResponse{
			Session: view.Session,
			Control: view.Control,
			Events:  view.Events,
		})
		return
	case strings.HasPrefix(r.URL.Path, "/api/debug/runs/"):
		if r.Method != http.MethodGet {
			writeJSON(w, http.StatusMethodNotAllowed, NewErrorResponse("method_not_allowed", "method not allowed"))
			return
		}
		if s.core == nil {
			writeJSON(w, http.StatusServiceUnavailable, NewErrorResponse("runtime_unavailable", "agent core is not configured"))
			return
		}
		path := strings.Trim(strings.TrimPrefix(r.URL.Path, "/api/debug/runs/"), "/")
		if strings.HasSuffix(path, "/context-provenance") {
			runID := strings.TrimSuffix(path, "/context-provenance")
			runID = strings.Trim(runID, "/")
			if runID == "" {
				writeJSON(w, http.StatusBadRequest, NewErrorResponse("bad_request", "missing run id"))
				return
			}
			provenance, err := s.core.DebugContextProvenance(runID)
			if err != nil {
				writeJSON(w, http.StatusBadRequest, NewRuntimeErrorResponse(err, "runtime_error", err.Error()))
				return
			}
			writeJSON(w, http.StatusOK, DebugContextResponse{Provenance: provenance})
			return
		}
		runID := path
		if runID == "" {
			writeJSON(w, http.StatusBadRequest, NewErrorResponse("bad_request", "missing run id"))
			return
		}
		eventLimit := parsePositiveQueryInt(r.URL.Query(), "event_limit", 20)
		view, ok, err := s.core.DebugRun(runID, eventLimit)
		if err != nil {
			writeJSON(w, http.StatusBadRequest, NewRuntimeErrorResponse(err, "runtime_error", err.Error()))
			return
		}
		if !ok {
			writeJSON(w, http.StatusNotFound, NewErrorResponse("not_found", "run not found"))
			return
		}
		writeJSON(w, http.StatusOK, DebugRunResponse{
			Run:    view.Run,
			Replay: view.Replay,
			Events: view.Events,
		})
		return
	default:
		writeJSON(w, http.StatusNotFound, NewErrorResponse("not_found", "resource not found"))
		return
	}
}

func parseRequiredChatID(values url.Values) (int64, error) {
	raw := strings.TrimSpace(values.Get("chat_id"))
	if raw == "" {
		return 0, fmt.Errorf("missing chat_id")
	}
	value, err := strconv.ParseInt(raw, 10, 64)
	if err != nil {
		return 0, fmt.Errorf("invalid chat_id")
	}
	return value, nil
}

func parsePositiveQueryInt(values url.Values, key string, fallback int) int {
	raw := strings.TrimSpace(values.Get(key))
	if raw == "" {
		return fallback
	}
	value, err := strconv.Atoi(raw)
	if err != nil || value <= 0 {
		return fallback
	}
	return value
}

func (s *Server) handlePlans(w http.ResponseWriter, r *http.Request) {
	if s.core == nil && s.runtime == nil {
		writeJSON(w, http.StatusServiceUnavailable, NewErrorResponse("runtime_unavailable", "runtime api is not configured"))
		return
	}
	path := strings.TrimPrefix(r.URL.Path, "/api/plans")
	path = strings.Trim(path, "/")
	if path == "" {
		switch r.Method {
		case http.MethodGet:
			query := runtime.PlanQuery{
				OwnerType: strings.TrimSpace(r.URL.Query().Get("owner_type")),
				OwnerID:   strings.TrimSpace(r.URL.Query().Get("owner_id")),
				Limit:     20,
			}
			if raw := strings.TrimSpace(r.URL.Query().Get("limit")); raw != "" {
				value, err := strconv.Atoi(raw)
				if err != nil || value <= 0 {
					writeJSON(w, http.StatusBadRequest, NewErrorResponse("bad_request", "invalid limit"))
					return
				}
				query.Limit = value
			}
			var (
				items []runtime.PlanRecord
				err   error
			)
			if s.core != nil {
				items, err = s.core.ListPlans(query)
			} else {
				items, err = s.runtime.ListPlans(query)
			}
			if err != nil {
				writeJSON(w, http.StatusInternalServerError, NewRuntimeErrorResponse(err, "runtime_error", err.Error()))
				return
			}
			writeJSON(w, http.StatusOK, PlanListResponse{Items: items})
			return
		case http.MethodPost:
			var req CreatePlanRequest
			if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
				writeJSON(w, http.StatusBadRequest, NewErrorResponse("bad_request", "invalid json body"))
				return
			}
			var (
				plan runtime.PlanRecord
				err  error
			)
			if s.core != nil {
				plan, err = s.core.CreatePlan(r.Context(), req.OwnerType, req.OwnerID, req.Title)
			} else {
				plan, err = s.runtime.CreatePlan(r.Context(), req.OwnerType, req.OwnerID, req.Title)
			}
			if err != nil {
				writeJSON(w, http.StatusBadRequest, NewRuntimeErrorResponse(err, "plan_error", err.Error()))
				return
			}
			writeJSON(w, http.StatusCreated, PlanResponse{Plan: plan})
			return
		default:
			writeJSON(w, http.StatusMethodNotAllowed, NewErrorResponse("method_not_allowed", "method not allowed"))
			return
		}
	}

	parts := strings.Split(path, "/")
	planID := parts[0]
	if len(parts) == 1 && r.Method == http.MethodGet {
		var (
			plan runtime.PlanRecord
			ok   bool
			err  error
		)
		if s.core != nil {
			plan, ok, err = s.core.Plan(planID)
		} else {
			plan, ok, err = s.runtime.Plan(planID)
		}
		if err != nil {
			writeJSON(w, http.StatusInternalServerError, NewRuntimeErrorResponse(err, "runtime_error", err.Error()))
			return
		}
		if !ok {
			writeJSON(w, http.StatusNotFound, NewErrorResponse("not_found", "plan not found"))
			return
		}
		writeJSON(w, http.StatusOK, PlanResponse{Plan: plan})
		return
	}
	if len(parts) >= 2 && parts[1] == "items" {
		switch {
		case len(parts) == 2 && (r.Method == http.MethodPost || r.Method == http.MethodPut):
			var req ReplacePlanItemsRequest
			if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
				writeJSON(w, http.StatusBadRequest, NewErrorResponse("bad_request", "invalid json body"))
				return
			}
			var (
				plan runtime.PlanRecord
				err  error
			)
			if s.core != nil {
				plan, err = s.core.ReplacePlanItems(planID, req.Items)
			} else {
				plan, err = s.runtime.ReplacePlanItems(planID, req.Items)
			}
			if err != nil {
				writeJSON(w, http.StatusBadRequest, NewRuntimeErrorResponse(err, "plan_error", err.Error()))
				return
			}
			writeJSON(w, http.StatusOK, PlanResponse{Plan: plan})
			return
		case len(parts) == 4 && r.Method == http.MethodPost && parts[3] == "start":
			var (
				plan runtime.PlanRecord
				err  error
			)
			if s.core != nil {
				plan, err = s.core.StartPlanItem(planID, parts[2])
			} else {
				plan, err = s.runtime.StartPlanItem(planID, parts[2])
			}
			if err != nil {
				writeJSON(w, http.StatusBadRequest, NewRuntimeErrorResponse(err, "plan_error", err.Error()))
				return
			}
			writeJSON(w, http.StatusOK, PlanResponse{Plan: plan})
			return
		case len(parts) == 4 && r.Method == http.MethodPost && parts[3] == "complete":
			var (
				plan runtime.PlanRecord
				err  error
			)
			if s.core != nil {
				plan, err = s.core.CompletePlanItem(planID, parts[2])
			} else {
				plan, err = s.runtime.CompletePlanItem(planID, parts[2])
			}
			if err != nil {
				writeJSON(w, http.StatusBadRequest, NewRuntimeErrorResponse(err, "plan_error", err.Error()))
				return
			}
			writeJSON(w, http.StatusOK, PlanResponse{Plan: plan})
			return
		}
	}
	if len(parts) == 2 && parts[1] == "notes" && r.Method == http.MethodPost {
		var req AppendPlanNoteRequest
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			writeJSON(w, http.StatusBadRequest, NewErrorResponse("bad_request", "invalid json body"))
			return
		}
		var (
			plan runtime.PlanRecord
			err  error
		)
		if s.core != nil {
			plan, err = s.core.AppendPlanNote(planID, req.Note)
		} else {
			plan, err = s.runtime.AppendPlanNote(planID, req.Note)
		}
		if err != nil {
			writeJSON(w, http.StatusBadRequest, NewRuntimeErrorResponse(err, "plan_error", err.Error()))
			return
		}
		writeJSON(w, http.StatusOK, PlanResponse{Plan: plan})
		return
	}
	writeJSON(w, http.StatusMethodNotAllowed, NewErrorResponse("method_not_allowed", "method not allowed"))
}

func (s *Server) handleSessionActions(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		writeJSON(w, http.StatusMethodNotAllowed, NewErrorResponse("method_not_allowed", "method not allowed"))
		return
	}
	if s.core == nil && s.sessionActions == nil {
		writeJSON(w, http.StatusServiceUnavailable, NewErrorResponse("runtime_unavailable", "session actions are not configured"))
		return
	}
	var req SessionActionRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeJSON(w, http.StatusBadRequest, NewErrorResponse("bad_request", "invalid json body"))
		return
	}
	actionReq := runtime.SessionActionRequest{
		ChatID:      req.ChatID,
		Action:      runtime.SessionAction(req.Action),
		SessionName: req.SessionName,
	}
	var (
		result runtime.SessionActionResult
		err    error
	)
	if s.core != nil {
		result, err = s.core.ExecuteSessionAction(actionReq)
	} else {
		result, err = s.sessionActions.Execute(req.ChatID, actionReq)
	}
	if err != nil {
		writeJSON(w, http.StatusBadRequest, NewRuntimeErrorResponse(err, "session_action_error", err.Error()))
		return
	}
	writeJSON(w, http.StatusOK, SessionActionResponse{Result: result})
}

func (s *Server) handleArtifacts(w http.ResponseWriter, r *http.Request) {
	if s.artifacts == nil {
		writeJSON(w, http.StatusServiceUnavailable, NewErrorResponse("artifacts_unavailable", "artifact store is not configured"))
		return
	}
	if r.Method != http.MethodGet {
		writeJSON(w, http.StatusMethodNotAllowed, NewErrorResponse("method_not_allowed", "method not allowed"))
		return
	}
	path := strings.TrimPrefix(r.URL.Path, "/api/artifacts/")
	if path == "" {
		writeJSON(w, http.StatusNotFound, NewErrorResponse("not_found", "artifact not found"))
		return
	}
	if path == "search" {
		query, err := artifactSearchQueryFromRequest(r)
		if err != nil {
			writeJSON(w, http.StatusBadRequest, NewErrorResponse("bad_request", err.Error()))
			return
		}
		items, err := s.artifacts.Search(query)
		if err != nil {
			writeJSON(w, http.StatusInternalServerError, NewErrorResponse("artifact_error", err.Error()))
			return
		}
		out := ArtifactSearchResponse{Items: make([]ArtifactSearchItem, 0, len(items))}
		for _, item := range items {
			out.Items = append(out.Items, ArtifactSearchItem{
				Ref:       item.Ref,
				Name:      item.Name,
				OwnerType: item.OwnerType,
				OwnerID:   item.OwnerID,
				SizeBytes: len(item.Payload),
				Preview:   runtime.PreviewArtifactContent(string(item.Payload), query.Query, 3),
				CreatedAt: item.CreatedAt,
			})
		}
		writeJSON(w, http.StatusOK, out)
		return
	}
	contentPath := false
	if strings.HasSuffix(path, "/content") {
		contentPath = true
		path = strings.TrimSuffix(path, "/content")
	}
	ref, err := url.PathUnescape(strings.Trim(path, "/"))
	if err != nil {
		writeJSON(w, http.StatusBadRequest, NewErrorResponse("bad_request", "invalid artifact ref"))
		return
	}
	item, ok, err := s.artifacts.Get(ref)
	if err != nil {
		writeJSON(w, http.StatusInternalServerError, NewErrorResponse("artifact_error", err.Error()))
		return
	}
	if !ok {
		writeJSON(w, http.StatusNotFound, NewErrorResponse("not_found", "artifact not found"))
		return
	}
	if contentPath {
		w.Header().Set("Content-Type", "text/plain; charset=utf-8")
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write(item.Payload)
		return
	}
	writeJSON(w, http.StatusOK, ArtifactResponse{
		Artifact: ArtifactMetadata{
			Ref:       item.Ref,
			Name:      item.Name,
			OwnerType: item.OwnerType,
			OwnerID:   item.OwnerID,
			SizeBytes: len(item.Payload),
			CreatedAt: item.CreatedAt,
		},
	})
}

func artifactSearchQueryFromRequest(r *http.Request) (artifacts.SearchQuery, error) {
	query := artifacts.SearchQuery{
		OwnerType: strings.TrimSpace(r.URL.Query().Get("owner_type")),
		OwnerID:   strings.TrimSpace(r.URL.Query().Get("owner_id")),
		Query:     strings.TrimSpace(r.URL.Query().Get("query")),
		Limit:     20,
	}
	if raw := strings.TrimSpace(r.URL.Query().Get("run_id")); raw != "" && query.OwnerType == "" && query.OwnerID == "" {
		query.OwnerType = "run"
		query.OwnerID = raw
	}
	if raw := strings.TrimSpace(r.URL.Query().Get("worker_id")); raw != "" && query.OwnerType == "" && query.OwnerID == "" {
		query.OwnerType = "worker"
		query.OwnerID = raw
	}
	if raw := strings.TrimSpace(r.URL.Query().Get("limit")); raw != "" {
		value, err := strconv.Atoi(raw)
		if err != nil || value <= 0 {
			return artifacts.SearchQuery{}, fmt.Errorf("invalid limit")
		}
		query.Limit = value
	}
	if raw := strings.TrimSpace(r.URL.Query().Get("global")); raw != "" {
		value, err := strconv.ParseBool(raw)
		if err != nil {
			return artifacts.SearchQuery{}, fmt.Errorf("invalid global")
		}
		query.Global = value
	}
	if !query.Global && (query.OwnerType == "" || query.OwnerID == "") {
		return artifacts.SearchQuery{}, fmt.Errorf("scoped artifact search requires owner_type and owner_id, run_id, or worker_id; use global=true to search all artifacts")
	}
	return query, nil
}

func (s *Server) handleEvents(w http.ResponseWriter, r *http.Request) {
	if s.core == nil && s.runtime == nil {
		writeJSON(w, http.StatusServiceUnavailable, NewErrorResponse("runtime_unavailable", "runtime api is not configured"))
		return
	}
	if r.Method != http.MethodGet {
		writeJSON(w, http.StatusMethodNotAllowed, NewErrorResponse("method_not_allowed", "method not allowed"))
		return
	}
	query, err := eventQueryFromRequest(r)
	if err != nil {
		writeJSON(w, http.StatusBadRequest, NewErrorResponse("bad_request", err.Error()))
		return
	}
	var items []runtime.RuntimeEvent
	if s.core != nil {
		items, err = s.core.ListEvents(query)
	} else {
		items, err = s.runtime.ListEvents(query)
	}
	if err != nil {
		writeJSON(w, http.StatusInternalServerError, NewRuntimeErrorResponse(err, "runtime_error", err.Error()))
		return
	}
	writeJSON(w, http.StatusOK, EventListResponse{Items: items})
}

func (s *Server) handleEventStream(w http.ResponseWriter, r *http.Request) {
	if s.core == nil && s.runtime == nil {
		writeJSON(w, http.StatusServiceUnavailable, NewErrorResponse("runtime_unavailable", "runtime api is not configured"))
		return
	}
	if r.Method != http.MethodGet {
		writeJSON(w, http.StatusMethodNotAllowed, NewErrorResponse("method_not_allowed", "method not allowed"))
		return
	}
	flusher, ok := w.(http.Flusher)
	if !ok {
		writeJSON(w, http.StatusInternalServerError, NewErrorResponse("stream_unavailable", "response streaming is not supported"))
		return
	}
	query, err := eventQueryFromRequest(r)
	if err != nil {
		writeJSON(w, http.StatusBadRequest, NewErrorResponse("bad_request", err.Error()))
		return
	}
	w.Header().Set("Content-Type", "text/event-stream; charset=utf-8")
	w.Header().Set("Cache-Control", "no-cache")
	w.Header().Set("Connection", "keep-alive")
	w.WriteHeader(http.StatusOK)
	flusher.Flush()

	pollTicker := time.NewTicker(250 * time.Millisecond)
	defer pollTicker.Stop()
	heartbeatTicker := time.NewTicker(10 * time.Second)
	defer heartbeatTicker.Stop()

	for {
		var (
			items []runtime.RuntimeEvent
			err   error
		)
		if s.core != nil {
			items, err = s.core.ListEvents(query)
		} else {
			items, err = s.runtime.ListEvents(query)
		}
		if err != nil {
			_ = writeSSEEvent(w, "error", map[string]string{"code": "runtime_error", "message": err.Error()})
			flusher.Flush()
			return
		}
		for _, item := range items {
			if err := writeSSEEvent(w, "runtime", item); err != nil {
				return
			}
			query.AfterID = item.ID
			flusher.Flush()
		}
		select {
		case <-r.Context().Done():
			return
		case <-heartbeatTicker.C:
			if _, err := fmt.Fprint(w, ": heartbeat\n\n"); err != nil {
				return
			}
			flusher.Flush()
		case <-pollTicker.C:
		}
	}
}

func eventQueryFromRequest(r *http.Request) (runtime.EventQuery, error) {
	query := runtime.EventQuery{
		EntityType: strings.TrimSpace(r.URL.Query().Get("entity_type")),
		EntityID:   strings.TrimSpace(r.URL.Query().Get("entity_id")),
		RunID:      strings.TrimSpace(r.URL.Query().Get("run_id")),
		SessionID:  strings.TrimSpace(r.URL.Query().Get("session_id")),
		Limit:      50,
	}
	if raw := strings.TrimSpace(r.URL.Query().Get("after_id")); raw != "" {
		value, err := strconv.ParseInt(raw, 10, 64)
		if err != nil || value < 0 {
			return runtime.EventQuery{}, fmt.Errorf("invalid after_id")
		}
		query.AfterID = value
	}
	if raw := strings.TrimSpace(r.URL.Query().Get("limit")); raw != "" {
		value, err := strconv.Atoi(raw)
		if err != nil || value <= 0 {
			return runtime.EventQuery{}, fmt.Errorf("invalid limit")
		}
		query.Limit = value
	}
	return query, nil
}

func writeSSEEvent(w http.ResponseWriter, event string, payload any) error {
	body, err := json.Marshal(payload)
	if err != nil {
		return err
	}
	if _, err := fmt.Fprintf(w, "event: %s\n", event); err != nil {
		return err
	}
	if _, err := fmt.Fprintf(w, "data: %s\n\n", body); err != nil {
		return err
	}
	return nil
}

func (s *Server) handleRuntime(w http.ResponseWriter, r *http.Request) {
	if strings.HasPrefix(r.URL.Path, "/api/runtime/sessions/") {
		s.handleRuntimeSession(w, r)
		return
	}
	if r.Method != http.MethodGet {
		writeJSON(w, http.StatusMethodNotAllowed, NewErrorResponse("method_not_allowed", "method not allowed"))
		return
	}
	sessionID := strings.TrimSpace(r.URL.Query().Get("session_id"))
	if sessionID == "" {
		writeJSON(w, http.StatusOK, RuntimeSummaryResponse{
			Runtime:      s.runtimeConfig,
			MemoryPolicy: s.memoryPolicy,
			ActionPolicy: s.actionPolicy,
		})
		return
	}
	if s.core == nil && s.runtime == nil {
		writeJSON(w, http.StatusServiceUnavailable, NewRuntimeErrorResponse(runtime.NewControlError(runtime.ErrRuntimeUnavailable, "runtime api is not configured"), "runtime_unavailable", "runtime api is not configured"))
		return
	}
	var (
		summary runtime.RuntimeSummary
		err     error
	)
	if s.core != nil {
		summary, err = s.core.RuntimeSummary(sessionID)
	} else {
		summary, err = s.runtime.RuntimeSummary(sessionID, s.runtimeConfig, s.memoryPolicy, s.actionPolicy)
	}
	if err != nil {
		writeJSON(w, http.StatusInternalServerError, NewErrorResponse("runtime_error", err.Error()))
		return
	}
	writeJSON(w, http.StatusOK, runtimeSummaryResponse(summary))
}

func (s *Server) handleSessions(w http.ResponseWriter, r *http.Request) {
	if r.URL.Path == "/api/sessions" {
		if r.Method != http.MethodGet {
			writeJSON(w, http.StatusMethodNotAllowed, NewErrorResponse("method_not_allowed", "method not allowed"))
			return
		}
		query := runtime.SessionQuery{Limit: 20}
		if raw := strings.TrimSpace(r.URL.Query().Get("chat_id")); raw != "" {
			chatID, err := strconv.ParseInt(raw, 10, 64)
			if err != nil {
				writeJSON(w, http.StatusBadRequest, NewErrorResponse("bad_request", "invalid chat_id"))
				return
			}
			query.ChatID = chatID
			query.HasChatID = true
		}
		if raw := strings.TrimSpace(r.URL.Query().Get("limit")); raw != "" {
			limit, err := strconv.Atoi(raw)
			if err != nil || limit <= 0 {
				writeJSON(w, http.StatusBadRequest, NewErrorResponse("bad_request", "invalid limit"))
				return
			}
			query.Limit = limit
		}
		var (
			items []runtime.SessionState
			err   error
		)
		if s.core != nil {
			items, err = s.core.ListSessions(query)
		} else if s.runtime != nil {
			items, err = s.runtime.ListSessions(query, s.runtimeConfig, s.memoryPolicy, s.actionPolicy)
		}
		if err != nil {
			writeJSON(w, http.StatusInternalServerError, NewErrorResponse("runtime_error", err.Error()))
			return
		}
		rawItems, err := s.listRawSessionStates(query.ChatID, query.HasChatID)
		if err != nil {
			writeJSON(w, http.StatusInternalServerError, NewErrorResponse("runtime_error", err.Error()))
			return
		}
		items = mergeSessionStates(items, rawItems, query.Limit)
		writeJSON(w, http.StatusOK, SessionListResponse{Items: items})
		return
	}
	if s.core == nil && s.runtime == nil {
		writeJSON(w, http.StatusServiceUnavailable, NewErrorResponse("runtime_unavailable", "runtime api is not configured"))
		return
	}
	sessionID := strings.Trim(strings.TrimPrefix(r.URL.Path, "/api/sessions/"), "/")
	if sessionID == "" {
		writeJSON(w, http.StatusNotFound, NewErrorResponse("not_found", "session not found"))
		return
	}
	switch r.Method {
	case http.MethodGet:
		chatID, err := parseOptionalChatID(r.URL.Query().Get("chat_id"))
		if err != nil {
			writeJSON(w, http.StatusBadRequest, NewErrorResponse("bad_request", "invalid chat_id"))
			return
		}
		var session runtime.SessionState
		if s.core != nil {
			session, err = s.core.SessionState(sessionID, chatID)
		} else {
			session, err = s.runtime.SessionState(sessionID, chatID, s.runtimeConfig, s.memoryPolicy, s.actionPolicy)
		}
		if err != nil {
			writeJSON(w, http.StatusInternalServerError, NewErrorResponse("runtime_error", err.Error()))
			return
		}
		writeJSON(w, http.StatusOK, SessionStateResponse{Session: session})
	case http.MethodPatch, http.MethodDelete:
		r.URL.Path = "/api/runtime/sessions/" + sessionID
		s.handleRuntimeSession(w, r)
	default:
		writeJSON(w, http.StatusMethodNotAllowed, NewErrorResponse("method_not_allowed", "method not allowed"))
	}
}

func (s *Server) handleControl(w http.ResponseWriter, r *http.Request) {
	if s.core == nil && s.runtime == nil {
		writeJSON(w, http.StatusServiceUnavailable, NewErrorResponse("runtime_unavailable", "runtime api is not configured"))
		return
	}
	path := strings.Trim(strings.TrimPrefix(r.URL.Path, "/api/control/"), "/")
	if path == "" {
		writeJSON(w, http.StatusNotFound, NewErrorResponse("not_found", "session not found"))
		return
	}
	if strings.HasSuffix(path, "/actions") {
		sessionID := strings.TrimSuffix(path, "/actions")
		sessionID = strings.Trim(sessionID, "/")
		if sessionID == "" {
			writeJSON(w, http.StatusNotFound, NewErrorResponse("not_found", "session not found"))
			return
		}
		if r.Method != http.MethodPost {
			writeJSON(w, http.StatusMethodNotAllowed, NewErrorResponse("method_not_allowed", "method not allowed"))
			return
		}
		var req ControlActionRequest
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			writeJSON(w, http.StatusBadRequest, NewErrorResponse("bad_request", "invalid json body"))
			return
		}
		var (
			result runtime.ControlActionResult
			err    error
		)
		if s.core != nil {
			result, err = s.core.ExecuteControlAction(sessionID, runtime.ControlActionRequest{
				Action: runtime.ControlAction(req.Action),
				ChatID: req.ChatID,
			})
		} else {
			result, err = s.runtime.ExecuteControlAction(sessionID, req.ChatID, s.runtimeConfig, s.memoryPolicy, s.actionPolicy, runtime.ControlAction(req.Action))
		}
		if err != nil {
			writeJSON(w, http.StatusBadRequest, NewRuntimeErrorResponse(err, "control_action_error", err.Error()))
			return
		}
		writeJSON(w, http.StatusOK, ControlActionResponse{Result: result})
		return
	}
	if r.Method != http.MethodGet {
		writeJSON(w, http.StatusMethodNotAllowed, NewErrorResponse("method_not_allowed", "method not allowed"))
		return
	}
	chatID, err := parseOptionalChatID(r.URL.Query().Get("chat_id"))
	if err != nil {
		writeJSON(w, http.StatusBadRequest, NewErrorResponse("bad_request", "invalid chat_id"))
		return
	}
	var control runtime.ControlState
	if s.core != nil {
		control, err = s.core.ControlState(path, chatID)
	} else {
		control, err = s.runtime.ControlState(path, chatID, s.runtimeConfig, s.memoryPolicy, s.actionPolicy)
	}
	if err != nil {
		writeJSON(w, http.StatusInternalServerError, NewErrorResponse("runtime_error", err.Error()))
		return
	}
	writeJSON(w, http.StatusOK, ControlStateResponse{Control: control})
}

func (s *Server) handleRuntimeSession(w http.ResponseWriter, r *http.Request) {
	if s.core == nil && s.runtime == nil {
		writeJSON(w, http.StatusServiceUnavailable, NewErrorResponse("runtime_unavailable", "runtime api is not configured"))
		return
	}
	sessionID := strings.TrimPrefix(r.URL.Path, "/api/runtime/sessions/")
	sessionID = strings.Trim(sessionID, "/")
	if sessionID == "" {
		writeJSON(w, http.StatusNotFound, NewErrorResponse("not_found", "session not found"))
		return
	}
	switch r.Method {
	case http.MethodGet:
		var (
			summary runtime.RuntimeSummary
			err     error
		)
		if s.core != nil {
			summary, err = s.core.RuntimeSummary(sessionID)
		} else {
			summary, err = s.runtime.RuntimeSummary(sessionID, s.runtimeConfig, s.memoryPolicy, s.actionPolicy)
		}
		if err != nil {
			writeJSON(w, http.StatusInternalServerError, NewErrorResponse("runtime_error", err.Error()))
			return
		}
		writeJSON(w, http.StatusOK, runtimeSummaryResponse(summary))
	case http.MethodPatch:
		var req SessionOverrideRequest
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			writeJSON(w, http.StatusBadRequest, NewErrorResponse("bad_request", "invalid json body"))
			return
		}
		var (
			existing runtime.SessionOverrides
			summary  runtime.RuntimeSummary
			err      error
		)
		if s.core != nil {
			existing, _, err = s.core.SessionOverrides(sessionID)
		} else {
			existing, _, err = s.runtime.SessionOverrides(sessionID)
		}
		if err != nil {
			writeJSON(w, http.StatusInternalServerError, NewErrorResponse("runtime_error", err.Error()))
			return
		}
		existing.SessionID = sessionID
		existing.Runtime = runtime.MergeRequestConfig(existing.Runtime, req.Runtime)
		existing.MemoryPolicy = mergeMemoryOverride(existing.MemoryPolicy, req.MemoryPolicy)
		existing.ActionPolicy = mergeActionOverride(existing.ActionPolicy, req.ActionPolicy)
		existing.UpdatedAt = nowUTC()
		if s.core != nil {
			err = s.core.SaveSessionOverrides(existing)
		} else {
			err = s.runtime.SaveSessionOverrides(existing)
		}
		if err != nil {
			writeJSON(w, http.StatusInternalServerError, NewErrorResponse("runtime_error", err.Error()))
			return
		}
		if s.core != nil {
			summary, err = s.core.RuntimeSummary(sessionID)
		} else {
			summary, err = s.runtime.RuntimeSummary(sessionID, s.runtimeConfig, s.memoryPolicy, s.actionPolicy)
		}
		if err != nil {
			writeJSON(w, http.StatusInternalServerError, NewErrorResponse("runtime_error", err.Error()))
			return
		}
		writeJSON(w, http.StatusOK, runtimeSummaryResponse(summary))
	case http.MethodDelete:
		var (
			summary runtime.RuntimeSummary
			err     error
		)
		if s.core != nil {
			err = s.core.ClearSessionOverrides(sessionID)
		} else {
			err = s.runtime.ClearSessionOverrides(sessionID)
		}
		if err != nil {
			writeJSON(w, http.StatusInternalServerError, NewErrorResponse("runtime_error", err.Error()))
			return
		}
		if s.core != nil {
			summary, err = s.core.RuntimeSummary(sessionID)
		} else {
			summary, err = s.runtime.RuntimeSummary(sessionID, s.runtimeConfig, s.memoryPolicy, s.actionPolicy)
		}
		if err != nil {
			writeJSON(w, http.StatusInternalServerError, NewErrorResponse("runtime_error", err.Error()))
			return
		}
		writeJSON(w, http.StatusOK, runtimeSummaryResponse(summary))
	default:
		writeJSON(w, http.StatusMethodNotAllowed, NewErrorResponse("method_not_allowed", "method not allowed"))
	}
}

func (s *Server) handleApprovals(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		writeJSON(w, http.StatusMethodNotAllowed, NewErrorResponse("method_not_allowed", "method not allowed"))
		return
	}
	if s.core == nil && s.runtime == nil {
		writeJSON(w, http.StatusServiceUnavailable, NewErrorResponse("runtime_unavailable", "runtime api is not configured"))
		return
	}
	sessionID := strings.TrimSpace(r.URL.Query().Get("session_id"))
	if sessionID == "" {
		writeJSON(w, http.StatusBadRequest, NewErrorResponse("missing_session_id", "session_id is required"))
		return
	}
	var items []runtime.ApprovalView
	if s.core != nil {
		items = s.core.ListApprovals(sessionID)
	} else {
		items = s.runtime.PendingApprovals(sessionID)
	}
	out := make([]ApprovalRecordResponse, 0, len(items))
	for _, item := range items {
		out = append(out, ApprovalRecordResponse{
			ID:               item.ID,
			WorkerID:         item.WorkerID,
			SessionID:        item.SessionID,
			Payload:          item.Payload,
			Status:           item.Status,
			Reason:           item.Reason,
			TargetType:       item.TargetType,
			TargetID:         item.TargetID,
			RequestedAt:      item.RequestedAt,
			DecidedAt:        item.DecidedAt,
			DecisionUpdateID: item.DecisionUpdateID,
		})
	}
	writeJSON(w, http.StatusOK, out)
}

func (s *Server) handleApprovalAction(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		writeJSON(w, http.StatusMethodNotAllowed, NewErrorResponse("method_not_allowed", "method not allowed"))
		return
	}
	if s.core == nil && s.runtime == nil {
		writeJSON(w, http.StatusServiceUnavailable, NewErrorResponse("runtime_unavailable", "runtime api is not configured"))
		return
	}
	path := strings.TrimPrefix(r.URL.Path, "/api/approvals/")
	parts := strings.Split(strings.Trim(path, "/"), "/")
	if len(parts) != 2 {
		writeJSON(w, http.StatusNotFound, NewErrorResponse("not_found", "approval endpoint not found"))
		return
	}
	var action approvals.Action
	switch parts[1] {
	case "approve":
		action = approvals.ActionApprove
	case "reject":
		action = approvals.ActionReject
	default:
		writeJSON(w, http.StatusNotFound, NewErrorResponse("not_found", "approval action not found"))
		return
	}
	var (
		view runtime.ApprovalView
		ok   bool
		err  error
	)
	if s.core != nil {
		if action == approvals.ActionApprove {
			view, ok, err = s.core.Approve(parts[0])
		} else {
			view, ok, err = s.core.Reject(parts[0])
		}
	} else {
		view, ok, err = s.runtime.DecideApproval(parts[0], r.Header.Get("X-Update-ID"), action)
	}
	if err != nil {
		writeJSON(w, http.StatusBadRequest, NewErrorResponse("approval_error", err.Error()))
		return
	}
	if !ok {
		writeJSON(w, http.StatusNotFound, NewErrorResponse("not_found", "approval not found"))
		return
	}
	writeJSON(w, http.StatusOK, ApprovalRecordResponse{
		ID:               view.ID,
		WorkerID:         view.WorkerID,
		SessionID:        view.SessionID,
		Payload:          view.Payload,
		Status:           view.Status,
		Reason:           view.Reason,
		TargetType:       view.TargetType,
		TargetID:         view.TargetID,
		RequestedAt:      view.RequestedAt,
		DecidedAt:        view.DecidedAt,
		DecisionUpdateID: view.DecisionUpdateID,
	})
}

func (s *Server) handleRunByID(w http.ResponseWriter, r *http.Request) {
	if r.Method == http.MethodPost && strings.HasSuffix(r.URL.Path, "/cancel") {
		s.handleCancelRun(w, r)
		return
	}
	if r.Method == http.MethodGet && strings.HasSuffix(r.URL.Path, "/replay") {
		s.handleReplayRun(w, r)
		return
	}
	if r.Method != http.MethodGet {
		writeJSON(w, http.StatusMethodNotAllowed, NewErrorResponse("method_not_allowed", "method not allowed"))
		return
	}
	if s.core == nil && s.runtime == nil {
		writeJSON(w, http.StatusServiceUnavailable, NewErrorResponse("runtime_unavailable", "runtime api is not configured"))
		return
	}
	runID := strings.TrimPrefix(r.URL.Path, "/api/runs/")
	runID = strings.Trim(runID, "/")
	if runID == "" {
		writeJSON(w, http.StatusNotFound, NewErrorResponse("not_found", "run not found"))
		return
	}
	var (
		view runtime.RunView
		ok   bool
		err  error
	)
	if s.core != nil {
		view, ok, err = s.core.Run(runID)
	} else {
		view, ok, err = s.runtime.RunView(runID)
	}
	if err != nil {
		writeJSON(w, http.StatusInternalServerError, NewErrorResponse("runtime_error", err.Error()))
		return
	}
	if !ok {
		writeJSON(w, http.StatusNotFound, NewErrorResponse("not_found", "run not found"))
		return
	}
	writeJSON(w, http.StatusOK, RunStatusResponse{Run: view})
}

func (s *Server) handleReplayRun(w http.ResponseWriter, r *http.Request) {
	if s.runtime == nil {
		writeJSON(w, http.StatusServiceUnavailable, NewErrorResponse("runtime_unavailable", "runtime api is not configured"))
		return
	}
	path := strings.TrimPrefix(r.URL.Path, "/api/runs/")
	path = strings.TrimSuffix(path, "/replay")
	runID := strings.Trim(path, "/")
	if runID == "" {
		writeJSON(w, http.StatusNotFound, NewErrorResponse("not_found", "run not found"))
		return
	}
	replay, ok, err := s.runtime.RunReplay(runID)
	if err != nil {
		writeJSON(w, http.StatusInternalServerError, NewErrorResponse("runtime_error", err.Error()))
		return
	}
	if !ok {
		writeJSON(w, http.StatusNotFound, NewErrorResponse("not_found", "run not found"))
		return
	}
	writeJSON(w, http.StatusOK, RunReplayResponse{Replay: replay})
}

func (s *Server) handleJobs(w http.ResponseWriter, r *http.Request) {
	if s.core == nil && s.jobs == nil {
		writeJSON(w, http.StatusServiceUnavailable, NewRuntimeErrorResponse(runtime.NewControlError(runtime.ErrRuntimeUnavailable, "jobs service is not configured"), "runtime_unavailable", "jobs service is not configured"))
		return
	}
	if r.URL.Path == "/api/jobs" {
		switch r.Method {
		case http.MethodGet:
			limit := 20
			if raw := strings.TrimSpace(r.URL.Query().Get("limit")); raw != "" {
				parsed, err := strconv.Atoi(raw)
				if err != nil || parsed <= 0 {
					writeJSON(w, http.StatusBadRequest, NewErrorResponse("bad_request", "invalid limit"))
					return
				}
				limit = parsed
			}
			var (
				items []runtime.JobView
				err   error
			)
			if s.core != nil {
				items, err = s.core.ListJobs(limit)
			} else {
				items, err = s.jobs.List(limit)
			}
			if err != nil {
				writeJSON(w, http.StatusInternalServerError, NewRuntimeErrorResponse(err, "job_error", err.Error()))
				return
			}
			writeJSON(w, http.StatusOK, JobListResponse{Items: items})
			return
		case http.MethodPost:
			var req CreateJobRequest
			if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
				writeJSON(w, http.StatusBadRequest, NewErrorResponse("bad_request", "invalid json body"))
				return
			}
			startReq := runtime.JobStartRequest{
				Kind:           req.Kind,
				OwnerRunID:     req.OwnerRunID,
				OwnerWorkerID:  req.OwnerWorkerID,
				ChatID:         req.ChatID,
				SessionID:      req.SessionID,
				Command:        req.Command,
				Args:           req.Args,
				Cwd:            req.Cwd,
				PolicySnapshot: s.policySnapshot(req.SessionID),
			}
			var (
				job runtime.JobView
				err error
			)
			if s.core != nil {
				job, err = s.core.StartJobDetached(r.Context(), startReq)
			} else {
				job, err = s.jobs.StartDetached(r.Context(), startReq)
			}
			if err != nil {
				writeJSON(w, http.StatusBadRequest, NewRuntimeErrorResponse(err, "job_error", err.Error()))
				return
			}
			writeJSON(w, http.StatusAccepted, CreateJobResponse{Job: job})
			return
		default:
			writeJSON(w, http.StatusMethodNotAllowed, NewErrorResponse("method_not_allowed", "method not allowed"))
			return
		}
	}

	path := strings.Trim(strings.TrimPrefix(r.URL.Path, "/api/jobs/"), "/")
	if path == "" {
		writeJSON(w, http.StatusNotFound, NewErrorResponse("not_found", "job not found"))
		return
	}
	parts := strings.Split(path, "/")
	jobID := parts[0]
	if len(parts) == 1 {
		if r.Method != http.MethodGet {
			writeJSON(w, http.StatusMethodNotAllowed, NewErrorResponse("method_not_allowed", "method not allowed"))
			return
		}
		var (
			job runtime.JobView
			ok  bool
			err error
		)
		if s.core != nil {
			job, ok, err = s.core.Job(jobID)
		} else {
			job, ok, err = s.jobs.Job(jobID)
		}
		if err != nil {
			writeJSON(w, http.StatusInternalServerError, NewRuntimeErrorResponse(err, "job_error", err.Error()))
			return
		}
		if !ok {
			writeJSON(w, http.StatusNotFound, NewErrorResponse("not_found", "job not found"))
			return
		}
		writeJSON(w, http.StatusOK, JobStatusResponse{Job: job})
		return
	}
	switch parts[1] {
	case "logs":
		if r.Method != http.MethodGet {
			writeJSON(w, http.StatusMethodNotAllowed, NewErrorResponse("method_not_allowed", "method not allowed"))
			return
		}
		query := runtime.JobLogQuery{JobID: jobID, Limit: 200}
		if stream := strings.TrimSpace(r.URL.Query().Get("stream")); stream != "" {
			query.Stream = stream
		}
		if raw := strings.TrimSpace(r.URL.Query().Get("after_id")); raw != "" {
			parsed, err := strconv.ParseInt(raw, 10, 64)
			if err != nil || parsed < 0 {
				writeJSON(w, http.StatusBadRequest, NewErrorResponse("bad_request", "invalid after_id"))
				return
			}
			query.AfterID = parsed
		}
		if raw := strings.TrimSpace(r.URL.Query().Get("limit")); raw != "" {
			parsed, err := strconv.Atoi(raw)
			if err != nil || parsed <= 0 {
				writeJSON(w, http.StatusBadRequest, NewErrorResponse("bad_request", "invalid limit"))
				return
			}
			query.Limit = parsed
		}
		var (
			items []runtime.JobLogChunk
			err   error
		)
		if s.core != nil {
			items, err = s.core.JobLogs(query)
		} else {
			items, err = s.jobs.Logs(query)
		}
		if err != nil {
			writeJSON(w, http.StatusInternalServerError, NewRuntimeErrorResponse(err, "job_error", err.Error()))
			return
		}
		writeJSON(w, http.StatusOK, JobLogsResponse{Items: items})
	case "cancel":
		if r.Method != http.MethodPost {
			writeJSON(w, http.StatusMethodNotAllowed, NewErrorResponse("method_not_allowed", "method not allowed"))
			return
		}
		var (
			ok  bool
			err error
		)
		if s.core != nil {
			ok, err = s.core.CancelJob(jobID)
		} else {
			ok, err = s.jobs.Cancel(jobID)
		}
		if err != nil {
			writeJSON(w, http.StatusInternalServerError, NewRuntimeErrorResponse(err, "job_error", err.Error()))
			return
		}
		if !ok {
			writeJSON(w, http.StatusNotFound, NewErrorResponse("not_found", "job not found"))
			return
		}
		writeJSON(w, http.StatusOK, map[string]any{"ok": true, "job_id": jobID})
	default:
		writeJSON(w, http.StatusNotFound, NewErrorResponse("not_found", "job endpoint not found"))
	}
}

func (s *Server) handleWorkers(w http.ResponseWriter, r *http.Request) {
	if s.core == nil && s.workers == nil {
		writeJSON(w, http.StatusServiceUnavailable, NewRuntimeErrorResponse(runtime.NewControlError(runtime.ErrRuntimeUnavailable, "workers service is not configured"), "runtime_unavailable", "workers service is not configured"))
		return
	}
	if r.URL.Path == "/api/workers" {
		switch r.Method {
		case http.MethodGet:
			query := runtime.WorkerQuery{Limit: 20}
			if raw := strings.TrimSpace(r.URL.Query().Get("chat_id")); raw != "" {
				chatID, err := strconv.ParseInt(raw, 10, 64)
				if err != nil {
					writeJSON(w, http.StatusBadRequest, NewErrorResponse("bad_request", "invalid chat_id"))
					return
				}
				query.ParentChatID = chatID
				query.HasParentChatID = true
			}
			var (
				items []runtime.WorkerView
				err   error
			)
			if s.core != nil {
				items, err = s.core.ListWorkers(query)
			} else {
				items, err = s.workers.List(query)
			}
			if err != nil {
				writeJSON(w, http.StatusInternalServerError, NewRuntimeErrorResponse(err, "worker_error", err.Error()))
				return
			}
			writeJSON(w, http.StatusOK, WorkerListResponse{Items: items})
			return
		case http.MethodPost:
			var req CreateWorkerRequest
			if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
				writeJSON(w, http.StatusBadRequest, NewErrorResponse("bad_request", "invalid json body"))
				return
			}
			spawnReq := runtime.WorkerSpawnRequest{
				WorkerID:        req.WorkerID,
				ParentChatID:    req.ChatID,
				ParentSessionID: req.SessionID,
				Prompt:          req.Prompt,
				PolicySnapshot:  s.policySnapshot(req.SessionID),
			}
			var (
				worker runtime.WorkerView
				err    error
			)
			if s.core != nil {
				worker, err = s.core.SpawnWorker(r.Context(), spawnReq)
			} else {
				worker, err = s.workers.Spawn(r.Context(), spawnReq)
			}
			if err != nil {
				writeJSON(w, http.StatusBadRequest, NewRuntimeErrorResponse(err, "worker_error", err.Error()))
				return
			}
			writeJSON(w, http.StatusAccepted, WorkerStatusResponse{Worker: worker})
			return
		default:
			writeJSON(w, http.StatusMethodNotAllowed, NewErrorResponse("method_not_allowed", "method not allowed"))
			return
		}
	}

	path := strings.Trim(strings.TrimPrefix(r.URL.Path, "/api/workers/"), "/")
	if path == "" {
		writeJSON(w, http.StatusNotFound, NewErrorResponse("not_found", "worker not found"))
		return
	}
	parts := strings.Split(path, "/")
	workerID := parts[0]
	if len(parts) == 1 {
		if r.Method != http.MethodGet {
			writeJSON(w, http.StatusMethodNotAllowed, NewErrorResponse("method_not_allowed", "method not allowed"))
			return
		}
		var (
			worker runtime.WorkerView
			ok     bool
			err    error
		)
		if s.core != nil {
			worker, ok, err = s.core.Worker(workerID)
		} else {
			worker, ok, err = s.workers.Worker(workerID)
		}
		if err != nil {
			writeJSON(w, http.StatusInternalServerError, NewRuntimeErrorResponse(err, "worker_error", err.Error()))
			return
		}
		if !ok {
			writeJSON(w, http.StatusNotFound, NewErrorResponse("not_found", "worker not found"))
			return
		}
		writeJSON(w, http.StatusOK, WorkerStatusResponse{Worker: worker})
		return
	}

	switch parts[1] {
	case "messages":
		if r.Method != http.MethodPost {
			writeJSON(w, http.StatusMethodNotAllowed, NewErrorResponse("method_not_allowed", "method not allowed"))
			return
		}
		var req WorkerMessageRequest
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			writeJSON(w, http.StatusBadRequest, NewErrorResponse("bad_request", "invalid json body"))
			return
		}
		var (
			worker runtime.WorkerView
			err    error
		)
		if s.core != nil {
			worker, err = s.core.MessageWorker(r.Context(), workerID, runtime.WorkerMessageRequest{Content: req.Content})
		} else {
			worker, err = s.workers.Message(r.Context(), workerID, runtime.WorkerMessageRequest{Content: req.Content})
		}
		if err != nil {
			writeJSON(w, http.StatusBadRequest, NewRuntimeErrorResponse(err, "worker_error", err.Error()))
			return
		}
		writeJSON(w, http.StatusAccepted, WorkerStatusResponse{Worker: worker})
	case "wait":
		if r.Method != http.MethodGet {
			writeJSON(w, http.StatusMethodNotAllowed, NewErrorResponse("method_not_allowed", "method not allowed"))
			return
		}
		afterCursor, _ := strconv.Atoi(strings.TrimSpace(r.URL.Query().Get("after_cursor")))
		afterEventID, _ := strconv.ParseInt(strings.TrimSpace(r.URL.Query().Get("after_event_id")), 10, 64)
		var (
			result runtime.WorkerWaitResult
			ok     bool
			err    error
		)
		if s.core != nil {
			result, ok, err = s.core.WaitWorker(workerID, afterCursor, afterEventID, 50)
		} else {
			result, ok, err = s.workers.Wait(workerID, afterCursor, afterEventID, 50)
		}
		if err != nil {
			writeJSON(w, http.StatusInternalServerError, NewRuntimeErrorResponse(err, "worker_error", err.Error()))
			return
		}
		if !ok {
			writeJSON(w, http.StatusNotFound, NewErrorResponse("not_found", "worker not found"))
			return
		}
		writeJSON(w, http.StatusOK, WorkerWaitResponse{
			Worker:         result.Worker,
			Handoff:        result.Handoff,
			Messages:       result.Messages,
			Events:         result.Events,
			NextCursor:     result.NextCursor,
			NextEventAfter: result.NextEventAfter,
		})
	case "handoff":
		if r.Method != http.MethodGet {
			writeJSON(w, http.StatusMethodNotAllowed, NewErrorResponse("method_not_allowed", "method not allowed"))
			return
		}
		var (
			handoff runtime.WorkerHandoff
			ok      bool
			err     error
		)
		if s.core != nil {
			handoff, ok, err = s.core.WorkerHandoff(workerID)
		} else {
			handoff, ok, err = s.workers.Handoff(workerID)
		}
		if err != nil {
			writeJSON(w, http.StatusInternalServerError, NewRuntimeErrorResponse(err, "worker_error", err.Error()))
			return
		}
		if !ok {
			writeJSON(w, http.StatusNotFound, NewErrorResponse("not_found", "worker handoff not found"))
			return
		}
		writeJSON(w, http.StatusOK, WorkerHandoffResponse{Handoff: handoff})
	case "close":
		if r.Method != http.MethodPost {
			writeJSON(w, http.StatusMethodNotAllowed, NewErrorResponse("method_not_allowed", "method not allowed"))
			return
		}
		var (
			worker runtime.WorkerView
			ok     bool
			err    error
		)
		if s.core != nil {
			worker, ok, err = s.core.CloseWorker(workerID)
		} else {
			worker, ok, err = s.workers.Close(workerID)
		}
		if err != nil {
			writeJSON(w, http.StatusInternalServerError, NewRuntimeErrorResponse(err, "worker_error", err.Error()))
			return
		}
		if !ok {
			writeJSON(w, http.StatusNotFound, NewErrorResponse("not_found", "worker not found"))
			return
		}
		writeJSON(w, http.StatusOK, WorkerStatusResponse{Worker: worker})
	default:
		writeJSON(w, http.StatusNotFound, NewErrorResponse("not_found", "worker endpoint not found"))
	}
}

func (s *Server) handleRuns(w http.ResponseWriter, r *http.Request) {
	if r.Method == http.MethodGet {
		s.handleRunList(w, r)
		return
	}
	if r.Method != http.MethodPost {
		writeJSON(w, http.StatusMethodNotAllowed, NewErrorResponse("method_not_allowed", "method not allowed"))
		return
	}
	if s.core == nil && s.runner == nil {
		writeJSON(w, http.StatusServiceUnavailable, NewRuntimeErrorResponse(runtime.NewControlError(runtime.ErrRuntimeUnavailable, "run starter is not configured"), "runner_unavailable", "run starter is not configured"))
		return
	}
	var req CreateRunRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeJSON(w, http.StatusBadRequest, NewErrorResponse("bad_request", "invalid json body"))
		return
	}
	startReq := runtime.StartRunRequest{
		ChatID:         req.ChatID,
		SessionID:      req.SessionID,
		Query:          req.Query,
		PolicySnapshot: mergePolicySnapshotRuntime(s.policySnapshot(req.SessionID), req.Config),
	}
	var (
		view runtime.RunView
		ok   bool
		err  error
	)
	if s.core != nil {
		view, ok, err = s.core.StartRunDetached(r.Context(), startReq)
	} else {
		view, ok, err = s.runner.StartDetached(r.Context(), startReq)
	}
	if err != nil {
		writeJSON(w, http.StatusInternalServerError, NewRuntimeErrorResponse(err, "run_start_failed", err.Error()))
		return
	}
	if !ok {
		writeJSON(w, http.StatusConflict, CreateRunResponse{
			Accepted: false,
			Error:    &APIError{Code: "run_busy", Message: "another run is already active for this chat"},
		})
		return
	}
	writeJSON(w, http.StatusAccepted, CreateRunResponse{
		RunID:    view.RunID,
		Accepted: true,
		Run:      view,
	})
}

func (s *Server) handleRunList(w http.ResponseWriter, r *http.Request) {
	if s.core == nil && s.runtime == nil {
		writeJSON(w, http.StatusServiceUnavailable, NewErrorResponse("runtime_unavailable", "runtime api is not configured"))
		return
	}
	query := runtime.RunQuery{Limit: 20}
	if raw := strings.TrimSpace(r.URL.Query().Get("chat_id")); raw != "" {
		chatID, err := strconv.ParseInt(raw, 10, 64)
		if err != nil {
			writeJSON(w, http.StatusBadRequest, NewErrorResponse("bad_request", "invalid chat_id"))
			return
		}
		query.ChatID = chatID
		query.HasChatID = true
	}
	query.SessionID = strings.TrimSpace(r.URL.Query().Get("session_id"))
	if raw := strings.TrimSpace(r.URL.Query().Get("status")); raw != "" {
		query.Status = runtime.RunStatus(raw)
		query.HasStatus = true
	}
	if raw := strings.TrimSpace(r.URL.Query().Get("limit")); raw != "" {
		limit, err := strconv.Atoi(raw)
		if err != nil || limit <= 0 {
			writeJSON(w, http.StatusBadRequest, NewErrorResponse("bad_request", "invalid limit"))
			return
		}
		query.Limit = limit
	}
	var (
		items []runtime.RunView
		err   error
	)
	if s.core != nil {
		items, err = s.core.ListRuns(query)
	} else {
		items, err = s.runtime.ListRuns(query)
	}
	if err != nil {
		writeJSON(w, http.StatusInternalServerError, NewErrorResponse("runtime_error", err.Error()))
		return
	}
	writeJSON(w, http.StatusOK, RunListResponse{Items: items})
}

func (s *Server) handleCancelRun(w http.ResponseWriter, r *http.Request) {
	if s.core == nil && s.runtime == nil {
		writeJSON(w, http.StatusServiceUnavailable, NewErrorResponse("runtime_unavailable", "runtime api is not configured"))
		return
	}
	path := strings.TrimPrefix(r.URL.Path, "/api/runs/")
	path = strings.TrimSuffix(path, "/cancel")
	runID := strings.Trim(path, "/")
	if runID == "" {
		writeJSON(w, http.StatusNotFound, NewErrorResponse("not_found", "run not found"))
		return
	}
	var (
		ok  bool
		err error
	)
	if s.core != nil {
		ok, err = s.core.CancelRunByID(runID)
	} else {
		ok, err = s.runtime.CancelRunByID(runID)
	}
	if err != nil {
		writeJSON(w, http.StatusInternalServerError, NewErrorResponse("runtime_error", err.Error()))
		return
	}
	if !ok {
		writeJSON(w, http.StatusNotFound, NewErrorResponse("not_found", "run not found"))
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{"ok": true, "run_id": runID})
}

func writeJSON(w http.ResponseWriter, status int, v any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(v)
}

func (s *Server) handleMemorySearch(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		writeJSON(w, http.StatusMethodNotAllowed, NewErrorResponse("method_not_allowed", "method not allowed"))
		return
	}
	if s.memory == nil {
		writeJSON(w, http.StatusServiceUnavailable, NewErrorResponse("memory_unavailable", "memory store is not configured"))
		return
	}
	query := strings.TrimSpace(r.URL.Query().Get("query"))
	if query == "" {
		writeJSON(w, http.StatusBadRequest, NewErrorResponse("missing_query", "query is required"))
		return
	}
	chatID := int64(0)
	if raw := strings.TrimSpace(r.URL.Query().Get("chat_id")); raw != "" {
		if parsed, err := strconv.ParseInt(raw, 10, 64); err == nil {
			chatID = parsed
		}
	}
	limit := 5
	if raw := strings.TrimSpace(r.URL.Query().Get("limit")); raw != "" {
		if parsed, err := strconv.Atoi(raw); err == nil && parsed > 0 {
			limit = parsed
		}
	}
	items, err := s.memory.Search(memory.RecallQuery{
		ChatID:    chatID,
		SessionID: strings.TrimSpace(r.URL.Query().Get("session_id")),
		Text:      query,
		Limit:     limit,
		Kinds:     splitKinds(r.URL.Query().Get("kinds")),
	})
	if err != nil {
		writeJSON(w, http.StatusInternalServerError, NewErrorResponse("memory_error", err.Error()))
		return
	}
	writeJSON(w, http.StatusOK, MemorySearchResponse{Items: items})
}

func (s *Server) handleMemoryRead(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		writeJSON(w, http.StatusMethodNotAllowed, NewErrorResponse("method_not_allowed", "method not allowed"))
		return
	}
	if s.memory == nil {
		writeJSON(w, http.StatusServiceUnavailable, NewErrorResponse("memory_unavailable", "memory store is not configured"))
		return
	}
	docKey := strings.Trim(strings.TrimPrefix(r.URL.Path, "/api/memory/"), "/")
	if docKey == "" {
		writeJSON(w, http.StatusNotFound, NewErrorResponse("not_found", "memory document not found"))
		return
	}
	doc, ok, err := s.memory.Get(docKey)
	if err != nil {
		writeJSON(w, http.StatusInternalServerError, NewErrorResponse("memory_error", err.Error()))
		return
	}
	if !ok {
		writeJSON(w, http.StatusNotFound, NewErrorResponse("not_found", "memory document not found"))
		return
	}
	writeJSON(w, http.StatusOK, MemoryDocumentResponse{Document: doc})
}

func splitKinds(raw string) []string {
	if strings.TrimSpace(raw) == "" {
		return nil
	}
	parts := strings.Split(raw, ",")
	out := make([]string, 0, len(parts))
	for _, part := range parts {
		part = strings.TrimSpace(part)
		if part != "" {
			out = append(out, part)
		}
	}
	if len(out) == 0 {
		return nil
	}
	return out
}

func parseOptionalChatID(raw string) (int64, error) {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return 0, nil
	}
	return strconv.ParseInt(raw, 10, 64)
}

func runtimeSummaryResponse(summary runtime.RuntimeSummary) RuntimeSummaryResponse {
	out := RuntimeSummaryResponse{
		SessionID:    summary.SessionID,
		Runtime:      summary.Runtime,
		MemoryPolicy: summary.MemoryPolicy,
		ActionPolicy: summary.ActionPolicy,
		HasOverrides: summary.HasOverrides,
	}
	if summary.HasOverrides {
		updatedAt := summary.Overrides.UpdatedAt
		out.Overrides = &SessionOverrideResponse{
			SessionID:    summary.Overrides.SessionID,
			Runtime:      summary.Overrides.Runtime,
			MemoryPolicy: summary.Overrides.MemoryPolicy,
			ActionPolicy: summary.Overrides.ActionPolicy,
			UpdatedAt:    &updatedAt,
		}
	}
	return out
}

func mergeMemoryOverride(base, patch runtime.MemoryPolicyOverride) runtime.MemoryPolicyOverride {
	out := base
	if strings.TrimSpace(patch.Profile) != "" {
		out.Profile = patch.Profile
	}
	if patch.PromoteCheckpoint != nil {
		out.PromoteCheckpoint = patch.PromoteCheckpoint
	}
	if patch.PromoteContinuity != nil {
		out.PromoteContinuity = patch.PromoteContinuity
	}
	if patch.AutomaticRecallKinds != nil {
		out.AutomaticRecallKinds = append([]string(nil), patch.AutomaticRecallKinds...)
	}
	if patch.MaxDocumentBodyChars != nil {
		out.MaxDocumentBodyChars = patch.MaxDocumentBodyChars
	}
	if patch.MaxResolvedFacts != nil {
		out.MaxResolvedFacts = patch.MaxResolvedFacts
	}
	return out
}

func mergeActionOverride(base, patch runtime.ActionPolicyOverride) runtime.ActionPolicyOverride {
	out := base
	if patch.ApprovalRequiredTools != nil {
		out.ApprovalRequiredTools = append([]string(nil), patch.ApprovalRequiredTools...)
	}
	return out
}
