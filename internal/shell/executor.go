package shell

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os/exec"
	"path/filepath"
	"runtime"
	"sort"
	"strings"
	"sync"
	"sync/atomic"
	"syscall"
	"time"

	"teamd/internal/contracts"
	"teamd/internal/runtime/eventing"
)

type runResult struct {
	stdout   string
	stderr   string
	exitCode int
}

type runFunc func(ctx context.Context, cwd, executable string, args []string) (runResult, error)
type lookupPathFunc func(file string) (string, error)
type startFunc func(ctx context.Context, cwd, executable string, args []string) (processHandle, error)

type invocation struct {
	executable string
	args       []string
	isolated   bool
}

type processHandle interface {
	StdoutPipe() (io.ReadCloser, error)
	StderrPipe() (io.ReadCloser, error)
	Start() error
	Wait() error
	Kill() error
}

type commandChunk struct {
	Offset int    `json:"offset"`
	Stream string `json:"stream"`
	Text   string `json:"text"`
}

type ExecutionMeta struct {
	SessionID   string
	RunID       string
	Source      string
	ActorID     string
	ActorType   string
	RecordEvent func(context.Context, eventing.Event) error
	Now         func() time.Time
	NewID       func(string) string
}

type PendingApprovalView struct {
	ApprovalID string
	CommandID  string
	SessionID  string
	RunID      string
	ToolName   string
	Command    string
	Args       []string
	Cwd        string
	Message    string
}

type ActiveCommandView struct {
	CommandID   string
	SessionID   string
	RunID       string
	Command     string
	Args        []string
	Cwd         string
	Status      string
	NextOffset  int
	LastChunk   string
	KillPending bool
	Error       string
}

type activeCommand struct {
	mu               sync.RWMutex
	id               string
	command          string
	args             []string
	cwd              string
	status           string
	exitCode         *int
	errorText        string
	nextOffset       int
	chunks           []commandChunk
	process          processHandle
	killRequested    bool
	cancel           context.CancelFunc
	recordedOffset   int
	terminalRecorded bool
	completedAt      time.Time
	meta             ExecutionMeta
}

type pendingApproval struct {
	PendingApprovalView
	contract   contracts.ShellExecutionContract
	invocation invocation
	meta       ExecutionMeta
}

type approvalDecision string

const (
	approvalDecisionAllow   approvalDecision = "allow"
	approvalDecisionRequire approvalDecision = "require"
	approvalDecisionDeny    approvalDecision = "deny"
)

type Executor struct {
	run        runFunc
	lookupPath lookupPathFunc
	start      startFunc
	goos       string

	eventMu   sync.Mutex
	mu        sync.RWMutex
	commands  map[string]*activeCommand
	completed map[string]*activeCommand
	approvals map[string]*pendingApproval
	nextID    atomic.Uint64
}

const (
	completedCommandRetention = 5 * time.Minute
	maxCompletedCommands      = 64
)

func NewExecutor() *Executor {
	return &Executor{
		run:        defaultRunCommand,
		lookupPath: exec.LookPath,
		start:      defaultStartCommand,
		goos:       runtime.GOOS,
		commands:   map[string]*activeCommand{},
		completed:  map[string]*activeCommand{},
		approvals:  map[string]*pendingApproval{},
	}
}

func (e *Executor) Execute(contract contracts.ShellExecutionContract, toolName string, argsMap map[string]any) (string, error) {
	return e.ExecuteWithMeta(context.Background(), contract, toolName, argsMap, ExecutionMeta{})
}

func (e *Executor) ExecuteWithMeta(ctx context.Context, contract contracts.ShellExecutionContract, toolName string, argsMap map[string]any, meta ExecutionMeta) (string, error) {
	if e == nil {
		return "", fmt.Errorf("shell executor is nil")
	}
	if e.commands == nil {
		e.commands = map[string]*activeCommand{}
	}
	if e.completed == nil {
		e.completed = map[string]*activeCommand{}
	}
	if e.approvals == nil {
		e.approvals = map[string]*pendingApproval{}
	}
	switch toolName {
	case "shell_exec":
		return e.executeSync(ctx, contract, toolName, argsMap)
	case "shell_start":
		return e.executeStart(ctx, contract, argsMap, meta)
	case "shell_poll":
		return e.executePoll(ctx, argsMap)
	case "shell_kill":
		return e.executeKill(ctx, argsMap)
	default:
		return "", fmt.Errorf("shell tool %q is not implemented", toolName)
	}
}

