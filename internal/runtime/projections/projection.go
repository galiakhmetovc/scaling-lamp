package projections

import "teamd/internal/runtime/eventing"

type Projection interface {
	ID() string
	Apply(event eventing.Event) error
	SnapshotValue() any
	RestoreSnapshot(raw []byte) error
}
