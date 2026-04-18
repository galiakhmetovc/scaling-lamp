package runtimev2

import (
	"strings"
	"testing"
)

func TestExecStartContract(t *testing.T) {
	descriptors := ToolDescriptorsV2()
	if len(descriptors) != 6 {
		t.Fatalf("descriptor count = %d, want 6", len(descriptors))
	}

	gotExecStart, ok := DescriptorForToolV2("exec_start")
	if !ok {
		t.Fatal("missing exec_start descriptor")
	}
	if gotExecStart.Kind != ToolKindStructuredExec {
		t.Fatalf("exec_start kind = %q, want %q", gotExecStart.Kind, ToolKindStructuredExec)
	}

	valid := ExecStartContractInput{
		Executable: "git",
		Args:       []string{"status", "--short"},
		Cwd:        "/tmp/project",
		Env:        map[string]string{"GIT_TERMINAL_PROMPT": "0"},
	}
	if err := ValidateExecStartContract(valid); err != nil {
		t.Fatalf("valid exec_start input rejected: %v", err)
	}

	for _, executable := range []string{
		"cd",
		"cd /tmp && pwd",
		"git && status",
		"git | cat",
		"git > out.txt",
		"git < in.txt",
		"git; pwd",
	} {
		err := ValidateExecStartContract(ExecStartContractInput{Executable: executable})
		if err == nil {
			t.Fatalf("exec_start accepted shelly executable %q", executable)
		}
	}
}

func TestShellSnippetContract(t *testing.T) {
	got, ok := DescriptorForToolV2("shell_snippet_start")
	if !ok {
		t.Fatal("missing shell_snippet_start descriptor")
	}
	if got.Kind != ToolKindShellSnippet {
		t.Fatalf("shell_snippet_start kind = %q, want %q", got.Kind, ToolKindShellSnippet)
	}

	snippet := "cd dir && cmd"
	input := ShellSnippetContractInput{
		Script: snippet,
		Cwd:    "/tmp/project",
		Env:    map[string]string{"FOO": "bar"},
	}
	if err := ValidateShellSnippetContract(input); err != nil {
		t.Fatalf("shell snippet rejected: %v", err)
	}

	classification, ok := DispatchKindForToolV2("shell_snippet_start")
	if !ok {
		t.Fatal("missing shell_snippet_start dispatch classification")
	}
	if classification != DispatchKindShellSnippet {
		t.Fatalf("dispatch classification = %q, want %q", classification, DispatchKindShellSnippet)
	}

	if strings.TrimSpace(input.Script) != snippet {
		t.Fatalf("script was altered: got %q, want %q", input.Script, snippet)
	}
}
