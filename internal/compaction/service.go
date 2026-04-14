package compaction

import (
	"context"
	"encoding/json"
	"strings"
	"time"

	"teamd/internal/provider"
	"teamd/internal/worker"
)

type Input struct {
	SessionID    string
	Transcript   []string
	ArchiveRefs  []string
	ArtifactRefs []string
}

type Output = worker.Checkpoint

type Deps struct {
	Provider        provider.Provider
	RequestConfig   provider.RequestConfig
	ProviderTimeout time.Duration
	Enabled         bool
}

type Service struct {
	deps Deps
}

func TestDeps() Deps {
	return Deps{}
}

func New(deps Deps) *Service {
	return &Service{deps: deps}
}

func (s *Service) Compact(ctx context.Context, input Input) (Output, error) {
	fallback := heuristicCompact(input)
	if !s.deps.Enabled || s.deps.Provider == nil {
		return fallback, nil
	}
	if out, ok := s.compactWithLLM(ctx, input); ok {
		return out, nil
	}
	return fallback, nil
}

func heuristicCompact(input Input) Output {
	refs := append([]string(nil), input.ArtifactRefs...)
	whatHappened := summarizeTranscript(input.Transcript)
	whatMattersNow := inferWhatMattersNow(input.Transcript)
	unresolved := inferUnresolvedItems(input.Transcript)
	nextActions := inferNextActions(input.Transcript)

	return Output{
		CompactionMethod: "heuristic-v1",
		SessionID:        input.SessionID,
		WhatHappened:     whatHappened,
		WhatMattersNow:   whatMattersNow,
		UnresolvedItems:  unresolved,
		NextActions:      nextActions,
		ArchiveRefs:      append([]string(nil), input.ArchiveRefs...),
		SourceArtifacts:  refs,
	}
}

type llmCheckpoint struct {
	WhatHappened    string   `json:"what_happened"`
	WhatMattersNow  string   `json:"what_matters_now"`
	UnresolvedItems []string `json:"unresolved_items"`
	NextActions     []string `json:"next_actions"`
}

func (s *Service) compactWithLLM(ctx context.Context, input Input) (Output, bool) {
	promptBody, err := json.Marshal(map[string]any{
		"session_id":       input.SessionID,
		"archive_refs":     input.ArchiveRefs,
		"artifact_refs":    input.ArtifactRefs,
		"transcript":       input.Transcript,
		"heuristic_seed":   heuristicCompact(input),
		"response_format":  "json",
		"required_fields":  []string{"what_happened", "what_matters_now", "unresolved_items", "next_actions"},
		"forbidden_output": []string{"markdown", "code fences", "extra prose"},
	})
	if err != nil {
		return Output{}, false
	}

	reqCtx := ctx
	cancel := func() {}
	if s.deps.ProviderTimeout > 0 {
		reqCtx, cancel = context.WithTimeout(ctx, s.deps.ProviderTimeout)
	}
	defer cancel()

	resp, err := s.deps.Provider.Generate(reqCtx, provider.PromptRequest{
		WorkerID: "compaction:" + input.SessionID,
		Messages: []provider.Message{
			{
				Role: "system",
				Content: strings.Join([]string{
					"You compress chat history into a strict JSON checkpoint.",
					"Return exactly one JSON object with fields: what_happened, what_matters_now, unresolved_items, next_actions.",
					"Preserve the user's originating intent and actionable context.",
					"Do not invent facts that are not grounded in the transcript.",
					"Keep each field concise and useful for continuing the session.",
				}, "\n"),
			},
			{
				Role:    "user",
				Content: string(promptBody),
			},
		},
		Config: s.deps.RequestConfig,
	})
	if err != nil {
		return Output{}, false
	}

	parsed, ok := parseLLMCheckpoint(resp.Text)
	if !ok {
		return Output{}, false
	}

	return Output{
		SessionID:        input.SessionID,
		CompactionMethod: "llm-v1",
		WhatHappened:     parsed.WhatHappened,
		WhatMattersNow:   parsed.WhatMattersNow,
		UnresolvedItems:  parsed.UnresolvedItems,
		NextActions:      parsed.NextActions,
		ArchiveRefs:      append([]string(nil), input.ArchiveRefs...),
		SourceArtifacts:  append([]string(nil), input.ArtifactRefs...),
	}, true
}

func parseLLMCheckpoint(raw string) (llmCheckpoint, bool) {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return llmCheckpoint{}, false
	}
	if start := strings.Index(raw, "{"); start >= 0 {
		if end := strings.LastIndex(raw, "}"); end > start {
			raw = raw[start : end+1]
		}
	}

	var parsed llmCheckpoint
	if err := json.Unmarshal([]byte(raw), &parsed); err != nil {
		return llmCheckpoint{}, false
	}
	parsed.WhatHappened = strings.TrimSpace(parsed.WhatHappened)
	parsed.WhatMattersNow = strings.TrimSpace(parsed.WhatMattersNow)
	parsed.UnresolvedItems = compactStringSlice(parsed.UnresolvedItems)
	parsed.NextActions = compactStringSlice(parsed.NextActions)
	if parsed.WhatHappened == "" || parsed.WhatMattersNow == "" {
		return llmCheckpoint{}, false
	}
	return parsed, true
}

func compactStringSlice(items []string) []string {
	out := make([]string, 0, len(items))
	for _, item := range items {
		item = strings.TrimSpace(item)
		if item == "" {
			continue
		}
		out = append(out, item)
	}
	return out
}

func summarizeTranscript(lines []string) string {
	if len(lines) == 0 {
		return "no transcript available"
	}
	if len(lines) == 1 {
		return lines[0]
	}
	if len(lines) == 2 {
		return strings.Join(lines, " | ")
	}
	return strings.Join(lines[len(lines)-3:], " | ")
}

func inferWhatMattersNow(lines []string) string {
	for i := len(lines) - 1; i >= 0; i-- {
		line := strings.TrimSpace(lines[i])
		if line != "" {
			return line
		}
	}
	return "no immediate focus recorded"
}

func inferUnresolvedItems(lines []string) []string {
	var out []string
	for _, line := range lines {
		lower := strings.ToLower(line)
		if containsWholeWord(lower, "todo") ||
			containsWholeWord(lower, "need") ||
			containsWholeWord(lower, "pending") ||
			containsWholeWord(lower, "unresolved") {
			out = append(out, line)
		}
	}
	return out
}

func inferNextActions(lines []string) []string {
	var out []string
	for _, line := range lines {
		lower := strings.ToLower(line)
		if containsWholeWord(lower, "next") ||
			strings.Contains(lower, "rollback") ||
			strings.Contains(lower, "deploy") ||
			strings.Contains(lower, "investigate") {
			out = append(out, line)
		}
	}
	return out
}

func containsWholeWord(line, word string) bool {
	fields := strings.FieldsFunc(line, func(r rune) bool {
		return !(r >= 'a' && r <= 'z') && !(r >= '0' && r <= '9')
	})
	for _, field := range fields {
		if field == word {
			return true
		}
	}
	return false
}
