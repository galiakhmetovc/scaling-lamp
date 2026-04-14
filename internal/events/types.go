package events

type InboundEvent struct {
	Source    string
	SessionID string
	Text      string
}

type OutboundEvent struct {
	SessionID string
	Text      string
}

type SystemEvent struct {
	Kind      string
	SessionID string
}
