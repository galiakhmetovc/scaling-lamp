package runtime

import (
	"bufio"
	"context"
	"encoding/json"
	"io"
	"os"
	"os/exec"
	"strings"
	"sync"
	"syscall"
	"time"
)

type WorkerSupervisor interface {
	Start(ctx context.Context, record WorkerRecord) (WorkerProcessRuntime, error)
	Stop(ctx context.Context, workerID string, record WorkerRecord) error
	Runtime(workerID string) (WorkerProcessRuntime, bool)
}

type WorkerSupervisorConfig struct {
	BinaryPath        string
	HeartbeatInterval time.Duration
	HeartbeatTimeout  time.Duration
	ShutdownGrace     time.Duration
}

type workerManagedProcess struct {
	pid    int
	stdout io.ReadCloser
	stderr io.ReadCloser
	wait   func() error
	signal func(os.Signal) error
	kill   func() error
}

type workerProcessLauncher interface {
	Start(ctx context.Context, binaryPath string, args []string, env []string) (workerManagedProcess, error)
}

type workerProcessLauncherFunc func(ctx context.Context, binaryPath string, args []string, env []string) (workerManagedProcess, error)

func (f workerProcessLauncherFunc) Start(ctx context.Context, binaryPath string, args []string, env []string) (workerManagedProcess, error) {
	return f(ctx, binaryPath, args, env)
}

type execWorkerProcessLauncher struct{}

func (execWorkerProcessLauncher) Start(ctx context.Context, binaryPath string, args []string, env []string) (workerManagedProcess, error) {
	cmd := exec.CommandContext(ctx, binaryPath, args...)
	cmd.Env = append(os.Environ(), env...)
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		return workerManagedProcess{}, err
	}
	stderr, err := cmd.StderrPipe()
	if err != nil {
		return workerManagedProcess{}, err
	}
	if err := cmd.Start(); err != nil {
		return workerManagedProcess{}, err
	}
	return workerManagedProcess{
		pid:    cmd.Process.Pid,
		stdout: stdout,
		stderr: stderr,
		wait:   cmd.Wait,
		signal: func(sig os.Signal) error { return cmd.Process.Signal(sig) },
		kill:   func() error { return cmd.Process.Kill() },
	}, nil
}

type supervisedWorker struct {
	process workerManagedProcess
	runtime WorkerProcessRuntime
}

type workerProcessMessage struct {
	Kind      string
	PID       int
	Timestamp time.Time
}

type workerSupervisor struct {
	mu       sync.Mutex
	launcher workerProcessLauncher
	cfg      WorkerSupervisorConfig
	items    map[string]*supervisedWorker
}

func NewWorkerSupervisor(launcher workerProcessLauncher, cfg WorkerSupervisorConfig) WorkerSupervisor {
	if launcher == nil {
		launcher = execWorkerProcessLauncher{}
	}
	if cfg.HeartbeatInterval <= 0 {
		cfg.HeartbeatInterval = time.Second
	}
	if cfg.HeartbeatTimeout <= 0 {
		cfg.HeartbeatTimeout = 10 * time.Second
	}
	if cfg.ShutdownGrace <= 0 {
		cfg.ShutdownGrace = 3 * time.Second
	}
	if strings.TrimSpace(cfg.BinaryPath) == "" {
		cfg.BinaryPath = defaultWorkerBinaryPath()
	}
	return &workerSupervisor{
		launcher: launcher,
		cfg:      cfg,
		items:    map[string]*supervisedWorker{},
	}
}

func (s *workerSupervisor) Start(ctx context.Context, record WorkerRecord) (WorkerProcessRuntime, error) {
	args := []string{"serve", "--worker-id", record.WorkerID, "--heartbeat-interval", s.cfg.HeartbeatInterval.String()}
	env := []string{
		"TEAMD_WORKER_ID=" + record.WorkerID,
	}
	process, err := s.launcher.Start(ctx, s.cfg.BinaryPath, args, env)
	if err != nil {
		return WorkerProcessRuntime{}, err
	}
	now := time.Now().UTC()
	runtime := WorkerProcessRuntime{
		PID:       process.pid,
		State:     WorkerProcessStarting,
		StartedAt: &now,
	}
	item := &supervisedWorker{process: process, runtime: runtime}

	s.mu.Lock()
	s.items[record.WorkerID] = item
	s.mu.Unlock()

	go s.consume(record.WorkerID, process.stdout)
	go drainWorkerProcessOutput(process.stderr)
	go s.wait(record.WorkerID)

	return runtime, nil
}

