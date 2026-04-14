package vfs

import (
	"bufio"
	"bytes"
	"errors"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strconv"
	"strings"
	"time"
)

var (
	ErrOutsideRoot   = errors.New("path outside vfs root")
	ErrReadTooLarge  = errors.New("file exceeds read limit")
	ErrTooManyLines  = errors.New("file exceeds line limit")
	ErrInvalidRange  = errors.New("invalid line range")
	ErrInvalidSearch = errors.New("invalid search query")
)

const (
	DefaultMaxReadBytes   int64 = 1 << 20
	DefaultMaxReadLines         = 20_000
	DefaultMaxSearchBytes int64 = 1 << 20
	DefaultMaxMatches           = 1_000
)

type Root struct {
	root string
}

type Entry struct {
	Path    string
	Name    string
	IsDir   bool
	Size    int64
	Mode    fs.FileMode
	ModTime time.Time
	Depth   int
}

type FileContent struct {
	Path    string
	Size    int64
	Lines   int
	Content []byte
}

type SearchHit struct {
	Path    string
	Line    int
	Column  int
	Snippet string
	Matches int
}

type PathInfo struct {
	Root         string
	RelativePath string
	AbsolutePath string
	Exists       bool
	IsDir        bool
	Size         int64
}

type DiffSource struct {
	Path    string
	Content []byte
	Label   string
}

type PatchResult struct {
	Path         string
	Replacements int
	Diff         string
}

func New(root string) (*Root, error) {
	root = strings.TrimSpace(root)
	if root == "" {
		return nil, fmt.Errorf("vfs root is required")
	}
	absRoot, err := filepath.Abs(root)
	if err != nil {
		return nil, fmt.Errorf("abs root: %w", err)
	}
	if err := os.MkdirAll(absRoot, 0o755); err != nil {
		return nil, fmt.Errorf("create root: %w", err)
	}
	resolved, err := filepath.EvalSymlinks(absRoot)
	if err != nil {
		return nil, fmt.Errorf("resolve root: %w", err)
	}
	return &Root{root: resolved}, nil
}

func (r *Root) Root() string {
	if r == nil {
		return ""
	}
	return r.root
}

func (r *Root) Path(rel string) (PathInfo, error) {
	target, err := r.resolveLexical(rel)
	if err != nil {
		return PathInfo{}, err
	}
	info := PathInfo{
		Root:         r.root,
		RelativePath: filepath.ToSlash(relForRoot(r.root, target)),
		AbsolutePath: target,
	}
	stat, err := os.Stat(target)
	if err == nil {
		info.Exists = true
		info.IsDir = stat.IsDir()
		info.Size = stat.Size()
		return info, nil
	}
	if errors.Is(err, os.ErrNotExist) {
		return info, nil
	}
	return PathInfo{}, err
}

func (r *Root) List(rel string) ([]Entry, error) {
	target, err := r.resolveExisting(rel)
	if err != nil {
		return nil, err
	}
	entries, err := os.ReadDir(target)
	if err != nil {
		return nil, err
	}
	out := make([]Entry, 0, len(entries))
	for _, entry := range entries {
		info, err := entry.Info()
		if err != nil {
			return nil, err
		}
		name := entry.Name()
		childPath := filepath.ToSlash(cleanJoin(rel, name))
		out = append(out, Entry{
			Path:    childPath,
			Name:    name,
			IsDir:   entry.IsDir(),
			Size:    info.Size(),
			Mode:    info.Mode(),
			ModTime: info.ModTime(),
		})
	}
	sortEntries(out)
	return out, nil
}

func (r *Root) Tree(rel string) ([]Entry, error) {
	target, err := r.resolveExisting(rel)
	if err != nil {
		return nil, err
	}
	out := make([]Entry, 0)
	err = filepath.WalkDir(target, func(path string, d fs.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		if path == target {
			return nil
		}
		relPath, err := filepath.Rel(r.root, path)
		if err != nil {
			return err
		}
		info, err := d.Info()
		if err != nil {
			return err
		}
		out = append(out, Entry{
			Path:    filepath.ToSlash(relPath),
			Name:    d.Name(),
			IsDir:   d.IsDir(),
			Size:    info.Size(),
			Mode:    info.Mode(),
			ModTime: info.ModTime(),
			Depth:   depthFrom(target, path),
		})
		return nil
	})
	if err != nil {
		return nil, err
	}
	sortEntries(out)
	return out, nil
}

