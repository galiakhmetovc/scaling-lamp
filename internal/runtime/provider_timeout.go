package runtime

import "fmt"

type ProviderRoundTimeoutError struct {
	TimeoutText string
}

func (e ProviderRoundTimeoutError) Error() string {
	return fmt.Sprintf("llm round timed out after %s", e.TimeoutText)
}