func (e *Executor) executeSync(ctx context.Context, contract contracts.ShellExecutionContract, toolName string, argsMap map[string]any) (string, error) {
	if ctx == nil {
		ctx = context.Background()
	}
	command, err := stringArg(argsMap, "command")
	if err != nil {
		return "", err
	}
	args, err := optionalStringSlice(argsMap, "args")
	if err != nil {
		return "", err
	}
	if err := validateCommand(contract.Command, command, args); err != nil {
		return "", err
	}
	cwd, err := resolveCwd(contract.Runtime, argsMap)
	if err != nil {
		return "", err
	}
	invocation, err := e.resolveInvocation(contract.Runtime, command, args)
	if err != nil {
		return "", err
	}
	approval, message := e.evaluateApproval(contract.Approval, command, args)
	if approval == approvalDecisionDeny {
		return "", fmt.Errorf("%s", message)
	}
	if approval == approvalDecisionRequire {
		return e.queueApproval(ctx, toolName, contract, ExecutionMeta{}, command, args, cwd, invocation, message)
	}
	timeout, err := parseTimeout(contract.Runtime)
	if err != nil {
		return "", err
	}
	runCtx, cancel := context.WithTimeout(ctx, timeout)
	defer cancel()
	start := time.Now()
	result, err := e.run(runCtx, cwd, invocation.executable, invocation.args)
	duration := time.Since(start)
	if runCtx.Err() == context.DeadlineExceeded {
		return "", fmt.Errorf("shell command timed out")
	}
	if runCtx.Err() == context.Canceled {
		return "", fmt.Errorf("shell command canceled")
	}
	maxOutput := contract.Runtime.Params.MaxOutputBytes
	if maxOutput > 0 && len(result.stdout)+len(result.stderr) > maxOutput {
		return "", fmt.Errorf("shell output exceeds max_output_bytes")
	}
	if err != nil {
		return "", fmt.Errorf("run shell command: %w", err)
	}
	if invocation.isolated && result.exitCode != 0 && strings.Contains(result.stderr, "unshare:") {
		return "", fmt.Errorf("shell network isolation unavailable: %s", strings.TrimSpace(result.stderr))
	}
	status := "ok"
	if result.exitCode != 0 {
		status = "error"
	}
	return jsonText(map[string]any{
		"status":      status,
		"tool":        toolName,
		"command":     command,
		"args":        args,
		"cwd":         cwd,
		"exit_code":   result.exitCode,
		"stdout":      result.stdout,
		"stderr":      result.stderr,
		"duration_ms": duration.Milliseconds(),
		"timed_out":   false,
	}), nil
}

func (e *Executor) executeStart(ctx context.Context, contract contracts.ShellExecutionContract, argsMap map[string]any, meta ExecutionMeta) (string, error) {
	command, err := stringArg(argsMap, "command")
	if err != nil {
		return "", err
	}
	args, err := optionalStringSlice(argsMap, "args")
	if err != nil {
		return "", err
	}
	if err := validateCommand(contract.Command, command, args); err != nil {
		return "", err
	}
	cwd, err := resolveCwd(contract.Runtime, argsMap)
	if err != nil {
		return "", err
	}
	invocation, err := e.resolveInvocation(contract.Runtime, command, args)
	if err != nil {
		return "", err
	}
	approval, message := e.evaluateApproval(contract.Approval, command, args)
	if approval == approvalDecisionDeny {
		return "", fmt.Errorf("%s", message)
	}
	if approval == approvalDecisionRequire {
		return e.queueApproval(ctx, "shell_start", contract, meta, command, args, cwd, invocation, message)
	}
	return e.startCommand(ctx, contract, meta, "", command, args, cwd, invocation, "shell_start", nil)
}

func (e *Executor) startCommand(ctx context.Context, contract contracts.ShellExecutionContract, meta ExecutionMeta, commandID string, command string, args []string, cwd string, invocation invocation, toolName string, beforeStart func() error) (string, error) {
	if ctx == nil {
		ctx = context.Background()
	}
	if err := ctx.Err(); err != nil {
		return "", fmt.Errorf("start shell command: %w", err)
	}
	runCtx, cancel := context.WithCancel(context.Background())
	proc, err := e.start(runCtx, cwd, invocation.executable, invocation.args)
	if err != nil {
		cancel()
		return "", fmt.Errorf("start shell command: %w", err)
	}
	stdoutPipe, err := proc.StdoutPipe()
	if err != nil {
		cancel()
		return "", fmt.Errorf("attach shell stdout: %w", err)
	}
	stderrPipe, err := proc.StderrPipe()
	if err != nil {
		cancel()
		return "", fmt.Errorf("attach shell stderr: %w", err)
	}
	if err := proc.Start(); err != nil {
		cancel()
		return "", fmt.Errorf("start shell process: %w", err)
	}

	if commandID == "" {
		commandID = fmt.Sprintf("cmd-%d", e.nextID.Add(1))
	}
	active := &activeCommand{
		id:      commandID,
		command: command,
		args:    append([]string{}, args...),
		cwd:     cwd,
		status:  "running",
		process: proc,
		cancel:  cancel,
		meta:    meta,
	}
	if beforeStart != nil {
		if err := beforeStart(); err != nil {
			_ = proc.Kill()
			cancel()
			return "", err
		}
	}
	if err := e.recordStarted(ctx, active); err != nil {
		_ = proc.Kill()
		cancel()
		return "", err
	}
	e.mu.Lock()
	e.commands[commandID] = active
	e.mu.Unlock()

	go e.captureOutput(active, "stdout", stdoutPipe)
	go e.captureOutput(active, "stderr", stderrPipe)
	go e.waitForCommand(active)

	return jsonText(map[string]any{
		"status":      "running",
		"tool":        toolName,
		"command_id":  commandID,
		"command":     command,
		"args":        args,
		"cwd":         cwd,
		"next_offset": 0,
	}), nil
}

