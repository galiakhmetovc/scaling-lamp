package runtime

import "time"

type DebugSessionView struct {
	Session SessionState
	Control ControlState
	Events  []RuntimeEvent
}

type DebugRunView struct {
	Run    RunView
	Replay *RunReplay
	Events []RuntimeEvent
}

type DebugLayerProvenance struct {
	Layer      string
	Summary    string
	SourceRef  string
	UpdatedAt  *time.Time
}

type DebugRecentWorkProvenance struct {
	LastCompletedRunID string
	CurrentGoal        string
	LastResultSummary  string
	CurrentProject     string
	ArtifactRefs       []string
	OpenLoops          []string
}

type DebugContextProvenance struct {
	RunID        string
	SessionID    string
	ChatID       int64
	SessionHead  *SessionHead
	RecentWork   *DebugRecentWorkProvenance
	Transcript   *DebugLayerProvenance
	MemoryRecall *DebugLayerProvenance
	Checkpoint   *DebugLayerProvenance
	Continuity   *DebugLayerProvenance
	Workspace    *DebugLayerProvenance
	Skills       *DebugLayerProvenance
}