func (s *workerSupervisor) Stop(ctx context.Context, workerID string, _ WorkerRecord) error {
	s.mu.Lock()
	item, ok := s.items[workerID]
	s.mu.Unlock()
	if !ok {
		return nil
	}
	_ = item.process.signal(syscall.SIGTERM)
	done := make(chan struct{})
	go func() {
		for {
			current, ok := s.Runtime(workerID)
			if !ok || current.State == WorkerProcessStopped || current.State == WorkerProcessFailed {
				close(done)
				return
			}
			time.Sleep(25 * time.Millisecond)
		}
	}()
	select {
	case <-done:
		return nil
	case <-ctx.Done():
		return ctx.Err()
	case <-time.After(s.cfg.ShutdownGrace):
		if err := item.process.kill(); err != nil {
			return err
		}
		return nil
	}
}

func (s *workerSupervisor) Runtime(workerID string) (WorkerProcessRuntime, bool) {
	s.mu.Lock()
	defer s.mu.Unlock()
	item, ok := s.items[workerID]
	if !ok {
		return WorkerProcessRuntime{}, false
	}
	out := item.runtime
	if out.State == WorkerProcessRunning && out.LastHeartbeatAt != nil && time.Since(*out.LastHeartbeatAt) > s.cfg.HeartbeatTimeout {
		out.State = WorkerProcessFailed
		out.ExitReason = "heartbeat timeout"
		item.runtime = out
	}
	return out, true
}

func (s *workerSupervisor) consume(workerID string, stdout io.ReadCloser) {
	if stdout == nil {
		return
	}
	defer stdout.Close()
	msgs := make(chan workerProcessMessage, 8)
	go func() {
		_ = readWorkerProcessMessages(bufio.NewScanner(stdout), msgs)
		close(msgs)
	}()
	for msg := range msgs {
		s.mu.Lock()
		item, ok := s.items[workerID]
		if ok {
			switch msg.Kind {
			case "worker.started":
				item.runtime.State = WorkerProcessRunning
				ts := msg.Timestamp
				item.runtime.StartedAt = &ts
				item.runtime.LastHeartbeatAt = &ts
				if msg.PID > 0 {
					item.runtime.PID = msg.PID
				}
			case "worker.heartbeat":
				item.runtime.State = WorkerProcessRunning
				ts := msg.Timestamp
				item.runtime.LastHeartbeatAt = &ts
				if msg.PID > 0 {
					item.runtime.PID = msg.PID
				}
			case "worker.stopped":
				item.runtime.State = WorkerProcessStopped
				ts := msg.Timestamp
				item.runtime.ExitedAt = &ts
			}
		}
		s.mu.Unlock()
	}
}

func (s *workerSupervisor) wait(workerID string) {
	s.mu.Lock()
	item, ok := s.items[workerID]
	s.mu.Unlock()
	if !ok {
		return
	}
	err := item.process.wait()
	now := time.Now().UTC()
	s.mu.Lock()
	defer s.mu.Unlock()
	current, ok := s.items[workerID]
	if !ok {
		return
	}
	if err != nil {
		current.runtime.State = WorkerProcessFailed
		current.runtime.ExitReason = err.Error()
	} else if current.runtime.State != WorkerProcessStopped {
		current.runtime.State = WorkerProcessStopped
	}
	current.runtime.ExitedAt = &now
}

func readWorkerProcessMessages(scanner *bufio.Scanner, out chan<- workerProcessMessage) error {
	for scanner.Scan() {
		msg, ok := parseWorkerProcessMessage(scanner.Text())
		if !ok {
			continue
		}
		out <- msg
	}
	return scanner.Err()
}

func parseWorkerProcessMessage(line string) (workerProcessMessage, bool) {
	var body map[string]any
	if err := json.Unmarshal([]byte(line), &body); err != nil {
		return workerProcessMessage{}, false
	}
	kind, _ := body["kind"].(string)
	kind = strings.TrimSpace(kind)
	if kind == "" {
		return workerProcessMessage{}, false
	}
	timestamp := time.Now().UTC()
	if raw, _ := body["timestamp"].(string); strings.TrimSpace(raw) != "" {
		if parsed, err := time.Parse(time.RFC3339Nano, raw); err == nil {
			timestamp = parsed
		}
	}
	return workerProcessMessage{
		Kind:      kind,
		PID:       parsePID(body["pid"]),
		Timestamp: timestamp,
	}, true
}

func drainWorkerProcessOutput(r io.ReadCloser) {
	if r == nil {
		return
	}
	defer r.Close()
	_, _ = io.Copy(io.Discard, r)
}

func defaultWorkerBinaryPath() string {
	if v := strings.TrimSpace(os.Getenv("TEAMD_WORKER_BINARY")); v != "" {
		return v
	}
	return "./teamd-worker"
}

func parsePID(raw any) int {
	switch v := raw.(type) {
	case float64:
		return int(v)
	case int:
		return v
	case int64:
		return int(v)
	case json.Number:
		n, err := v.Int64()
		if err == nil {
			return int(n)
		}
	case string:
		v = strings.TrimSpace(v)
		if v == "" {
			return 0
		}
		n, err := json.Number(v).Int64()
		if err == nil {
			return int(n)
		}
	}
	return 0
}
