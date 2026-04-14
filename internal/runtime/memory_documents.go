package runtime

import (
	"fmt"
	"strings"
	"time"

	"teamd/internal/memory"
	"teamd/internal/worker"
)

func BuildCheckpointDocument(chatID int64, sessionID, originatingIntent string, checkpoint worker.Checkpoint, updatedAt time.Time) (memory.Document, bool) {
	return BuildCheckpointDocumentWithPolicy(DefaultMemoryPolicy(), chatID, sessionID, originatingIntent, checkpoint, updatedAt)
}

func BuildCheckpointDocumentWithPolicy(policy MemoryPolicy, chatID int64, sessionID, originatingIntent string, checkpoint worker.Checkpoint, updatedAt time.Time) (memory.Document, bool) {
	policy = NormalizeMemoryPolicy(policy)
	if !policy.PromoteCheckpoint {
		return memory.Document{}, false
	}
	title := strings.TrimSpace(originatingIntent)
	whatHappened := strings.TrimSpace(checkpoint.WhatHappened)
	whatMatters := strings.TrimSpace(checkpoint.WhatMattersNow)
	if title == "" || whatMatters == "" {
		return memory.Document{}, false
	}
	if isNoisyMemoryText(whatHappened) || isNoisyMemoryText(whatMatters) {
		return memory.Document{}, false
	}
	body := trimMemoryBody(policy, strings.TrimSpace(whatHappened+"\n\nWhat matters now: "+whatMatters))
	body = appendReferenceSections(body, checkpoint.ArchiveRefs, checkpoint.SourceArtifacts)
	if isNoisyMemoryText(body) {
		return memory.Document{}, false
	}
	return memory.Document{
		DocKey:    fmt.Sprintf("checkpoint:%d:%s", chatID, sessionID),
		Scope:     memory.ScopeSession,
		ChatID:    chatID,
		SessionID: sessionID,
		Kind:      "checkpoint",
		Title:     title,
		Body:      body,
		Source:    "runtime_checkpoint",
		UpdatedAt: updatedAt,
	}, true
}

func BuildContinuityDocument(c Continuity) (memory.Document, bool) {
	return BuildContinuityDocumentWithPolicy(DefaultMemoryPolicy(), c)
}

func BuildContinuityDocumentWithPolicy(policy MemoryPolicy, c Continuity) (memory.Document, bool) {
	policy = NormalizeMemoryPolicy(policy)
	if !policy.PromoteContinuity {
		return memory.Document{}, false
	}
	if strings.TrimSpace(c.UserGoal) == "" {
		return memory.Document{}, false
	}
	body := strings.TrimSpace("User goal: " + c.UserGoal + "\nCurrent state: " + c.CurrentState)
	if len(c.ResolvedFacts) > 0 {
		resolved := c.ResolvedFacts
		if len(resolved) > policy.MaxResolvedFacts {
			resolved = resolved[:policy.MaxResolvedFacts]
		}
		body += "\nResolved facts:\n- " + strings.Join(resolved, "\n- ")
	}
	if len(c.UnresolvedItems) > 0 {
		body += "\nUnresolved:\n- " + strings.Join(c.UnresolvedItems, "\n- ")
	}
	body = appendReferenceSections(body, c.ArchiveRefs, c.ArtifactRefs)
	body = trimMemoryBody(policy, body)
	return memory.Document{
		DocKey:    fmt.Sprintf("continuity:%d:%s", c.ChatID, c.SessionID),
		Scope:     memory.ScopeSession,
		ChatID:    c.ChatID,
		SessionID: c.SessionID,
		Kind:      "continuity",
		Title:     c.UserGoal,
		Body:      body,
		Source:    "runtime_continuity",
		UpdatedAt: c.UpdatedAt,
	}, true
}

func appendReferenceSections(body string, archiveRefs, artifactRefs []string) string {
	var sections []string
	if len(archiveRefs) > 0 {
		sections = append(sections, "Archive refs: "+strings.Join(compactReferenceList(archiveRefs), ", "))
	}
	if len(artifactRefs) > 0 {
		sections = append(sections, "Artifact refs: "+strings.Join(compactReferenceList(artifactRefs), ", "))
	}
	if len(sections) == 0 {
		return body
	}
	if body != "" {
		sections = append([]string{body}, sections...)
		return strings.Join(sections, "\n")
	}
	return strings.Join(sections, "\n")
}

func compactReferenceList(refs []string) []string {
	out := make([]string, 0, len(refs))
	for _, ref := range refs {
		ref = strings.TrimSpace(ref)
		if ref == "" {
			continue
		}
		out = append(out, ref)
	}
	return out
}

func CompactResolvedFacts(text string) []string {
	return CompactResolvedFactsWithPolicy(DefaultMemoryPolicy(), text)
}

func CompactResolvedFactsWithPolicy(policy MemoryPolicy, text string) []string {
	policy = NormalizeMemoryPolicy(policy)
	trimmed := strings.TrimSpace(text)
	if trimmed == "" {
		return nil
	}
	lines := strings.Split(trimmed, "\n")
	out := make([]string, 0, 3)
	for _, line := range lines {
		line = strings.TrimSpace(strings.TrimLeft(line, "-*•0123456789. "))
		if line == "" {
			continue
		}
		out = append(out, line)
		if len(out) == policy.MaxResolvedFacts {
			break
		}
	}
	if len(out) == 0 {
		return []string{trimmed}
	}
	return out
}

func trimMemoryBody(policy MemoryPolicy, body string) string {
	body = strings.TrimSpace(body)
	if len(body) <= policy.MaxDocumentBodyChars {
		return body
	}
	if policy.MaxDocumentBodyChars <= 3 {
		return body[:policy.MaxDocumentBodyChars]
	}
	return strings.TrimSpace(body[:policy.MaxDocumentBodyChars-3]) + "..."
}

func isNoisyMemoryText(text string) bool {
	normalized := strings.ToLower(strings.TrimSpace(text))
	if normalized == "" {
		return true
	}
	noisyHints := []string{
		"results count:",
		"answers count:",
		"infoboxes count:",
		"unresponsive_engines",
		"publisheddate",
		"positions",
		"score:",
		"title:",
		"content:",
		"url:",
		"parsed_url",
		"quelle est la température",
		"http://",
		"https://",
	}
	for _, hint := range noisyHints {
		if strings.Contains(normalized, hint) {
			return true
		}
	}
	if strings.Count(normalized, "---") >= 1 {
		return true
	}
	return false
}
