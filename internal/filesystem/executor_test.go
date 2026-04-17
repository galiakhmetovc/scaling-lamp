package filesystem_test

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
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
	if payload["created"] != true {
		t.Fatalf("created = %#v, want true", payload["created"])
	}
	if payload["overwritten"] != false {
		t.Fatalf("overwritten = %#v, want false", payload["overwritten"])
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

func TestExecutorMovesPathToTrashWhenMoveAllowed(t *testing.T) {
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
			Strategy: "allow_writes",
			Params: contracts.FilesystemMutationParams{
				AllowMove: true,
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
	if payload["entry_count"] != float64(2) {
		t.Fatalf("entry_count = %#v, want 2", payload["entry_count"])
	}
}

func TestExecutorWriteTextCreateModeRejectsExistingFile(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	path := filepath.Join(dir, "src", "hello.txt")
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatalf("MkdirAll: %v", err)
	}
	if err := os.WriteFile(path, []byte("old"), 0o644); err != nil {
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
			Params:   contracts.FilesystemMutationParams{AllowWrite: true},
		},
	}, "fs_write_text", map[string]any{
		"path":    "src/hello.txt",
		"content": "new",
		"mode":    "create",
	})
	if err == nil {
		t.Fatal("Execute returned nil error, want create-mode conflict")
	}
}

func TestExecutorWriteTextOverwriteModeRejectsMissingFile(t *testing.T) {
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
			Params:   contracts.FilesystemMutationParams{AllowWrite: true},
		},
	}, "fs_write_text", map[string]any{
		"path":    "src/hello.txt",
		"content": "new",
		"mode":    "overwrite",
	})
	if err == nil {
		t.Fatal("Execute returned nil error, want overwrite-mode missing-file failure")
	}
}

func TestExecutorWriteTextOverwriteModeMarksOverwrite(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	path := filepath.Join(dir, "src", "hello.txt")
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatalf("MkdirAll: %v", err)
	}
	if err := os.WriteFile(path, []byte("old"), 0o644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}
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
			Params:   contracts.FilesystemMutationParams{AllowWrite: true},
		},
	}, "fs_write_text", map[string]any{
		"path":    "src/hello.txt",
		"content": "new",
		"mode":    "overwrite",
	})
	if err != nil {
		t.Fatalf("Execute returned error: %v", err)
	}
	if got := mustReadFile(t, path); got != "new" {
		t.Fatalf("file content = %q, want new", got)
	}
	var payload map[string]any
	if err := json.Unmarshal([]byte(out), &payload); err != nil {
		t.Fatalf("unmarshal result: %v", err)
	}
	if payload["created"] != false {
		t.Fatalf("created = %#v, want false", payload["created"])
	}
	if payload["overwritten"] != true {
		t.Fatalf("overwritten = %#v, want true", payload["overwritten"])
	}
	if payload["mode"] != "overwrite" {
		t.Fatalf("mode = %#v, want overwrite", payload["mode"])
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

func TestExecutorReadsLineRangeInsideWorkspaceScope(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	path := filepath.Join(dir, "src", "note.txt")
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatalf("MkdirAll: %v", err)
	}
	if err := os.WriteFile(path, []byte("one\ntwo\nthree\nfour\n"), 0o644); err != nil {
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
	}, "fs_read_lines", map[string]any{
		"path":       "src/note.txt",
		"start_line": 2,
		"end_line":   3,
	})
	if err != nil {
		t.Fatalf("Execute returned error: %v", err)
	}
	var payload struct {
		Status string `json:"status"`
		Lines  []struct {
			Line int    `json:"line"`
			Text string `json:"text"`
		} `json:"lines"`
	}
	if err := json.Unmarshal([]byte(out), &payload); err != nil {
		t.Fatalf("unmarshal result: %v", err)
	}
	if payload.Status != "ok" {
		t.Fatalf("status = %q, want ok", payload.Status)
	}
	if len(payload.Lines) != 2 || payload.Lines[0].Line != 2 || payload.Lines[0].Text != "two" || payload.Lines[1].Line != 3 || payload.Lines[1].Text != "three" {
		t.Fatalf("lines = %#v", payload.Lines)
	}
}

