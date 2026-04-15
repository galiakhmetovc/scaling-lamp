package tui

import (
	"strings"

	"github.com/charmbracelet/glamour"
)

func renderMarkdown(input, style string) (string, error) {
	if strings.TrimSpace(input) == "" {
		return "", nil
	}
	options := []glamour.TermRendererOption{glamour.WithWordWrap(0)}
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
