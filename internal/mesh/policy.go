package mesh

import (
	"fmt"
	"sort"
	"strconv"
	"strings"
	"time"
)

type OrchestrationPolicy struct {
	Profile                string
	ClarificationMode      string
	MaxClarificationRounds int
	ProposalMode           string
	SampleK                int
	MinQuorumSize          int
	ProposalTimeout        time.Duration
	ExecutionMode          string
	AllowToolExecution     bool
	CompositePlanning      string
	JudgeMode              string
}

type PolicyChange struct {
	Field    string
	OldValue string
	NewValue string
}

func DefaultPolicy() OrchestrationPolicy {
	policy, _ := PolicyForProfile("direct")
	return policy
}

func PolicyForProfile(profile string) (OrchestrationPolicy, error) {
	switch strings.TrimSpace(strings.ToLower(profile)) {
	case "direct":
		return OrchestrationPolicy{
			Profile:                "direct",
			ClarificationMode:      "off",
			MaxClarificationRounds: 1,
			ProposalMode:           "off",
			SampleK:                1,
			MinQuorumSize:          1,
			ProposalTimeout:        10 * time.Second,
			ExecutionMode:          "owner",
			AllowToolExecution:     true,
			CompositePlanning:      "off",
			JudgeMode:              "owner",
		}, nil
	case "", "balanced":
		return OrchestrationPolicy{
			Profile:                "balanced",
			ClarificationMode:      "single",
			MaxClarificationRounds: 2,
			ProposalMode:           "sampled",
			SampleK:                2,
			MinQuorumSize:          1,
			ProposalTimeout:        20 * time.Second,
			ExecutionMode:          "winner",
			AllowToolExecution:     true,
			CompositePlanning:      "auto",
			JudgeMode:              "hybrid",
		}, nil
	case "fast":
		return OrchestrationPolicy{
			Profile:                "fast",
			ClarificationMode:      "off",
			MaxClarificationRounds: 1,
			ProposalMode:           "off",
			SampleK:                1,
			MinQuorumSize:          1,
			ProposalTimeout:        10 * time.Second,
			ExecutionMode:          "owner",
			AllowToolExecution:     true,
			CompositePlanning:      "off",
			JudgeMode:              "owner",
		}, nil
	case "deep":
		return OrchestrationPolicy{
			Profile:                "deep",
			ClarificationMode:      "sampled",
			MaxClarificationRounds: 2,
			ProposalMode:           "all",
			SampleK:                3,
			MinQuorumSize:          2,
			ProposalTimeout:        30 * time.Second,
			ExecutionMode:          "winner",
			AllowToolExecution:     true,
			CompositePlanning:      "auto",
			JudgeMode:              "hybrid",
		}, nil
	case "composite":
		return OrchestrationPolicy{
			Profile:                "composite",
			ClarificationMode:      "sampled",
			MaxClarificationRounds: 2,
			ProposalMode:           "sampled",
			SampleK:                3,
			MinQuorumSize:          2,
			ProposalTimeout:        30 * time.Second,
			ExecutionMode:          "winner",
			AllowToolExecution:     true,
			CompositePlanning:      "force",
			JudgeMode:              "hybrid",
		}, nil
	default:
		return OrchestrationPolicy{}, fmt.Errorf("unknown mesh profile %q", profile)
	}
}

func (p OrchestrationPolicy) ApplyOverride(field, value string) (OrchestrationPolicy, PolicyChange, error) {
	field = strings.TrimSpace(strings.ToLower(field))
	value = strings.TrimSpace(strings.ToLower(value))
	if field == "" || value == "" {
		return p, PolicyChange{}, fmt.Errorf("field and value are required")
	}

	change := PolicyChange{Field: field}
	switch field {
	case "clarification_mode":
		if !oneOf(value, "off", "single", "sampled", "all") {
			return p, PolicyChange{}, fmt.Errorf("invalid clarification_mode %q", value)
		}
		change.OldValue = p.ClarificationMode
		p.ClarificationMode = value
		change.NewValue = p.ClarificationMode
	case "proposal_mode":
		if !oneOf(value, "off", "sampled", "all") {
			return p, PolicyChange{}, fmt.Errorf("invalid proposal_mode %q", value)
		}
		change.OldValue = p.ProposalMode
		p.ProposalMode = value
		change.NewValue = p.ProposalMode
	case "sample_k":
		n, err := strconv.Atoi(value)
		if err != nil || n < 1 {
			return p, PolicyChange{}, fmt.Errorf("invalid sample_k %q", value)
		}
		change.OldValue = strconv.Itoa(p.SampleK)
		p.SampleK = n
		if p.MinQuorumSize > p.SampleK {
			p.MinQuorumSize = p.SampleK
		}
		change.NewValue = strconv.Itoa(p.SampleK)
	case "execution_mode":
		if !oneOf(value, "owner", "winner") {
			return p, PolicyChange{}, fmt.Errorf("invalid execution_mode %q", value)
		}
		change.OldValue = p.ExecutionMode
		p.ExecutionMode = value
		change.NewValue = p.ExecutionMode
	case "composite_planning":
		if !oneOf(value, "off", "auto", "force") {
			return p, PolicyChange{}, fmt.Errorf("invalid composite_planning %q", value)
		}
		change.OldValue = p.CompositePlanning
		p.CompositePlanning = value
		change.NewValue = p.CompositePlanning
	default:
		return p, PolicyChange{}, fmt.Errorf("unsupported mesh field %q", field)
	}

	if p.Profile != "custom" {
		p.Profile = "custom"
	}
	return p, change, nil
}

func FormatPolicy(policy OrchestrationPolicy) string {
	lines := []string{
		"Mesh policy",
		"profile: " + policy.Profile,
		"clarification_mode: " + policy.ClarificationMode,
		"max_clarification_rounds: " + strconv.Itoa(policy.MaxClarificationRounds),
		"proposal_mode: " + policy.ProposalMode,
		"sample_k: " + strconv.Itoa(policy.SampleK),
		"min_quorum_size: " + strconv.Itoa(policy.MinQuorumSize),
		"proposal_timeout: " + policy.ProposalTimeout.String(),
		"execution_mode: " + policy.ExecutionMode,
		"allow_tool_execution: " + strconv.FormatBool(policy.AllowToolExecution),
		"composite_planning: " + policy.CompositePlanning,
		"judge_mode: " + policy.JudgeMode,
	}
	return strings.Join(lines, "\n")
}

func FormatPolicyHelp() string {
	commands := []string{
		"/mesh",
		"/mesh help",
		"/mesh mode <direct|fast|balanced|deep|composite>",
		"/mesh set clarification_mode=<off|single|sampled|all>",
		"/mesh set proposal_mode=<off|sampled|all>",
		"/mesh set sample_k=<n>",
		"/mesh set execution_mode=<owner|winner>",
		"/mesh set composite_planning=<off|auto|force>",
	}
	sort.Strings(commands)
	return "Mesh commands\n" + strings.Join(commands, "\n")
}

func (p OrchestrationPolicy) MeshEnabled() bool {
	return strings.ToLower(strings.TrimSpace(p.Profile)) != "direct"
}

func oneOf(value string, options ...string) bool {
	for _, option := range options {
		if value == option {
			return true
		}
	}
	return false
}
