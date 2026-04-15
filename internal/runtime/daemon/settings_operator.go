package daemon

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"os"
	"path"
	"path/filepath"
	"slices"
	"strconv"
	"strings"

	"gopkg.in/yaml.v3"

	"teamd/internal/contracts"
	"teamd/internal/runtime"
)

type SettingsSnapshot struct {
	Revision   string                 `json:"revision"`
	FormFields []SettingsFieldState   `json:"form_fields"`
	RawFiles   []SettingsRawFileState `json:"raw_files"`
}

type SettingsFieldState struct {
	Key      string `json:"key"`
	Label    string `json:"label"`
	Type     string `json:"type"`
	Value    any    `json:"value"`
	FilePath string `json:"file_path"`
	Revision string `json:"revision"`
}

type SettingsRawFileState struct {
	Path     string `json:"path"`
	Revision string `json:"revision"`
	Size     int    `json:"size"`
}

type SettingsRawFileContent struct {
	Path     string `json:"path"`
	Revision string `json:"revision"`
	Content  string `json:"content"`
}

func (s *Server) settingsSnapshot() (SettingsSnapshot, error) {
	agent := s.currentAgent()
	root := filepath.Dir(agent.ConfigPath)
	params := agent.Contracts.OperatorSurface.Settings.Params

	fileRevisions, err := s.settingsFileRevisions(agent)
	if err != nil {
		return SettingsSnapshot{}, err
	}
	fields := make([]SettingsFieldState, 0, len(params.FormFields))
	for _, field := range params.FormFields {
		value, err := readYAMLScalar(filepath.Join(root, field.FilePath), field.YAMLPath)
		if err != nil {
			return SettingsSnapshot{}, fmt.Errorf("read settings field %q: %w", field.Key, err)
		}
		fields = append(fields, SettingsFieldState{
			Key:      field.Key,
			Label:    field.Label,
			Type:     field.Type,
			Value:    value,
			FilePath: field.FilePath,
			Revision: fileRevisions[field.FilePath],
		})
	}
	rawFiles, err := s.discoverRawFiles(root, params.RawFileGlobs)
	if err != nil {
		return SettingsSnapshot{}, err
	}
	raw := make([]SettingsRawFileState, 0, len(rawFiles))
	for _, rel := range rawFiles {
		body, err := os.ReadFile(filepath.Join(root, rel))
		if err != nil {
			return SettingsSnapshot{}, err
		}
		raw = append(raw, SettingsRawFileState{
			Path:     rel,
			Revision: hashBytes(body),
			Size:     len(body),
		})
	}
	return SettingsSnapshot{
		Revision:   hashSettingsSnapshot(fields, raw),
		FormFields: fields,
		RawFiles:   raw,
	}, nil
}

func (s *Server) settingsRawFile(path string) (SettingsRawFileContent, error) {
	agent := s.currentAgent()
	root := filepath.Dir(agent.ConfigPath)
	allowed, err := s.discoverRawFiles(root, agent.Contracts.OperatorSurface.Settings.Params.RawFileGlobs)
	if err != nil {
		return SettingsRawFileContent{}, err
	}
	clean := filepath.Clean(path)
	if !slices.Contains(allowed, clean) {
		return SettingsRawFileContent{}, fmt.Errorf("raw settings path %q is not allowed", path)
	}
	body, err := os.ReadFile(filepath.Join(root, clean))
	if err != nil {
		return SettingsRawFileContent{}, err
	}
	return SettingsRawFileContent{
		Path:     clean,
		Revision: hashBytes(body),
		Content:  string(body),
	}, nil
}