func (r *Root) Search(rel, pattern string) ([]SearchHit, error) {
	target, err := r.resolveExisting(rel)
	if err != nil {
		return nil, err
	}
	if strings.TrimSpace(pattern) == "" {
		return nil, ErrInvalidSearch
	}
	re, err := regexp.Compile(pattern)
	if err != nil {
		return nil, err
	}

	hits := make([]SearchHit, 0)
	err = filepath.WalkDir(target, func(path string, d fs.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		if d.IsDir() {
			return nil
		}
		info, err := d.Info()
		if err != nil {
			return err
		}
		if info.Size() > DefaultMaxSearchBytes {
			return nil
		}
		fileHits, err := searchFile(r.root, path, re, DefaultMaxMatches-len(hits))
		if err != nil {
			return err
		}
		hits = append(hits, fileHits...)
		if len(hits) >= DefaultMaxMatches {
			return errSearchDone
		}
		return nil
	})
	if errors.Is(err, errSearchDone) {
		err = nil
	}
	if err != nil {
		return nil, err
	}
	sort.Slice(hits, func(i, j int) bool {
		if hits[i].Path == hits[j].Path {
			return hits[i].Line < hits[j].Line
		}
		return hits[i].Path < hits[j].Path
	})
	return hits, nil
}

func (r *Root) ReadFile(rel string) (FileContent, error) {
	return r.ReadFileWithLimits(rel, DefaultMaxReadBytes, DefaultMaxReadLines)
}

func (r *Root) ReadFileWithLimits(rel string, maxBytes int64, maxLines int) (FileContent, error) {
	target, err := r.resolveExisting(rel)
	if err != nil {
		return FileContent{}, err
	}
	info, err := os.Stat(target)
	if err != nil {
		return FileContent{}, err
	}
	if info.Size() > maxBytes {
		return FileContent{}, ErrReadTooLarge
	}
	body, err := os.ReadFile(target)
	if err != nil {
		return FileContent{}, err
	}
	lines := countLines(body)
	if lines > maxLines {
		return FileContent{}, ErrTooManyLines
	}
	return FileContent{
		Path:    filepath.ToSlash(relForRoot(r.root, target)),
		Size:    int64(len(body)),
		Lines:   lines,
		Content: body,
	}, nil
}

func (r *Root) ReadLines(rel string, start, end int) ([]string, error) {
	if start < 1 || end < start {
		return nil, ErrInvalidRange
	}
	target, err := r.resolveExisting(rel)
	if err != nil {
		return nil, err
	}
	file, err := os.Open(target)
	if err != nil {
		return nil, err
	}
	defer file.Close()

	scanner := bufio.NewScanner(file)
	scanner.Buffer(make([]byte, 0, 64*1024), int(DefaultMaxReadBytes))
	lines := make([]string, 0, end-start+1)
	lineNo := 0
	for scanner.Scan() {
		lineNo++
		if lineNo < start {
			continue
		}
		if lineNo > end {
			break
		}
		lines = append(lines, scanner.Text())
	}
	if err := scanner.Err(); err != nil {
		return nil, err
	}
	return lines, nil
}

func (r *Root) WriteFile(rel string, content []byte) error {
	target, err := r.resolveForCreate(rel)
	if err != nil {
		return err
	}
	if err := os.MkdirAll(filepath.Dir(target), 0o755); err != nil {
		return err
	}
	return os.WriteFile(target, content, 0o644)
}

func (r *Root) Mkdir(rel string) error {
	target, err := r.resolveForCreate(rel)
	if err != nil {
		return err
	}
	return os.MkdirAll(target, 0o755)
}

