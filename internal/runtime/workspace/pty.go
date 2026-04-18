package workspace

import (
	"bytes"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"

	"github.com/creack/pty"
)

var (
	errPTYNotFound = errors.New("pty not found")
	errSessionID   = errors.New("session id is empty")
	errPTYID       = errors.New("pty id is empty")
)

// WorkspacePTYManager keeps one real PTY-backed terminal per session.
type WorkspacePTYManager struct {
	mu       sync.Mutex
	sessions map[string]*ptySession
	ptyIDs   map[string]*ptySession
}

type ptySession struct {
	mu        sync.Mutex
	id        string
	sessionID string
	cols      int
	rows      int
	cwd       string
	inputLine string
	cmd       *exec.Cmd
	file      *os.File
	output    bytes.Buffer
	alive     bool
	exitCode  *int
}

func NewWorkspacePTYManager() *WorkspacePTYManager {
	return &WorkspacePTYManager{
		sessions: map[string]*ptySession{},
		ptyIDs:   map[string]*ptySession{},
	}
}

func (m *WorkspacePTYManager) Open(sessionID string, cols, rows int) (PTYSnapshot, error) {
	if sessionID == "" {
		return PTYSnapshot{}, errSessionID
	}

	m.mu.Lock()
	session, ok := m.sessions[sessionID]
	if !ok {
		session = newPTYSession(sessionID)
		m.sessions[sessionID] = session
		m.ptyIDs[session.id] = session
	}
	m.mu.Unlock()

	if err := session.ensureRunning(cols, rows, false); err != nil {
		m.mu.Lock()
		if current, ok := m.sessions[sessionID]; ok && current == session && !session.alive {
			delete(m.sessions, sessionID)
			delete(m.ptyIDs, session.id)
		}
		m.mu.Unlock()
		return PTYSnapshot{}, err
	}
	return session.snapshot(), nil
}

func (m *WorkspacePTYManager) Input(ptyID string, data []byte) error {
	if ptyID == "" {
		return errPTYID
	}

	m.mu.Lock()
	session, ok := m.ptyIDs[ptyID]
	m.mu.Unlock()
	if !ok {
		return errPTYNotFound
	}
	return session.writeInput(data)
}

func (m *WorkspacePTYManager) Resize(ptyID string, cols, rows int) error {
	if ptyID == "" {
		return errPTYID
	}

	m.mu.Lock()
	session, ok := m.ptyIDs[ptyID]
	m.mu.Unlock()
	if !ok {
		return errPTYNotFound
	}
	return session.resize(cols, rows)
}

func (m *WorkspacePTYManager) Snapshot(sessionID string) (PTYSnapshot, bool) {
	if sessionID == "" {
		return PTYSnapshot{}, false
	}

	m.mu.Lock()
	session, ok := m.sessions[sessionID]
	m.mu.Unlock()
	if !ok {
		return PTYSnapshot{}, false
	}
	return session.snapshot(), true
}

func (m *WorkspacePTYManager) Reset(sessionID string) error {
	if sessionID == "" {
		return errSessionID
	}

	m.mu.Lock()
	session, ok := m.sessions[sessionID]
	m.mu.Unlock()
	if !ok {
		return errPTYNotFound
	}
	return session.restart()
}

func (m *WorkspacePTYManager) Close(sessionID string) error {
	if sessionID == "" {
		return errSessionID
	}

	m.mu.Lock()
	session, ok := m.sessions[sessionID]
	if !ok {
		m.mu.Unlock()
		return errPTYNotFound
	}
	delete(m.sessions, sessionID)
	delete(m.ptyIDs, session.id)
	m.mu.Unlock()
	return session.shutdown()
}

func (m *WorkspacePTYManager) SessionIDs() []string {
	m.mu.Lock()
	defer m.mu.Unlock()

	out := make([]string, 0, len(m.sessions))
	for sessionID := range m.sessions {
		out = append(out, sessionID)
	}
	return out
}

func newPTYSession(sessionID string) *ptySession {
	return &ptySession{
		id:        sessionPTYID(sessionID),
		sessionID: sessionID,
	}
}