func (e *Executor) executePoll(ctx context.Context, argsMap map[string]any) (string, error) {
	commandID, err := stringArg(argsMap, "command_id")
	if err != nil {
		return "", err
	}
	afterOffset, err := optionalIntArg(argsMap, "after_offset")
	if err != nil {
		return "", err
	}
	active, err := e.lookupCommand(commandID)
	if err != nil {
		return "", err
	}
	active.mu.RLock()
	chunks := make([]commandChunk, 0)
	for _, chunk := range active.chunks {
		if chunk.Offset > afterOffset {
			chunks = append(chunks, chunk)
		}
	}
	status := active.status
	nextOffset := active.nextOffset
	exitCode := active.exitCode
	errorText := active.errorText
	active.mu.RUnlock()

	payload := map[string]any{
		"tool":        "shell_poll",
		"command_id":  commandID,
		"status":      status,
		"chunks":      chunks,
		"next_offset": nextOffset,
	}
	if exitCode != nil {
		payload["exit_code"] = *exitCode
	}
	if errorText != "" {
		payload["error"] = errorText
	}
	if err := e.recordPollEvents(ctx, active); err != nil {
		return "", err
	}
	return jsonText(payload), nil
}

func (e *Executor) executeKill(ctx context.Context, argsMap map[string]any) (string, error) {
	commandID, err := stringArg(argsMap, "command_id")
	if err != nil {
		return "", err
	}
	active, err := e.lookupCommand(commandID)
	if err != nil {
		return "", err
	}
	active.mu.Lock()
	defer active.mu.Unlock()
	if active.status != "running" {
		return jsonText(map[string]any{
			"tool":       "shell_kill",
			"command_id": commandID,
			"status":     active.status,
		}), nil
	}
	active.killRequested = true
	if err := active.process.Kill(); err != nil {
		return "", fmt.Errorf("kill shell command: %w", err)
	}
	if err := e.recordKillRequested(ctx, active); err != nil {
		return "", err
	}
	return jsonText(map[string]any{
		"tool":       "shell_kill",
		"command_id": commandID,
		"status":     "killing",
	}), nil
}

func (e *Executor) PendingApprovals(sessionID string) []PendingApprovalView {
	e.mu.RLock()
	defer e.mu.RUnlock()
	out := make([]PendingApprovalView, 0, len(e.approvals))
	for _, approval := range e.approvals {
		if sessionID != "" && approval.SessionID != sessionID {
			continue
		}
		out = append(out, approval.PendingApprovalView)
	}
	return out
}

func (e *Executor) ActiveCommands(sessionID string) []ActiveCommandView {
	e.mu.RLock()
	defer e.mu.RUnlock()
	out := make([]ActiveCommandView, 0, len(e.commands))
	for _, active := range e.commands {
		active.mu.RLock()
		switch active.status {
		case "running", "killing":
		default:
			active.mu.RUnlock()
			continue
		}
		if sessionID != "" && active.meta.SessionID != sessionID {
			active.mu.RUnlock()
			continue
		}
		lastChunk := ""
		if n := len(active.chunks); n > 0 {
			lastChunk = active.chunks[n-1].Text
		}
		out = append(out, ActiveCommandView{
			CommandID:   active.id,
			SessionID:   active.meta.SessionID,
			RunID:       active.meta.RunID,
			Command:     active.command,
			Args:        append([]string{}, active.args...),
			Cwd:         active.cwd,
			Status:      active.status,
			NextOffset:  active.nextOffset,
			LastChunk:   lastChunk,
			KillPending: active.killRequested,
			Error:       active.errorText,
		})
		active.mu.RUnlock()
	}
	sort.Slice(out, func(i, j int) bool {
		return out[i].CommandID < out[j].CommandID
	})
	return out
}

func (e *Executor) RecoverApproval(contract contracts.ShellExecutionContract, view PendingApprovalView, meta ExecutionMeta) error {
	if e == nil {
		return fmt.Errorf("shell executor is nil")
	}
	if strings.TrimSpace(view.ApprovalID) == "" {
		return fmt.Errorf("approval id is empty")
	}
	invocation, err := e.resolveInvocation(contract.Runtime, view.Command, view.Args)
	if err != nil {
		return err
	}
	e.mu.Lock()
	defer e.mu.Unlock()
	if _, ok := e.approvals[view.ApprovalID]; ok {
		return nil
	}
	e.approvals[view.ApprovalID] = &pendingApproval{
		PendingApprovalView: PendingApprovalView{
			ApprovalID: view.ApprovalID,
			CommandID:  view.CommandID,
			SessionID:  view.SessionID,
			RunID:      view.RunID,
			ToolName:   firstNonEmpty(view.ToolName, "shell_start"),
			Command:    view.Command,
			Args:       append([]string{}, view.Args...),
			Cwd:        view.Cwd,
			Message:    view.Message,
		},
		contract:   contract,
		invocation: invocation,
		meta:       meta,
	}
	return nil
}

