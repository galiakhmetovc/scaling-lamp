package runtime

import (
	"fmt"
	"strings"
	"time"
)

type TransportPolicy struct {
	Enabled  bool
	Strategy string
	BaseURL  string
	Path     string
	Timeout  time.Duration
	Headers  map[string]string
	Auth     TransportAuthPolicy
}

type TransportAuthPolicy struct {
	Header   string
	Prefix   string
	Value    string
	TokenEnv string
}

type RequestShapePolicy struct {
	Enabled        bool
	Strategy       string
	Model          string
	ReasoningMode  string
	ResponseFormat string
}

type PromptLayerPolicy struct {
	Enabled  bool
	Strategy string
	Params   map[string]any
}

type PromptPolicy struct {
	SessionHead    PromptLayerPolicy
	WorkspaceFocus PromptLayerPolicy
	Plan           PromptLayerPolicy
	RecentArtifacts PromptLayerPolicy
	TreeHint       PromptLayerPolicy
	HistorySummary PromptLayerPolicy
}

type OffloadPolicy struct {
	Enabled           bool
	Strategy          string
	SmallKeepChars    int
	OffloadChars      int
	ForceOffloadChars int
	PreviewMode       string
	OffloadLastResult bool
}

type SummarizationPolicy struct {
	Enabled       bool
	Strategy      string
	KeepLastN     int
	RefreshMode   string
	SummaryBudget int
}

type WorkspacePolicy struct {
	Enabled       bool
	Strategy      string
	MaxOpenFiles  int
	MaxArtifacts  int
	TreeDepth     int
	IncludeChecksums bool
}

type ToolPolicy struct {
	Enabled      bool
	Strategy     string
	AllowedTools []string
	AutoApprove  bool
}

type DisplayPolicy struct {
	Enabled      bool
	Strategy     string
	TurnLimit    int
	MessageLimit int
	CharLimit    int
}

type ContextPolicy struct {
	Transport     TransportPolicy
	RequestShape  RequestShapePolicy
	Prompt        PromptPolicy
	Offload       OffloadPolicy
	Summarization SummarizationPolicy
	Workspace     WorkspacePolicy
	Tools         ToolPolicy
	Display       DisplayPolicy
}

type EffectiveContextPolicy = ContextPolicy

func ValidateContextPolicy(policy ContextPolicy) error {
	if policy.Offload.Enabled && policy.Offload.OffloadLastResult && strings.EqualFold(strings.TrimSpace(policy.Offload.PreviewMode), "none") {
		return fmt.Errorf("offload preview_mode cannot be none when offload_last_result is enabled")
	}
	if policy.Summarization.KeepLastN < 0 {
		return fmt.Errorf("summarization keep_last_n cannot be negative")
	}
	return nil
}

func ResolveContextPolicy(global, session, run ContextPolicy) EffectiveContextPolicy {
	return EffectiveContextPolicy{
		Transport:     mergeTransportPolicy(global.Transport, session.Transport, run.Transport),
		RequestShape:  mergeRequestShapePolicy(global.RequestShape, session.RequestShape, run.RequestShape),
		Prompt:        mergePromptPolicy(global.Prompt, session.Prompt, run.Prompt),
		Offload:       mergeOffloadPolicy(global.Offload, session.Offload, run.Offload),
		Summarization: mergeSummarizationPolicy(global.Summarization, session.Summarization, run.Summarization),
		Workspace:     mergeWorkspacePolicy(global.Workspace, session.Workspace, run.Workspace),
		Tools:         mergeToolPolicy(global.Tools, session.Tools, run.Tools),
		Display:       mergeDisplayPolicy(global.Display, session.Display, run.Display),
	}
}

func mergeTransportPolicy(parts ...TransportPolicy) TransportPolicy {
	var out TransportPolicy
	for _, part := range parts {
		if part.Enabled {
			out.Enabled = true
		}
		if strings.TrimSpace(part.Strategy) != "" {
			out.Strategy = part.Strategy
		}
		if strings.TrimSpace(part.BaseURL) != "" {
			out.BaseURL = part.BaseURL
		}
		if strings.TrimSpace(part.Path) != "" {
			out.Path = part.Path
		}
		if part.Timeout > 0 {
			out.Timeout = part.Timeout
		}
		if len(part.Headers) > 0 {
			if out.Headers == nil {
				out.Headers = map[string]string{}
			}
			for k, v := range part.Headers {
				out.Headers[k] = v
			}
		}
		if strings.TrimSpace(part.Auth.Header) != "" || strings.TrimSpace(part.Auth.Prefix) != "" || strings.TrimSpace(part.Auth.Value) != "" || strings.TrimSpace(part.Auth.TokenEnv) != "" {
			out.Auth = part.Auth
		}
	}
	return out
}

