package daemon

import (
	"time"

	"teamd/internal/runtime"
)

type WebsocketEnvelope struct {
	Type        string           `json:"type"`
	ID          string           `json:"id,omitempty"`
	Command     string           `json:"command,omitempty"`
	Payload     any              `json:"payload,omitempty"`
	Error       string           `json:"error,omitempty"`
	Event       *runtime.UIEvent `json:"event,omitempty"`
	GeneratedAt time.Time        `json:"generated_at"`
}

type CommandRequest struct {
	Type    string         `json:"type"`
	ID      string         `json:"id"`
	Command string         `json:"command"`
	Payload map[string]any `json:"payload,omitempty"`
}