func (e *Executor) lookupCompletedCommand(commandID string) (*activeCommand, bool) {
	e.mu.RLock()
	defer e.mu.RUnlock()
	command, ok := e.completed[commandID]
	return command, ok
}

func (e *Executor) Approve(ctx context.Context, approvalID string) (string, error) {
	e.mu.RLock()
	approval, ok := e.approvals[approvalID]
	if !ok {
		e.mu.RUnlock()
		return "", fmt.Errorf("shell approval %q not found", approvalID)
	}
	e.mu.RUnlock()
	out, err := e.startCommand(ctx, approval.contract, approval.meta, approval.CommandID, approval.Command, approval.Args, approval.Cwd, approval.invocation, approval.ToolName, func() error {
		return e.recordApprovalGranted(ctx, approval)
	})
	if err != nil {
		return "", err
	}
	e.mu.Lock()
	delete(e.approvals, approvalID)
	e.mu.Unlock()
	return out, nil
}

func (e *Executor) Deny(ctx context.Context, approvalID string) error {
	e.mu.Lock()
	approval, ok := e.approvals[approvalID]
	if !ok {
		e.mu.Unlock()
		return fmt.Errorf("shell approval %q not found", approvalID)
	}
	delete(e.approvals, approvalID)
	e.mu.Unlock()
	return e.recordApprovalDenied(ctx, approval, "shell command denied by operator")
}

func (e *Executor) waitForCommand(active *activeCommand) {
	if active.cancel != nil {
		defer active.cancel()
	}
	err := active.process.Wait()
	active.mu.Lock()

	var exitCode *int
	var errorText string
	switch {
	case err == nil:
		code := 0
		exitCode = &code
		if active.killRequested {
			active.status = "killed"
		} else {
			active.status = "completed"
		}
	case isExitError(err):
		code := exitCodeOf(err)
		exitCode = &code
		if active.killRequested {
			active.status = "killed"
		} else {
			active.status = "failed"
		}
	case active.killRequested:
		active.status = "killed"
		errorText = err.Error()
	default:
		active.status = "failed"
		errorText = err.Error()
	}
	active.exitCode = exitCode
	active.errorText = errorText
	active.completedAt = time.Now().UTC()
	active.mu.Unlock()
	_ = e.recordCompleted(context.Background(), active)
	e.archiveCompletedCommand(active)
}

func (e *Executor) archiveCompletedCommand(active *activeCommand) {
	e.mu.Lock()
	defer e.mu.Unlock()
	delete(e.commands, active.id)
	if e.completed == nil {
		e.completed = map[string]*activeCommand{}
	}
	e.completed[active.id] = active
	e.reapCompletedLocked(time.Now().UTC())
}

func (e *Executor) reapCompletedLocked(now time.Time) {
	for id, command := range e.completed {
		if !command.completedAt.IsZero() && now.Sub(command.completedAt) > completedCommandRetention {
			delete(e.completed, id)
		}
	}
	if len(e.completed) <= maxCompletedCommands {
		return
	}
	type completedEntry struct {
		id          string
		completedAt time.Time
	}
	entries := make([]completedEntry, 0, len(e.completed))
	for id, command := range e.completed {
		entries = append(entries, completedEntry{id: id, completedAt: command.completedAt})
	}
	sort.Slice(entries, func(i, j int) bool {
		return entries[i].completedAt.Before(entries[j].completedAt)
	})
	for len(e.completed) > maxCompletedCommands && len(entries) > 0 {
		delete(e.completed, entries[0].id)
		entries = entries[1:]
	}
}

func (e *Executor) captureOutput(active *activeCommand, stream string, reader io.ReadCloser) {
	defer reader.Close()
	scanner := bufio.NewScanner(reader)
	buffer := make([]byte, 0, 64*1024)
	scanner.Buffer(buffer, 1024*1024)
	for scanner.Scan() {
		e.appendChunk(active, stream, scanner.Text())
	}
	if err := scanner.Err(); err != nil {
		e.appendChunk(active, stream, "scanner error: "+err.Error())
	}
}

func (e *Executor) appendChunk(active *activeCommand, stream, text string) {
	active.mu.Lock()
	active.nextOffset++
	offset := active.nextOffset
	active.chunks = append(active.chunks, commandChunk{
		Offset: offset,
		Stream: stream,
		Text:   text,
	})
	active.recordedOffset = offset
	active.mu.Unlock()
	_ = e.recordChunk(context.Background(), active, commandChunk{
		Offset: offset,
		Stream: stream,
		Text:   text,
	})
}

