package runtime

import (
	"strings"
)

func PreviewArtifactContent(content, query string, maxLines int) string {
	if maxLines <= 0 {
		maxLines = 3
	}
	trimmed := strings.TrimSpace(content)
	if trimmed == "" {
		return ""
	}
	lines := strings.Split(trimmed, "\n")
	if query != "" {
		needle := strings.ToLower(strings.TrimSpace(query))
		for i, line := range lines {
			if !strings.Contains(strings.ToLower(line), needle) {
				continue
			}
			start := i - 1
			if start < 0 {
				start = 0
			}
			end := i + 2
			if end > len(lines) {
				end = len(lines)
			}
			return strings.Join(lines[start:end], "\n")
		}
	}
	if len(lines) > maxLines {
		lines = lines[:maxLines]
	}
	return strings.Join(lines, "\n")
}
