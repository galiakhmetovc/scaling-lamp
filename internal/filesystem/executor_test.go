package filesystem_test

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"

	"teamd/internal/contracts"
	"teamd/internal/filesystem"
)

func TestExecutorWritesTextInsideWorkspaceScope(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	executor := filesystem.NewExecutor()
	out, err := executor.Execute(contracts.FilesystemExecutionContract{
		Scope: contracts.FilesystemScopePolicy{
			Enabled:  true,
			Strategy: "workspace_only",
			Params: contracts.FilesystemScopeParams{
				RootPath:      dir,
				WriteSubpaths: []string{"src"},
			},
		},
		Mutation: contracts.FilesystemMutationPolicy{
			Enabled:  true,
			Strategy: "allow_writes",
			Params: contracts.FilesystemMutationParams{
				AllowWrite: true,
			},
		},
		IO: contracts.FilesystemIOPolicy{
			Enabled:  true,
			Strategy: "bounded_text_io",
			Params: contracts.FilesystemIOParams{
				MaxWriteBytes: 1024,
				Encoding:      "utf-8",
			},
		},
	}, "fs_write_text", map[string]any{
		"path":    "src/hello.txt",
		"content": "hello",
	})
	if err != nil {
		t.Fatalf("Execute returned error: %v", err)
	}
	if got := mustReadFile(t, filepath.Join(dir, "src", "hello.txt")); got != "hello" {
		t.Fatalf("file content = %q, want hello", got)
	}
	var payload map[string]any
	if err := json.Unmarshal([]byte(out), &payload); err != nil {
		t.Fatalf("unmarshal result: %v", err)
	}
	if payload["status"] != "ok" {
		t.Fatalf("status = %#v, want ok", payload["status"])
	}
}

func TestExecutorRejectsWriteOutsideWorkspaceScope(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	executor := filesystem.NewExecutor()
	_, err := executor.Execute(contracts.FilesystemExecutionContract{
		Scope: contracts.FilesystemScopePolicy{
			Enabled:  true,
			Strategy: "workspace_only",
			Params: contracts.FilesystemScopeParams{
				RootPath:      dir,
				WriteSubpaths: []string{"src"},
			},
		},
		Mutation: contracts.FilesystemMutationPolicy{
			Enabled:  true,
			Strategy: "allow_writes",
			Params: contracts.FilesystemMutationParams{
				AllowWrite: true,
			},
		},
	}, "fs_write_text", map[string]any{
		"path":    "../escape.txt",
		"content": "nope",
	})
	if err == nil {
		t.Fatal("Execute returned nil error, want scope failure")
	}
}

func TestExecutorMovesPathToTrash(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	path := filepath.Join(dir, "src", "old.txt")
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatalf("MkdirAll: %v", err)
	}
	if err := os.WriteFile(path, []byte("hello"), 0o644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}
	executor := filesystem.NewExecutor()
	_, err := executor.Execute(contracts.FilesystemExecutionContract{
		Scope: contracts.FilesystemScopePolicy{
			Enabled:  true,
			Strategy: "workspace_only",
			Params: contracts.FilesystemScopeParams{
				RootPath:      dir,
				WriteSubpaths: []string{"src"},
			},
		},
		Mutation: contracts.FilesystemMutationPolicy{
			Enabled:  true,
			Strategy: "trash_only_delete",
			Params: contracts.FilesystemMutationParams{
				TrashDir: ".trash",
			},
		},
	}, "fs_trash", map[string]any{
		"path": "src/old.txt",
	})
	if err != nil {
		t.Fatalf("Execute returned error: %v", err)
	}
	if _, err := os.Stat(path); !os.IsNotExist(err) {
		t.Fatalf("original path still exists or wrong error: %v", err)
	}
	entries, err := os.ReadDir(filepath.Join(dir, ".trash"))
	if err != nil {
		t.Fatalf("ReadDir trash: %v", err)
	}
	if len(entries) != 1 {
		t.Fatalf("trash entries = %d, want 1", len(entries))
	}
}

func mustReadFile(t *testing.T, path string) string {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("ReadFile(%q): %v", path, err)
	}
	return string(data)
}
