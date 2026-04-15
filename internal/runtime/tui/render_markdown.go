package tui

import (
	"strings"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/glamour"
)

func renderMarkdown(input, style string, width int) (string, error) {
	if strings.TrimSpace(input) == "" {
		return "", nil
	}
	if width <= 0 {
		width = 80
	}
	options := []glamour.TermRendererOption{glamour.WithWordWrap(width)}
	if strings.TrimSpace(style) != "" {
		options = append(options, glamour.WithStandardStyle(style))
	} else {
		options = append(options, glamour.WithAutoStyle())
	}
	renderer, err := glamour.NewTermRenderer(options...)
	if err != nil {
		return "", err
	}
	return renderer.Render(input)
}

func wrapText(input string, width int) string {
	if strings.TrimSpace(input) == "" {
		return input
	}
	if width <= 0 {
		width = 80
	}
	return strings.TrimRight(lipgloss.NewStyle().Width(width).Render(input), "\n")
}

func coalesce(value, fallback string) string {
	if strings.TrimSpace(value) == "" {
		return fallback
	}
	return value
}

func max(a, b int) int {
	if a > b {
		return a
	}
	return b
}

func cursor(current, want int) string {
	if current == want {
		return ">"
	}
	return " "
}

func isWheelUp(msg tea.MouseMsg) bool {
	return msg.Button == tea.MouseButtonWheelUp || msg.Type == tea.MouseWheelUp
}

func isWheelDown(msg tea.MouseMsg) bool {
	return msg.Button == tea.MouseButtonWheelDown || msg.Type == tea.MouseWheelDown
}

func splitPaneWidths(total, leftPreferred, rightPreferred int) (int, int) {
	if total <= 0 {
		return leftPreferred, rightPreferred
	}
	gap := 1
	available := total - gap
	if available < 2 {
		return max(1, total), 1
	}
	left := leftPreferred
	right := rightPreferred
	if left+right > available {
		left = (available * 2) / 3
		right = available - left
	}
	if left < 20 {
		left = 20
	}
	if right < 20 {
		right = 20
	}
	if left+right > available {
		if left > right {
			left = available - right
		} else {
			right = available - left
		}
	}
	if left < 1 {
		left = 1
	}
	if right < 1 {
		right = 1
	}
	return left, right
}