func (s *ptySession) ensureRunning(cols, rows int, clear bool) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	if s.alive && s.file != nil && s.cmd != nil && s.cmd.Process != nil {
		s.cols = cols
		s.rows = rows
		if err := pty.Setsize(s.file, &pty.Winsize{Cols: uint16(cols), Rows: uint16(rows)}); err != nil {
			return err
		}
		return nil
	}

	if clear {
		s.output.Reset()
	}
	s.exitCode = nil
	if s.cwd == "" {
		if cwd, err := os.Getwd(); err == nil {
			s.cwd = cwd
		}
	}

	shellPath := defaultShellPath()
	cmd := exec.Command(shellPath, "-l")
	if s.cwd != "" {
		cmd.Dir = s.cwd
	}
	cmd.Env = os.Environ()
	f, err := pty.StartWithSize(cmd, &pty.Winsize{Cols: uint16(cols), Rows: uint16(rows)})
	if err != nil {
		return err
	}

	s.cmd = cmd
	s.file = f
	s.cols = cols
	s.rows = rows
	s.inputLine = ""
	s.alive = true
	go s.captureOutput(f)
	return nil
}

func (s *ptySession) restart() error {
	if err := s.shutdown(); err != nil {
		return err
	}
	return s.ensureRunning(s.cols, s.rows, true)
}

func (s *ptySession) shutdown() error {
	s.mu.Lock()
	file := s.file
	cmd := s.cmd
	s.file = nil
	s.cmd = nil
	s.alive = false
	s.mu.Unlock()

	var errs []error
	if file != nil {
		if err := file.Close(); err != nil && !errors.Is(err, os.ErrClosed) {
			errs = append(errs, err)
		}
	}
	if cmd != nil && cmd.Process != nil {
		_ = cmd.Process.Kill()
		_ = cmd.Wait()
		if cmd.ProcessState != nil {
			exitCode := cmd.ProcessState.ExitCode()
			s.mu.Lock()
			s.exitCode = &exitCode
			s.mu.Unlock()
		}
	}
	return errors.Join(errs...)
}

func (s *ptySession) writeInput(data []byte) error {
	if len(data) == 0 {
		return nil
	}

	s.mu.Lock()
	file := s.file
	alive := s.alive
	s.mu.Unlock()
	if !alive || file == nil {
		return errPTYNotFound
	}
	_, err := file.Write(data)
	if err != nil {
		return err
	}
	s.mu.Lock()
	s.applyInputLocked(data)
	s.mu.Unlock()
	return err
}

func (s *ptySession) resize(cols, rows int) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	if !s.alive || s.file == nil {
		return errPTYNotFound
	}
	s.cols = cols
	s.rows = rows
	return pty.Setsize(s.file, &pty.Winsize{Cols: uint16(cols), Rows: uint16(rows)})
}

func (s *ptySession) snapshot() PTYSnapshot {
	s.mu.Lock()
	defer s.mu.Unlock()

	return PTYSnapshot{
		PTYID:      s.id,
		SessionID:  s.sessionID,
		PID:        s.pid(),
		Cols:       s.cols,
		Rows:       s.rows,
		Alive:      s.alive,
		CWD:        s.cwd,
		ExitCode:   cloneInt(s.exitCode),
		Scrollback: splitScrollback(s.output.Bytes()),
	}
}

func (s *ptySession) pid() int {
	if s.cmd == nil || s.cmd.Process == nil {
		return 0
	}
	return s.cmd.Process.Pid
}

func (s *ptySession) captureOutput(file *os.File) {
	buf := make([]byte, 4096)
	for {
		n, err := file.Read(buf)
		if n > 0 {
			s.mu.Lock()
			_, _ = s.output.Write(buf[:n])
			s.mu.Unlock()
		}
		if err != nil {
			if !errors.Is(err, os.ErrClosed) && !errors.Is(err, io.EOF) && !errors.Is(err, os.ErrDeadlineExceeded) {
				// Treat read errors as terminal for the session. The next Open or
				// Reset call will recreate the PTY if needed.
			}
			s.mu.Lock()
			s.alive = false
			s.file = nil
			s.cmd = nil
			s.mu.Unlock()
			return
		}
	}
}

