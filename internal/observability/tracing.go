package observability

type Trace struct {
	ID string
}

func NewTrace(id string) Trace {
	return Trace{ID: id}
}