func (s *Server) applyFormSettings(ctx context.Context, baseRevision string, values map[string]any) (SettingsSnapshot, error) {
	if err := s.ensureSettingsApplyAllowed(); err != nil {
		return SettingsSnapshot{}, err
	}
	current, err := s.settingsSnapshot()
	if err != nil {
		return SettingsSnapshot{}, err
	}
	if strings.TrimSpace(baseRevision) == "" {
		return SettingsSnapshot{}, fmt.Errorf("settings.form.apply requires base_revision")
	}
	if current.Revision != baseRevision {
		return SettingsSnapshot{}, fmt.Errorf("settings revision conflict")
	}
	agent := s.currentAgent()
	root := filepath.Dir(agent.ConfigPath)
	fieldsByKey := map[string]contracts.SettingsFormField{}
	for _, field := range agent.Contracts.OperatorSurface.Settings.Params.FormFields {
		fieldsByKey[field.Key] = field
	}
	updatesByFile := map[string]map[string]any{}
	originalByFile := map[string][]byte{}
	for key, rawValue := range values {
		field, ok := fieldsByKey[key]
		if !ok {
			return SettingsSnapshot{}, fmt.Errorf("settings field %q is not allowed", key)
		}
		typedValue, err := coerceSettingsValue(field, rawValue)
		if err != nil {
			return SettingsSnapshot{}, fmt.Errorf("settings field %q: %w", key, err)
		}
		if updatesByFile[field.FilePath] == nil {
			updatesByFile[field.FilePath] = map[string]any{}
		}
		updatesByFile[field.FilePath][key] = typedValue
		if _, ok := originalByFile[field.FilePath]; !ok {
			body, err := os.ReadFile(filepath.Join(root, field.FilePath))
			if err != nil {
				return SettingsSnapshot{}, err
			}
			originalByFile[field.FilePath] = body
		}
	}
	for _, field := range agent.Contracts.OperatorSurface.Settings.Params.FormFields {
		fileUpdates := updatesByFile[field.FilePath]
		value, ok := fileUpdates[field.Key]
		if !ok {
			continue
		}
		if err := updateYAMLScalar(filepath.Join(root, field.FilePath), field.YAMLPath, value); err != nil {
			return SettingsSnapshot{}, fmt.Errorf("write settings field %q: %w", field.Key, err)
		}
	}
	if err := s.reloadAgentFromDisk(ctx); err != nil {
		if rollbackErr := restoreFiles(root, originalByFile); rollbackErr != nil {
			return SettingsSnapshot{}, fmt.Errorf("%w (rollback failed: %v)", err, rollbackErr)
		}
		return SettingsSnapshot{}, err
	}
	return s.settingsSnapshot()
}

func (s *Server) applyRawSettings(ctx context.Context, path, baseRevision, content string) (SettingsSnapshot, error) {
	if err := s.ensureSettingsApplyAllowed(); err != nil {
		return SettingsSnapshot{}, err
	}
	current, err := s.settingsRawFile(path)
	if err != nil {
		return SettingsSnapshot{}, err
	}
	if strings.TrimSpace(baseRevision) == "" {
		return SettingsSnapshot{}, fmt.Errorf("settings.raw.apply requires base_revision")
	}
	if current.Revision != baseRevision {
		return SettingsSnapshot{}, fmt.Errorf("settings raw revision conflict")
	}
	agent := s.currentAgent()
	root := filepath.Dir(agent.ConfigPath)
	clean := filepath.Clean(path)
	original, err := os.ReadFile(filepath.Join(root, clean))
	if err != nil {
		return SettingsSnapshot{}, err
	}
	if err := os.WriteFile(filepath.Join(root, clean), []byte(content), 0o644); err != nil {
		return SettingsSnapshot{}, err
	}
	if err := s.reloadAgentFromDisk(ctx); err != nil {
		if rollbackErr := os.WriteFile(filepath.Join(root, clean), original, 0o644); rollbackErr != nil {
			return SettingsSnapshot{}, fmt.Errorf("%w (rollback failed: %v)", err, rollbackErr)
		}
		return SettingsSnapshot{}, err
	}
	return s.settingsSnapshot()
}

func (s *Server) reloadAgentFromDisk(ctx context.Context) error {
	_ = ctx
	current := s.currentAgent()
	reloaded, err := runtime.BuildAgent(current.ConfigPath)
	if err != nil {
		return err
	}
	reloaded.UIBus = current.UIBus
	s.swapAgent(reloaded)
	return nil
}

func (s *Server) publishSettingsApplied() {
	reloaded := s.currentAgent()
	s.publishDaemon(WebsocketEnvelope{
		Type: "settings_applied",
		Payload: map[string]any{
			"agent_id":     reloaded.Config.ID,
			"config_path":  reloaded.ConfigPath,
			"generated_at": reloaded.Now().UTC(),
		},
	})
}