func TestExecutorSearchesTextAndReturnsLineNumbers(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	path := filepath.Join(dir, "src", "note.txt")
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatalf("MkdirAll: %v", err)
	}
	if err := os.WriteFile(path, []byte("alpha\nbeta alpha\ngamma\nalpha tail\n"), 0o644); err != nil {
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
	}, "fs_search_text", map[string]any{
		"path":  "src/note.txt",
		"query": "alpha",
	})
	if err != nil {
		t.Fatalf("Execute returned error: %v", err)
	}
	var payload struct {
		Matches []struct {
			Line int    `json:"line"`
			Text string `json:"text"`
		} `json:"matches"`
	}
	if err := json.Unmarshal([]byte(out), &payload); err != nil {
		t.Fatalf("unmarshal result: %v", err)
	}
	if len(payload.Matches) != 3 {
		t.Fatalf("matches len = %d, want 3", len(payload.Matches))
	}
	if payload.Matches[1].Line != 2 || payload.Matches[1].Text != "beta alpha" {
		t.Fatalf("second match = %#v", payload.Matches[1])
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

func TestExecutorReplacesLineRangeInsideWorkspaceScope(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	path := filepath.Join(dir, "src", "replace.txt")
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatalf("MkdirAll: %v", err)
	}
	if err := os.WriteFile(path, []byte("one\ntwo\nthree\nfour\n"), 0o644); err != nil {
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
	}, "fs_replace_lines", map[string]any{
		"path":       "src/replace.txt",
		"start_line": 2,
		"end_line":   3,
		"content":    "middle a\nmiddle b",
	})
	if err != nil {
		t.Fatalf("Execute returned error: %v", err)
	}
	if got := mustReadFile(t, path); got != "one\nmiddle a\nmiddle b\nfour\n" {
		t.Fatalf("replaced content = %q", got)
	}
}

func TestExecutorInsertsTextAroundLineOrFileEdges(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	base := filepath.Join(dir, "src")
	if err := os.MkdirAll(base, 0o755); err != nil {
		t.Fatalf("MkdirAll: %v", err)
	}
	cases := []struct {
		name     string
		position string
		line     int
		want     string
	}{
		{name: "before", position: "before", line: 2, want: "one\ninserted\ntwo\nthree\n"},
		{name: "after", position: "after", line: 2, want: "one\ntwo\ninserted\nthree\n"},
		{name: "prepend", position: "prepend", line: 0, want: "inserted\none\ntwo\nthree\n"},
		{name: "append", position: "append", line: 0, want: "one\ntwo\nthree\ninserted\n"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			path := filepath.Join(base, tc.name+".txt")
			if err := os.WriteFile(path, []byte("one\ntwo\nthree\n"), 0o644); err != nil {
				t.Fatalf("WriteFile: %v", err)
			}
			executor := filesystem.NewExecutor()
			args := map[string]any{
				"path":     filepath.Join("src", tc.name+".txt"),
				"position": tc.position,
				"content":  "inserted",
			}
			if tc.line > 0 {
				args["line"] = tc.line
			}
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
			}, "fs_insert_text", args)
			if err != nil {
				t.Fatalf("Execute returned error: %v", err)
			}
			if got := mustReadFile(t, path); got != tc.want {
				t.Fatalf("inserted content = %q, want %q", got, tc.want)
			}
		})
	}
}

func TestExecutorRejectsInvalidLineRanges(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	path := filepath.Join(dir, "src", "bad.txt")
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatalf("MkdirAll: %v", err)
	}
	if err := os.WriteFile(path, []byte("one\ntwo\n"), 0o644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}
	executor := filesystem.NewExecutor()
	_, err := executor.Execute(contracts.FilesystemExecutionContract{
		Scope: contracts.FilesystemScopePolicy{
			Enabled:  true,
			Strategy: "workspace_only",
			Params: contracts.FilesystemScopeParams{
				RootPath:     dir,
				ReadSubpaths: []string{"src"},
			},
		},
	}, "fs_read_lines", map[string]any{
		"path":       "src/bad.txt",
		"start_line": 3,
		"end_line":   2,
	})
	if err == nil || !strings.Contains(err.Error(), "start_line") {
		t.Fatalf("error = %v, want invalid range failure", err)
	}
}

func TestExecutorClampsEndLineToFileLength(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	path := filepath.Join(dir, "src", "note.txt")
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatalf("MkdirAll: %v", err)
	}
	if err := os.WriteFile(path, []byte("one\ntwo\nthree\n"), 0o644); err != nil {
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
	}, "fs_read_lines", map[string]any{
		"path":       "src/note.txt",
		"start_line": 2,
		"end_line":   999,
	})
	if err != nil {
		t.Fatalf("Execute returned error: %v", err)
	}
	var payload struct {
		Status string `json:"status"`
		Lines  []struct {
			Line int    `json:"line"`
			Text string `json:"text"`
		} `json:"lines"`
	}
	if err := json.Unmarshal([]byte(out), &payload); err != nil {
		t.Fatalf("unmarshal result: %v", err)
	}
	if payload.Status != "ok" {
		t.Fatalf("status = %q, want ok", payload.Status)
	}
	if len(payload.Lines) != 2 || payload.Lines[0].Line != 2 || payload.Lines[0].Text != "two" || payload.Lines[1].Line != 3 || payload.Lines[1].Text != "three" {
		t.Fatalf("lines = %#v", payload.Lines)
	}
}

