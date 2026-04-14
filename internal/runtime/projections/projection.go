package projections

import "teamd/internal/runtime/eventing"

type Projection interface {
	Apply(event eventing.Event) error
}