func (e *Executor) recordStarted(ctx context.Context, active *activeCommand) error {
	if active.meta.RecordEvent == nil {
		return nil
	}
	return e.emitEvent(ctx, active.meta, eventing.Event{
		ID:            metaID(active.meta, "evt-shell-command-started"),
		Kind:          eventing.EventShellCommandStarted,
		OccurredAt:    metaNow(active.meta),
		AggregateID:   active.id,
		AggregateType: eventing.AggregateShellCommand,
		Source:        active.meta.Source,
		ActorID:       active.meta.ActorID,
		ActorType:     firstNonEmpty(active.meta.ActorType, "agent"),
		TraceSummary:  "shell command started",
		Payload: map[string]any{
			"session_id": active.meta.SessionID,
			"run_id":     active.meta.RunID,
			"command_id": active.id,
			"command":    active.command,
			"args":       append([]string{}, active.args...),
			"cwd":        active.cwd,
			"status":     "running",
		},
	})
}

func (e *Executor) recordApprovalRequested(ctx context.Context, approval *pendingApproval) error {
	if approval.meta.RecordEvent == nil {
		return nil
	}
	return e.emitEvent(ctx, approval.meta, eventing.Event{
		ID:            metaID(approval.meta, "evt-shell-command-approval-requested"),
		Kind:          eventing.EventShellCommandApprovalRequested,
		OccurredAt:    metaNow(approval.meta),
		AggregateID:   approval.CommandID,
		AggregateType: eventing.AggregateShellCommand,
		Source:        approval.meta.Source,
		ActorID:       approval.meta.ActorID,
		ActorType:     firstNonEmpty(approval.meta.ActorType, "agent"),
		TraceSummary:  "shell command approval requested",
		Payload: map[string]any{
			"session_id":       approval.SessionID,
			"run_id":           approval.RunID,
			"approval_id":      approval.ApprovalID,
			"tool_name":        approval.ToolName,
			"command":          approval.Command,
			"args":             append([]string{}, approval.Args...),
			"cwd":              approval.Cwd,
			"approval_message": approval.Message,
		},
	})
}

func (e *Executor) recordApprovalGranted(ctx context.Context, approval *pendingApproval) error {
	if approval.meta.RecordEvent == nil {
		return nil
	}
	return e.emitEvent(ctx, approval.meta, eventing.Event{
		ID:            metaID(approval.meta, "evt-shell-command-approval-granted"),
		Kind:          eventing.EventShellCommandApprovalGranted,
		OccurredAt:    metaNow(approval.meta),
		AggregateID:   approval.CommandID,
		AggregateType: eventing.AggregateShellCommand,
		Source:        approval.meta.Source,
		ActorID:       approval.meta.ActorID,
		ActorType:     "operator",
		TraceSummary:  "shell command approval granted",
		Payload: map[string]any{
			"session_id":  approval.SessionID,
			"run_id":      approval.RunID,
			"approval_id": approval.ApprovalID,
		},
	})
}

func (e *Executor) recordApprovalDenied(ctx context.Context, approval *pendingApproval, reason string) error {
	if approval.meta.RecordEvent == nil {
		return nil
	}
	return e.emitEvent(ctx, approval.meta, eventing.Event{
		ID:            metaID(approval.meta, "evt-shell-command-approval-denied"),
		Kind:          eventing.EventShellCommandApprovalDenied,
		OccurredAt:    metaNow(approval.meta),
		AggregateID:   approval.CommandID,
		AggregateType: eventing.AggregateShellCommand,
		Source:        approval.meta.Source,
		ActorID:       approval.meta.ActorID,
		ActorType:     "operator",
		TraceSummary:  "shell command approval denied",
		Payload: map[string]any{
			"session_id":  approval.SessionID,
			"run_id":      approval.RunID,
			"approval_id": approval.ApprovalID,
			"reason":      reason,
		},
	})
}

func (e *Executor) recordPollEvents(ctx context.Context, active *activeCommand) error {
	return nil
}

func (e *Executor) recordChunk(ctx context.Context, active *activeCommand, chunk commandChunk) error {
	if active.meta.RecordEvent == nil {
		return nil
	}
	return e.emitEvent(ctx, active.meta, eventing.Event{
		ID:            metaID(active.meta, "evt-shell-command-chunk"),
		Kind:          eventing.EventShellCommandOutputChunk,
		OccurredAt:    metaNow(active.meta),
		AggregateID:   active.id,
		AggregateType: eventing.AggregateShellCommand,
		Source:        active.meta.Source,
		ActorID:       active.meta.ActorID,
		ActorType:     firstNonEmpty(active.meta.ActorType, "agent"),
		TraceSummary:  "shell command output chunk",
		Payload: map[string]any{
			"session_id": active.meta.SessionID,
			"run_id":     active.meta.RunID,
			"command_id": active.id,
			"offset":     chunk.Offset,
			"stream":     chunk.Stream,
			"text":       chunk.Text,
		},
	})
}

