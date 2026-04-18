package runtimev2

import (
	"fmt"
	"strings"
)

type ToolKindV2 string

const (
	ToolKindStructuredExec ToolKindV2 = "structured-exec"
	ToolKindShellSnippet   ToolKindV2 = "shell-snippet"
)

type ToolDescriptorV2 struct {
	Name        string
	Kind        ToolKindV2
	Description string
	Parameters  map[string]any
}

type ExecStartContractInput struct {
	Executable string
	Args       []string
	Cwd        string
	Env        map[string]string
}

type ShellSnippetContractInput struct {
	Script string
	Cwd    string
	Env    map[string]string
}

func ToolDescriptorsV2() []ToolDescriptorV2 {
	return []ToolDescriptorV2{
		{
			Name:        "exec_start",
			Kind:        ToolKindStructuredExec,
			Description: "Start a structured executable invocation using executable, args, cwd, and env.",
			Parameters: map[string]any{
				"type": "object",
				"properties": map[string]any{
					"executable": map[string]any{"type": "string"},
					"args": map[string]any{
						"type":  "array",
						"items": map[string]any{"type": "string"},
					},
					"cwd": map[string]any{"type": "string"},
					"env": map[string]any{
						"type": "object",
						"additionalProperties": map[string]any{
							"type": "string",
						},
					},
				},
				"required": []string{"executable"},
			},
		},
		{
			Name:        "exec_wait",
			Kind:        ToolKindStructuredExec,
			Description: "Wait for a structured executable invocation to finish.",
			Parameters: map[string]any{
				"type": "object",
				"properties": map[string]any{
					"process_id": map[string]any{"type": "string"},
				},
				"required": []string{"process_id"},
			},
		},
		{
			Name:        "exec_kill",
			Kind:        ToolKindStructuredExec,
			Description: "Cancel a structured executable invocation.",
			Parameters: map[string]any{
				"type": "object",
				"properties": map[string]any{
					"process_id": map[string]any{"type": "string"},
				},
				"required": []string{"process_id"},
			},
		},
		{
			Name:        "shell_snippet_start",
			Kind:        ToolKindShellSnippet,
			Description: "Start a shell snippet with shell parsing semantics.",
			Parameters: map[string]any{
				"type": "object",
				"properties": map[string]any{
					"script": map[string]any{"type": "string"},
					"cwd":    map[string]any{"type": "string"},
					"env": map[string]any{
						"type": "object",
						"additionalProperties": map[string]any{
							"type": "string",
						},
					},
				},
				"required": []string{"script"},
			},
		},
		{
			Name:        "shell_snippet_wait",
			Kind:        ToolKindShellSnippet,
			Description: "Wait for a shell snippet to finish.",
			Parameters: map[string]any{
				"type": "object",
				"properties": map[string]any{
					"process_id": map[string]any{"type": "string"},
				},
				"required": []string{"process_id"},
			},
		},
		{
			Name:        "shell_snippet_kill",
			Kind:        ToolKindShellSnippet,
			Description: "Cancel a shell snippet.",
			Parameters: map[string]any{
				"type": "object",
				"properties": map[string]any{
					"process_id": map[string]any{"type": "string"},
				},
				"required": []string{"process_id"},
			},
		},
	}
}

func DescriptorForToolV2(toolName string) (ToolDescriptorV2, bool) {
	for _, descriptor := range ToolDescriptorsV2() {
		if descriptor.Name == toolName {
			return descriptor, true
		}
	}
	return ToolDescriptorV2{}, false
}

func ValidateExecStartContract(input ExecStartContractInput) error {
	executable := strings.TrimSpace(input.Executable)
	if executable == "" {
		return fmt.Errorf("exec_start executable is required")
	}
	if hasShellStructure(executable) {
		return fmt.Errorf("exec_start executable must be a literal executable, not shell syntax")
	}
	if isShellBuiltin(executable) {
		return fmt.Errorf("exec_start executable %q must be launched through shell_snippet_start", executable)
	}
	return nil
}

func ValidateShellSnippetContract(input ShellSnippetContractInput) error {
	if strings.TrimSpace(input.Script) == "" {
		return fmt.Errorf("shell_snippet_start script is required")
	}
	return nil
}

func hasShellStructure(executable string) bool {
	return strings.ContainsAny(executable, "&|><;")
}

func isShellBuiltin(executable string) bool {
	switch executable {
	case ".", ":", "[", "alias", "bg", "bind", "break", "builtin", "cd", "command", "continue", "declare", "dirs", "disown", "echo", "enable", "eval", "exec", "exit", "export", "fc", "fg", "getopts", "hash", "help", "history", "jobs", "kill", "let", "local", "logout", "popd", "printf", "pushd", "pwd", "read", "readonly", "return", "set", "shift", "source", "test", "times", "trap", "type", "typeset", "ulimit", "umask", "unalias", "unset", "wait":
		return true
	default:
		return false
	}
}
