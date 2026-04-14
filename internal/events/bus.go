package events

import "context"

type Bus interface {
	Publish(context.Context, InboundEvent) error
}

type InMemoryBus struct{}

func (InMemoryBus) Publish(context.Context, InboundEvent) error {
	return nil
}
