package runtime

import (
	"bufio"
	"context"
	"encoding/json"
	"errors"
	"os"
	"os/signal"
	"os/exec"
	"syscall"
	"testing"
	"time"
)

func TestWorkerSupervisorTracksHeartbeatAndStopsProcess(t *testing.T) {
	if os.Getenv("TEAMD_TEST_HELPER_WORKER") == "1" {
		runWorkerHelperProcess()
		return
	}

	supervisor := NewWorkerSupervisor(workerProcessLauncherFunc(func(ctx context.Context, binaryPath string, args []string, env []string) (workerManagedProcess, error) {
		cmd := exec.CommandContext(ctx, os.Args[0], "-test.run=TestWorkerSupervisorTracksHeartbeatAndStopsProcess")
		cmd.Env = append(os.Environ(), append(env, "TEAMD_TEST_HELPER_WORKER=1")...)
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
	}), WorkerSupervisorConfig{
		HeartbeatTimeout:  2 * time.Second,
		ShutdownGrace:     500 * time.Millisecond,
		HeartbeatInterval: 50 * time.Millisecond,
	})

	record := WorkerRecord{WorkerID: "worker-1"}
	runtimeState, err := supervisor.Start(context.Background(), record)
	if err != nil {
		t.Fatalf("start supervisor: %v", err)
	}
	if runtimeState.PID == 0 {
		t.Fatalf("expected worker pid")
	}

	deadline := time.Now().Add(2 * time.Second)
	for time.Now().Before(deadline) {
		current, ok := supervisor.Runtime(record.WorkerID)
		if ok && current.LastHeartbeatAt != nil && current.State == WorkerProcessRunning {
			break
		}
		time.Sleep(20 * time.Millisecond)
	}
	current, ok := supervisor.Runtime(record.WorkerID)
	if !ok || current.LastHeartbeatAt == nil || current.State != WorkerProcessRunning {
		t.Fatalf("expected running worker with heartbeat, got ok=%v runtime=%+v", ok, current)
	}

	if err := supervisor.Stop(context.Background(), record.WorkerID, record); err != nil {
		t.Fatalf("stop supervisor: %v", err)
	}
	current, ok = supervisor.Runtime(record.WorkerID)
	if !ok || current.State != WorkerProcessStopped {
		t.Fatalf("expected stopped worker process state, got ok=%v runtime=%+v", ok, current)
	}
}

func runWorkerHelperProcess() {
	ticker := time.NewTicker(50 * time.Millisecond)
	defer ticker.Stop()
	signals := make(chan os.Signal, 1)
	signalNotify(signals, syscall.SIGTERM, syscall.SIGINT)
	enc := json.NewEncoder(os.Stdout)
	_ = enc.Encode(map[string]any{
		"kind":      "worker.started",
		"pid":       os.Getpid(),
		"timestamp": time.Now().UTC().Format(time.RFC3339Nano),
	})
	for {
		select {
		case <-ticker.C:
			_ = enc.Encode(map[string]any{
				"kind":      "worker.heartbeat",
				"pid":       os.Getpid(),
				"timestamp": time.Now().UTC().Format(time.RFC3339Nano),
			})
		case <-signals:
			_ = enc.Encode(map[string]any{
				"kind":      "worker.stopped",
				"pid":       os.Getpid(),
				"timestamp": time.Now().UTC().Format(time.RFC3339Nano),
			})
			os.Exit(0)
		}
	}
}

var signalNotify = func(c chan<- os.Signal, sig ...os.Signal) {
	signalNotifyImpl(c, sig...)
}

func signalNotifyImpl(c chan<- os.Signal, sig ...os.Signal) {
	signalNotifyPkg(c, sig...)
}

var signalNotifyPkg = func(c chan<- os.Signal, sig ...os.Signal) {
	signal.Notify(c, sig...)
}

func TestParseWorkerHeartbeatLine(t *testing.T) {
	line := `{"kind":"worker.heartbeat","pid":42,"timestamp":"2026-04-12T13:00:00Z"}`
	msg, ok := parseWorkerProcessMessage(line)
	if !ok {
		t.Fatal("expected parsed heartbeat")
	}
	if msg.Kind != "worker.heartbeat" || msg.PID != 42 {
		t.Fatalf("unexpected message: %+v", msg)
	}
}

func TestParseWorkerHeartbeatLineRejectsGarbage(t *testing.T) {
	if _, ok := parseWorkerProcessMessage("not-json"); ok {
		t.Fatal("expected garbage line to be rejected")
	}
}

func TestWorkerManagedProcessReadLoopSkipsGarbage(t *testing.T) {
	reader, writer, err := os.Pipe()
	if err != nil {
		t.Fatalf("pipe: %v", err)
	}
	defer reader.Close()
	defer writer.Close()

	msgs := make(chan workerProcessMessage, 4)
	done := make(chan error, 1)
	go func() {
		done <- readWorkerProcessMessages(bufio.NewScanner(reader), msgs)
	}()
	_, _ = writer.WriteString("garbage\n")
	_, _ = writer.WriteString("{\"kind\":\"worker.heartbeat\",\"pid\":7,\"timestamp\":\"2026-04-12T13:00:00Z\"}\n")
	_ = writer.Close()

	select {
	case msg := <-msgs:
		if msg.PID != 7 || msg.Kind != "worker.heartbeat" {
			t.Fatalf("unexpected message: %+v", msg)
		}
	case <-time.After(time.Second):
		t.Fatal("expected parsed heartbeat")
	}
	if err := <-done; err != nil && !errors.Is(err, os.ErrClosed) {
		t.Fatalf("read loop: %v", err)
	}
}

func TestWorkerProcessMessageTimestampFallback(t *testing.T) {
	msg, ok := parseWorkerProcessMessage(`{"kind":"worker.heartbeat","pid":"15","timestamp":"bad-time"}`)
	if !ok {
		t.Fatal("expected message")
	}
	if msg.PID != 15 {
		t.Fatalf("unexpected pid: %+v", msg)
	}
	if msg.Timestamp.IsZero() {
		t.Fatal("expected fallback timestamp")
	}
}

func TestWorkerProcessMessageHandlesStringPID(t *testing.T) {
	msg, ok := parseWorkerProcessMessage(`{"kind":"worker.started","pid":"99","timestamp":"2026-04-12T13:00:00Z"}`)
	if !ok {
		t.Fatal("expected message")
	}
	if msg.PID != 99 {
		t.Fatalf("unexpected pid: %+v", msg)
	}
}
