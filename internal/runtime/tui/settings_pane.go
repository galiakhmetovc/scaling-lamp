package tui

import (
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"slices"
	"strconv"
	"strings"

	"github.com/charmbracelet/lipgloss"
	"gopkg.in/yaml.v3"

	tea "github.com/charmbracelet/bubbletea"
)

func (m *model) updateSettings(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	switch msg.String() {
	case "1":
		m.settingsMode = settingsSession
	case "2":
		m.settingsMode = settingsForm
	case "3":
		m.settingsMode = settingsRaw
	}
	switch m.settingsMode {
	case settingsSession:
		return m.updateSessionOverrides(msg)
	case settingsForm:
		return m.updateConfigForm(msg)
	case settingsRaw:
		return m.updateRawEditor(msg)
	}
	return m, nil
}

func (m *model) handleMouseSettings(msg tea.MouseMsg) bool {
	switch msg.Button {
	case tea.MouseButtonWheelUp:
		if m.settingsMode == settingsRaw {
			for i := 0; i < 3; i++ {
				m.rawEditor.CursorUp()
			}
		} else {
			m.settingsView.LineUp(3)
		}
		return true
	case tea.MouseButtonWheelDown:
		if m.settingsMode == settingsRaw {
			for i := 0; i < 3; i++ {
				m.rawEditor.CursorDown()
			}
		} else {
			m.settingsView.LineDown(3)
		}
		return true
	}
	return false
}

func (m *model) updateSessionOverrides(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	state := m.currentSessionState()
	if state == nil {
		return m, nil
	}
	switch msg.String() {
	case "pgup":
		m.settingsView.LineUp(max(1, m.settingsView.Height/2))
		return m, nil
	case "pgdown":
		m.settingsView.LineDown(max(1, m.settingsView.Height/2))
		return m, nil
	case "up", "k":
		if m.sessionField > 0 {
			m.sessionField--
		}
	case "down", "j":
		if m.sessionField < 5 {
			m.sessionField++
		}
	case "left", "h":
		if m.sessionField == 0 && state.Overrides.MaxToolRounds > 1 {
			state.Overrides.MaxToolRounds--
		}
	case "right", "l":
		if m.sessionField == 0 {
			state.Overrides.MaxToolRounds++
		}
	case " ":
		switch m.sessionField {
		case 1:
			state.Overrides.RenderMarkdown = !state.Overrides.RenderMarkdown
		case 3:
			state.Overrides.ShowToolCalls = !state.Overrides.ShowToolCalls
		case 4:
			state.Overrides.ShowToolResults = !state.Overrides.ShowToolResults
		case 5:
			state.Overrides.ShowPlanAfterPlanTools = !state.Overrides.ShowPlanAfterPlanTools
		}
	case "enter":
		if m.sessionField == 2 {
			if state.Overrides.MarkdownStyle == "dark" {
				state.Overrides.MarkdownStyle = "light"
			} else {
				state.Overrides.MarkdownStyle = "dark"
			}
		}
	}
	return m, nil
}