func (e *Executor) recordCompleted(ctx context.Context, active *activeCommand) error {
	if active.meta.RecordEvent == nil {
		return nil
	}
	active.mu.Lock()
	if active.terminalRecorded {
		active.mu.Unlock()
		return nil
	}
	payload := map[string]any{
		"session_id": active.meta.SessionID,
		"run_id":     active.meta.RunID,
		"command_id": active.id,
		"status":     active.status,
	}
	if active.exitCode != nil {
		payload["exit_code"] = *active.exitCode
	}
	if active.errorText != "" {
		payload["error"] = active.errorText
	}
	active.terminalRecorded = true
	active.mu.Unlock()
	return e.emitEvent(ctx, active.meta, eventing.Event{
		ID:            metaID(active.meta, "evt-shell-command-completed"),
		Kind:          eventing.EventShellCommandCompleted,
		OccurredAt:    metaNow(active.meta),
		AggregateID:   active.id,
		AggregateType: eventing.AggregateShellCommand,
		Source:        active.meta.Source,
		ActorID:       active.meta.ActorID,
		ActorType:     firstNonEmpty(active.meta.ActorType, "agent"),
		TraceSummary:  "shell command completed",
		Payload:       payload,
	})
}

func (e *Executor) recordKillRequested(ctx context.Context, active *activeCommand) error {
	if active.meta.RecordEvent == nil {
		return nil
	}
	return e.emitEvent(ctx, active.meta, eventing.Event{
		ID:            metaID(active.meta, "evt-shell-command-kill-requested"),
		Kind:          eventing.EventShellCommandKillRequested,
		OccurredAt:    metaNow(active.meta),
		AggregateID:   active.id,
		AggregateType: eventing.AggregateShellCommand,
		Source:        active.meta.Source,
		ActorID:       active.meta.ActorID,
		ActorType:     firstNonEmpty(active.meta.ActorType, "agent"),
		TraceSummary:  "shell command kill requested",
		Payload: map[string]any{
			"session_id": active.meta.SessionID,
			"run_id":     active.meta.RunID,
			"command_id": active.id,
			"status":     "killing",
		},
	})
}

func (e *Executor) lookupCommand(commandID string) (*activeCommand, error) {
	e.mu.RLock()
	active, ok := e.commands[commandID]
	e.mu.RUnlock()
	if ok {
		return active, nil
	}
	if completed, ok := e.lookupCompletedCommand(commandID); ok {
		return completed, nil
	}
	return nil, fmt.Errorf("shell command %q not found", commandID)
}

func (e *Executor) resolveInvocation(policy contracts.ShellRuntimePolicy, command string, args []string) (invocation, error) {
	if e.platform() == "windows" {
		if builtin, ok := windowsBuiltinInvocation(command, args); ok {
			return builtin, nil
		}
	}
	if !policy.Enabled || policy.Params.AllowNetwork {
		return invocation{executable: command, args: args}, nil
	}
	if e.platform() != "linux" {
		return invocation{}, fmt.Errorf("shell network isolation requires linux; current platform is %s", e.platform())
	}
	lookup := e.lookupPath
	if lookup == nil {
		lookup = exec.LookPath
	}
	launcher, err := lookup("unshare")
	if err != nil {
		return invocation{}, fmt.Errorf("shell network isolation requires linux unshare launcher: %w", err)
	}
	argv := []string{"--fork", "--kill-child", "--net", "--", command}
	argv = append(argv, args...)
	return invocation{
		executable: launcher,
		args:       argv,
		isolated:   true,
	}, nil
}

func (e *Executor) platform() string {
	if e != nil && e.goos != "" {
		return e.goos
	}
	return runtime.GOOS
}

func windowsBuiltinInvocation(command string, args []string) (invocation, bool) {
	switch strings.ToLower(command) {
	case "echo", "dir", "type":
		argv := append([]string{"/C", command}, args...)
		return invocation{
			executable: "cmd",
			args:       argv,
		}, true
	default:
		return invocation{}, false
	}
}

func defaultRunCommand(ctx context.Context, cwd, executable string, args []string) (runResult, error) {
	cmd := exec.CommandContext(ctx, executable, args...)
	cmd.Dir = cwd
	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	err := cmd.Run()
	if err != nil {
		if exitErr, ok := err.(*exec.ExitError); ok {
			return runResult{
				stdout:   stdout.String(),
				stderr:   stderr.String(),
				exitCode: exitErr.ExitCode(),
			}, nil
		}
		return runResult{}, err
	}
	return runResult{
		stdout:   stdout.String(),
		stderr:   stderr.String(),
		exitCode: 0,
	}, nil
}

func defaultStartCommand(ctx context.Context, cwd, executable string, args []string) (processHandle, error) {
	cmd := exec.CommandContext(ctx, executable, args...)
	cmd.Dir = cwd
	return &execProcess{cmd: cmd}, nil
}

type execProcess struct {
	cmd *exec.Cmd
}

func (p *execProcess) StdoutPipe() (io.ReadCloser, error) { return p.cmd.StdoutPipe() }
func (p *execProcess) StderrPipe() (io.ReadCloser, error) { return p.cmd.StderrPipe() }
func (p *execProcess) Start() error                       { return p.cmd.Start() }
func (p *execProcess) Wait() error                        { return p.cmd.Wait() }
func (p *execProcess) Kill() error {
	if p.cmd.Process == nil {
		return fmt.Errorf("process is not running")
	}
	return p.cmd.Process.Kill()
}

