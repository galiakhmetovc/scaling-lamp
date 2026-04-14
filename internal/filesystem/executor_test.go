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

func TestExecutorListsDirectoryEntriesInsideWorkspaceScope(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	if err := os.MkdirAll(filepath.Join(dir, "src"), 0o755); err != nil {
		t.Fatalf("MkdirAll: %v", err)
	}
	if err := os.WriteFile(filepath.Join(dir, "src", "a.txt"), []byte("a"), 0o644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}
	if err := os.WriteFile(filepath.Join(dir, "src", "b.txt"), []byte("b"), 0o644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}
	executor := filesystem.NewExecutor()
	out, err := executor.Execute(contracts.FilesystemExecutionContract{
		Scope: contracts.FilesystemScopePolicy{
			Enabled:  true,
			Strategy: "workspace_only",
			Params: contracts.FilesystemScopeParams{
				RootPath:     dir,
				ReadSubpaths: []string{"src"},
			},
		},
	}, "fs_list", map[string]any{
		"path": "src",
	})
	if err != nil {
		t.Fatalf("Execute returned error: %v", err)
	}
	var payload map[string]any
	if err := json.Unmarshal([]byte(out), &payload); err != nil {
		t.Fatalf("unmarshal result: %v", err)
	}
	entries, ok := payload["entries"].([]any)
	if !ok || len(entries) != 2 {
		t.Fatalf("entries = %#v", payload["entries"])
	}
}

func TestExecutorReadsTextInsideWorkspaceScope(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	path := filepath.Join(dir, "src", "note.txt")
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatalf("MkdirAll: %v", err)
	}
	if err := os.WriteFile(path, []byte("hello read"), 0o644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}
	executor := filesystem.NewExecutor()
	out, err := executor.Execute(contracts.FilesystemExecutionContract{
		Scope: contracts.FilesystemScopePolicy{
			Enabled:  true,
			Strategy: "workspace_only",
			Params: contracts.FilesystemScopeParams{
				RootPath:     dir,
				ReadSubpaths: []string{"src"},
			},
		},
		IO: contracts.FilesystemIOPolicy{
			Enabled:  true,
			Strategy: "bounded_text_io",
			Params: contracts.FilesystemIOParams{
				MaxReadBytes: 1024,
				Encoding:     "utf-8",
			},
		},
	}, "fs_read_text", map[string]any{
		"path": "src/note.txt",
	})
	if err != nil {
		t.Fatalf("Execute returned error: %v", err)
	}
	var payload map[string]any
	if err := json.Unmarshal([]byte(out), &payload); err != nil {
		t.Fatalf("unmarshal result: %v", err)
	}
	if payload["content"] != "hello read" {
		t.Fatalf("content = %#v, want hello read", payload["content"])
	}
}

func TestExecutorPatchesTextInsideWorkspaceScope(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	path := filepath.Join(dir, "src", "patch.txt")
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatalf("MkdirAll: %v", err)
	}
	if err := os.WriteFile(path, []byte("hello old world"), 0o644); err != nil {
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
	}, "fs_patch_text", map[string]any{
		"path":    "src/patch.txt",
		"search":  "old",
		"replace": "new",
	})
	if err != nil {
		t.Fatalf("Execute returned error: %v", err)
	}
	if got := mustReadFile(t, path); got != "hello new world" {
		t.Fatalf("patched content = %q, want hello new world", got)
	}
}

func TestExecutorCreatesDirectoryInsideWorkspaceScope(t *testing.T) {
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
				AllowMkdir: true,
			},
		},
	}, "fs_mkdir", map[string]any{
		"path": "src/newdir",
	})
	if err != nil {
		t.Fatalf("Execute returned error: %v", err)
	}
	if _, err := os.Stat(filepath.Join(dir, "src", "newdir")); err != nil {
		t.Fatalf("Stat returned error: %v", err)
	}
}

func TestExecutorMovesFileInsideWorkspaceScope(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	src := filepath.Join(dir, "src", "a.txt")
	if err := os.MkdirAll(filepath.Dir(src), 0o755); err != nil {
		t.Fatalf("MkdirAll: %v", err)
	}
	if err := os.WriteFile(src, []byte("move me"), 0o644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}
	executor := filesystem.NewExecutor()
	_, err := executor.Execute(contracts.FilesystemExecutionContract{
		Scope: contracts.FilesystemScopePolicy{
			Enabled:  true,
			Strategy: "workspace_only",
			Params: contracts.FilesystemScopeParams{
				RootPath:      dir,
				WriteSubpaths: []string{"src", "dst"},
			},
		},
		Mutation: contracts.FilesystemMutationPolicy{
			Enabled:  true,
			Strategy: "allow_writes",
			Params: contracts.FilesystemMutationParams{
				AllowMove: true,
			},
		},
	}, "fs_move", map[string]any{
		"src":  "src/a.txt",
		"dest": "dst/b.txt",
	})
	if err != nil {
		t.Fatalf("Execute returned error: %v", err)
	}
	if _, err := os.Stat(src); !os.IsNotExist(err) {
		t.Fatalf("src still exists or wrong error: %v", err)
	}
	if got := mustReadFile(t, filepath.Join(dir, "dst", "b.txt")); got != "move me" {
		t.Fatalf("moved content = %q, want move me", got)
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
