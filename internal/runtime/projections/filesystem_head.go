package projections

import (
	"encoding/json"
	"fmt"
	"strings"

	"teamd/internal/runtime/eventing"
)

type FilesystemRecentView struct {
	Edited  []string `json:"edited,omitempty"`
	Read    []string `json:"read,omitempty"`
	Found   []string `json:"found,omitempty"`
	Moved   []string `json:"moved,omitempty"`
	Trashed []string `json:"trashed,omitempty"`
}

type FilesystemHeadSnapshot struct {
	Sessions map[string]FilesystemRecentView `json:"sessions"`
}

type FilesystemHeadProjection struct {
	snapshot FilesystemHeadSnapshot
}

func NewFilesystemHeadProjection() *FilesystemHeadProjection {
	return &FilesystemHeadProjection{snapshot: FilesystemHeadSnapshot{Sessions: map[string]FilesystemRecentView{}}}
}

func (p *FilesystemHeadProjection) ID() string { return "filesystem_head" }

func (p *FilesystemHeadProjection) Apply(event eventing.Event) error {
	if event.Kind != eventing.EventToolCallCompleted {
		return nil
	}
	sessionID, _ := event.Payload["session_id"].(string)
	toolName, _ := event.Payload["tool_name"].(string)
	if strings.TrimSpace(sessionID) == "" || strings.TrimSpace(toolName) == "" {
		return nil
	}
	if p.snapshot.Sessions == nil {
		p.snapshot.Sessions = map[string]FilesystemRecentView{}
	}
	view := p.snapshot.Sessions[sessionID]
	args, _ := event.Payload["arguments"].(map[string]any)
	resultText, _ := event.Payload["result_text"].(string)
	addFilesystemRecentFromTool(&view, toolName, args, resultText)
	p.snapshot.Sessions[sessionID] = view
	return nil
}

func (p *FilesystemHeadProjection) Snapshot() FilesystemHeadSnapshot { return p.snapshot }
func (p *FilesystemHeadProjection) SnapshotValue() any               { return p.snapshot }

func (p *FilesystemHeadProjection) SnapshotForSession(sessionID string) FilesystemRecentView {
	if p.snapshot.Sessions == nil {
		return FilesystemRecentView{}
	}
	return p.snapshot.Sessions[sessionID]
}

func (p *FilesystemHeadProjection) RestoreSnapshot(raw []byte) error {
	var snapshot FilesystemHeadSnapshot
	if err := json.Unmarshal(raw, &snapshot); err != nil {
		return fmt.Errorf("restore filesystem head snapshot: %w", err)
	}
	if snapshot.Sessions == nil {
		snapshot.Sessions = map[string]FilesystemRecentView{}
	}
	p.snapshot = snapshot
	return nil
}

func addFilesystemRecentFromTool(view *FilesystemRecentView, toolName string, args map[string]any, resultText string) {
	if view == nil {
		return
	}
	switch toolName {
	case "fs_read_text", "fs_read_lines", "fs_search_text":
		appendUniqueRecent(&view.Read, argString(args, "path"))
	case "fs_replace_lines", "fs_replace_in_line", "fs_insert_text", "fs_write_text", "fs_patch_text":
		appendUniqueRecent(&view.Edited, argString(args, "path"))
	case "fs_find_in_files":
		for _, path := range resultPaths(resultText) {
			appendUniqueRecent(&view.Found, path)
		}
	case "fs_move":
		src := argString(args, "src")
		dest := argString(args, "dest")
		if src != "" && dest != "" {
			appendUniqueRecent(&view.Moved, src+" -> "+dest)
		}
	case "fs_trash":
		appendUniqueRecent(&view.Trashed, argString(args, "path"))
	}
}

func appendUniqueRecent(target *[]string, value string) {
	value = strings.TrimSpace(value)
	if value == "" {
		return
	}
	items := append([]string(nil), *target...)
	filtered := make([]string, 0, len(items)+1)
	filtered = append(filtered, value)
	for _, item := range items {
		if item == value {
			continue
		}
		filtered = append(filtered, item)
	}
	*target = filtered
}

func argString(args map[string]any, key string) string {
	if args == nil {
		return ""
	}
	value, _ := args[key].(string)
	return value
}

func resultPaths(resultText string) []string {
	if strings.TrimSpace(resultText) == "" {
		return nil
	}
	var payload struct {
		Path    string `json:"path"`
		Matches []struct {
			Path string `json:"path"`
		} `json:"matches"`
	}
	if err := json.Unmarshal([]byte(resultText), &payload); err != nil {
		return nil
	}
	out := make([]string, 0, len(payload.Matches)+1)
	if strings.TrimSpace(payload.Path) != "" {
		out = append(out, payload.Path)
	}
	for _, match := range payload.Matches {
		if strings.TrimSpace(match.Path) == "" {
			continue
		}
		out = append(out, match.Path)
	}
	return out
}
