package mcp_test

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"teamd/internal/mcp"
	mcptools "teamd/internal/mcp/tools"
)

func TestRuntimeListsRegisteredTools(t *testing.T) {
	runtime := mcp.NewRuntime()
	runtime.Register(mcp.Tool{
		Name:        "filesystem.read_file",
		Description: "Read a file",
	})

	tools, err := runtime.ListTools("researcher")
	if err != nil {
		t.Fatalf("list tools: %v", err)
	}
	if len(tools) != 1 || tools[0].Name != "filesystem.read_file" {
		t.Fatalf("unexpected tools: %#v", tools)
	}
}

func TestFilesystemToolsReadAndList(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "note.txt")
	if err := os.WriteFile(path, []byte("hello"), 0o644); err != nil {
		t.Fatalf("write fixture: %v", err)
	}

	runtime := mcp.NewRuntime()
	mcptools.RegisterFilesystemTools(runtime, dir)

	list, err := runtime.CallTool(context.Background(), "filesystem.list_dir", mcp.CallInput{
		Arguments: map[string]any{"path": dir},
	})
	if err != nil {
		t.Fatalf("list dir: %v", err)
	}
	if !strings.Contains(list.Content, "note.txt") {
		t.Fatalf("unexpected list result: %q", list.Content)
	}

	read, err := runtime.CallTool(context.Background(), "filesystem.read_file", mcp.CallInput{
		Arguments: map[string]any{"path": path},
	})
	if err != nil {
		t.Fatalf("read file: %v", err)
	}
	if read.Content != "hello" {
		t.Fatalf("unexpected file content: %q", read.Content)
	}
}

func TestShellToolExecutesCommand(t *testing.T) {
	runtime := mcp.NewRuntime()
	mcptools.RegisterShellTools(runtime)

	out, err := runtime.CallTool(context.Background(), "shell.exec", mcp.CallInput{
		Arguments: map[string]any{
			"command": "printf hello",
		},
	})
	if err != nil {
		t.Fatalf("shell exec: %v", err)
	}
	if out.Content != "hello" {
		t.Fatalf("unexpected shell output: %q", out.Content)
	}
}
