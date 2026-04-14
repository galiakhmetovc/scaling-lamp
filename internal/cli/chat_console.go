package cli

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"strings"
	"sync"
	"time"

	"github.com/chzyer/readline"
	"github.com/mattn/go-isatty"

	"teamd/internal/api"
	"teamd/internal/runtime"
)

type ChatRunner interface {
	StartRun(chatID int64, sessionID, query string) (api.CreateRunResponse, error)
	RunStatus(runID string) (api.RunStatusResponse, error)
	Events(req api.EventListRequest) (api.EventListResponse, error)
	StreamEvents(ctx context.Context, req api.EventListRequest, onEvent func(runtime.RuntimeEvent) error) error
	CancelRun(runID string) error
	Approvals(sessionID string) ([]api.ApprovalRecordResponse, error)
	ControlState(sessionID string, chatID int64) (api.ControlStateResponse, error)
	Approve(id string) (api.ApprovalRecordResponse, error)
	Reject(id string) (api.ApprovalRecordResponse, error)
	Plan(planID string) (api.PlanResponse, error)
	Plans(ownerType, ownerID string, limit int) (api.PlanListResponse, error)
	WorkerHandoff(workerID string) (api.WorkerHandoffResponse, error)
	Artifact(ref string) (api.ArtifactResponse, error)
	ArtifactContent(ref string) (api.ArtifactContentResponse, error)
}

type ChatConsole struct {
	client         ChatRunner
	in             io.Reader
	out            io.Writer
	knownPlans     []string
	knownApprovals []string
	knownWorkers   []string
	knownArtifacts []string

	mu               sync.Mutex
	lastRunID        string
	sessionID        string
	activeRunID      string
	activeDone       chan struct{}
	activeTerminal   bool
	activeAssistant  bool
}

func NewChatConsole(client ChatRunner, in io.Reader, out io.Writer) *ChatConsole {
	return &ChatConsole{client: client, in: in, out: out}
}

func (c *ChatConsole) Run(ctx context.Context, chatID int64, sessionID string) error {
	if chatID == 0 || strings.TrimSpace(sessionID) == "" {
		return fmt.Errorf("chat_id and session_id are required")
	}
	c.mu.Lock()
	c.sessionID = sessionID
	c.mu.Unlock()
	if inFile, outFile, ok := terminalFiles(c.in, c.out); ok {
		return c.runInteractive(ctx, chatID, sessionID, inFile, outFile)
	}
	return c.runScanner(ctx, chatID, sessionID)
}

func (c *ChatConsole) runScanner(ctx context.Context, chatID int64, sessionID string) error {
	scanner := bufio.NewScanner(c.in)
	for scanner.Scan() {
		if done, err := c.handleInputLine(ctx, chatID, sessionID, scanner.Text()); done || err != nil {
			if done {
				c.waitForActiveRun()
			}
			return err
		}
	}
	c.waitForActiveRun()
	return scanner.Err()
}

func (c *ChatConsole) runInteractive(ctx context.Context, chatID int64, sessionID string, inFile *os.File, outFile *os.File) error {
	rl, err := readline.NewEx(&readline.Config{
		Prompt:       "you> ",
		Stdin:        inFile,
		Stdout:       outFile,
		Stderr:       outFile,
		HistoryLimit: 200,
		AutoComplete: readline.SegmentFunc(func(segments [][]rune, _ int) [][]rune {
			line := joinSegments(segments)
			cands := c.completionCandidates(line)
			out := make([][]rune, 0, len(cands))
			for _, cand := range cands {
				out = append(out, []rune(cand))
			}
			return out
		}),
	})
	if err != nil {
		return err
	}
	defer rl.Close()
	c.out = rl.Stdout()
	for {
		line, err := rl.Readline()
		if err == io.EOF {
			return nil
		}
		if err == readline.ErrInterrupt {
			continue
		}
		if err != nil {
			return err
		}
		if done, err := c.handleInputLine(ctx, chatID, sessionID, line); done || err != nil {
			if done {
				c.waitForActiveRun()
			}
			return err
		}
	}
}

