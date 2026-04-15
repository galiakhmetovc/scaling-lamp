package tui

import (
	"fmt"
	"path/filepath"
	"strconv"
	"strings"

	"github.com/charmbracelet/lipgloss"

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
	if isWheelUp(msg) {
		if m.settingsMode == settingsRaw {
			for i := 0; i < 3; i++ {
				m.rawEditor.CursorUp()
			}
		} else {
			m.settingsView.LineUp(3)
		}
		return true
	}
	if isWheelDown(msg) {
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
			m.statusMessage = "config applied"
		}
	case "ctrl+a":
		if err := m.saveFormDraft(); err != nil {
			m.errMessage = err.Error()
		} else {
			m.statusMessage = "config applied"
		}
		return m, nil
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
			m.statusMessage = "raw config applied"
		}
		return m, nil
	case "ctrl+a":
		if err := m.saveRawEditor(); err != nil {
			m.errMessage = err.Error()
		} else {
			m.statusMessage = "raw config applied"
		}
		return m, nil
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
			"Ctrl+S apply",
			"Ctrl+A apply",
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
		right := "Editor\n" + m.rawEditor.View() + "\nCtrl+S apply  Ctrl+A apply"
		leftWidth, rightWidth := splitPaneWidths(m.width, max(24, m.width/4), max(30, m.width-(m.width/4)-4))
		return head + "\n\n" + lipgloss.JoinHorizontal(lipgloss.Top, lipgloss.NewStyle().Width(leftWidth).MaxWidth(leftWidth).Render(left), lipgloss.NewStyle().Width(rightWidth).MaxWidth(rightWidth).Render(right))
	}
}

func (m *model) loadRawFileList() error {
	settings, err := m.client.GetSettings(m.ctx)
	if err != nil {
		return err
	}
	m.settingsSnapshot = settings
	files := make([]string, 0, len(settings.RawFiles))
	for _, file := range settings.RawFiles {
		files = append(files, file.Path)
	}
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
	file, err := m.client.GetSettingsRaw(m.ctx, path)
	if err != nil {
		return err
	}
	m.rawLoadedPath = path
	m.rawEditor.SetValue(file.Content)
	return nil
}

func (m *model) saveRawEditor() error {
	if m.rawLoadedPath == "" {
		return nil
	}
	base := ""
	for _, file := range m.settingsSnapshot.RawFiles {
		if file.Path == m.rawLoadedPath {
			base = file.Revision
			break
		}
	}
	settings, err := m.client.ApplySettingsRaw(m.ctx, m.rawLoadedPath, base, m.rawEditor.Value())
	if err != nil {
		return err
	}
	m.settingsSnapshot = settings
	return m.loadRawFileList()
}

func (m *model) resetFormDraft() {
	values := map[string]any{}
	for _, field := range m.settingsSnapshot.FormFields {
		values[field.Key] = field.Value
	}
	m.formDraft = configFormDraft{
		MaxToolRounds:          stringifySetting(values["max_tool_rounds"]),
		RenderMarkdown:         boolSetting(values["render_markdown"]),
		MarkdownStyle:          coalesce(stringifySetting(values["markdown_style"]), "dark"),
		ShowToolCalls:          boolSetting(values["show_tool_calls"]),
		ShowToolResults:        boolSetting(values["show_tool_results"]),
		ShowPlanAfterPlanTools: boolSetting(values["show_plan_after_plan_tools"]),
	}
	m.formMaxRounds.SetValue(m.formDraft.MaxToolRounds)
	m.formStyle.SetValue(m.formDraft.MarkdownStyle)
}

func (m *model) formDraftDirty() bool {
	current := m.formDraft
	m.resetFormDraft()
	expected := m.formDraft
	m.formDraft = current
	return current != expected
}

func (m *model) saveFormDraft() error {
	values := map[string]any{
		"max_tool_rounds":            m.formDraft.MaxToolRounds,
		"render_markdown":            m.formDraft.RenderMarkdown,
		"markdown_style":             m.formDraft.MarkdownStyle,
		"show_tool_calls":            m.formDraft.ShowToolCalls,
		"show_tool_results":          m.formDraft.ShowToolResults,
		"show_plan_after_plan_tools": m.formDraft.ShowPlanAfterPlanTools,
	}
	settings, err := m.client.ApplySettingsForm(m.ctx, m.settingsSnapshot.Revision, values)
	if err != nil {
		return err
	}
	m.settingsSnapshot = settings
	m.resetFormDraft()
	return nil
}

func stringifySetting(value any) string {
	switch typed := value.(type) {
	case string:
		return typed
	case float64:
		return strconv.Itoa(int(typed))
	case int:
		return strconv.Itoa(typed)
	default:
		return fmt.Sprint(value)
	}
}

func boolSetting(value any) bool {
	switch typed := value.(type) {
	case bool:
		return typed
	case string:
		parsed, _ := strconv.ParseBool(typed)
		return parsed
	default:
		return false
	}
}