func (s *Server) ensureSettingsApplyAllowed() error {
	agent := s.currentAgent()
	if !agent.Contracts.OperatorSurface.Settings.Params.RequireIdleForApply {
		return nil
	}
	s.runtimeMu.RLock()
	defer s.runtimeMu.RUnlock()
	for sessionID, state := range s.sessionRuntime {
		if state.active {
			return fmt.Errorf("cannot apply settings while session %q is running", sessionID)
		}
		if len(state.queue) > 0 {
			return fmt.Errorf("cannot apply settings while session %q has queued drafts", sessionID)
		}
	}
	for _, session := range agent.ListSessions() {
		if len(agent.PendingShellApprovals(session.SessionID)) > 0 {
			return fmt.Errorf("cannot apply settings while session %q has pending approvals", session.SessionID)
		}
		if len(agent.CurrentRunningShellCommands(session.SessionID)) > 0 {
			return fmt.Errorf("cannot apply settings while session %q has running shell commands", session.SessionID)
		}
	}
	return nil
}

func (s *Server) settingsFileRevisions(agent *runtime.Agent) (map[string]string, error) {
	root := filepath.Dir(agent.ConfigPath)
	files := map[string]struct{}{}
	for _, field := range agent.Contracts.OperatorSurface.Settings.Params.FormFields {
		files[field.FilePath] = struct{}{}
	}
	out := make(map[string]string, len(files))
	for file := range files {
		body, err := os.ReadFile(filepath.Join(root, file))
		if err != nil {
			return nil, err
		}
		out[file] = hashBytes(body)
	}
	return out, nil
}

func (s *Server) discoverRawFiles(root string, globs []string) ([]string, error) {
	seen := map[string]struct{}{}
	var out []string
	for _, pattern := range globs {
		if strings.TrimSpace(pattern) == "" {
			continue
		}
		if strings.Contains(pattern, "**") {
			if err := filepath.WalkDir(root, func(match string, d os.DirEntry, err error) error {
				if err != nil || d.IsDir() {
					return err
				}
				rel, err := filepath.Rel(root, match)
				if err != nil {
					return err
				}
				rel = filepath.ToSlash(rel)
				ok, err := doublestarMatch(pattern, rel)
				if err != nil || !ok {
					return err
				}
				if _, exists := seen[rel]; exists {
					return nil
				}
				seen[rel] = struct{}{}
				out = append(out, rel)
				return nil
			}); err != nil {
				return nil, fmt.Errorf("glob %q: %w", pattern, err)
			}
			continue
		}
		matches, err := filepath.Glob(filepath.Join(root, filepath.FromSlash(pattern)))
		if err != nil {
			return nil, fmt.Errorf("glob %q: %w", pattern, err)
		}
		for _, match := range matches {
			info, err := os.Stat(match)
			if err != nil || info.IsDir() {
				continue
			}
			rel, err := filepath.Rel(root, match)
			if err != nil {
				return nil, err
			}
			rel = filepath.ToSlash(rel)
			if _, ok := seen[rel]; ok {
				continue
			}
			seen[rel] = struct{}{}
			out = append(out, rel)
		}
	}
	slices.Sort(out)
	return out, nil
}

func doublestarMatch(pattern, target string) (bool, error) {
	return matchPatternSegments(strings.Split(path.Clean(pattern), "/"), strings.Split(path.Clean(target), "/"))
}

func matchPatternSegments(pattern, target []string) (bool, error) {
	if len(pattern) == 0 {
		return len(target) == 0, nil
	}
	if pattern[0] == "**" {
		if len(pattern) == 1 {
			return true, nil
		}
		for i := 0; i <= len(target); i++ {
			ok, err := matchPatternSegments(pattern[1:], target[i:])
			if err != nil {
				return false, err
			}
			if ok {
				return true, nil
			}
		}
		return false, nil
	}
	if len(target) == 0 {
		return false, nil
	}
	ok, err := path.Match(pattern[0], target[0])
	if err != nil || !ok {
		return ok, err
	}
	return matchPatternSegments(pattern[1:], target[1:])
}