func (r *Root) Touch(rel string) error {
	target, err := r.resolveForCreate(rel)
	if err != nil {
		return err
	}
	if err := os.MkdirAll(filepath.Dir(target), 0o755); err != nil {
		return err
	}
	now := time.Now()
	if _, err := os.Stat(target); err == nil {
		return os.Chtimes(target, now, now)
	}
	file, err := os.OpenFile(target, os.O_CREATE|os.O_WRONLY, 0o644)
	if err != nil {
		return err
	}
	return file.Close()
}

func (r *Root) UnifiedDiffFiles(leftRel, rightRel string) (string, error) {
	left, err := r.ReadFile(leftRel)
	if err != nil {
		return "", err
	}
	right, err := r.ReadFile(rightRel)
	if err != nil {
		return "", err
	}
	return unifiedDiff(filepath.ToSlash(left.Path), filepath.ToSlash(right.Path), left.Content, right.Content), nil
}

func (r *Root) UnifiedDiffContent(leftRel string, rightContent []byte) (string, error) {
	left, err := r.ReadFile(leftRel)
	if err != nil {
		return "", err
	}
	label := "content"
	return unifiedDiff(filepath.ToSlash(left.Path), label, left.Content, rightContent), nil
}

func (r *Root) PatchReplace(rel, oldText, newText string, replaceAll bool) (PatchResult, error) {
	if oldText == "" {
		return PatchResult{}, fmt.Errorf("old text is required")
	}
	file, err := r.ReadFile(rel)
	if err != nil {
		return PatchResult{}, err
	}
	content := string(file.Content)
	count := strings.Count(content, oldText)
	if count == 0 {
		return PatchResult{}, fmt.Errorf("old text not found in %s", rel)
	}
	replacements := 1
	if replaceAll {
		replacements = count
	}
	updated := strings.Replace(content, oldText, newText, replacements)
	diff := unifiedDiff(filepath.ToSlash(file.Path), filepath.ToSlash(file.Path), file.Content, []byte(updated))
	if err := r.WriteFile(rel, []byte(updated)); err != nil {
		return PatchResult{}, err
	}
	return PatchResult{
		Path:         filepath.ToSlash(file.Path),
		Replacements: replacements,
		Diff:         diff,
	}, nil
}

func (r *Root) resolveExisting(rel string) (string, error) {
	target, err := r.resolveLexical(rel)
	if err != nil {
		return "", err
	}
	resolved, err := filepath.EvalSymlinks(target)
	if err != nil {
		return "", err
	}
	if !withinRoot(r.root, resolved) {
		return "", ErrOutsideRoot
	}
	return resolved, nil
}

func (r *Root) resolveForCreate(rel string) (string, error) {
	target, err := r.resolveLexical(rel)
	if err != nil {
		return "", err
	}
	if info, err := os.Lstat(target); err == nil {
		resolved, err := filepath.EvalSymlinks(target)
		if err != nil {
			return "", err
		}
		if !withinRoot(r.root, resolved) {
			return "", ErrOutsideRoot
		}
		if info.Mode()&os.ModeSymlink != 0 {
			return target, nil
		}
		return target, nil
	} else if !os.IsNotExist(err) {
		return "", err
	}
	parent := filepath.Dir(target)
	resolvedParent, err := r.resolveNearestExisting(parent)
	if err != nil {
		return "", err
	}
	if !withinRoot(r.root, resolvedParent) {
		return "", ErrOutsideRoot
	}
	return target, nil
}

func (r *Root) resolveNearestExisting(path string) (string, error) {
	current := path
	for {
		info, err := os.Lstat(current)
		if err == nil {
			if info.Mode()&os.ModeSymlink != 0 {
				resolved, err := filepath.EvalSymlinks(current)
				if err != nil {
					return "", err
				}
				return resolved, nil
			}
			return current, nil
		}
		if !os.IsNotExist(err) {
			return "", err
		}
		next := filepath.Dir(current)
		if next == current {
			return "", fmt.Errorf("no existing path under %q", path)
		}
		current = next
	}
}

