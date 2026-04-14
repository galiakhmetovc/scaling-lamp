package runtime

import "fmt"

type ErrorCode string

const (
	ErrValidation         ErrorCode = "validation_error"
	ErrNotFound           ErrorCode = "not_found"
	ErrConflict           ErrorCode = "conflict"
	ErrPolicyDenied       ErrorCode = "policy_denied"
	ErrApprovalRequired   ErrorCode = "approval_required"
	ErrRuntimeUnavailable ErrorCode = "runtime_unavailable"
	ErrProvider           ErrorCode = "provider_error"
	ErrTool               ErrorCode = "tool_error"
	ErrJob                ErrorCode = "job_error"
	ErrWorker             ErrorCode = "worker_error"
	ErrTimeout            ErrorCode = "timeout"
	ErrCancelled          ErrorCode = "cancelled"
	ErrInternal           ErrorCode = "internal_error"
)

type ControlError struct {
	Code       ErrorCode
	Message    string
	EntityType string
	EntityID   string
	Retryable  bool
	Cause      error
}

func (e *ControlError) Error() string {
	if e == nil {
		return ""
	}
	if e.Message != "" {
		if e.Cause != nil {
			return fmt.Sprintf("%s: %v", e.Message, e.Cause)
		}
		return e.Message
	}
	if e.Cause != nil {
		return e.Cause.Error()
	}
	return string(e.Code)
}

func (e *ControlError) Unwrap() error {
	if e == nil {
		return nil
	}
	return e.Cause
}

func NewControlError(code ErrorCode, message string) *ControlError {
	return &ControlError{Code: code, Message: message}
}
