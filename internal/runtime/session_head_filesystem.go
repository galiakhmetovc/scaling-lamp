package runtime

import (
	"encoding/json"
	"os"
	"path/filepath"
	"slices"
	"strings"

	"teamd/internal/contracts"
	"teamd/internal/promptassembly"
	"teamd/internal/runtime/projections"
)

func (a *Agent) filesystemHeadProjection() *projections.FilesystemHeadProjection {
	for _, projection := range a.Projections {
		filesystemHead, ok := projection.(*projections.FilesystemHeadProjection)
		if ok {
			return filesystemHead
		}
	}
	return nil
}

func (a *Agent) buildFilesystemHeadInput(contractSet contracts.ResolvedContracts, sessionID string, fallback []contracts.Message) (promptassembly.FilesystemHeadInput, error) {
	params := contractSet.PromptAssembly.SessionHead.Params
	out := promptassembly.FilesystemHeadInput{}
	if projection := a.filesystemHeadProjection(); projection != nil {
		view := projection.SnapshotForSession(sessionID)
		out.Recent = promptassembly.FilesystemRecentHeadInput{
			Edited:  append([]string(nil), view.Edited...),
			Read:    append([]string(nil), view.Read...),
			Found:   append([]string(nil), view.Found...),
			Moved:   append([]string(nil), view.Moved...),
			Trashed: append([]string(nil), view.Trashed...),
		}
	}
	out.Recent = mergeFilesystemRecent(out.Recent, buildFilesystemHeadInputForMessages(params, fallback).Recent)
	if params.IncludeFilesystemTree {
		root, err := filesystemRootPath(contractSet.FilesystemExecution.Scope)
		if err != nil {
			return promptassembly.FilesystemHeadInput{}, err
		}
		tree, err := buildFilesystemTreeEntries(params, root)
		if err != nil {
			return promptassembly.FilesystemHeadInput{}, err
		}
		out.Tree = tree
	}
	return out, nil
}

func buildFilesystemHeadInputForMessages(params contracts.SessionHeadParams, messages []contracts.Message) promptassembly.FilesystemHeadInput {
	if !params.IncludeFilesystemRecent {
		return promptassembly.FilesystemHeadInput{}
	}
	out := promptassembly.FilesystemHeadInput{}
	for _, message := range messages {
		if message.Role != "tool" || strings.TrimSpace(message.Name) == "" {
			continue
		}
		addFilesystemRecentFromTool(&out.Recent, message.Name, nil, message.Content)
	}
	return out
}

func mergeFilesystemRecent(base, overlay promptassembly.FilesystemRecentHeadInput) promptassembly.FilesystemRecentHeadInput {
	return promptassembly.FilesystemRecentHeadInput{
		Edited:  mergeRecentLists(base.Edited, overlay.Edited),
		Read:    mergeRecentLists(base.Read, overlay.Read),
		Found:   mergeRecentLists(base.Found, overlay.Found),
		Moved:   mergeRecentLists(base.Moved, overlay.Moved),
		Trashed: mergeRecentLists(base.Trashed, overlay.Trashed),
	}
}

func mergeRecentLists(base, overlay []string) []string {
	out := append([]string(nil), overlay...)
	for _, item := range base {
		if item == "" || slices.Contains(out, item) {
			continue
		}
		out = append(out, item)
	}
	return out
}

func buildFilesystemTreeEntries(params contracts.SessionHeadParams, root string) ([]promptassembly.FilesystemTreeEntry, error) {
	if !params.IncludeFilesystemTree {
		return nil, nil
	}
	entries, err := os.ReadDir(root)
	if err != nil {
		return nil, err
	}
	out := make([]promptassembly.FilesystemTreeEntry, 0, len(entries))
	for _, entry := range entries {
		if entry.IsDir() && !params.FilesystemTreeIncludeDirs {
			continue
		}
		if !entry.IsDir() && !params.FilesystemTreeIncludeFiles {
			continue
		}
		out = append(out, promptassembly.FilesystemTreeEntry{Name: entry.Name(), IsDir: entry.IsDir()})
	}
	slices.SortFunc(out, func(a, b promptassembly.FilesystemTreeEntry) int {
		if a.Name < b.Name {
			return -1
		}
		if a.Name > b.Name {
			return 1
		}
		return 0
	})
	if params.FilesystemTreeMaxEntries > 0 && len(out) > params.FilesystemTreeMaxEntries {
		out = out[:params.FilesystemTreeMaxEntries]
	}
	return out, nil
}

func filesystemRootPath(policy contracts.FilesystemScopePolicy) (string, error) {
	root := strings.TrimSpace(policy.Params.RootPath)
	if root == "" {
		root = "."
	}
	return filepath.Abs(root)
}

func addFilesystemRecentFromTool(view *promptassembly.FilesystemRecentHeadInput, toolName string, args map[string]any, resultText string) {
	if view == nil {
		return
	}
	switch toolName {
	case "fs_read_text", "fs_read_lines", "fs_search_text":
		appendUniqueRecent(&view.Read, argString(args, "path"), payloadPrimaryPath(resultText))
	case "fs_replace_lines", "fs_replace_in_line", "fs_insert_text", "fs_write_text", "fs_patch_text":
		appendUniqueRecent(&view.Edited, argString(args, "path"), payloadPrimaryPath(resultText))
	case "fs_find_in_files":
		for _, path := range payloadMatchPaths(resultText) {
			appendUniqueRecent(&view.Found, path)
		}
	case "fs_move":
		src := firstNonEmpty(argString(args, "src"), payloadValue(resultText, "source"))
		dest := firstNonEmpty(argString(args, "dest"), payloadValue(resultText, "dest"))
		if src != "" && dest != "" {
			appendUniqueRecent(&view.Moved, src+" -> "+dest)
		}
	case "fs_trash":
		appendUniqueRecent(&view.Trashed, argString(args, "path"), payloadPrimaryPath(resultText))
	}
}

func appendUniqueRecent(target *[]string, values ...string) {
	items := append([]string(nil), *target...)
	for _, value := range values {
		value = strings.TrimSpace(value)
		if value == "" {
			continue
		}
		next := []string{value}
		for _, item := range items {
			if item == value {
				continue
			}
			next = append(next, item)
		}
		items = next
	}
	*target = items
}

func argString(args map[string]any, key string) string {
	if args == nil {
		return ""
	}
	value, _ := args[key].(string)
	return value
}

func payloadPrimaryPath(resultText string) string {
	return payloadValue(resultText, "path")
}

func payloadValue(resultText, key string) string {
	var payload map[string]any
	if err := json.Unmarshal([]byte(resultText), &payload); err != nil {
		return ""
	}
	value, _ := payload[key].(string)
	return value
}

func payloadMatchPaths(resultText string) []string {
	var payload struct {
		Matches []struct {
			Path string `json:"path"`
		} `json:"matches"`
	}
	if err := json.Unmarshal([]byte(resultText), &payload); err != nil {
		return nil
	}
	out := make([]string, 0, len(payload.Matches))
	for _, match := range payload.Matches {
		if strings.TrimSpace(match.Path) == "" {
			continue
		}
		out = append(out, match.Path)
	}
	return out
}

func firstNonEmpty(values ...string) string {
	for _, value := range values {
		if strings.TrimSpace(value) != "" {
			return value
		}
	}
	return ""
}
