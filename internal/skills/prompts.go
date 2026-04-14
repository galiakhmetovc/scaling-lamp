package skills

import (
	"sort"
	"strings"
)

const defaultPromptBudget = 4096

func ComposePrompt(bundles []Bundle) string {
	if len(bundles) == 0 {
		return ""
	}

	sorted := append([]Bundle(nil), bundles...)
	sort.Slice(sorted, func(i, j int) bool {
		return sorted[i].Name < sorted[j].Name
	})

	parts := make([]string, 0, len(sorted))
	remaining := defaultPromptBudget
	for _, bundle := range sorted {
		prompt := strings.TrimSpace(bundle.Prompt)
		if prompt == "" || remaining <= 0 {
			continue
		}
		if len(prompt) > remaining {
			limit := remaining
			if limit > len("[truncated]") {
				limit -= len("[truncated]")
			}
			if limit < 0 {
				limit = 0
			}
			prompt = prompt[:limit] + "[truncated]"
		}
		parts = append(parts, prompt)
		remaining -= len(prompt)
		if remaining > 0 {
			remaining--
		}
	}

	return strings.Join(parts, "\n")
}

func ComposeCatalog(bundles []Bundle) string {
	if len(bundles) == 0 {
		return ""
	}

	summaries := Summaries(bundles)
	lines := []string{"## Available skills"}
	for _, item := range summaries {
		desc := strings.TrimSpace(item.Description)
		if desc == "" {
			desc = "No description"
		}
		lines = append(lines, "- `"+item.Name+"` — "+desc)
	}
	lines = append(lines, "")
	lines = append(lines, "Use `skills.read` to inspect one skill in detail and `activate_skill` to load a chosen skill's full instructions.")
	lines = append(lines, "If user control is preferred, ask the user to activate a skill with `/skills use <name>`.")
	return strings.Join(lines, "\n")
}