func validateCommand(policy contracts.ShellCommandPolicy, command string, args []string) error {
	if policy.Enabled {
		switch policy.Strategy {
		case "deny_all":
			return fmt.Errorf("shell commands are denied by policy")
		case "static_allowlist":
			allowed := len(policy.Params.AllowedCommands) == 0
			for _, candidate := range policy.Params.AllowedCommands {
				if candidate == command {
					allowed = true
					break
				}
			}
			if !allowed {
				return fmt.Errorf("shell command %q is not in allowlist", command)
			}
			full := strings.TrimSpace(strings.Join(append([]string{command}, args...), " "))
			for _, pattern := range policy.Params.DenyPatterns {
				if pattern != "" && strings.Contains(full, pattern) {
					return fmt.Errorf("shell command matches denied pattern")
				}
			}
			if len(policy.Params.AllowedPrefixes) > 0 {
				prefixAllowed := false
				for _, prefix := range policy.Params.AllowedPrefixes {
					if strings.HasPrefix(full, prefix) {
						prefixAllowed = true
						break
					}
				}
				if !prefixAllowed {
					return fmt.Errorf("shell command %q does not match allowed prefixes", full)
				}
			}
			if err := validateCommandRules(policy.Params.CommandRules, command, args); err != nil {
				return err
			}
		default:
			return fmt.Errorf("unsupported shell command strategy %q", policy.Strategy)
		}
	}
	return nil
}

func validateCommandRules(rules []contracts.ShellCommandRule, command string, args []string) error {
	if len(rules) == 0 {
		return nil
	}
	argLine := strings.TrimSpace(strings.Join(args, " "))
	matched := false
	for _, rule := range rules {
		if rule.Command != "" && rule.Command != command {
			continue
		}
		matched = true
		for _, pattern := range rule.DeniedArgPatterns {
			if pattern != "" && strings.Contains(argLine, pattern) {
				return fmt.Errorf("shell command arguments for %q match denied pattern", command)
			}
		}
		for _, prefix := range rule.DeniedArgPrefixes {
			if prefix != "" && strings.HasPrefix(argLine, prefix) {
				return fmt.Errorf("shell command arguments for %q match denied prefix", command)
			}
		}
		if len(rule.AllowedArgPatterns) == 0 && len(rule.AllowedArgPrefixes) == 0 {
			continue
		}
		allowed := false
		for _, pattern := range rule.AllowedArgPatterns {
			if pattern != "" && strings.Contains(argLine, pattern) {
				allowed = true
				break
			}
		}
		if !allowed {
			for _, prefix := range rule.AllowedArgPrefixes {
				if prefix == "" {
					continue
				}
				if argLine == prefix || strings.HasPrefix(argLine, prefix) {
					allowed = true
					break
				}
			}
		}
		if !allowed {
			return fmt.Errorf("shell command arguments for %q do not match allowed rule set", command)
		}
	}
	if matched {
		return nil
	}
	return nil
}

func resolveCwd(policy contracts.ShellRuntimePolicy, args map[string]any) (string, error) {
	base := policy.Params.Cwd
	if base == "" {
		base = "."
	}
	baseAbs, err := filepath.Abs(base)
	if err != nil {
		return "", fmt.Errorf("resolve shell base cwd: %w", err)
	}
	requested := baseAbs
	if raw, ok := args["cwd"]; ok {
		text, ok := raw.(string)
		if !ok || text == "" {
			return "", fmt.Errorf("argument %q must be a non-empty string", "cwd")
		}
		if filepath.IsAbs(text) {
			requested = filepath.Clean(text)
		} else {
			requested = filepath.Clean(filepath.Join(baseAbs, text))
		}
	}
	rel, err := filepath.Rel(baseAbs, requested)
	if err != nil {
		return "", fmt.Errorf("resolve shell cwd: %w", err)
	}
	if rel == ".." || strings.HasPrefix(rel, ".."+string(filepath.Separator)) {
		return "", fmt.Errorf("shell cwd escapes runtime scope")
	}
	return requested, nil
}

func parseTimeout(policy contracts.ShellRuntimePolicy) (time.Duration, error) {
	timeout := 30 * time.Second
	if policy.Params.Timeout == "" {
		return timeout, nil
	}
	parsed, err := time.ParseDuration(policy.Params.Timeout)
	if err != nil {
		return 0, fmt.Errorf("parse shell timeout: %w", err)
	}
	return parsed, nil
}

func stringArg(args map[string]any, key string) (string, error) {
	value, ok := args[key]
	if !ok {
		return "", fmt.Errorf("missing required argument %q", key)
	}
	text, ok := value.(string)
	if !ok || text == "" {
		return "", fmt.Errorf("argument %q must be a non-empty string", key)
	}
	return text, nil
}