func (c *ChatConsole) handleInputLine(ctx context.Context, chatID int64, sessionID, raw string) (bool, error) {
	line := strings.TrimSpace(raw)
	if line == "" {
		return false, nil
	}
	if line == "/quit" || line == "quit" || line == "exit" {
		return true, nil
	}
	if strings.HasPrefix(line, "/") {
		return false, c.handleCommand(line)
	}
	if active := c.currentActiveRunID(); strings.TrimSpace(active) != "" {
		c.printf("system: run %s still active; use /status, /approve, /reject, /cancel\n", active)
		return false, nil
	}
	c.printf("you: %s\n", line)
	started, err := c.startRunWithRetry(chatID, sessionID, line)
	if err != nil {
		return false, err
	}
	afterID := c.latestSessionEventID(sessionID)
	c.beginActiveRun(started.RunID, sessionID)
	go c.monitorRun(context.WithoutCancel(ctx), started.RunID, sessionID, afterID)
	return false, nil
}

func (c *ChatConsole) startRunWithRetry(chatID int64, sessionID, line string) (api.CreateRunResponse, error) {
	var lastErr error
	for attempt := 0; attempt < 20; attempt++ {
		started, err := c.client.StartRun(chatID, sessionID, line)
		if err == nil {
			return started, nil
		}
		lastErr = err
		if !strings.HasPrefix(err.Error(), "run_busy:") {
			return api.CreateRunResponse{}, err
		}
		time.Sleep(50 * time.Millisecond)
	}
	return api.CreateRunResponse{}, lastErr
}

func (c *ChatConsole) renderEvent(item runtime.RuntimeEvent) {
	switch item.Kind {
	case "run.started":
		c.printf("system: run started\n")
	case "run.completed":
		c.printf("system: run completed\n")
	case "assistant.final":
		payload := decodeEventPayload(item.Payload)
		text := strings.TrimSpace(stringValue(payload["text"]))
		if text != "" {
			c.printf("assistant: %s\n", text)
		}
	case "artifact.offloaded":
		payload := decodeEventPayload(item.Payload)
		ref := stringValue(payload["artifact_ref"])
		c.printf("memory: artifact offloaded %s\n", ref)
		if ref != "" && !containsString(c.knownArtifacts, ref) {
			c.knownArtifacts = append(c.knownArtifacts, ref)
		}
	case "job.created", "job.started", "job.completed", "job.failed", "job.cancelled":
		c.printf("job: %s %s\n", item.EntityID, strings.TrimPrefix(item.Kind, "job."))
	case "worker.spawned", "worker.run_started", "worker.run_completed", "worker.handoff_created":
		c.printf("worker: %s %s\n", item.EntityID, strings.TrimPrefix(item.Kind, "worker."))
		if item.EntityID != "" && !containsString(c.knownWorkers, item.EntityID) {
			c.knownWorkers = append(c.knownWorkers, item.EntityID)
		}
	case "plan.created", "plan.updated", "plan.item_started", "plan.item_completed":
		c.printf("plan: %s %s\n", item.EntityID, strings.TrimPrefix(item.Kind, "plan."))
		if !containsString(c.knownPlans, item.EntityID) {
			c.knownPlans = append(c.knownPlans, item.EntityID)
		}
	case "approval.requested":
		payload := decodeEventPayload(item.Payload)
		approvalID := stringValue(payload["approval_id"])
		tool := stringValue(payload["tool"])
		if approvalID != "" && !containsString(c.knownApprovals, approvalID) {
			c.knownApprovals = append(c.knownApprovals, approvalID)
		}
		if tool != "" {
			c.printf("approval: requested %s for %s\n", approvalID, tool)
		} else {
			c.printf("approval: requested %s\n", approvalID)
		}
	case "worker.approval_requested":
		payload := decodeEventPayload(item.Payload)
		approvalID := stringValue(payload["approval_id"])
		tool := stringValue(payload["tool"])
		if approvalID != "" && !containsString(c.knownApprovals, approvalID) {
			c.knownApprovals = append(c.knownApprovals, approvalID)
		}
		if item.EntityID != "" && !containsString(c.knownWorkers, item.EntityID) {
			c.knownWorkers = append(c.knownWorkers, item.EntityID)
		}
		c.printf("worker: %s approval requested %s for %s\n", item.EntityID, approvalID, tool)
	case "run.cancelled":
		c.printf("system: run cancelled\n")
	case "run.failed":
		c.printf("system: run failed\n")
	default:
		c.printf("system: %s\n", item.Kind)
	}
}

