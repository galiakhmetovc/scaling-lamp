package runtime

import (
	"fmt"

	"teamd/internal/provider"
)

type DebugService struct {
	api *API
}

func NewDebugService(api *API) *DebugService {
	return &DebugService{api: api}
}

func (s *DebugService) SessionView(sessionID string, chatID int64, eventLimit int) (DebugSessionView, error) {
	if s == nil || s.api == nil {
		return DebugSessionView{}, NewControlError(ErrRuntimeUnavailable, "debug service is not configured")
	}
	session, err := s.api.SessionState(sessionID, chatID, provider.RequestConfig{}, MemoryPolicy{}, ActionPolicy{})
	if err != nil {
		return DebugSessionView{}, err
	}
	control, err := s.api.ControlState(sessionID, chatID, provider.RequestConfig{}, MemoryPolicy{}, ActionPolicy{})
	if err != nil {
		return DebugSessionView{}, err
	}
	events, err := s.api.ListEvents(EventQuery{SessionID: sessionID, Limit: eventLimit})
	if err != nil {
		return DebugSessionView{}, err
	}
	return DebugSessionView{
		Session: session,
		Control: control,
		Events:  events,
	}, nil
}

func (s *DebugService) RunView(runID string, eventLimit int) (DebugRunView, bool, error) {
	if s == nil || s.api == nil {
		return DebugRunView{}, false, NewControlError(ErrRuntimeUnavailable, "debug service is not configured")
	}
	run, ok, err := s.api.RunView(runID)
	if err != nil || !ok {
		return DebugRunView{}, ok, err
	}
	replay, replayOK, err := s.api.RunReplay(runID)
	if err != nil {
		return DebugRunView{}, false, err
	}
	events, err := s.api.ListEvents(EventQuery{RunID: runID, Limit: eventLimit})
	if err != nil {
		return DebugRunView{}, false, err
	}
	view := DebugRunView{
		Run:    run,
		Events: events,
	}
	if replayOK {
		view.Replay = &replay
	}
	return view, true, nil
}

func (s *DebugService) ContextProvenance(runID string) (DebugContextProvenance, error) {
	if s == nil || s.api == nil {
		return DebugContextProvenance{}, NewControlError(ErrRuntimeUnavailable, "debug service is not configured")
	}
	run, ok, err := s.api.RunView(runID)
	if err != nil {
		return DebugContextProvenance{}, err
	}
	if !ok {
		return DebugContextProvenance{}, NewControlError(ErrNotFound, fmt.Sprintf("run %s not found", runID))
	}
	head, err := s.api.sessionHead(run.ChatID, run.SessionID)
	if err != nil {
		return DebugContextProvenance{}, err
	}
	view := DebugContextProvenance{
		RunID:     run.RunID,
		SessionID: run.SessionID,
		ChatID:    run.ChatID,
	}
	if head != nil {
		headCopy := *head
		view.SessionHead = &headCopy
		view.RecentWork = &DebugRecentWorkProvenance{
			LastCompletedRunID: head.LastCompletedRunID,
			CurrentGoal:        head.CurrentGoal,
			LastResultSummary:  head.LastResultSummary,
			CurrentProject:     head.CurrentProject,
			ArtifactRefs:       append([]string(nil), head.RecentArtifactRefs...),
			OpenLoops:          append([]string(nil), head.OpenLoops...),
		}
		view.MemoryRecall = &DebugLayerProvenance{
			Layer:     "memory_recall",
			Summary:   "semantic recall may supplement recent session state for this run",
			SourceRef: run.SessionID,
			UpdatedAt: &head.UpdatedAt,
		}
		view.Transcript = &DebugLayerProvenance{
			Layer:     "transcript",
			Summary:   "session transcript remains the canonical stored message history",
			SourceRef: run.SessionID,
			UpdatedAt: &head.UpdatedAt,
		}
	}
	return view, nil
}
