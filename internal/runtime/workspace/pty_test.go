package workspace

import (
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
	"time"
)

func TestPTYCreatesOnePTYPerSession(t *testing.T) {
	requireLinuxPTY(t)

	mgr := NewWorkspacePTYManager()
	t.Cleanup(func() { closePTYSessions(t, mgr, "session-1", "session-2") })

	first, err := mgr.Open("session-1", 80, 24)
	if err != nil {
		t.Fatalf("Open session-1: %v", err)
	}
	second, err := mgr.Open("session-2", 100, 40)
	if err != nil {
		t.Fatalf("Open session-2: %v", err)
	}

	if first.PTYID == second.PTYID {
		t.Fatalf("pty ids should differ across sessions: %q", first.PTYID)
	}
	if first.SessionID != "session-1" {
		t.Fatalf("first snapshot session id = %q, want session-1", first.SessionID)
	}
	if second.SessionID != "session-2" {
		t.Fatalf("second snapshot session id = %q, want session-2", second.SessionID)
	}
	if first.PID == 0 || second.PID == 0 {
		t.Fatalf("expected live shell PIDs, got first=%d second=%d", first.PID, second.PID)
	}
	if !first.Alive || !second.Alive {
		t.Fatalf("new PTYs should be alive: first=%v second=%v", first.Alive, second.Alive)
	}
}

func TestPTYReopenReturnsSamePTYID(t *testing.T) {
	requireLinuxPTY(t)

	mgr := NewWorkspacePTYManager()
	t.Cleanup(func() { closePTYSessions(t, mgr, "session-1") })

	first, err := mgr.Open("session-1", 80, 24)
	if err != nil {
		t.Fatalf("Open first: %v", err)
	}
	second, err := mgr.Open("session-1", 120, 50)
	if err != nil {
		t.Fatalf("Open second: %v", err)
	}

	if first.PTYID != second.PTYID {
		t.Fatalf("pty ids differ: %q vs %q", first.PTYID, second.PTYID)
	}
	if second.Cols != 120 || second.Rows != 50 {
		t.Fatalf("reopen size = %dx%d, want 120x50", second.Cols, second.Rows)
	}
	if !second.Alive {
		t.Fatal("reopened PTY is not alive")
	}
}

func TestPTYResizeUpdatesColsAndRows(t *testing.T) {
	requireLinuxPTY(t)

	mgr := NewWorkspacePTYManager()
	t.Cleanup(func() { closePTYSessions(t, mgr, "session-1") })

	opened, err := mgr.Open("session-1", 80, 24)
	if err != nil {
		t.Fatalf("Open: %v", err)
	}
	if err := mgr.Resize(opened.PTYID, 132, 43); err != nil {
		t.Fatalf("Resize: %v", err)
	}

	snap, ok := mgr.Snapshot("session-1")
	if !ok {
		t.Fatal("Snapshot missing after resize")
	}
	if snap.Cols != 132 || snap.Rows != 43 {
		t.Fatalf("snapshot size = %dx%d, want 132x43", snap.Cols, snap.Rows)
	}
	if !snap.Alive {
		t.Fatal("PTY should remain alive after resize")
	}
}

func TestPTYInputWritesToPTY(t *testing.T) {
	requireLinuxPTY(t)

	mgr := NewWorkspacePTYManager()
	t.Cleanup(func() { closePTYSessions(t, mgr, "session-1") })

	opened, err := mgr.Open("session-1", 80, 24)
	if err != nil {
		t.Fatalf("Open: %v", err)
	}
	if err := mgr.Input(opened.PTYID, []byte("printf 'hello workspace\\n'\n")); err != nil {
		t.Fatalf("Input: %v", err)
	}
	waitForScrollbackContains(t, mgr, "session-1", "hello workspace")

	snap, ok := mgr.Snapshot("session-1")
	if !ok {
		t.Fatal("Snapshot missing after input")
	}
	if got := strings.Join(snap.Scrollback, "\n"); !strings.Contains(got, "hello workspace") {
		t.Fatalf("scrollback %q does not contain PTY output", got)
	}
}

