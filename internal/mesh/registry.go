package mesh

import (
	"context"
	"time"
)

type Registry interface {
	Register(ctx context.Context, peer PeerDescriptor) error
	Heartbeat(ctx context.Context, agentID string, at time.Time) error
	ListOnline(ctx context.Context) ([]PeerDescriptor, error)
	ListScores(ctx context.Context, taskClass string) ([]ScoreRecord, error)
	RecordScore(ctx context.Context, score ScoreRecord) error
}