func (r *Root) resolveLexical(rel string) (string, error) {
	if r == nil || r.root == "" {
		return "", fmt.Errorf("vfs root is not initialized")
	}
	if strings.TrimSpace(rel) == "" || rel == "." {
		return r.root, nil
	}
	rel = filepath.FromSlash(rel)
	if filepath.IsAbs(rel) {
		return "", ErrOutsideRoot
	}
	clean := filepath.Clean(rel)
	if clean == "." {
		return r.root, nil
	}
	if clean == ".." || strings.HasPrefix(clean, ".."+string(filepath.Separator)) {
		return "", ErrOutsideRoot
	}
	target := filepath.Join(r.root, clean)
	if !withinRoot(r.root, target) {
		return "", ErrOutsideRoot
	}
	return target, nil
}

func withinRoot(root, target string) bool {
	root = filepath.Clean(root)
	target = filepath.Clean(target)
	rel, err := filepath.Rel(root, target)
	if err != nil {
		return false
	}
	if rel == "." {
		return true
	}
	return rel != ".." && !strings.HasPrefix(rel, ".."+string(filepath.Separator))
}

func cleanJoin(base, name string) string {
	if base == "" || base == "." {
		return name
	}
	return filepath.Join(base, name)
}

func relForRoot(root, target string) string {
	rel, err := filepath.Rel(root, target)
	if err != nil {
		return target
	}
	if rel == "." {
		return ""
	}
	return rel
}

func countLines(body []byte) int {
	if len(body) == 0 {
		return 0
	}
	return bytes.Count(body, []byte{'\n'}) + 1
}

func sortEntries(entries []Entry) {
	sort.Slice(entries, func(i, j int) bool {
		if entries[i].Path == entries[j].Path {
			return entries[i].Name < entries[j].Name
		}
		if entries[i].Depth != entries[j].Depth {
			return entries[i].Depth < entries[j].Depth
		}
		return entries[i].Path < entries[j].Path
	})
}

func depthFrom(root, path string) int {
	rel, err := filepath.Rel(root, path)
	if err != nil || rel == "." || rel == "" {
		return 0
	}
	return len(strings.Split(filepath.ToSlash(rel), "/"))
}

var errSearchDone = errors.New("search limit reached")

func searchFile(root, path string, re *regexp.Regexp, remaining int) ([]SearchHit, error) {
	if remaining <= 0 {
		return nil, nil
	}
	file, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer file.Close()

	scanner := bufio.NewScanner(file)
	scanner.Buffer(make([]byte, 0, 64*1024), int(DefaultMaxSearchBytes))
	hits := make([]SearchHit, 0)
	lineNo := 0
	for scanner.Scan() {
		lineNo++
		line := scanner.Text()
		locs := re.FindAllStringIndex(line, -1)
		for _, loc := range locs {
			hits = append(hits, SearchHit{
				Path:    filepath.ToSlash(relForRoot(root, path)),
				Line:    lineNo,
				Column:  loc[0] + 1,
				Snippet: line,
				Matches: len(locs),
			})
			if len(hits) >= remaining {
				return hits, errSearchDone
			}
		}
	}
	if err := scanner.Err(); err != nil {
		return nil, err
	}
	return hits, nil
}

func unifiedDiff(leftLabel, rightLabel string, left, right []byte) string {
	leftLines := splitLines(left)
	rightLines := splitLines(right)
	ops := lineDiffOps(leftLines, rightLines)
	var b strings.Builder
	b.WriteString("--- ")
	b.WriteString(leftLabel)
	b.WriteByte('\n')
	b.WriteString("+++ ")
	b.WriteString(rightLabel)
	b.WriteByte('\n')
	writeUnifiedHunks(&b, ops, leftLines, rightLines)
	return b.String()
}

type diffOp struct {
	kind  byte
	value string
}

