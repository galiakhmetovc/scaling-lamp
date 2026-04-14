package runtime

import (
	"fmt"
	"strings"

	"teamd/internal/artifacts"
)

func MaybeOffloadToolResult(store artifacts.Store, policy ArtifactOffloadPolicy, owner ArtifactOwnerRef, toolName, content string) (OffloadedToolResult, error) {
	if store == nil {
		return OffloadedToolResult{Content: content}, nil
	}
	if !shouldOffloadToolResult(policy, content) {
		return OffloadedToolResult{Content: content}, nil
	}
	name := fmt.Sprintf("%s-%s", sanitizeArtifactName(owner.OwnerID), sanitizeArtifactName(toolName))
	ref, err := store.Save(owner.OwnerType, owner.OwnerID, name, []byte(content))
	if err != nil {
		return OffloadedToolResult{}, err
	}
	preview := previewLines(content, policy.PreviewLines)
	body := []string{
		"tool output offloaded",
		"artifact_ref: " + ref,
		"",
		preview,
	}
	return OffloadedToolResult{
		Content:     strings.TrimSpace(strings.Join(body, "\n")),
		ArtifactRef: ref,
		Offloaded:   true,
	}, nil
}

func shouldOffloadToolResult(policy ArtifactOffloadPolicy, content string) bool {
	if policy.MaxInlineChars > 0 && len(content) > policy.MaxInlineChars {
		return true
	}
	if policy.MaxInlineLines > 0 && countLines(content) > policy.MaxInlineLines {
		return true
	}
	return false
}

func previewLines(content string, limit int) string {
	if limit <= 0 {
		limit = 3
	}
	lines := strings.Split(content, "\n")
	if len(lines) > limit {
		lines = lines[:limit]
	}
	return strings.Join(lines, "\n")
}

func countLines(content string) int {
	if content == "" {
		return 0
	}
	return strings.Count(content, "\n") + 1
}

func sanitizeArtifactName(v string) string {
	v = strings.TrimSpace(v)
	v = strings.ReplaceAll(v, "/", "-")
	v = strings.ReplaceAll(v, ":", "-")
	if v == "" {
		return "artifact"
	}
	return v
}
