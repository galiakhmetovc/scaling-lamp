package telegram

import (
	"html"
	"regexp"
	"strings"
)

func FormatTelegramReply(input string) string {
	return renderTelegramHTML(normalizeTelegramText(input))
}

func normalizeTelegramText(input string) string {
	lines := strings.Split(strings.TrimSpace(input), "\n")
	out := make([]string, 0, len(lines))
	for _, line := range lines {
		line = strings.TrimSpace(line)
		switch {
		case strings.HasPrefix(line, "### "):
			line = "• " + strings.TrimPrefix(line, "### ")
		case strings.HasPrefix(line, "## "):
			line = "• " + strings.TrimPrefix(line, "## ")
		case strings.HasPrefix(line, "# "):
			line = "• " + strings.TrimPrefix(line, "# ")
		}
		line = flattenMarkdownNumbering(line)
		line = strings.ReplaceAll(line, "**", "*")
		out = append(out, line)
	}
	return strings.ToValidUTF8(strings.TrimSpace(strings.Join(out, "\n")), "")
}

var numberedItemPattern = regexp.MustCompile(`^\d+\.\s+`)

func flattenMarkdownNumbering(line string) string {
	if numberedItemPattern.MatchString(line) {
		return "• " + numberedItemPattern.ReplaceAllString(line, "")
	}
	return line
}

func renderTelegramHTML(input string) string {
	lines := strings.Split(strings.TrimSpace(input), "\n")
	out := make([]string, 0, len(lines))
	for _, line := range lines {
		out = append(out, renderInlineTelegramHTML(line))
	}
	return strings.ToValidUTF8(strings.TrimSpace(strings.Join(out, "\n")), "")
}

func renderInlineTelegramHTML(input string) string {
	var b strings.Builder
	runes := []rune(input)
	for i := 0; i < len(runes); {
		switch runes[i] {
		case '*':
			end := indexRune(runes[i+1:], '*')
			if end >= 0 {
				content := string(runes[i+1 : i+1+end])
				if content != "" {
					b.WriteString("<b>")
					b.WriteString(html.EscapeString(content))
					b.WriteString("</b>")
					i += end + 2
					continue
				}
			}
		case '`':
			end := indexRune(runes[i+1:], '`')
			if end >= 0 {
				content := string(runes[i+1 : i+1+end])
				if content != "" {
					b.WriteString("<code>")
					b.WriteString(html.EscapeString(content))
					b.WriteString("</code>")
					i += end + 2
					continue
				}
			}
		}
		b.WriteString(html.EscapeString(string(runes[i])))
		i++
	}
	return b.String()
}

func indexRune(input []rune, target rune) int {
	for i, r := range input {
		if r == target {
			return i
		}
	}
	return -1
}
