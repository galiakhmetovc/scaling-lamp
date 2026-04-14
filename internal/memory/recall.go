package memory

import (
	"fmt"
	"strings"
)

func NormalizeRecallKinds(kinds []string) []string {
	out := make([]string, 0, len(kinds))
	seen := map[string]struct{}{}
	for _, kind := range kinds {
		kind = strings.ToLower(strings.TrimSpace(kind))
		if kind == "" {
			continue
		}
		if _, ok := seen[kind]; ok {
			continue
		}
		seen[kind] = struct{}{}
		out = append(out, kind)
	}
	return out
}

func FormatRecallBlock(items []RecallItem) string {
	if len(items) == 0 {
		return ""
	}
	lines := []string{"Relevant memory recall."}
	for _, item := range items {
		title := strings.TrimSpace(item.Title)
		if title == "" {
			title = item.Kind
		}
		body := strings.TrimSpace(item.Body)
		if len(body) > 280 {
			body = body[:280] + "..."
		}
		lines = append(lines, fmt.Sprintf("- [%s] %s: %s", item.Kind, title, body))
	}
	return strings.Join(lines, "\n")
}
