package tui

import (
	"fmt"
	"strings"
	"time"
)

func humanTimestamp(ts time.Time) string {
	if ts.IsZero() {
		return ""
	}
	return ts.Local().Format("2006-01-02 15:04")
}

func prefixTimestamp(ts time.Time, text string) string {
	stamp := humanTimestamp(ts)
	if stamp == "" {
		return text
	}
	if text == "" {
		return stamp
	}
	return fmt.Sprintf("%s %s", stamp, text)
}

func ansiChatUser(text string) string {
	return "\x1b[1;38;5;159m" + text + "\x1b[0m"
}

func renderChatRoleLabel(role string) string {
	switch role {
	case "user":
		return ansiChatUser("USER:")
	default:
		return strings.ToUpper(role) + ":"
	}
}
