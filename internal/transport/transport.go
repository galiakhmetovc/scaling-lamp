package transport

import "teamd/internal/events"

type Adapter interface {
	Normalize(any) (events.InboundEvent, error)
}
