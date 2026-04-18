package workspace

import "fmt"

// PTYSnapshot captures the current state of a session-bound PTY.
type PTYSnapshot struct {
	PTYID      string   `json:"pty_id"`
	SessionID  string   `json:"session_id"`
	PID        int      `json:"pid,omitempty"`
	Cols       int      `json:"cols"`
	Rows       int      `json:"rows"`
	Alive      bool     `json:"alive"`
	CWD        string   `json:"cwd,omitempty"`
	ExitCode   *int     `json:"exit_code,omitempty"`
	Scrollback []string `json:"scrollback,omitempty"`
}

var errWaitPrompt = fmt.Errorf("timed out waiting for PTY prompt")

type errWaitOutput struct {
	needle string
}

func (e errWaitOutput) Error() string {
	return fmt.Sprintf("timed out waiting for PTY output %q", e.needle)
}