func (m *model) updateConfigForm(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	switch msg.String() {
	case "pgup":
		m.settingsView.LineUp(max(1, m.settingsView.Height/2))
		return m, nil
	case "pgdown":
		m.settingsView.LineDown(max(1, m.settingsView.Height/2))
		return m, nil
	case "up", "k":
		if m.formField > 0 {
			m.formField--
		}
	case "down", "j":
		if m.formField < 5 {
			m.formField++
		}
	case "left", "h":
		if m.formField == 0 {
			if value, err := strconv.Atoi(strings.TrimSpace(m.formDraft.MaxToolRounds)); err == nil && value > 1 {
				m.formDraft.MaxToolRounds = strconv.Itoa(value - 1)
				m.formMaxRounds.SetValue(m.formDraft.MaxToolRounds)
			}
		}
	case "right", "l":
		if m.formField == 0 {
			value, _ := strconv.Atoi(strings.TrimSpace(m.formDraft.MaxToolRounds))
			if value <= 0 {
				value = 100
			}
			m.formDraft.MaxToolRounds = strconv.Itoa(value + 1)
			m.formMaxRounds.SetValue(m.formDraft.MaxToolRounds)
		}
	case " ":
		switch m.formField {
		case 1:
			m.formDraft.RenderMarkdown = !m.formDraft.RenderMarkdown
		case 3:
			m.formDraft.ShowToolCalls = !m.formDraft.ShowToolCalls
		case 4:
			m.formDraft.ShowToolResults = !m.formDraft.ShowToolResults
		case 5:
			m.formDraft.ShowPlanAfterPlanTools = !m.formDraft.ShowPlanAfterPlanTools
		}
	case "enter":
		if m.formField == 2 {
			if m.formDraft.MarkdownStyle == "dark" {
				m.formDraft.MarkdownStyle = "light"
			} else {
				m.formDraft.MarkdownStyle = "dark"
			}
			m.formStyle.SetValue(m.formDraft.MarkdownStyle)
		}
	case "ctrl+s":
		if err := m.saveFormDraft(); err != nil {
			m.errMessage = err.Error()
		} else {
			m.statusMessage = "config draft saved"
		}
	case "ctrl+a":
		if err := m.saveFormDraft(); err != nil {
			m.errMessage = err.Error()
			return m, nil
		}
		return m, rebuildAgentCmd(m.agent.ConfigPath)
	case "r":
		m.resetFormDraft()
		m.statusMessage = "config form reset"
	}
	return m, nil
}

func (m *model) updateRawEditor(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	switch msg.String() {
	case "up", "k":
		if m.rawCursor > 0 {
			m.rawCursor--
			_ = m.loadRawEditorFile()
		}
		return m, nil
	case "down", "j":
		if m.rawCursor < len(m.rawFiles)-1 {
			m.rawCursor++
			_ = m.loadRawEditorFile()
		}
		return m, nil
	case "ctrl+s":
		if err := m.saveRawEditor(); err != nil {
			m.errMessage = err.Error()
		} else {
			m.statusMessage = "raw config saved"
		}
		return m, nil
	case "ctrl+a":
		if err := m.saveRawEditor(); err != nil {
			m.errMessage = err.Error()
			return m, nil
		}
		return m, rebuildAgentCmd(m.agent.ConfigPath)
	}
	var cmd tea.Cmd
	m.rawEditor, cmd = m.rawEditor.Update(msg)
	return m, cmd
}

func (m *model) viewSettings() string {
	modeTitle := []string{"Session Overrides", "Config Form", "Raw YAML"}[m.settingsMode]
	head := "Settings\n\n1=session overrides  2=config form  3=raw yaml\nmode: " + modeTitle
	switch m.settingsMode {
	case settingsSession:
		state := m.currentSessionState()
		if state == nil {
			return head + "\n\nNo active session"
		}
		rows := []string{
			fmt.Sprintf("%s max_tool_rounds: %d", cursor(m.sessionField, 0), state.Overrides.MaxToolRounds),
			fmt.Sprintf("%s render_markdown: %t", cursor(m.sessionField, 1), state.Overrides.RenderMarkdown),
			fmt.Sprintf("%s markdown_style: %s", cursor(m.sessionField, 2), state.Overrides.MarkdownStyle),
			fmt.Sprintf("%s show_tool_calls: %t", cursor(m.sessionField, 3), state.Overrides.ShowToolCalls),
			fmt.Sprintf("%s show_tool_results: %t", cursor(m.sessionField, 4), state.Overrides.ShowToolResults),
			fmt.Sprintf("%s show_plan_after_plan_tools: %t", cursor(m.sessionField, 5), state.Overrides.ShowPlanAfterPlanTools),
		}
		m.settingsView.SetContent(head + "\n\n" + strings.Join(rows, "\n"))
		return m.settingsView.View()
	case settingsForm:
		status := "clean"
		if m.formDraftDirty() {
			status = "modified"
		}
		rows := []string{
			"Draft: " + status,
			"",
			fmt.Sprintf("%s max_tool_rounds: %s", cursor(m.formField, 0), m.formDraft.MaxToolRounds),
			fmt.Sprintf("%s render_markdown: %t", cursor(m.formField, 1), m.formDraft.RenderMarkdown),
			fmt.Sprintf("%s markdown_style: %s", cursor(m.formField, 2), m.formDraft.MarkdownStyle),
			fmt.Sprintf("%s show_tool_calls: %t", cursor(m.formField, 3), m.formDraft.ShowToolCalls),
			fmt.Sprintf("%s show_tool_results: %t", cursor(m.formField, 4), m.formDraft.ShowToolResults),
			fmt.Sprintf("%s show_plan_after_plan_tools: %t", cursor(m.formField, 5), m.formDraft.ShowPlanAfterPlanTools),
			"",
			"Ctrl+S save to disk",
			"Ctrl+A save and reload agent",
			"r reset form to loaded config",
		}
		m.settingsView.SetContent(head + "\n\n" + strings.Join(rows, "\n"))
		return m.settingsView.View()
	default:
		var fileLines []string
		for i, path := range m.rawFiles {
			prefix := cursor(m.rawCursor, i)
			fileLines = append(fileLines, prefix+filepath.Base(path))
		}
		left := "Files\n" + strings.Join(fileLines, "\n")
		right := "Editor\n" + m.rawEditor.View() + "\nCtrl+S save  Ctrl+A save+reload"
		return head + "\n\n" + lipgloss.JoinHorizontal(lipgloss.Top, lipgloss.NewStyle().Width(max(24, m.width/4)).Render(left), lipgloss.NewStyle().Width(max(30, m.width-(m.width/4)-4)).Render(right))
	}
}