func (c *ChatConsole) renderRunStatus(run runtime.RunView) {
	switch run.Status {
	case runtime.StatusCompleted:
		c.printf("system: run completed\n")
	case runtime.StatusCancelled:
		c.printf("system: run cancelled\n")
	case runtime.StatusFailed:
		c.printf("system: run failed\n")
	case runtime.StatusWaitingApproval:
		c.printf("system: run waiting approval\n")
	}
}

func (c *ChatConsole) renderFinalResponse(text string) {
	if text = strings.TrimSpace(text); text != "" {
		c.printf("assistant: %s\n", text)
	}
}

func (c *ChatConsole) handleCommand(line string) error {
	fields := strings.Fields(strings.TrimSpace(line))
	if len(fields) == 0 {
		return nil
	}
	switch fields[0] {
	case "/help":
		c.printf("system: commands /help /status /plan /plans /approve <id> /reject <id> /handoff <worker_id> /artifact <ref> /cancel /quit\n")
	case "/status":
		if strings.TrimSpace(c.sessionID) == "" {
			c.printf("system: no active run\n")
			return nil
		}
		control, err := c.client.ControlState(c.sessionID, 0)
		if err != nil {
			return err
		}
		c.renderControlState(control.Control)
	case "/approve":
		if len(fields) != 2 {
			c.printf("system: usage /approve <approval_id>\n")
			return nil
		}
		approvalID, err := c.resolveApprovalID(fields[1])
		if err != nil {
			c.printf("system: %v\n", err)
			return nil
		}
		item, err := c.client.Approve(approvalID)
		if err != nil {
			c.printf("system: %v\n", err)
			return nil
		}
		if item.ID != "" && !containsString(c.knownApprovals, item.ID) {
			c.knownApprovals = append(c.knownApprovals, item.ID)
		}
		c.printf("approval: %s %s\n", item.ID, item.Status)
	case "/reject":
		if len(fields) != 2 {
			c.printf("system: usage /reject <approval_id>\n")
			return nil
		}
		approvalID, err := c.resolveApprovalID(fields[1])
		if err != nil {
			c.printf("system: %v\n", err)
			return nil
		}
		item, err := c.client.Reject(approvalID)
		if err != nil {
			c.printf("system: %v\n", err)
			return nil
		}
		if item.ID != "" && !containsString(c.knownApprovals, item.ID) {
			c.knownApprovals = append(c.knownApprovals, item.ID)
		}
		c.printf("approval: %s %s\n", item.ID, item.Status)
	case "/plan", "/plans":
		runID := c.currentRunID()
		if strings.TrimSpace(runID) == "" {
			c.printf("system: no active run\n")
			return nil
		}
		plans, err := c.client.Plans("run", runID, 10)
		if err != nil {
			return err
		}
		if len(plans.Items) == 0 {
			c.printf("system: no plans\n")
			return nil
		}
		for _, plan := range plans.Items {
			c.printf("plan: %s\n", plan.Title)
			for _, item := range plan.Items {
				c.printf("- [%s] %s\n", item.Status, item.Content)
			}
		}
	case "/handoff":
		if len(fields) != 2 {
			c.printf("system: usage /handoff <worker_id>\n")
			return nil
		}
		handoff, err := c.client.WorkerHandoff(fields[1])
		if err != nil {
			return err
		}
		if handoff.Handoff.WorkerID != "" && !containsString(c.knownWorkers, handoff.Handoff.WorkerID) {
			c.knownWorkers = append(c.knownWorkers, handoff.Handoff.WorkerID)
		}
		for _, ref := range handoff.Handoff.Artifacts {
			if !containsString(c.knownArtifacts, ref) {
				c.knownArtifacts = append(c.knownArtifacts, ref)
			}
		}
		c.printf("worker: %s handoff %s\n", handoff.Handoff.WorkerID, handoff.Handoff.Summary)
	case "/artifact":
		if len(fields) != 2 {
			c.printf("system: usage /artifact <ref>\n")
			return nil
		}
		meta, err := c.client.Artifact(fields[1])
		if err != nil {
			return err
		}
		if meta.Artifact.Ref != "" && !containsString(c.knownArtifacts, meta.Artifact.Ref) {
			c.knownArtifacts = append(c.knownArtifacts, meta.Artifact.Ref)
		}
		content, err := c.client.ArtifactContent(fields[1])
		if err != nil {
			return err
		}
		c.printf("artifact: %s bytes=%d\n%s\n", meta.Artifact.Ref, meta.Artifact.SizeBytes, content.Content)
	case "/cancel":
		runID := c.currentRunID()
		if strings.TrimSpace(runID) == "" {
			c.printf("system: no active run\n")
			return nil
		}
		if err := c.client.CancelRun(runID); err != nil {
			return err
		}
		c.printf("system: cancel requested for %s\n", runID)
	default:
		c.printf("system: unsupported command %s\n", line)
	}
	return nil
}

