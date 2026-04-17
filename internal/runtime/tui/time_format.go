package tui

import (
	"fmt"
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