func TestExecutorFindsInFilesWithinScope(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	if err := os.MkdirAll(filepath.Join(dir, "src", "nested"), 0o755); err != nil {
		t.Fatalf("MkdirAll: %v", err)
	}
	if err := os.WriteFile(filepath.Join(dir, "src", "a.txt"), []byte("one\nneedle here\n"), 0o644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}
	if err := os.WriteFile(filepath.Join(dir, "src", "nested", "b.txt"), []byte("needle again\nother\n"), 0o644); err != nil {
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
				MaxReadBytes:   2048,
				MaxSearchFiles: 10,
				MaxSearchHits:  10,
			},
		},
	}, "fs_find_in_files", map[string]any{
		"query": "needle",
		"glob":  "src/**/*.txt",
	})
	if err != nil {
		t.Fatalf("Execute returned error: %v", err)
	}
	var payload struct {
		Matches []struct {
			Path string `json:"path"`
			Line int    `json:"line"`
			Text string `json:"text"`
		} `json:"matches"`
	}
	if err := json.Unmarshal([]byte(out), &payload); err != nil {
		t.Fatalf("unmarshal result: %v", err)
	}
	if len(payload.Matches) != 2 {
		t.Fatalf("matches len = %d, want 2", len(payload.Matches))
	}
}

func TestExecutorReplacesInSingleLine(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	path := filepath.Join(dir, "src", "line.txt")
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatalf("MkdirAll: %v", err)
	}
	if err := os.WriteFile(path, []byte("one\nbeta alpha\nthree\n"), 0o644); err != nil {
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
			Params:   contracts.FilesystemMutationParams{AllowWrite: true},
		},
		IO: contracts.FilesystemIOPolicy{
			Enabled:  true,
			Strategy: "bounded_text_io",
			Params: contracts.FilesystemIOParams{
				MaxWriteBytes: 1024,
			},
		},
	}, "fs_replace_in_line", map[string]any{
		"path":    "src/line.txt",
		"line":    2,
		"search":  "alpha",
		"replace": "omega",
	})
	if err != nil {
		t.Fatalf("Execute returned error: %v", err)
	}
	if got := mustReadFile(t, path); got != "one\nbeta omega\nthree\n" {
		t.Fatalf("replaced content = %q", got)
	}
}

func TestExecutorReplacesAcrossFilesWithinLimits(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	if err := os.MkdirAll(filepath.Join(dir, "src"), 0o755); err != nil {
		t.Fatalf("MkdirAll: %v", err)
	}
	if err := os.WriteFile(filepath.Join(dir, "src", "a.txt"), []byte("needle x\n"), 0o644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}
	if err := os.WriteFile(filepath.Join(dir, "src", "b.txt"), []byte("needle y\n"), 0o644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}
	executor := filesystem.NewExecutor()
	out, err := executor.Execute(contracts.FilesystemExecutionContract{
		Scope: contracts.FilesystemScopePolicy{
			Enabled:  true,
			Strategy: "workspace_only",
			Params: contracts.FilesystemScopeParams{
				RootPath:      dir,
				ReadSubpaths:  []string{"src"},
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
				MaxReadBytes:    2048,
				MaxWriteBytes:   2048,
				MaxReplaceFiles: 4,
				MaxReplaceHits:  4,
			},
		},
	}, "fs_replace_in_files", map[string]any{
		"query":   "needle",
		"replace": "done",
		"glob":    "src/*.txt",
	})
	if err != nil {
		t.Fatalf("Execute returned error: %v", err)
	}
	if got := mustReadFile(t, filepath.Join(dir, "src", "a.txt")); got != "done x\n" {
		t.Fatalf("a.txt content = %q", got)
	}
	if got := mustReadFile(t, filepath.Join(dir, "src", "b.txt")); got != "done y\n" {
		t.Fatalf("b.txt content = %q", got)
	}
	var payload struct {
		ChangedFiles int `json:"changed_files"`
		ReplaceHits  int `json:"replace_hits"`
	}
	if err := json.Unmarshal([]byte(out), &payload); err != nil {
		t.Fatalf("unmarshal result: %v", err)
	}
	if payload.ChangedFiles != 2 || payload.ReplaceHits != 2 {
		t.Fatalf("payload = %#v", payload)
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
