package observability

type Logger struct {
	Events []string
}

func NewLogger() *Logger {
	return &Logger{Events: []string{}}
}

func (l *Logger) Record(event string) {
	l.Events = append(l.Events, event)
}
