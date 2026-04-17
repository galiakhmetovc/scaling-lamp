package tui

import (
	"encoding/json"
	"fmt"
	"sort"
	"strings"

	tea "github.com/charmbracelet/bubbletea"
)

type headLine struct {
	Path     string
	Text     string
	Toggle   bool
	Expanded bool
}

func (m *model) updateHead(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	switch msg.String() {
	case "up", "k":
		if m.headCursor > 0 {
			m.headCursor--
		}
	case "down", "j":
		lines := m.currentHeadLines()
		if m.headCursor < len(lines)-1 {
			m.headCursor++
		}
	case "enter", " ":
		lines := m.currentHeadLines()
		if m.headCursor >= 0 && m.headCursor < len(lines) && lines[m.headCursor].Toggle {
			path := lines[m.headCursor].Path
			m.headExpanded[path] = !m.headExpanded[path]
		}
	case "c":
		m.headExpanded = map[string]bool{}
	case "e":
		m.headExpanded = map[string]bool{"$": true}
		for _, line := range m.currentHeadLines() {
			if line.Toggle {
				m.headExpanded[line.Path] = true
			}
		}
	}
	return m, nil
}

func (m *model) viewHead() string {
	state := m.currentSessionState()
	if state == nil {
		return "No active session"
	}
	lines := m.currentHeadLines()
	if m.headCursor >= len(lines) && len(lines) > 0 {
		m.headCursor = len(lines) - 1
	}
	if len(lines) == 0 {
		return "Head\n\nNo session head available"
	}
	rendered := make([]string, 0, len(lines)+2)
	rendered = append(rendered, "Session Head", "", "Enter toggle, c collapse all, e expand all")
	for i, line := range lines {
		prefix := "  "
		if i == m.headCursor {
			prefix = "> "
		}
		rendered = append(rendered, prefix+line.Text)
	}
	return strings.Join(rendered, "\n")
}

func (m *model) currentHeadLines() []headLine {
	state := m.currentSessionState()
	if state == nil {
		return nil
	}
	if len(m.headExpanded) == 0 {
		m.headExpanded["$"] = true
	}
	body, _ := json.Marshal(state.Snapshot)
	var value any
	if err := json.Unmarshal(body, &value); err != nil {
		return []headLine{{Path: "$", Text: err.Error()}}
	}
	lines := []headLine{}
	m.appendHeadLines(&lines, "$", "session", value, 0)
	return lines
}

func (m *model) appendHeadLines(lines *[]headLine, path, label string, value any, depth int) {
	indent := strings.Repeat("  ", depth)
	switch typed := value.(type) {
	case map[string]any:
		keys := make([]string, 0, len(typed))
		for key := range typed {
			keys = append(keys, key)
		}
		sort.Strings(keys)
		expanded := m.headExpanded[path]
		*lines = append(*lines, headLine{
			Path:     path,
			Toggle:   true,
			Expanded: expanded,
			Text:     fmt.Sprintf("%s%s %s {…}", indent, toggleGlyph(expanded), label),
		})
		if !expanded {
			return
		}
		for _, key := range keys {
			childPath := path + "." + key
			m.appendHeadLines(lines, childPath, key, typed[key], depth+1)
		}
	case []any:
		expanded := m.headExpanded[path]
		*lines = append(*lines, headLine{
			Path:     path,
			Toggle:   true,
			Expanded: expanded,
			Text:     fmt.Sprintf("%s%s %s [%d]", indent, toggleGlyph(expanded), label, len(typed)),
		})
		if !expanded {
			return
		}
		for idx, item := range typed {
			childPath := fmt.Sprintf("%s[%d]", path, idx)
			m.appendHeadLines(lines, childPath, fmt.Sprintf("[%d]", idx), item, depth+1)
		}
	default:
		*lines = append(*lines, headLine{
			Path: path,
			Text: fmt.Sprintf("%s%s: %s", indent, label, formatHeadScalar(typed)),
		})
	}
}

func toggleGlyph(expanded bool) string {
	if expanded {
		return "▼"
	}
	return "▶"
}

func formatHeadScalar(value any) string {
	switch typed := value.(type) {
	case string:
		return strconvQuote(typed)
	default:
		body, _ := json.Marshal(typed)
		return string(body)
	}
}

func strconvQuote(text string) string {
	body, _ := json.Marshal(text)
	return string(body)
}
