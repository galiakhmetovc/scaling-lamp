package vfs

import (
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestRootConfinesPaths(t *testing.T) {
	base := t.TempDir()
	outside := filepath.Join(base, "outside.txt")
	if err := os.WriteFile(outside, []byte("outside"), 0o644); err != nil {
		t.Fatal(err)
	}

	rootDir := filepath.Join(base, "root")
	vfsRoot, err := New(rootDir)
	if err != nil {
		t.Fatal(err)
	}

	if _, err := vfsRoot.ReadFile("../outside.txt"); !errors.Is(err, ErrOutsideRoot) {
		t.Fatalf("expected outside-root error, got %v", err)
	}
	if _, err := vfsRoot.ReadFile(outside); !errors.Is(err, ErrOutsideRoot) {
		t.Fatalf("expected outside-root error, got %v", err)
	}

	if err := os.Symlink(outside, filepath.Join(rootDir, "escape")); err != nil {
		t.Fatal(err)
	}
	if _, err := vfsRoot.ReadFile("escape"); !errors.Is(err, ErrOutsideRoot) {
		t.Fatalf("expected symlink escape rejection, got %v", err)
	}
}

func TestRootCoreOperations(t *testing.T) {
	base := t.TempDir()
	vfsRoot, err := New(filepath.Join(base, "workspace"))
	if err != nil {
		t.Fatal(err)
	}

	if err := vfsRoot.Mkdir("notes"); err != nil {
		t.Fatal(err)
	}
	if err := vfsRoot.WriteFile("notes/a.txt", []byte("alpha\nbeta\ngamma\n")); err != nil {
		t.Fatal(err)
	}
	if err := vfsRoot.Touch("notes/empty.txt"); err != nil {
		t.Fatal(err)
	}

	list, err := vfsRoot.List("notes")
	if err != nil {
		t.Fatal(err)
	}
	if got, want := len(list), 2; got != want {
		t.Fatalf("unexpected list size: got=%d want=%d", got, want)
	}
	if list[0].Name != "a.txt" || list[1].Name != "empty.txt" {
		t.Fatalf("unexpected list order: %#v", list)
	}

	tree, err := vfsRoot.Tree(".")
	if err != nil {
		t.Fatal(err)
	}
	if len(tree) != 3 {
		t.Fatalf("unexpected tree size: %#v", tree)
	}

	content, err := vfsRoot.ReadFile("notes/a.txt")
	if err != nil {
		t.Fatal(err)
	}
	if got, want := string(content.Content), "alpha\nbeta\ngamma\n"; got != want {
		t.Fatalf("unexpected content: got=%q want=%q", got, want)
	}
	if content.Size != int64(len(content.Content)) {
		t.Fatalf("unexpected size: %#v", content)
	}
	if content.Lines != 4 {
		t.Fatalf("unexpected line count: %#v", content)
	}

	lines, err := vfsRoot.ReadLines("notes/a.txt", 2, 3)
	if err != nil {
		t.Fatal(err)
	}
	if got, want := strings.Join(lines, ","), "beta,gamma"; got != want {
		t.Fatalf("unexpected lines: got=%q want=%q", got, want)
	}

	hits, err := vfsRoot.Search(".", "beta")
	if err != nil {
		t.Fatal(err)
	}
	if len(hits) != 1 || hits[0].Line != 2 || !strings.HasSuffix(hits[0].Path, "notes/a.txt") {
		t.Fatalf("unexpected hits: %#v", hits)
	}

	diff, err := vfsRoot.UnifiedDiffContent("notes/a.txt", []byte("alpha\nBETA\ngamma\n"))
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(diff, "--- notes/a.txt") || !strings.Contains(diff, "+ BETA") && !strings.Contains(diff, "+BETA") {
		t.Fatalf("unexpected diff output: %q", diff)
	}
	if !strings.Contains(diff, "-beta") {
		t.Fatalf("expected removal in diff: %q", diff)
	}

	pathInfo, err := vfsRoot.Path("notes/a.txt")
	if err != nil {
		t.Fatal(err)
	}
	if !pathInfo.Exists || pathInfo.IsDir {
		t.Fatalf("unexpected path info for file: %#v", pathInfo)
	}
	if got, want := pathInfo.RelativePath, "notes/a.txt"; got != want {
		t.Fatalf("unexpected relative path: got=%q want=%q", got, want)
	}
	if !strings.HasSuffix(filepath.ToSlash(pathInfo.AbsolutePath), "workspace/notes/a.txt") {
		t.Fatalf("unexpected absolute path: %#v", pathInfo)
	}

	missingInfo, err := vfsRoot.Path("notes/missing.txt")
	if err != nil {
		t.Fatal(err)
	}
	if missingInfo.Exists {
		t.Fatalf("expected missing path to report exists=false: %#v", missingInfo)
	}
}

func TestReadFileLimits(t *testing.T) {
	base := t.TempDir()
	vfsRoot, err := New(filepath.Join(base, "workspace"))
	if err != nil {
		t.Fatal(err)
	}
	huge := strings.Repeat("x", 32)
	if err := vfsRoot.WriteFile("big.txt", []byte(huge)); err != nil {
		t.Fatal(err)
	}
	if _, err := vfsRoot.ReadFileWithLimits("big.txt", 8, 10); !errors.Is(err, ErrReadTooLarge) {
		t.Fatalf("expected size limit error, got %v", err)
	}

	manyLines := strings.Repeat("a\n", 5)
	if err := vfsRoot.WriteFile("lines.txt", []byte(manyLines)); err != nil {
		t.Fatal(err)
	}
	if _, err := vfsRoot.ReadFileWithLimits("lines.txt", 1024, 2); !errors.Is(err, ErrTooManyLines) {
		t.Fatalf("expected line limit error, got %v", err)
	}
}

func TestPatchReplace(t *testing.T) {
	base := t.TempDir()
	vfsRoot, err := New(filepath.Join(base, "workspace"))
	if err != nil {
		t.Fatal(err)
	}
	if err := vfsRoot.WriteFile("notes/a.txt", []byte("alpha\nbeta\nbeta\n")); err != nil {
		t.Fatal(err)
	}

	result, err := vfsRoot.PatchReplace("notes/a.txt", "beta", "BETA", false)
	if err != nil {
		t.Fatal(err)
	}
	if result.Replacements != 1 {
		t.Fatalf("unexpected replacement count: %#v", result)
	}
	body, err := vfsRoot.ReadFile("notes/a.txt")
	if err != nil {
		t.Fatal(err)
	}
	if got, want := string(body.Content), "alpha\nBETA\nbeta\n"; got != want {
		t.Fatalf("unexpected patched body: got=%q want=%q", got, want)
	}
	if !strings.Contains(result.Diff, "-beta") || !strings.Contains(result.Diff, "+BETA") {
		t.Fatalf("unexpected diff: %q", result.Diff)
	}

	result, err = vfsRoot.PatchReplace("notes/a.txt", "beta", "BETA", true)
	if err != nil {
		t.Fatal(err)
	}
	if result.Replacements != 1 {
		t.Fatalf("unexpected replace_all count after prior patch: %#v", result)
	}
}