func (c *ChatConsole) beginActiveRun(runID, sessionID string) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.lastRunID = runID
	c.sessionID = sessionID
	c.activeRunID = runID
	c.activeTerminal = false
	c.activeAssistant = false
	c.activeDone = make(chan struct{})
}

func (c *ChatConsole) finishActiveRun(runID string) {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.activeRunID != runID {
		return
	}
	done := c.activeDone
	c.activeRunID = ""
	c.activeDone = nil
	c.activeTerminal = false
	c.activeAssistant = false
	if done != nil {
		close(done)
	}
}

func (c *ChatConsole) currentRunID() string {
	c.mu.Lock()
	defer c.mu.Unlock()
	if strings.TrimSpace(c.activeRunID) != "" {
		return c.activeRunID
	}
	return c.lastRunID
}

func (c *ChatConsole) currentActiveRunID() string {
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.activeRunID
}

func (c *ChatConsole) waitForActiveRun() {
	c.mu.Lock()
	done := c.activeDone
	c.mu.Unlock()
	if done != nil {
		<-done
	}
}

func (c *ChatConsole) monitorRun(ctx context.Context, runID, sessionID string, afterID int64) {
	defer c.finishActiveRun(runID)
	streamErr := c.client.StreamEvents(ctx, api.EventListRequest{
		SessionID: sessionID,
		AfterID:   afterID,
		Limit:     50,
	}, func(item runtime.RuntimeEvent) error {
		c.renderEvent(item)
		if item.EntityType == "run" && item.EntityID == runID {
			switch item.Kind {
			case "assistant.final":
				c.mu.Lock()
				c.activeAssistant = true
				c.mu.Unlock()
			case "run.completed", "run.cancelled", "run.failed":
				c.mu.Lock()
				c.activeTerminal = true
				c.mu.Unlock()
				return ErrStopStream
			}
		}
		return nil
	})
	if streamErr != nil && streamErr != ErrStopStream && streamErr != context.Canceled {
		c.printf("system: event stream disconnected\n")
	}
	status, err := c.client.RunStatus(runID)
	if err != nil {
		c.printf("system: run status failed: %v\n", err)
		return
	}
	c.mu.Lock()
	assistantSeen := c.activeAssistant
	terminalSeen := c.activeTerminal
	c.mu.Unlock()
	if !assistantSeen {
		c.renderFinalResponse(status.Run.FinalResponse)
	}
	if !terminalSeen {
		c.renderRunStatus(status.Run)
	}
}

func (c *ChatConsole) printf(format string, args ...any) {
	c.mu.Lock()
	defer c.mu.Unlock()
	fmt.Fprintf(c.out, format, args...)
}

func (c *ChatConsole) latestSessionEventID(sessionID string) int64 {
	if strings.TrimSpace(sessionID) == "" {
		return 0
	}
	items, err := c.client.Events(api.EventListRequest{SessionID: sessionID, Limit: 1})
	if err != nil || len(items.Items) == 0 {
		return 0
	}
	return items.Items[len(items.Items)-1].ID
}

func (c *ChatConsole) renderControlState(control runtime.ControlState) {
	if control.Session.LatestRun != nil {
		c.printf("system: run %s %s\n", control.Session.LatestRun.RunID, control.Session.LatestRun.Status)
	} else {
		c.printf("system: no recorded run\n")
	}
	if len(control.Approvals) == 0 {
		c.printf("approval: none\n")
	} else {
		for _, item := range control.Approvals {
			target := item.TargetID
			if target == "" {
				target = item.WorkerID
			}
			c.printf("approval: %s %s target=%s\n", item.ID, item.Status, target)
		}
	}
	activeWorkers := 0
	for _, worker := range control.Workers {
		if worker.Status == runtime.WorkerIdle || worker.Status == runtime.WorkerClosed {
			continue
		}
		activeWorkers++
		line := fmt.Sprintf("worker: %s %s", worker.WorkerID, worker.Status)
		if worker.LastRunID != "" {
			line += " run=" + worker.LastRunID
		}
		if worker.Status == runtime.WorkerWaitingApproval {
			for _, item := range control.Approvals {
				if item.TargetID == worker.LastRunID {
					line += " approval=" + item.ID
					break
				}
			}
		}
		c.printf("%s\n", line)
	}
	if activeWorkers == 0 {
		c.printf("worker: none\n")
	}
	activeJobs := 0
	for _, job := range control.Jobs {
		if job.Status != runtime.JobQueued && job.Status != runtime.JobRunning {
			continue
		}
		activeJobs++
		c.printf("job: %s %s\n", job.JobID, job.Status)
	}
	if activeJobs == 0 {
		c.printf("job: none\n")
	}
}

