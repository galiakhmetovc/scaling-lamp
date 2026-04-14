package telegram

import (
	"fmt"
	"strings"

	"teamd/internal/skills"
)

func (a *Adapter) handleSkillsCommand(chatID int64, text string) (bool, string, error) {
	fields := strings.Fields(text)
	if len(fields) == 0 || fields[0] != "/skills" {
		return false, "", nil
	}
	if len(fields) == 1 {
		active := a.skillState.Active(a.meshSessionID(chatID))
		if len(active) == 0 {
			return true, "active skills: none\nworkspace: " + valueOrUnknown(a.workspaceRoot), nil
		}
		return true, "active skills: " + strings.Join(active, ", ") + "\nworkspace: " + valueOrUnknown(a.workspaceRoot), nil
	}
	switch fields[1] {
	case "list":
		if len(fields) != 2 {
			return true, "usage: /skills list", nil
		}
		bundles, err := a.skillBundles()
		if err != nil {
			return true, "", err
		}
		if len(bundles) == 0 {
			return true, "no skills discovered", nil
		}
		lines := make([]string, 0, len(bundles))
		for _, bundle := range skills.Summaries(bundles) {
			desc := bundle.Description
			if strings.TrimSpace(desc) == "" {
				desc = "No description"
			}
			lines = append(lines, fmt.Sprintf("%s — %s", bundle.Name, desc))
		}
		return true, strings.Join(lines, "\n"), nil
	case "show":
		if len(fields) != 3 {
			return true, "usage: /skills show <name>", nil
		}
		bundle, ok, err := a.skills.Get(fields[2])
		if err != nil {
			return true, "", err
		}
		if !ok {
			return true, "unknown skill: " + fields[2], nil
		}
		prompt := bundle.Prompt
		if len(prompt) > 600 {
			prompt = prompt[:600] + "\n[truncated]"
		}
		lines := []string{
			"name: " + bundle.Name,
			"description: " + valueOrUnknown(bundle.Description),
			"version: " + valueOrUnknown(bundle.Version),
			"license: " + valueOrUnknown(bundle.License),
			"path: " + valueOrUnknown(bundle.Path),
		}
		if len(bundle.AllowedTools) > 0 {
			lines = append(lines, "allowed_tools: "+strings.Join(bundle.AllowedTools, ", "))
		}
		if len(bundle.Scripts) > 0 {
			lines = append(lines, "scripts: "+strings.Join(bundle.Scripts, ", "))
		}
		if len(bundle.References) > 0 {
			lines = append(lines, "references: "+strings.Join(bundle.References, ", "))
		}
		if len(bundle.Assets) > 0 {
			lines = append(lines, "assets: "+strings.Join(bundle.Assets, ", "))
		}
		lines = append(lines, "", prompt)
		return true, strings.Join(lines, "\n"), nil
	case "use":
		if len(fields) != 3 {
			return true, "usage: /skills use <name>", nil
		}
		if a.skills == nil {
			return true, "no skills catalog configured", nil
		}
		bundle, ok, err := a.skills.Get(fields[2])
		if err != nil {
			return true, "", err
		}
		if !ok {
			return true, "unknown skill: " + fields[2], nil
		}
		a.skillState.Activate(a.meshSessionID(chatID), bundle.Name)
		return true, "skill activated: " + bundle.Name, nil
	case "drop":
		if len(fields) != 3 {
			return true, "usage: /skills drop <name>", nil
		}
		a.skillState.Deactivate(a.meshSessionID(chatID), fields[2])
		return true, "skill deactivated: " + fields[2], nil
	case "reset":
		if len(fields) != 2 {
			return true, "usage: /skills reset", nil
		}
		a.skillState.Reset(a.meshSessionID(chatID))
		return true, "skills reset for this session", nil
	default:
		return true, "usage: /skills [list|show|use|drop|reset]", nil
	}
}
