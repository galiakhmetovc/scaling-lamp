package api

import (
	"errors"
	"time"

	"teamd/internal/runtime"
)

func NewErrorResponse(code, message string) ErrorResponse {
	return ErrorResponse{
		Error: APIError{
			Code:    code,
			Message: message,
		},
		Time: time.Now().UTC(),
	}
}

func NewRuntimeErrorResponse(err error, fallbackCode, fallbackMessage string) ErrorResponse {
	var controlErr *runtime.ControlError
	if errors.As(err, &controlErr) {
		message := controlErr.Message
		if message == "" {
			message = fallbackMessage
		}
		return ErrorResponse{
			Error: APIError{
				Code:       string(controlErr.Code),
				Message:    message,
				EntityType: controlErr.EntityType,
				EntityID:   controlErr.EntityID,
				Retryable:  controlErr.Retryable,
			},
			Time: time.Now().UTC(),
		}
	}
	return NewErrorResponse(fallbackCode, fallbackMessage)
}