func defaultShellPath() string {
	if shell := os.Getenv("SHELL"); shell != "" {
		return shell
	}
	return "/bin/bash"
}

func sessionPTYID(sessionID string) string {
	return fmt.Sprintf("pty:%s", sessionID)
}

func splitScrollback(raw []byte) []string {
	if len(raw) == 0 {
		return nil
	}
	normalized := bytes.ReplaceAll(raw, []byte("\r"), []byte{})
	lines := bytes.Split(normalized, []byte("\n"))
	out := make([]string, 0, len(lines))
	for _, line := range lines {
		if len(line) == 0 {
			continue
		}
		out = append(out, string(line))
	}
	return out
}

func (s *ptySession) applyInputLocked(data []byte) {
	for _, b := range data {
		switch b {
		case '\r', '\n':
			s.processInputLineLocked(strings.TrimSpace(s.inputLine))
			s.inputLine = ""
		case '\x7f', '\b':
			if len(s.inputLine) > 0 {
				s.inputLine = s.inputLine[:len(s.inputLine)-1]
			}
		default:
			s.inputLine += string(b)
		}
	}
}

func (s *ptySession) processInputLineLocked(line string) {
	if line == "" {
		return
	}
	if line == "cd" {
		if home, err := os.UserHomeDir(); err == nil && home != "" {
			s.cwd = home
		}
		return
	}
	if !strings.HasPrefix(line, "cd ") && !strings.HasPrefix(line, "cd\t") {
		return
	}
	rest := strings.TrimSpace(strings.TrimPrefix(line, "cd"))
	if rest == "" {
		if home, err := os.UserHomeDir(); err == nil && home != "" {
			s.cwd = home
		}
		return
	}
	target := parseShellPath(rest)
	if target == "" {
		return
	}
	if !filepath.IsAbs(target) {
		base := s.cwd
		if base == "" {
			if cwd, err := os.Getwd(); err == nil {
				base = cwd
			}
		}
		if base != "" {
			target = filepath.Join(base, target)
		}
	}
	if abs, err := filepath.Abs(target); err == nil {
		s.cwd = abs
		return
	}
	s.cwd = filepath.Clean(target)
}

func parseShellPath(raw string) string {
	trimmed := strings.TrimSpace(raw)
	if trimmed == "" {
		return ""
	}
	if trimmed[0] == '\'' {
		return parseSingleQuotedShellPath(trimmed)
	}
	if trimmed[0] == '"' {
		return parseDoubleQuotedShellPath(trimmed)
	}
	fields := strings.Fields(trimmed)
	if len(fields) == 0 {
		return ""
	}
	return fields[0]
}

func parseSingleQuotedShellPath(raw string) string {
	if len(raw) < 2 {
		return ""
	}
	var out strings.Builder
	for i := 1; i < len(raw); i++ {
		switch raw[i] {
		case '\'':
			return out.String()
		case '\\':
			if i+3 < len(raw) && raw[i+1] == '\'' && raw[i+2] == '\\' && raw[i+3] == '\'' {
				out.WriteByte('\'')
				i += 3
				continue
			}
			out.WriteByte(raw[i])
		default:
			out.WriteByte(raw[i])
		}
	}
	return out.String()
}

func parseDoubleQuotedShellPath(raw string) string {
	if len(raw) < 2 {
		return ""
	}
	var out strings.Builder
	escaped := false
	for i := 1; i < len(raw); i++ {
		ch := raw[i]
		if escaped {
			out.WriteByte(ch)
			escaped = false
			continue
		}
		switch ch {
		case '"':
			return out.String()
		case '\\':
			escaped = true
		default:
			out.WriteByte(ch)
		}
	}
	return out.String()
}

func cloneInt(v *int) *int {
	if v == nil {
		return nil
	}
	n := *v
	return &n
}
