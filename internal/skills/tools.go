package skills

import (
	"fmt"
	"path/filepath"
	"strings"
)

type Summary struct {
	Name        string `json:"name"`
	Description string `json:"description"`
}

type Detail struct {
	Name        string `json:"name"`
	Description string `json:"description"`
	Prompt      string `json:"prompt"`
	Version     string `json:"version"`
	License     string   `json:"license,omitempty"`
	AllowedTools []string `json:"allowed_tools,omitempty"`
	Scripts      []string `json:"scripts,omitempty"`
	References   []string `json:"references,omitempty"`
	Assets       []string `json:"assets,omitempty"`
}

func ToolList(bundles []Bundle) []Summary {
	return Summaries(bundles)
}

func ToolListCompact(bundles []Bundle, limit int) string {
	summaries := Summaries(bundles)
	if len(summaries) == 0 {
		return "skills_count: 0"
	}
	lines := []string{fmt.Sprintf("skills_count: %d", len(summaries))}
	for _, item := range summaries {
		desc := strings.TrimSpace(item.Description)
		if desc == "" {
			desc = "No description"
		}
		lines = append(lines, fmt.Sprintf("- %s — %s", item.Name, desc))
	}
	lines = append(lines, "Use skills.read for details on one skill or activate_skill to load a chosen skill.")
	return strings.Join(lines, "\n")
}

func ToolRead(bundle Bundle) Detail {
	return Detail{
		Name:         bundle.Name,
		Description:  bundle.Description,
		Prompt:       bundle.Prompt,
		Version:      bundle.Version,
		License:      bundle.License,
		AllowedTools: append([]string(nil), bundle.AllowedTools...),
		Scripts:      append([]string(nil), bundle.Scripts...),
		References:   append([]string(nil), bundle.References...),
		Assets:       append([]string(nil), bundle.Assets...),
	}
}

func ToolActivate(bundle Bundle) string {
	var b strings.Builder
	_, _ = fmt.Fprintf(&b, `<skill_content name="%s">`, bundle.Name)
	b.WriteString("\n")
	if strings.TrimSpace(bundle.Prompt) != "" {
		b.WriteString(strings.TrimSpace(bundle.Prompt))
		b.WriteString("\n\n")
	}
	_, _ = fmt.Fprintf(&b, "Skill directory: %s\n", filepath.Dir(bundle.Path))
	b.WriteString("Relative paths in this skill are relative to the skill directory.\n")
	resources := append(append([]string{}, bundle.Scripts...), bundle.References...)
	resources = append(resources, bundle.Assets...)
	if len(resources) > 0 {
		b.WriteString("\n<skill_resources>\n")
		for _, resource := range resources {
			_, _ = fmt.Fprintf(&b, "  <file>%s</file>\n", resource)
		}
		b.WriteString("</skill_resources>\n")
	}
	b.WriteString("</skill_content>")
	return b.String()
}