func (m *model) loadRawFileList() error {
	root := filepath.Dir(m.agent.ConfigPath)
	files := []string{m.agent.ConfigPath}
	if err := filepath.WalkDir(root, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() {
			return nil
		}
		if path == m.agent.ConfigPath {
			return nil
		}
		if strings.HasSuffix(path, ".yaml") || strings.HasSuffix(path, ".yml") {
			files = append(files, path)
		}
		return nil
	}); err != nil {
		return err
	}
	slices.Sort(files)
	m.rawFiles = files
	if m.rawCursor >= len(m.rawFiles) {
		m.rawCursor = 0
	}
	return m.loadRawEditorFile()
}

func (m *model) loadRawEditorFile() error {
	if len(m.rawFiles) == 0 {
		m.rawEditor.SetValue("")
		m.rawLoadedPath = ""
		return nil
	}
	path := m.rawFiles[m.rawCursor]
	body, err := os.ReadFile(path)
	if err != nil {
		return err
	}
	m.rawLoadedPath = path
	m.rawEditor.SetValue(string(body))
	return nil
}

func (m *model) saveRawEditor() error {
	if m.rawLoadedPath == "" {
		return nil
	}
	return os.WriteFile(m.rawLoadedPath, []byte(m.rawEditor.Value()), 0o644)
}

func (m *model) resetFormDraft() {
	m.formDraft = configFormDraft{
		MaxToolRounds:          strconv.Itoa(m.agent.Config.Spec.Runtime.MaxToolRounds),
		RenderMarkdown:         m.agent.Contracts.Chat.Output.Params.RenderMarkdown,
		MarkdownStyle:          coalesce(m.agent.Contracts.Chat.Output.Params.MarkdownStyle, "dark"),
		ShowToolCalls:          m.agent.Contracts.Chat.Status.Params.ShowToolCalls,
		ShowToolResults:        m.agent.Contracts.Chat.Status.Params.ShowToolResults,
		ShowPlanAfterPlanTools: m.agent.Contracts.Chat.Status.Params.ShowPlanAfterPlanTools,
	}
	m.formMaxRounds.SetValue(m.formDraft.MaxToolRounds)
	m.formStyle.SetValue(m.formDraft.MarkdownStyle)
}

