package runtime

import (
	"context"
	"time"
)

type DelegateBackend string

const (
	DelegateBackendLocalWorker DelegateBackend = "local_worker"
	DelegateBackendRemoteMesh  DelegateBackend = "remote_mesh"
)

type DelegateStatus string

const (
	DelegateStatusQueued          DelegateStatus = "queued"
	DelegateStatusIdle            DelegateStatus = "idle"
	DelegateStatusRunning         DelegateStatus = "running"
	DelegateStatusWaitingApproval DelegateStatus = "waiting_approval"
	DelegateStatusFailed          DelegateStatus = "failed"
	DelegateStatusClosed          DelegateStatus = "closed"
)

type DelegateSpawnRequest struct {
	DelegateID     string
	Backend        DelegateBackend
	OwnerSessionID string
	Prompt         string
	PolicySnapshot map[string]any
	Metadata       map[string]any
}

type DelegateMessageRequest struct {
	Content string
}

type DelegateWaitRequest struct {
	DelegateID   string
	AfterCursor  int
	AfterEventID int64
	EventLimit   int
}

type DelegateArtifactRef struct {
	Ref         string
	Kind        string
	Label       string
	ContentType string
}

type DelegateEventRef struct {
	EventID int64
	Kind    string
}

type DelegateMessage struct {
	Cursor     int
	Role       string
	Content    string
	Name       string
	ToolCallID string
}

type DelegateHandoff struct {
	DelegateID          string
	Backend             DelegateBackend
	LastRunID           string
	Summary             string
	Artifacts           []DelegateArtifactRef
	PromotedFacts       []string
	OpenQuestions       []string
	RecommendedNextStep string
	CreatedAt           time.Time
	UpdatedAt           time.Time
}

type DelegateView struct {
	DelegateID     string
	Backend        DelegateBackend
	OwnerSessionID string
	Status         DelegateStatus
	LastRunID      string
	ArtifactRefs   []DelegateArtifactRef
	EventRefs      []DelegateEventRef
	PolicySnapshot map[string]any
	LastError      string
	CreatedAt      time.Time
	UpdatedAt      time.Time
	LastMessageAt  *time.Time
	ClosedAt       *time.Time
}

type DelegateWaitResult struct {
	Delegate       DelegateView
	Handoff        *DelegateHandoff
	Messages       []DelegateMessage
	Events         []DelegateEventRef
	NextCursor     int
	NextEventAfter int64
}

type DelegateRuntime interface {
	Spawn(context.Context, DelegateSpawnRequest) (DelegateView, error)
	Message(context.Context, string, DelegateMessageRequest) (DelegateView, error)
	Wait(context.Context, DelegateWaitRequest) (DelegateWaitResult, bool, error)
	Close(context.Context, string) (DelegateView, bool, error)
	Handoff(context.Context, string) (DelegateHandoff, bool, error)
}
