package runtime

import (
	"context"
	"encoding/json"
	"fmt"

	"teamd/internal/runtime/eventing"
)

func (a *Agent) recordProviderRequestEvent(ctx context.Context, runID, sessionID, correlationID, source string, requestBody []byte) error {
	if len(requestBody) == 0 {
		return nil
	}
	policy := a.Contracts.ProviderTrace.Request
	if !policy.Enabled || policy.Strategy == "" || policy.Strategy == "none" {
		return nil
	}

	payload := map[string]any{
		"session_id":   sessionID,
		"content_type": "application/json",
		"body_size":    len(requestBody),
	}
	switch policy.Strategy {
	case "inline_request":
		if policy.Params.IncludeRawBody {
			payload["raw_body"] = string(requestBody)
		}
		if policy.Params.IncludeDecodedPayload {
			var decoded map[string]any
			if err := json.Unmarshal(requestBody, &decoded); err != nil {
				payload["request_payload_decode_error"] = err.Error()
			} else {
				payload["request_payload"] = decoded
			}
		}
	default:
		return fmt.Errorf("unsupported provider trace strategy %q", policy.Strategy)
	}

	return a.RecordEvent(ctx, eventing.Event{
		ID:               a.newID("evt-provider-request"),
		Kind:             eventing.EventProviderRequestCaptured,
		OccurredAt:       a.now(),
		AggregateID:      runID,
		AggregateType:    eventing.AggregateRun,
		AggregateVersion: 2,
		CorrelationID:    correlationID,
		CausationID:      runID,
		Source:           source,
		ActorID:          a.Config.ID,
		ActorType:        "agent",
		TraceSummary:     "provider request captured",
		Payload:          payload,
	})
}