func optionalStringSlice(args map[string]any, key string) ([]string, error) {
	value, ok := args[key]
	if !ok || value == nil {
		return nil, nil
	}
	items, ok := value.([]any)
	if !ok {
		if typed, ok := value.([]string); ok {
			return typed, nil
		}
		return nil, fmt.Errorf("argument %q must be an array of strings", key)
	}
	out := make([]string, 0, len(items))
	for _, item := range items {
		text, ok := item.(string)
		if !ok {
			return nil, fmt.Errorf("argument %q must be an array of strings", key)
		}
		out = append(out, text)
	}
	return out, nil
}

func optionalIntArg(args map[string]any, key string) (int, error) {
	value, ok := args[key]
	if !ok || value == nil {
		return 0, nil
	}
	switch typed := value.(type) {
	case int:
		return typed, nil
	case float64:
		return int(typed), nil
	default:
		return 0, fmt.Errorf("argument %q must be an integer", key)
	}
}

func jsonText(value any) string {
	data, _ := json.Marshal(value)
	return string(data)
}

func (e *Executor) evaluateApproval(policy contracts.ShellApprovalPolicy, command string, args []string) (approvalDecision, string) {
	if !policy.Enabled {
		return approvalDecisionAllow, ""
	}
	full := commandPrefix(command, args)
	for _, prefix := range policy.Params.DenyPrefixes {
		prefix = strings.TrimSpace(prefix)
		if prefix != "" && strings.HasPrefix(full, prefix) {
			return approvalDecisionDeny, "shell command denied by persistent policy: " + full
		}
	}
	for _, prefix := range policy.Params.AllowPrefixes {
		prefix = strings.TrimSpace(prefix)
		if prefix != "" && strings.HasPrefix(full, prefix) {
			return approvalDecisionAllow, ""
		}
	}
	switch policy.Strategy {
	case "always_allow":
		return approvalDecisionAllow, ""
	case "always_require":
		return approvalDecisionRequire, approvalMessage(policy, full)
	case "require_for_patterns":
		for _, pattern := range policy.Params.Patterns {
			if pattern != "" && strings.Contains(full, pattern) {
				return approvalDecisionRequire, approvalMessage(policy, full)
			}
		}
		return approvalDecisionAllow, ""
	default:
		return approvalDecisionAllow, ""
	}
}

func approvalMessage(policy contracts.ShellApprovalPolicy, full string) string {
	if strings.TrimSpace(policy.Params.ApprovalMessageTemplate) != "" {
		return strings.ReplaceAll(policy.Params.ApprovalMessageTemplate, "{{command}}", full)
	}
	return "shell command requires operator approval: " + full
}

func commandPrefix(command string, args []string) string {
	return strings.TrimSpace(strings.Join(append([]string{command}, args...), " "))
}

func (e *Executor) queueApproval(ctx context.Context, toolName string, contract contracts.ShellExecutionContract, meta ExecutionMeta, command string, args []string, cwd string, invocation invocation, message string) (string, error) {
	commandID := fmt.Sprintf("cmd-%d", e.nextID.Add(1))
	approvalID := fmt.Sprintf("approval-%d", e.nextID.Add(1))
	approval := &pendingApproval{
		PendingApprovalView: PendingApprovalView{
			ApprovalID: approvalID,
			CommandID:  commandID,
			SessionID:  meta.SessionID,
			RunID:      meta.RunID,
			ToolName:   toolName,
			Command:    command,
			Args:       append([]string{}, args...),
			Cwd:        cwd,
			Message:    message,
		},
		contract:   contract,
		invocation: invocation,
		meta:       meta,
	}
	e.mu.Lock()
	e.approvals[approvalID] = approval
	e.mu.Unlock()
	if err := e.recordApprovalRequested(ctx, approval); err != nil {
		return "", err
	}
	return jsonText(map[string]any{
		"status":      "approval_pending",
		"tool":        toolName,
		"approval_id": approvalID,
		"command_id":  commandID,
		"command":     command,
		"args":        args,
		"cwd":         cwd,
		"message":     message,
	}), nil
}

func isExitError(err error) bool {
	var exitErr *exec.ExitError
	return errors.As(err, &exitErr)
}

func exitCodeOf(err error) int {
	var exitErr *exec.ExitError
	if !errors.As(err, &exitErr) {
		return 1
	}
	if status, ok := exitErr.Sys().(syscall.WaitStatus); ok {
		return status.ExitStatus()
	}
	return exitErr.ExitCode()
}

func metaNow(meta ExecutionMeta) time.Time {
	if meta.Now != nil {
		return meta.Now().UTC()
	}
	return time.Now().UTC()
}

func metaID(meta ExecutionMeta, prefix string) string {
	if meta.NewID != nil {
		return meta.NewID(prefix)
	}
	return fmt.Sprintf("%s-%d", prefix, time.Now().UTC().UnixNano())
}

func firstNonEmpty(values ...string) string {
	for _, value := range values {
		if value != "" {
			return value
		}
	}
	return ""
}

func (e *Executor) emitEvent(ctx context.Context, meta ExecutionMeta, event eventing.Event) error {
	if meta.RecordEvent == nil {
		return nil
	}
	e.eventMu.Lock()
	defer e.eventMu.Unlock()
	return meta.RecordEvent(ctx, event)
}
