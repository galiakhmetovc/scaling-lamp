package telegram

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	runtimex "teamd/internal/runtime"
	"teamd/internal/provider"
)

func (a *Adapter) executeProjectCaptureRecentTool(chatID int64, call provider.ToolCall) (string, error) {
	if strings.TrimSpace(a.workspaceRoot) == "" {
		return "", fmt.Errorf("project capture requires workspace root")
	}
	snapshot, ok, err := a.recentWorkSnapshotRecord(chatID, "save this as a project")
	if err != nil {
		return "", err
	}
	if !ok {
		return "recent project capture unavailable", nil
	}
	projectPath, err := a.resolveProjectCapturePath(snapshot, call)
	if err != nil {
		return "", err
	}
	title := strings.TrimSpace(stringArg(call.Arguments, "title"))
	if title == "" {
		title = strings.TrimSpace(snapshot.Head.CurrentGoal)
	}
	if title == "" {
		title = "Recent work capture"
	}
	if err := a.writeProjectCapture(projectPath, title, snapshot); err != nil {
		return "", err
	}
	if err := a.updateSessionHeadProject(chatID, projectPath, snapshot.Head); err != nil {
		return "", err
	}
	return fmt.Sprintf("project captured: %s", projectPath), nil
}

func (a *Adapter) recentWorkSnapshotRecord(chatID int64, query string) (runtimex.RecentWorkSnapshot, bool, error) {
	sessionID := a.meshSessionID(chatID)
	switch {
	case a.agentCore != nil:
		return a.agentCore.RecentWorkSnapshot(chatID, sessionID, query)
	case a.runtimeAPI != nil:
		return a.runtimeAPI.RecentWorkSnapshot(chatID, sessionID, query)
	default:
		return runtimex.RecentWorkSnapshot{}, false, nil
	}
}

func (a *Adapter) resolveProjectCapturePath(snapshot runtimex.RecentWorkSnapshot, call provider.ToolCall) (string, error) {
	if raw := strings.TrimSpace(stringArg(call.Arguments, "project_path")); raw != "" {
		return a.normalizeProjectPath(raw)
	}
	if raw := strings.TrimSpace(snapshot.Head.CurrentProject); raw != "" {
		return a.normalizeProjectPath(raw)
	}
	base := "recent-work"
	if text := asciiSlug(snapshot.Head.CurrentGoal); text != "" {
		base = text
	}
	if text := strings.TrimSpace(snapshot.Head.LastCompletedRunID); text != "" {
		base += "-" + asciiSlug(text)
	}
	return a.normalizeProjectPath(filepath.Join("projects", base))
}

func (a *Adapter) normalizeProjectPath(raw string) (string, error) {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return "", fmt.Errorf("empty project path")
	}
	path := raw
	if !filepath.IsAbs(path) {
		path = filepath.Join(a.workspaceRoot, path)
	}
	path = filepath.Clean(path)
	root := filepath.Clean(a.workspaceRoot)
	rel, err := filepath.Rel(root, path)
	if err != nil {
		return "", err
	}
	if rel == ".." || strings.HasPrefix(rel, ".."+string(filepath.Separator)) {
		return "", fmt.Errorf("project path escapes workspace root")
	}
	return path, nil
}