func (m *model) formDraftDirty() bool {
	expected := configFormDraft{
		MaxToolRounds:          strconv.Itoa(m.agent.Config.Spec.Runtime.MaxToolRounds),
		RenderMarkdown:         m.agent.Contracts.Chat.Output.Params.RenderMarkdown,
		MarkdownStyle:          coalesce(m.agent.Contracts.Chat.Output.Params.MarkdownStyle, "dark"),
		ShowToolCalls:          m.agent.Contracts.Chat.Status.Params.ShowToolCalls,
		ShowToolResults:        m.agent.Contracts.Chat.Status.Params.ShowToolResults,
		ShowPlanAfterPlanTools: m.agent.Contracts.Chat.Status.Params.ShowPlanAfterPlanTools,
	}
	return m.formDraft != expected
}

func (m *model) saveFormDraft() error {
	if err := updateAgentRuntimeMaxToolRounds(m.agent.ConfigPath, m.formDraft.MaxToolRounds); err != nil {
		return err
	}
	root := filepath.Dir(m.agent.ConfigPath)
	if err := updateChatOutputPolicy(filepath.Join(root, "policies", "chat", "output.yaml"), m.formDraft.RenderMarkdown, m.formDraft.MarkdownStyle); err != nil {
		return err
	}
	if err := updateChatStatusPolicy(filepath.Join(root, "policies", "chat", "status.yaml"), m.formDraft.ShowToolCalls, m.formDraft.ShowToolResults, m.formDraft.ShowPlanAfterPlanTools); err != nil {
		return err
	}
	return nil
}

func updateAgentRuntimeMaxToolRounds(path, value string) error {
	var cfg struct {
		Kind    string `yaml:"kind"`
		Version string `yaml:"version"`
		ID      string `yaml:"id"`
		Spec    struct {
			Runtime   map[string]any    `yaml:"runtime"`
			Contracts map[string]string `yaml:"contracts"`
		} `yaml:"spec"`
	}
	body, err := os.ReadFile(path)
	if err != nil {
		return err
	}
	if err := yaml.Unmarshal(body, &cfg); err != nil {
		return err
	}
	if cfg.Spec.Runtime == nil {
		cfg.Spec.Runtime = map[string]any{}
	}
	parsed, err := strconv.Atoi(strings.TrimSpace(value))
	if err != nil {
		return err
	}
	cfg.Spec.Runtime["max_tool_rounds"] = parsed
	out, err := yaml.Marshal(&cfg)
	if err != nil {
		return err
	}
	return os.WriteFile(path, out, 0o644)
}

func updateChatOutputPolicy(path string, renderMarkdown bool, style string) error {
	var doc map[string]any
	body, err := os.ReadFile(path)
	if err != nil {
		return err
	}
	if err := yaml.Unmarshal(body, &doc); err != nil {
		return err
	}
	spec := ensureMap(doc, "spec")
	params := ensureMap(spec, "params")
	params["render_markdown"] = renderMarkdown
	params["markdown_style"] = style
	out, err := yaml.Marshal(doc)
	if err != nil {
		return err
	}
	return os.WriteFile(path, out, 0o644)
}

func updateChatStatusPolicy(path string, showToolCalls, showToolResults, showPlanAfter bool) error {
	var doc map[string]any
	body, err := os.ReadFile(path)
	if err != nil {
		return err
	}
	if err := yaml.Unmarshal(body, &doc); err != nil {
		return err
	}
	spec := ensureMap(doc, "spec")
	params := ensureMap(spec, "params")
	params["show_tool_calls"] = showToolCalls
	params["show_tool_results"] = showToolResults
	params["show_plan_after_plan_tools"] = showPlanAfter
	out, err := yaml.Marshal(doc)
	if err != nil {
		return err
	}
	return os.WriteFile(path, out, 0o644)
}

func ensureMap(parent map[string]any, key string) map[string]any {
	if existing, ok := parent[key].(map[string]any); ok {
		return existing
	}
	if existing, ok := parent[key].(map[any]any); ok {
		out := map[string]any{}
		for k, v := range existing {
			if text, ok := k.(string); ok {
				out[text] = v
			}
		}
		parent[key] = out
		return out
	}
	out := map[string]any{}
	parent[key] = out
	return out
}