func readYAMLScalar(path string, yamlPath []string) (any, error) {
	var doc map[string]any
	body, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	if err := yaml.Unmarshal(body, &doc); err != nil {
		return nil, err
	}
	return lookupYAMLPath(doc, yamlPath)
}

func updateYAMLScalar(path string, yamlPath []string, value any) error {
	var doc map[string]any
	body, err := os.ReadFile(path)
	if err != nil {
		return err
	}
	if err := yaml.Unmarshal(body, &doc); err != nil {
		return err
	}
	setYAMLPath(doc, yamlPath, value)
	out, err := yaml.Marshal(doc)
	if err != nil {
		return err
	}
	return os.WriteFile(path, out, 0o644)
}

func lookupYAMLPath(doc map[string]any, yamlPath []string) (any, error) {
	var current any = doc
	for _, part := range yamlPath {
		asMap, ok := normalizeYAMLMap(current)
		if !ok {
			return nil, fmt.Errorf("yaml path %q is not a map", strings.Join(yamlPath, "."))
		}
		next, ok := asMap[part]
		if !ok {
			return nil, fmt.Errorf("yaml path %q not found", strings.Join(yamlPath, "."))
		}
		current = next
	}
	return current, nil
}

func setYAMLPath(doc map[string]any, yamlPath []string, value any) {
	current := doc
	for i, part := range yamlPath {
		if i == len(yamlPath)-1 {
			current[part] = value
			return
		}
		next, ok := normalizeYAMLMap(current[part])
		if !ok {
			next = map[string]any{}
			current[part] = next
		}
		current = next
	}
}

func normalizeYAMLMap(value any) (map[string]any, bool) {
	if typed, ok := value.(map[string]any); ok {
		return typed, true
	}
	if typed, ok := value.(map[any]any); ok {
		out := map[string]any{}
		for key, nested := range typed {
			text, ok := key.(string)
			if !ok {
				continue
			}
			out[text] = nested
		}
		return out, true
	}
	return nil, false
}

func coerceSettingsValue(field contracts.SettingsFormField, raw any) (any, error) {
	switch field.Type {
	case "int":
		switch value := raw.(type) {
		case int:
			return value, nil
		case int64:
			return int(value), nil
		case float64:
			return int(value), nil
		case string:
			parsed, err := strconv.Atoi(strings.TrimSpace(value))
			if err != nil {
				return nil, err
			}
			return parsed, nil
		default:
			return nil, fmt.Errorf("unsupported int value %T", raw)
		}
	case "bool":
		switch value := raw.(type) {
		case bool:
			return value, nil
		case string:
			parsed, err := strconv.ParseBool(strings.TrimSpace(value))
			if err != nil {
				return nil, err
			}
			return parsed, nil
		default:
			return nil, fmt.Errorf("unsupported bool value %T", raw)
		}
	case "string":
		value, ok := raw.(string)
		if !ok {
			return nil, fmt.Errorf("unsupported string value %T", raw)
		}
		if len(field.Enum) > 0 && !slices.Contains(field.Enum, value) {
			return nil, fmt.Errorf("value %q is not allowed", value)
		}
		return value, nil
	default:
		return nil, fmt.Errorf("unsupported settings field type %q", field.Type)
	}
}

func hashSettingsSnapshot(fields []SettingsFieldState, raw []SettingsRawFileState) string {
	var builder strings.Builder
	for _, field := range fields {
		builder.WriteString(field.Key)
		builder.WriteString("=")
		builder.WriteString(fmt.Sprintf("%v", field.Value))
		builder.WriteString("@")
		builder.WriteString(field.Revision)
		builder.WriteString("\n")
	}
	for _, file := range raw {
		builder.WriteString(file.Path)
		builder.WriteString("@")
		builder.WriteString(file.Revision)
		builder.WriteString("\n")
	}
	return hashBytes([]byte(builder.String()))
}

func hashBytes(body []byte) string {
	sum := sha256.Sum256(body)
	return hex.EncodeToString(sum[:])
}

func restoreFiles(root string, originals map[string][]byte) error {
	for file, body := range originals {
		if err := os.WriteFile(filepath.Join(root, file), body, 0o644); err != nil {
			return err
		}
	}
	return nil
}