func (a *Adapter) writeProjectCapture(projectPath, title string, snapshot runtimex.RecentWorkSnapshot) error {
	dirs := []string{
		projectPath,
		filepath.Join(projectPath, "docs"),
		filepath.Join(projectPath, "state"),
		filepath.Join(projectPath, "notes"),
		filepath.Join(projectPath, "artifacts"),
	}
	for _, dir := range dirs {
		if err := os.MkdirAll(dir, 0o755); err != nil {
			return err
		}
	}
	date := time.Now().UTC().Format("2006-01-02")
	readme := fmt.Sprintf("# %s\n\n- current state: `state/current.md`\n- architecture: `docs/architecture.md`\n- decisions: `docs/decisions.md`\n- notes: `notes/%s.md`\n", title, date)
	architecture := strings.Join([]string{
		"# Architecture",
		"",
		"## Recent Run",
		"- run_id: " + strings.TrimSpace(snapshot.Head.LastCompletedRunID),
		"- goal: " + strings.TrimSpace(snapshot.Head.CurrentGoal),
		"",
		"## Result",
		strings.TrimSpace(snapshot.Head.LastResultSummary),
	}, "\n")
	decisions := strings.Join([]string{
		"# Decisions",
		"",
		"- Seeded from recent completed run in SessionHead",
		"- Recent artifacts: " + strings.Join(snapshot.Head.RecentArtifactRefs, ", "),
	}, "\n")
	current := strings.Join([]string{
		"# Current State",
		"",
		"## Goal",
		strings.TrimSpace(snapshot.Head.CurrentGoal),
		"",
		"## Last Result",
		strings.TrimSpace(snapshot.Head.LastResultSummary),
		"",
		"## Recent Run",
		strings.TrimSpace(snapshot.Head.LastCompletedRunID),
	}, "\n")
	backlog := "# Backlog\n\n- Review captured artifacts and add follow-up items if needed.\n"
	note := strings.Join([]string{
		"# " + date,
		"",
		"Captured from recent completed run.",
		"",
		"Run: " + strings.TrimSpace(snapshot.Head.LastCompletedRunID),
		"Goal: " + strings.TrimSpace(snapshot.Head.CurrentGoal),
		"Result: " + strings.TrimSpace(snapshot.Head.LastResultSummary),
	}, "\n")
	files := map[string]string{
		filepath.Join(projectPath, "README.md"):                 readme,
		filepath.Join(projectPath, "docs", "architecture.md"):   architecture,
		filepath.Join(projectPath, "docs", "decisions.md"):      decisions,
		filepath.Join(projectPath, "state", "current.md"):       current,
		filepath.Join(projectPath, "state", "backlog.md"):       backlog,
		filepath.Join(projectPath, "notes", date+".md"):         note,
	}
	for path, body := range files {
		if err := os.WriteFile(path, []byte(body), 0o644); err != nil {
			return err
		}
	}
	return a.updateProjectsIndex(projectPath, title)
}

func (a *Adapter) updateProjectsIndex(projectPath, title string) error {
	indexPath := filepath.Join(a.workspaceRoot, "projects", "index.md")
	if err := os.MkdirAll(filepath.Dir(indexPath), 0o755); err != nil {
		return err
	}
	rel, err := filepath.Rel(a.workspaceRoot, projectPath)
	if err != nil {
		return err
	}
	entry := strings.Join([]string{
		"## " + title,
		"- path: `" + rel + "`",
		"- canonical state: `" + filepath.ToSlash(filepath.Join(rel, "state", "current.md")) + "`",
		"",
	}, "\n")
	existing, _ := os.ReadFile(indexPath)
	text := string(existing)
	if strings.Contains(text, "- path: `"+rel+"`") {
		return nil
	}
	if strings.TrimSpace(text) == "" {
		text = "# Projects Index\n\n"
	}
	return os.WriteFile(indexPath, []byte(text+entry), 0o644)
}

func (a *Adapter) updateSessionHeadProject(chatID int64, projectPath string, head runtimex.SessionHead) error {
	if a.runStore == nil {
		return nil
	}
	rel, err := filepath.Rel(a.workspaceRoot, projectPath)
	if err != nil {
		return err
	}
	head.CurrentProject = filepath.ToSlash(rel)
	head.UpdatedAt = time.Now().UTC()
	return a.runStore.SaveSessionHead(head)
}

func asciiSlug(input string) string {
	input = strings.ToLower(strings.TrimSpace(input))
	var b strings.Builder
	lastDash := false
	for _, r := range input {
		switch {
		case (r >= 'a' && r <= 'z') || (r >= '0' && r <= '9'):
			b.WriteRune(r)
			lastDash = false
		case r == '-' || r == '_' || r == ' ' || r == ':':
			if !lastDash && b.Len() > 0 {
				b.WriteByte('-')
				lastDash = true
			}
		}
	}
	out := strings.Trim(b.String(), "-")
	if out == "" {
		return "recent-work"
	}
	return out
}

func stringArg(args map[string]any, key string) string {
	if args == nil {
		return ""
	}
	value, _ := args[key].(string)
	return value
}