func mergeRequestShapePolicy(parts ...RequestShapePolicy) RequestShapePolicy {
	var out RequestShapePolicy
	for _, part := range parts {
		if part.Enabled {
			out.Enabled = true
		}
		if strings.TrimSpace(part.Strategy) != "" {
			out.Strategy = part.Strategy
		}
		if strings.TrimSpace(part.Model) != "" {
			out.Model = part.Model
		}
		if strings.TrimSpace(part.ReasoningMode) != "" {
			out.ReasoningMode = part.ReasoningMode
		}
		if strings.TrimSpace(part.ResponseFormat) != "" {
			out.ResponseFormat = part.ResponseFormat
		}
	}
	return out
}

func mergePromptPolicy(parts ...PromptPolicy) PromptPolicy {
	var out PromptPolicy
	for _, part := range parts {
		out.SessionHead = mergePromptLayer(out.SessionHead, part.SessionHead)
		out.WorkspaceFocus = mergePromptLayer(out.WorkspaceFocus, part.WorkspaceFocus)
		out.Plan = mergePromptLayer(out.Plan, part.Plan)
		out.RecentArtifacts = mergePromptLayer(out.RecentArtifacts, part.RecentArtifacts)
		out.TreeHint = mergePromptLayer(out.TreeHint, part.TreeHint)
		out.HistorySummary = mergePromptLayer(out.HistorySummary, part.HistorySummary)
	}
	return out
}

func mergePromptLayer(base, override PromptLayerPolicy) PromptLayerPolicy {
	out := base
	if override.Enabled {
		out.Enabled = true
	}
	if strings.TrimSpace(override.Strategy) != "" {
		out.Strategy = override.Strategy
	}
	if len(override.Params) > 0 {
		out.Params = map[string]any{}
		for k, v := range override.Params {
			out.Params[k] = v
		}
	}
	return out
}

func mergeOffloadPolicy(parts ...OffloadPolicy) OffloadPolicy {
	var out OffloadPolicy
	for _, part := range parts {
		if part.Enabled {
			out.Enabled = true
		}
		if strings.TrimSpace(part.Strategy) != "" {
			out.Strategy = part.Strategy
		}
		if part.SmallKeepChars > 0 {
			out.SmallKeepChars = part.SmallKeepChars
		}
		if part.OffloadChars > 0 {
			out.OffloadChars = part.OffloadChars
		}
		if part.ForceOffloadChars > 0 {
			out.ForceOffloadChars = part.ForceOffloadChars
		}
		if strings.TrimSpace(part.PreviewMode) != "" {
			out.PreviewMode = part.PreviewMode
		}
		if part.OffloadLastResult {
			out.OffloadLastResult = true
		}
	}
	return out
}

func mergeSummarizationPolicy(parts ...SummarizationPolicy) SummarizationPolicy {
	var out SummarizationPolicy
	for _, part := range parts {
		if part.Enabled {
			out.Enabled = true
		}
		if strings.TrimSpace(part.Strategy) != "" {
			out.Strategy = part.Strategy
		}
		if part.KeepLastN != 0 {
			out.KeepLastN = part.KeepLastN
		}
		if strings.TrimSpace(part.RefreshMode) != "" {
			out.RefreshMode = part.RefreshMode
		}
		if part.SummaryBudget > 0 {
			out.SummaryBudget = part.SummaryBudget
		}
	}
	return out
}

func mergeWorkspacePolicy(parts ...WorkspacePolicy) WorkspacePolicy {
	var out WorkspacePolicy
	for _, part := range parts {
		if part.Enabled {
			out.Enabled = true
		}
		if strings.TrimSpace(part.Strategy) != "" {
			out.Strategy = part.Strategy
		}
		if part.MaxOpenFiles > 0 {
			out.MaxOpenFiles = part.MaxOpenFiles
		}
		if part.MaxArtifacts > 0 {
			out.MaxArtifacts = part.MaxArtifacts
		}
		if part.TreeDepth > 0 {
			out.TreeDepth = part.TreeDepth
		}
		if part.IncludeChecksums {
			out.IncludeChecksums = true
		}
	}
	return out
}

func mergeToolPolicy(parts ...ToolPolicy) ToolPolicy {
	var out ToolPolicy
	for _, part := range parts {
		if part.Enabled {
			out.Enabled = true
		}
		if strings.TrimSpace(part.Strategy) != "" {
			out.Strategy = part.Strategy
		}
		if part.AllowedTools != nil {
			out.AllowedTools = append([]string(nil), part.AllowedTools...)
		}
		if part.AutoApprove {
			out.AutoApprove = true
		}
	}
	return out
}

func mergeDisplayPolicy(parts ...DisplayPolicy) DisplayPolicy {
	var out DisplayPolicy
	for _, part := range parts {
		if part.Enabled {
			out.Enabled = true
		}
		if strings.TrimSpace(part.Strategy) != "" {
			out.Strategy = part.Strategy
		}
		if part.TurnLimit > 0 {
			out.TurnLimit = part.TurnLimit
		}
		if part.MessageLimit > 0 {
			out.MessageLimit = part.MessageLimit
		}
		if part.CharLimit > 0 {
			out.CharLimit = part.CharLimit
		}
	}
	return out
}