func (c *ChatConsole) completeLine(line string) []string {
	fields := strings.Fields(line)
	if len(fields) == 0 {
		return filterByPrefix([]string{"/help", "/status", "/plan", "/plans", "/approve", "/reject", "/handoff", "/artifact", "/cancel", "/quit"}, "")
	}
	if len(fields) == 1 && !strings.Contains(line, " ") {
		base := []string{"/help", "/status", "/plan", "/plans", "/approve", "/reject", "/handoff", "/artifact", "/cancel", "/quit"}
		matches := filterByPrefix(base, fields[0])
		if len(matches) == 1 {
			return []string{matches[0], matches[0] + " "}
		}
		return matches
	}
	switch fields[0] {
	case "/approve", "/reject":
		return filterByPrefix(c.knownApprovals, lastField(line))
	case "/handoff":
		return filterByPrefix(c.knownWorkers, lastField(line))
	case "/artifact":
		return filterByPrefix(c.knownArtifacts, lastField(line))
	default:
		return nil
	}
}

func (c *ChatConsole) completionCandidates(line string) []string {
	raw := c.completeLine(line)
	out := make([]string, 0, len(raw))
	for _, item := range raw {
		item = strings.TrimRight(item, " ")
		if item == "" || containsString(out, item) {
			continue
		}
		out = append(out, item)
	}
	return out
}

func lastField(line string) string {
	fields := strings.Fields(line)
	if len(fields) == 0 {
		return ""
	}
	return fields[len(fields)-1]
}

func filterByPrefix(items []string, prefix string) []string {
	out := make([]string, 0, len(items))
	for _, item := range items {
		if strings.HasPrefix(item, prefix) {
			out = append(out, item)
		}
	}
	return out
}

func decodeEventPayload(raw []byte) map[string]any {
	if len(raw) == 0 {
		return map[string]any{}
	}
	var out map[string]any
	if err := json.Unmarshal(raw, &out); err != nil {
		return map[string]any{}
	}
	return out
}

func stringValue(v any) string {
	s, _ := v.(string)
	return s
}

func containsString(items []string, target string) bool {
	for _, item := range items {
		if item == target {
			return true
		}
	}
	return false
}

func (c *ChatConsole) resolveApprovalID(input string) (string, error) {
	input = strings.TrimSpace(input)
	if input == "" {
		return "", fmt.Errorf("approval id is required")
	}
	if containsString(c.knownApprovals, input) {
		return input, nil
	}
	matches := filterByPrefix(c.knownApprovals, input)
	switch len(matches) {
	case 0:
		return input, nil
	case 1:
		return matches[0], nil
	default:
		return "", fmt.Errorf("approval id is ambiguous: %s", input)
	}
}

func terminalFiles(in io.Reader, out io.Writer) (*os.File, *os.File, bool) {
	inFile, ok := in.(*os.File)
	if !ok {
		return nil, nil, false
	}
	outFile, ok := out.(*os.File)
	if !ok {
		return nil, nil, false
	}
	if !isatty.IsTerminal(inFile.Fd()) || !isatty.IsTerminal(outFile.Fd()) {
		return nil, nil, false
	}
	return inFile, outFile, true
}

func joinSegments(segments [][]rune) string {
	parts := make([]string, 0, len(segments))
	for _, segment := range segments {
		parts = append(parts, string(segment))
	}
	return strings.Join(parts, " ")
}

type stringerBuffer interface {
	String() string
}

func (c *ChatConsole) Output() interface{ String() string } {
	if out, ok := c.out.(stringerBuffer); ok {
		return out
	}
	return nil
}
