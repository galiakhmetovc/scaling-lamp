package shell

import (
	"fmt"

	"teamd/internal/contracts"
	"teamd/internal/tools"
)

type DefinitionExecutor struct{}

func NewDefinitionExecutor() *DefinitionExecutor {
	return &DefinitionExecutor{}
}

func (e *DefinitionExecutor) Build(contract contracts.ShellToolContract) ([]tools.Definition, error) {
	if e == nil {
		return nil, fmt.Errorf("shell definition executor is nil")
	}
	if !contract.Catalog.Enabled {
		return nil, nil
	}
	if contract.Catalog.Strategy != "static_allowlist" {
		return nil, fmt.Errorf("unsupported shell catalog strategy %q", contract.Catalog.Strategy)
	}
	if contract.Description.Enabled && contract.Description.Strategy != "static_builtin_descriptions" {
		return nil, fmt.Errorf("unsupported shell description strategy %q", contract.Description.Strategy)
	}

	all := defaultDefinitions()
	byID := make(map[string]tools.Definition, len(all))
	for _, definition := range all {
		byID[definition.ID] = definition
	}

	out := make([]tools.Definition, 0, len(contract.Catalog.Params.ToolIDs))
	for _, id := range contract.Catalog.Params.ToolIDs {
		definition, ok := byID[id]
		if !ok {
			return nil, fmt.Errorf("shell tool %q is not defined", id)
		}
		out = append(out, definition)
	}
	return out, nil
}

func defaultDefinitions() []tools.Definition {
	return []tools.Definition{
		{
			ID:          "shell_exec",
			Name:        "shell_exec",
			Description: "Run one bounded non-interactive shell command inside the configured workspace scope. Pass the executable name in command and each argument separately in args. Do not send a whole shell snippet in command. Shell policy may also restrict which argument shapes are allowed for a given command. Windows builtin commands like echo, dir, and type are supported and may be launched through cmd automatically. Windows launchers like powershell, pwsh, and cmd may also be allowlisted directly when the operator wants shell-managed networked commands. POSIX example: {\"command\":\"printf\",\"args\":[\"hello\\n\"]}. Windows example: builtin {\"command\":\"echo\",\"args\":[\"hello\"]} or pwsh {\"command\":\"pwsh\",\"args\":[\"-NoProfile\",\"-Command\",\"Invoke-WebRequest https://example.com\"]}.",
			Parameters: map[string]any{
				"type": "object",
				"properties": map[string]any{
					"command": map[string]any{"type": "string"},
					"args": map[string]any{
						"type":  "array",
						"items": map[string]any{"type": "string"},
					},
					"cwd": map[string]any{"type": "string"},
				},
				"required": []string{"command"},
			},
		},
		{
			ID:          "shell_start",
			Name:        "shell_start",
			Description: "Start a bounded shell command asynchronously and return a command_id. Use shell_poll to fetch intermediate stdout/stderr chunks and shell_kill to stop it if needed. Shell policy may also restrict which argument shapes are allowed for a given command.",
			Parameters: map[string]any{
				"type": "object",
				"properties": map[string]any{
					"command": map[string]any{"type": "string"},
					"args": map[string]any{
						"type":  "array",
						"items": map[string]any{"type": "string"},
					},
					"cwd": map[string]any{"type": "string"},
				},
				"required": []string{"command"},
			},
		},
		{
			ID:          "shell_poll",
			Name:        "shell_poll",
			Description: "Fetch new output chunks and current status for a previously started shell command. Pass after_offset to receive only chunks after the last seen offset.",
			Parameters: map[string]any{
				"type": "object",
				"properties": map[string]any{
					"command_id":   map[string]any{"type": "string"},
					"after_offset": map[string]any{"type": "integer"},
				},
				"required": []string{"command_id"},
			},
		},
		{
			ID:          "shell_kill",
			Name:        "shell_kill",
			Description: "Request termination of a previously started shell command by command_id.",
			Parameters: map[string]any{
				"type": "object",
				"properties": map[string]any{
					"command_id": map[string]any{"type": "string"},
				},
				"required": []string{"command_id"},
			},
		},
	}
}
