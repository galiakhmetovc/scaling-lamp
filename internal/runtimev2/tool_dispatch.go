package runtimev2

import "fmt"

type DispatchKindV2 string

const (
	DispatchKindStructuredExec DispatchKindV2 = "structured-exec"
	DispatchKindShellSnippet   DispatchKindV2 = "shell-snippet"
)

func DispatchKindForToolV2(toolName string) (DispatchKindV2, bool) {
	switch toolName {
	case "exec_start", "exec_wait", "exec_kill":
		return DispatchKindStructuredExec, true
	case "shell_snippet_start", "shell_snippet_wait", "shell_snippet_kill":
		return DispatchKindShellSnippet, true
	default:
		return "", false
	}
}

func DispatchForToolV2(toolName string) (DispatchKindV2, ToolDescriptorV2, error) {
	kind, ok := DispatchKindForToolV2(toolName)
	if !ok {
		return "", ToolDescriptorV2{}, fmt.Errorf("tool %q is not part of runtime v2", toolName)
	}
	descriptor, ok := DescriptorForToolV2(toolName)
	if !ok {
		return "", ToolDescriptorV2{}, fmt.Errorf("tool %q descriptor is missing", toolName)
	}
	return kind, descriptor, nil
}
