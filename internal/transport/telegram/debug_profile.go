package telegram

import (
	"context"

	"teamd/internal/provider"
	runtimex "teamd/internal/runtime"
	"teamd/internal/worker"
)

type debugRunIDKey struct{}

func withDebugRunID(ctx context.Context, runID string) context.Context {
	if ctx == nil || runID == "" {
		return ctx
	}
	return context.WithValue(ctx, debugRunIDKey{}, runID)
}

func debugRunIDFromContext(ctx context.Context) string {
	if ctx == nil {
		return ""
	}
	value, _ := ctx.Value(debugRunIDKey{}).(string)
	return value
}

func (a *Adapter) rememberDebugProfile(runID string, profile *runtimex.DebugExecutionProfile) {
	if a == nil || runID == "" || profile == nil {
		return
	}
	cloned := *profile
	a.debugProfiles.Store(runID, cloned)
}

func (a *Adapter) forgetDebugProfile(runID string) {
	if a == nil || runID == "" {
		return
	}
	a.debugProfiles.Delete(runID)
}

func (a *Adapter) debugProfileForContext(ctx context.Context) *runtimex.DebugExecutionProfile {
	if a == nil {
		return nil
	}
	runID := debugRunIDFromContext(ctx)
	if runID == "" {
		return nil
	}
	value, ok := a.debugProfiles.Load(runID)
	if !ok {
		return nil
	}
	profile, _ := value.(runtimex.DebugExecutionProfile)
	return &profile
}

type debugProfileConversationStore struct {
	base    Store
	profile runtimex.DebugExecutionProfile
}

func (s debugProfileConversationStore) Append(chatID int64, msg provider.Message) error {
	return s.base.Append(chatID, msg)
}

func (s debugProfileConversationStore) Messages(chatID int64) ([]provider.Message, error) {
	messages, err := s.base.Messages(chatID)
	if err != nil {
		return nil, err
	}
	if s.profile.Transcript {
		return messages, nil
	}
	return trimMessagesToActiveTurn(messages), nil
}

func (s debugProfileConversationStore) Checkpoint(chatID int64) (worker.Checkpoint, bool, error) {
	if !s.profile.Checkpoint {
		return worker.Checkpoint{}, false, nil
	}
	return s.base.Checkpoint(chatID)
}

func (s debugProfileConversationStore) SaveCheckpoint(chatID int64, checkpoint worker.Checkpoint) error {
	if !s.profile.Checkpoint {
		return nil
	}
	return s.base.SaveCheckpoint(chatID, checkpoint)
}

func (s debugProfileConversationStore) ActiveSession(chatID int64) (string, error) {
	return s.base.ActiveSession(chatID)
}

func trimMessagesToActiveTurn(messages []provider.Message) []provider.Message {
	if len(messages) == 0 {
		return nil
	}
	start := 0
	for i := len(messages) - 1; i >= 0; i-- {
		if messages[i].Role == "user" {
			start = i
			break
		}
	}
	out := make([]provider.Message, len(messages[start:]))
	copy(out, messages[start:])
	return out
}
