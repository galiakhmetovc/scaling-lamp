package skills

import (
	"reflect"
	"testing"
)

func TestSessionStateTracksActiveSkillsPerSession(t *testing.T) {
	state := NewSessionState()
	state.Activate("chat:1/default", "deploy")
	state.Activate("chat:1/default", "shell")
	state.Activate("chat:1/ops", "incident")

	got := state.Active("chat:1/default")
	want := []string{"deploy", "shell"}
	if !reflect.DeepEqual(want, got) {
		t.Fatalf("unexpected skills: %#v", got)
	}
}

func TestSessionStateDeactivateAndResetAreIdempotent(t *testing.T) {
	state := NewSessionState()
	state.Activate("chat:1/default", "deploy")
	state.Deactivate("chat:1/default", "deploy")
	state.Deactivate("chat:1/default", "deploy")
	if got := state.Active("chat:1/default"); len(got) != 0 {
		t.Fatalf("expected empty skills, got %#v", got)
	}
	state.Reset("chat:1/default")
}
