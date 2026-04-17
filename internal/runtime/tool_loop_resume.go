package runtime

import (
	"context"
	"fmt"
	"strings"

	"teamd/internal/contracts"
	"teamd/internal/provider"
	itools "teamd/internal/tools"
)

type suspendedToolLoop struct {
	ApprovalID           string
	ContractSet          contracts.ResolvedContracts
	SessionID            string
	RunID                string
	CorrelationID        string
	Source               string
	PromptAssetSelection []string
	Tools                []itools.Definition
	BaseMessages         []contracts.Message
	ToolMessages         []contracts.Message
	Calls                []provider.ToolCall
	Decisions            []provider.ToolDecision
	PendingIndex         int
}

func (a *Agent) storeSuspendedToolLoop(state suspendedToolLoop) {
	if strings.TrimSpace(state.ApprovalID) == "" {
		return
	}
	a.suspendedToolMu.Lock()
	defer a.suspendedToolMu.Unlock()
	if a.suspendedTools == nil {
		a.suspendedTools = map[string]suspendedToolLoop{}
	}
	a.suspendedTools[state.ApprovalID] = state
}

func (a *Agent) popSuspendedToolLoop(approvalID string) (suspendedToolLoop, bool) {
	a.suspendedToolMu.Lock()
	defer a.suspendedToolMu.Unlock()
	if a.suspendedTools == nil {
		return suspendedToolLoop{}, false
	}
	state, ok := a.suspendedTools[approvalID]
	if ok {
		delete(a.suspendedTools, approvalID)
	}
	return state, ok
}

func (a *Agent) CopySuspendedToolLoopTo(approvalID string, target *Agent) {
	if target == nil {
		return
	}
	a.suspendedToolMu.Lock()
	state, ok := a.suspendedTools[approvalID]
	a.suspendedToolMu.Unlock()
	if !ok {
		return
	}
	target.storeSuspendedToolLoop(state)
}

func (a *Agent) resumeSuspendedToolLoopAfterApproval(ctx context.Context, approvalID, resultText string) error {
	state, ok := a.popSuspendedToolLoop(approvalID)
	if !ok {
		return nil
	}
	return a.resumeSuspendedToolLoop(ctx, state, resultText, "")
}

func (a *Agent) resumeSuspendedToolLoopAfterDenial(ctx context.Context, approvalID, reason string) error {
	state, ok := a.popSuspendedToolLoop(approvalID)
	if !ok {
		return nil
	}
	call := state.Calls[state.PendingIndex]
	resultText := toolErrorResult(call.Name, fmt.Errorf("%s", reason))
	return a.resumeSuspendedToolLoop(ctx, state, resultText, reason)
}

func (a *Agent) resumeSuspendedToolLoop(ctx context.Context, state suspendedToolLoop, resultText, errorText string) error {
	call := state.Calls[state.PendingIndex]
	toolContent := resultText
	displayText := resultText
	var artifactRefs []string
	var err error
	if strings.TrimSpace(errorText) == "" {
		displayText, artifactRefs, err = a.maybeOffloadToolResult(ctx, state.ContractSet, call.Name, resultText)
		if err != nil {
			return fmt.Errorf("offload resumed tool result: %w", err)
		}
	}
	if err := a.recordToolCallCompleted(ctx, state.RunID, state.SessionID, state.CorrelationID, state.Source, call.Name, call.Arguments, displayText, errorText, artifactRefs); err != nil {
		return fmt.Errorf("record resumed tool call completed: %w", err)
	}
	if a.UIBus != nil {
		a.UIBus.Publish(UIEvent{
			Kind:      UIEventToolCompleted,
			SessionID: state.SessionID,
			RunID:     state.RunID,
			Tool: ToolActivity{
				Phase:      ToolActivityPhaseCompleted,
				OccurredAt: a.now(),
				Name:       call.Name,
				Arguments:  call.Arguments,
				ResultText: displayText,
				ErrorText:  errorText,
			},
		})
	}

	messages := append([]contracts.Message{}, state.BaseMessages...)
	messages = append(messages, assistantToolCallMessage(state.Calls))
	messages = append(messages, state.ToolMessages...)
	messages = append(messages, contracts.Message{
		Role:       "tool",
		Name:       call.Name,
		ToolCallID: call.ID,
		Content:    toolContent,
	})

	remainingCalls := state.Calls[state.PendingIndex+1:]
	if len(remainingCalls) > 0 {
		moreToolMessages, suspension, err := a.executeToolCalls(ctx, state.ContractSet, state.RunID, state.SessionID, state.CorrelationID, state.Source, messages, state.PromptAssetSelection, state.Tools, remainingCalls, state.Decisions, nil)
		if err != nil {
			if state.Source == "agent.chat" {
				_ = a.recordChatRunFailure(ctx, state.SessionID, state.RunID, state.CorrelationID, err)
			}
			return err
		}
		if suspension != nil {
			a.storeSuspendedToolLoop(*suspension)
			if a.UIBus != nil {
				a.UIBus.Publish(UIEvent{Kind: UIEventStatusChanged, SessionID: state.SessionID, RunID: state.RunID, Status: "approval_pending"})
			}
			return nil
		}
		messages = append(messages, moreToolMessages...)
	}

	result, err := a.executeProviderLoop(ctx, state.ContractSet, state.SessionID, state.RunID, state.CorrelationID, state.Source, provider.ClientInput{
		PromptAssetSelection: state.PromptAssetSelection,
		Messages:             messages,
		Tools:                state.Tools,
	}, nil, 0)
	if err != nil {
		if state.Source == "agent.chat" {
			_ = a.recordChatRunFailure(ctx, state.SessionID, state.RunID, state.CorrelationID, err)
		}
		return err
	}
	if result.Provider.FinishReason == "approval_pending" {
		if a.UIBus != nil {
			a.UIBus.Publish(UIEvent{Kind: UIEventStatusChanged, SessionID: state.SessionID, RunID: state.RunID, Status: "approval_pending"})
		}
		return nil
	}
	if state.Source != "agent.chat" {
		return nil
	}
	session, err := a.ResumeChatSession(ctx, state.SessionID)
	if err != nil {
		return fmt.Errorf("resume chat session after approval: %w", err)
	}
	return a.completeChatRun(ctx, session, state.RunID, state.CorrelationID, result)
}