func lineDiffOps(left, right []string) []diffOp {
	m := len(left)
	n := len(right)
	table := make([][]int, m+1)
	for i := range table {
		table[i] = make([]int, n+1)
	}
	for i := m - 1; i >= 0; i-- {
		for j := n - 1; j >= 0; j-- {
			if left[i] == right[j] {
				table[i][j] = table[i+1][j+1] + 1
			} else if table[i+1][j] >= table[i][j+1] {
				table[i][j] = table[i+1][j]
			} else {
				table[i][j] = table[i][j+1]
			}
		}
	}

	ops := make([]diffOp, 0, m+n)
	i, j := 0, 0
	for i < m && j < n {
		if left[i] == right[j] {
			ops = append(ops, diffOp{kind: ' ', value: left[i]})
			i++
			j++
			continue
		}
		if table[i+1][j] >= table[i][j+1] {
			ops = append(ops, diffOp{kind: '-', value: left[i]})
			i++
			continue
		}
		ops = append(ops, diffOp{kind: '+', value: right[j]})
		j++
	}
	for ; i < m; i++ {
		ops = append(ops, diffOp{kind: '-', value: left[i]})
	}
	for ; j < n; j++ {
		ops = append(ops, diffOp{kind: '+', value: right[j]})
	}
	return ops
}

func writeUnifiedHunks(b *strings.Builder, ops []diffOp, left, right []string) {
	const context = 3
	leftLine := 1
	rightLine := 1
	i := 0
	for i < len(ops) {
		if ops[i].kind == ' ' {
			leftLine++
			rightLine++
			i++
			continue
		}

		start := i
		pre := start - context
		if pre < 0 {
			pre = 0
		}
		leftStart, rightStart := lineOffsets(ops, pre)
		hunkOps, nextIndex, leftCount, rightCount := collectHunk(ops, start, context)
		if len(hunkOps) == 0 {
			i++
			continue
		}
		b.WriteString("@@ -")
		b.WriteString(strconv.Itoa(leftStart))
		b.WriteByte(',')
		b.WriteString(strconv.Itoa(leftCount))
		b.WriteString(" +")
		b.WriteString(strconv.Itoa(rightStart))
		b.WriteByte(',')
		b.WriteString(strconv.Itoa(rightCount))
		b.WriteString(" @@\n")
		for _, op := range hunkOps {
			b.WriteByte(op.kind)
			b.WriteString(op.value)
			b.WriteByte('\n')
		}
		i = nextIndex
	}
}

func collectHunk(ops []diffOp, start, context int) ([]diffOp, int, int, int) {
	leftCount := 0
	rightCount := 0
	hunk := make([]diffOp, 0)
	i := start
	pre := start
	for pre > 0 && pre-start < context {
		pre--
		if ops[pre].kind != '+' {
			hunk = append([]diffOp{ops[pre]}, hunk...)
			if ops[pre].kind != '+' {
				leftCount++
			}
			if ops[pre].kind != '-' {
				rightCount++
			}
		}
	}
	for i < len(ops) {
		op := ops[i]
		hunk = append(hunk, op)
		if op.kind != '+' {
			leftCount++
		}
		if op.kind != '-' {
			rightCount++
		}
		i++
		if op.kind == ' ' {
			trailing := 0
			j := i
			for j < len(ops) && ops[j].kind == ' ' && trailing < context {
				hunk = append(hunk, ops[j])
				leftCount++
				rightCount++
				j++
				trailing++
			}
			if j >= len(ops) || ops[j].kind == ' ' {
				i = j
				break
			}
			i = j
		}
	}
	return hunk, i, leftCount, rightCount
}

func lineOffsets(ops []diffOp, upto int) (int, int) {
	left := 1
	right := 1
	for i := 0; i < upto && i < len(ops); i++ {
		switch ops[i].kind {
		case ' ':
			left++
			right++
		case '-':
			left++
		case '+':
			right++
		}
	}
	return left, right
}

func splitLines(body []byte) []string {
	if len(body) == 0 {
		return nil
	}
	raw := bytes.Split(body, []byte{'\n'})
	lines := make([]string, 0, len(raw))
	for i, item := range raw {
		if i == len(raw)-1 && len(item) == 0 {
			continue
		}
		lines = append(lines, string(item))
	}
	return lines
}
