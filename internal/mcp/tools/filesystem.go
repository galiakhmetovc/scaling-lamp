package tools

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"teamd/internal/mcp"
)

type registrar interface {
	Register(tool mcp.Tool)
}

func NewRuntimeWithLocalTools(root string) *mcp.Runtime {
	runtime := mcp.NewRuntime()
	RegisterFilesystemTools(runtime, root)
	RegisterShellTools(runtime)
	return runtime
}

func RegisterFilesystemTools(runtime registrar, root string) {
	runtime.Register(mcp.Tool{
		Name:        "filesystem.read_file",
		Description: "Read a file from the local filesystem.",
		Parameters: map[string]any{
			"type": "object",
			"properties": map[string]any{
				"path": map[string]any{"type": "string"},
			},
			"required": []string{"path"},
		},
		Call: func(_ context.Context, input mcp.CallInput) (mcp.CallResult, error) {
			path, err := sanitizePath(root, stringArg(input, "path"))
			if err != nil {
				return mcp.CallResult{}, err
			}

			body, err := os.ReadFile(path)
			if err != nil {
				return mcp.CallResult{}, err
			}
			return mcp.CallResult{Content: string(body)}, nil
		},
	})

	runtime.Register(mcp.Tool{
		Name:        "filesystem.write_file",
		Description: "Write a file to the local filesystem.",
		Parameters: map[string]any{
			"type": "object",
			"properties": map[string]any{
				"path":    map[string]any{"type": "string"},
				"content": map[string]any{"type": "string"},
			},
			"required": []string{"path", "content"},
		},
		Call: func(_ context.Context, input mcp.CallInput) (mcp.CallResult, error) {
			path, err := sanitizePath(root, stringArg(input, "path"))
			if err != nil {
				return mcp.CallResult{}, err
			}

			if err := os.WriteFile(path, []byte(stringArg(input, "content")), 0o644); err != nil {
				return mcp.CallResult{}, err
			}
			return mcp.CallResult{Content: path}, nil
		},
	})

	runtime.Register(mcp.Tool{
		Name:        "filesystem.list_dir",
		Description: "List files in a local directory.",
		Parameters: map[string]any{
			"type": "object",
			"properties": map[string]any{
				"path": map[string]any{"type": "string"},
			},
			"required": []string{"path"},
		},
		Call: func(_ context.Context, input mcp.CallInput) (mcp.CallResult, error) {
			path, err := sanitizePath(root, stringArg(input, "path"))
			if err != nil {
				return mcp.CallResult{}, err
			}

			entries, err := os.ReadDir(path)
			if err != nil {
				return mcp.CallResult{}, err
			}

			names := make([]string, 0, len(entries))
			for _, entry := range entries {
				names = append(names, entry.Name())
			}
			return mcp.CallResult{Content: strings.Join(names, "\n")}, nil
		},
	})
}

func sanitizePath(root, input string) (string, error) {
	if root == "" {
		root = string(filepath.Separator)
	}

	cleanRoot, err := filepath.Abs(filepath.Clean(root))
	if err != nil {
		return "", fmt.Errorf("clean root: %w", err)
	}

	target := input
	if target == "" {
		target = cleanRoot
	}
	if !filepath.IsAbs(target) {
		target = filepath.Join(cleanRoot, target)
	}

	cleanTarget, err := filepath.Abs(filepath.Clean(target))
	if err != nil {
		return "", fmt.Errorf("clean path: %w", err)
	}

	rel, err := filepath.Rel(cleanRoot, cleanTarget)
	if err != nil {
		return "", fmt.Errorf("rel path: %w", err)
	}
	if rel == ".." || strings.HasPrefix(rel, ".."+string(filepath.Separator)) {
		return "", fmt.Errorf("path %q outside allowed root %q", input, cleanRoot)
	}

	return cleanTarget, nil
}

func stringArg(input mcp.CallInput, key string) string {
	if input.Arguments == nil {
		return ""
	}
	value, _ := input.Arguments[key].(string)
	return value
}
