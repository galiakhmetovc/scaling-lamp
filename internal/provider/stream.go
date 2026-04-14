package provider

type StreamEventKind string

const (
	StreamEventText      StreamEventKind = "text"
	StreamEventReasoning StreamEventKind = "reasoning"
)

type StreamEvent struct {
	Kind StreamEventKind
	Text string
}