func TestPTYCloseAndResetTearDownCleanly(t *testing.T) {
	requireLinuxPTY(t)

	mgr := NewWorkspacePTYManager()
	t.Cleanup(func() { closePTYSessions(t, mgr, "session-1") })

	opened, err := mgr.Open("session-1", 80, 24)
	if err != nil {
		t.Fatalf("Open: %v", err)
	}
	if err := mgr.Input(opened.PTYID, []byte("printf 'line one\\n'\n")); err != nil {
		t.Fatalf("Input before reset: %v", err)
	}
	waitForScrollbackContains(t, mgr, "session-1", "line one")

	beforeReset := opened.PID
	if err := mgr.Reset("session-1"); err != nil {
		t.Fatalf("Reset: %v", err)
	}
	resetSnap, ok := mgr.Snapshot("session-1")
	if !ok {
		t.Fatal("Snapshot missing after reset")
	}
	if resetSnap.PTYID != opened.PTYID {
		t.Fatalf("reset PTY id = %q, want same session PTY id %q", resetSnap.PTYID, opened.PTYID)
	}
	if resetSnap.PID == 0 {
		t.Fatal("reset PTY PID is zero")
	}
	if resetSnap.PID == beforeReset {
		t.Fatalf("reset PTY PID = %d, want fresh process", resetSnap.PID)
	}
	if len(resetSnap.Scrollback) != 0 {
		t.Fatalf("scrollback after reset = %#v, want empty", resetSnap.Scrollback)
	}
	if err := mgr.Input(opened.PTYID, []byte("printf 'after reset\\n'\n")); err != nil {
		t.Fatalf("Input after reset: %v", err)
	}
	waitForScrollbackContains(t, mgr, "session-1", "after reset")

	if err := mgr.Close("session-1"); err != nil {
		t.Fatalf("Close: %v", err)
	}
	if _, ok := mgr.Snapshot("session-1"); ok {
		t.Fatal("Snapshot still present after close")
	}
	if err := mgr.Input(resetSnap.PTYID, []byte("after close")); err == nil {
		t.Fatal("Input after close succeeded, want error")
	}

	reopened, err := mgr.Open("session-1", 80, 24)
	if err != nil {
		t.Fatalf("Reopen after close: %v", err)
	}
	if reopened.PTYID != opened.PTYID {
		t.Fatalf("reopened PTYID = %q, want %q", reopened.PTYID, opened.PTYID)
	}
	if reopened.PID == 0 {
		t.Fatal("reopened PTY PID is zero")
	}
	if reopened.PID == resetSnap.PID {
		t.Fatalf("reopened PTY PID = %d, want fresh process", reopened.PID)
	}
	if len(reopened.Scrollback) != 0 {
		t.Fatalf("scrollback after reopen = %#v, want empty", reopened.Scrollback)
	}
}

func requireLinuxPTY(t *testing.T) {
	t.Helper()
	if runtime.GOOS != "linux" {
		t.Skip("real PTY backend is linux-only")
	}
	shellPath := defaultShellPath()
	if filepath.IsAbs(shellPath) {
		if _, err := os.Stat(shellPath); err != nil {
			t.Skipf("shell %q unavailable: %v", shellPath, err)
		}
		return
	}
	if _, err := exec.LookPath(shellPath); err != nil {
		t.Skipf("shell %q unavailable: %v", shellPath, err)
	}
}

func waitForScrollbackContains(t *testing.T, mgr *WorkspacePTYManager, sessionID, want string) {
	t.Helper()
	deadline := time.Now().Add(5 * time.Second)
	for time.Now().Before(deadline) {
		snap, ok := mgr.Snapshot(sessionID)
		if ok && strings.Contains(strings.Join(snap.Scrollback, "\n"), want) {
			return
		}
		time.Sleep(25 * time.Millisecond)
	}
	snap, _ := mgr.Snapshot(sessionID)
	t.Fatalf("scrollback never contained %q; got %#v", want, snap.Scrollback)
}

func closePTYSessions(t *testing.T, mgr *WorkspacePTYManager, sessionIDs ...string) {
	t.Helper()
	for _, sessionID := range sessionIDs {
		_ = mgr.Close(sessionID)
	}
}
