package workspace

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

const maxAgentsChars = 4000
const maxSelectedFileChars = 4000

func BuildAGENTSContext(root string) string {
	path, body := findNearestAgents(root)
	if path == "" || strings.TrimSpace(body) == "" {
		return ""
	}
	if len(body) > maxAgentsChars {
		body = body[:maxAgentsChars] + "\n[...truncated]"
	}
	return fmt.Sprintf("# Workspace Context\n## AGENTS.md\n_Source: %s_\n%s", path, strings.TrimSpace(body))
}

func BuildSelectedContext(root string, files []string) string {
	start := strings.TrimSpace(root)
	if start == "" || len(files) == 0 {
		return ""
	}
	base, err := filepath.Abs(start)
	if err != nil {
		return ""
	}
	sections := make([]string, 0, len(files))
	seen := map[string]struct{}{}
	for _, item := range files {
		trimmed := strings.TrimSpace(item)
		if trimmed == "" {
			continue
		}
		cleaned := filepath.Clean(trimmed)
		full := cleaned
		if !filepath.IsAbs(cleaned) {
			full = filepath.Join(base, cleaned)
		}
		full, err = filepath.Abs(full)
		if err != nil {
			continue
		}
		rel, err := filepath.Rel(base, full)
		if err != nil || strings.HasPrefix(rel, "..") {
			continue
		}
		if _, ok := seen[full]; ok {
			continue
		}
		body, err := os.ReadFile(full)
		if err != nil {
			continue
		}
		content := string(body)
		if len(content) > maxSelectedFileChars {
			content = content[:maxSelectedFileChars] + "\n[...truncated]"
		}
		sections = append(sections, fmt.Sprintf("## %s\n_Source: %s_\n%s", rel, full, strings.TrimSpace(content)))
		seen[full] = struct{}{}
	}
	if len(sections) == 0 {
		return ""
	}
	return "# Workspace Context\n" + strings.Join(sections, "\n\n")
}

func findNearestAgents(root string) (string, string) {
	start := strings.TrimSpace(root)
	if start == "" {
		return "", ""
	}
	current, err := filepath.Abs(start)
	if err != nil {
		return "", ""
	}
	for {
		candidate := filepath.Join(current, "AGENTS.md")
		body, err := os.ReadFile(candidate)
		if err == nil {
			return candidate, string(body)
		}
		parent := filepath.Dir(current)
		if parent == current {
			return "", ""
		}
		current = parent
	}
}
