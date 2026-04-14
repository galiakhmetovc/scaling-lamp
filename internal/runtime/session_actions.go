package runtime

import (
	"strings"

	"teamd/internal/provider"
)

type SessionAction string

const (
	SessionActionShow   SessionAction = "session.show"
	SessionActionCreate SessionAction = "session.create"
	SessionActionUse    SessionAction = "session.use"
	SessionActionList   SessionAction = "session.list"
	SessionActionStats  SessionAction = "session.stats"
	SessionActionReset  SessionAction = "session.reset"
)

type SessionActionRequest struct {
	ChatID      int64
	Action      SessionAction
	SessionName string
}

type SessionActionResult struct {
	Action        SessionAction `json:"action"`
	ActiveSession string        `json:"active_session"`
	Sessions      []string      `json:"sessions,omitempty"`
	MessageCount  int           `json:"message_count"`
}

type SessionActionStore interface {
	ActiveSession(chatID int64) (string, error)
	CreateSession(chatID int64, session string) error
	UseSession(chatID int64, session string) error
	ListSessions(chatID int64) ([]string, error)
	Reset(chatID int64) error
	Messages(chatID int64) ([]provider.Message, error)
}

type SessionActions struct {
	store SessionActionStore
}

func NewSessionActions(store SessionActionStore) *SessionActions {
	return &SessionActions{store: store}
}

func (s *SessionActions) Execute(chatID int64, req SessionActionRequest) (SessionActionResult, error) {
	if s == nil || s.store == nil {
		return SessionActionResult{}, NewControlError(ErrRuntimeUnavailable, "session actions are not configured")
	}
	name := normalizeSessionActionName(req.SessionName)
	switch req.Action {
	case SessionActionShow:
		return s.snapshot(chatID, req.Action)
	case SessionActionCreate:
		if name == "" {
			return SessionActionResult{}, NewControlError(ErrValidation, "session name is required")
		}
		if err := s.store.CreateSession(chatID, name); err != nil {
			return SessionActionResult{}, err
		}
		if err := s.store.UseSession(chatID, name); err != nil {
			return SessionActionResult{}, err
		}
		return s.snapshot(chatID, req.Action)
	case SessionActionUse:
		if name == "" {
			return SessionActionResult{}, NewControlError(ErrValidation, "session name is required")
		}
		if err := s.store.UseSession(chatID, name); err != nil {
			return SessionActionResult{}, err
		}
		return s.snapshot(chatID, req.Action)
	case SessionActionList:
		return s.snapshot(chatID, req.Action)
	case SessionActionStats:
		return s.snapshot(chatID, req.Action)
	case SessionActionReset:
		if err := s.store.Reset(chatID); err != nil {
			return SessionActionResult{}, err
		}
		return s.snapshot(chatID, req.Action)
	default:
		return SessionActionResult{}, NewControlError(ErrValidation, "unsupported session action")
	}
}

func (s *SessionActions) snapshot(chatID int64, action SessionAction) (SessionActionResult, error) {
	active, err := s.store.ActiveSession(chatID)
	if err != nil {
		return SessionActionResult{}, err
	}
	sessions, err := s.store.ListSessions(chatID)
	if err != nil {
		return SessionActionResult{}, err
	}
	messages, err := s.store.Messages(chatID)
	if err != nil {
		return SessionActionResult{}, err
	}
	return SessionActionResult{
		Action:        action,
		ActiveSession: active,
		Sessions:      sessions,
		MessageCount:  len(messages),
	}, nil
}

func normalizeSessionActionName(session string) string {
	return strings.TrimSpace(strings.ToLower(session))
}
