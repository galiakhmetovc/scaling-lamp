package skills

import (
	"fmt"
	"path/filepath"
	"strings"

	"gopkg.in/yaml.v3"
)

func ParseMarkdown(path string, raw string) (Bundle, error) {
	bundle := Bundle{Path: path}
	body := raw

	if strings.HasPrefix(raw, "---\n") {
		rest := strings.TrimPrefix(raw, "---\n")
		if idx := strings.Index(rest, "\n---\n"); idx >= 0 {
			header := rest[:idx]
			body = rest[idx+5:]
			applyFrontmatter(&bundle, header)
		}
	}

	body = strings.TrimSpace(body)
	if bundle.Name == "" {
		bundle.Name = filepath.Base(filepath.Dir(path))
	}
	if bundle.Description == "" {
		for _, line := range strings.Split(body, "\n") {
			line = strings.TrimSpace(line)
			if line == "" || strings.HasPrefix(line, "#") {
				continue
			}
			bundle.Description = line
			break
		}
	}
	if bundle.Name == "" {
		return Bundle{}, fmt.Errorf("skill name could not be determined")
	}
	bundle.Prompt = body
	return bundle, nil
}

func applyFrontmatter(bundle *Bundle, header string) {
	var meta map[string]any
	if err := yaml.Unmarshal([]byte(header), &meta); err == nil {
		applyMetaMap(bundle, meta)
		return
	}

	// Lenient fallback for malformed YAML: preserve simple key:value parsing.
	for _, line := range strings.Split(header, "\n") {
		line = strings.TrimSpace(line)
		if line == "" || !strings.Contains(line, ":") {
			continue
		}
		key, value, _ := strings.Cut(line, ":")
		key = strings.TrimSpace(strings.ToLower(key))
		value = strings.Trim(strings.TrimSpace(value), `"'`)
		switch key {
		case "name":
			bundle.Name = value
		case "description":
			bundle.Description = value
		case "version":
			bundle.Version = value
		case "license":
			bundle.License = value
		}
	}
}

func applyMetaMap(bundle *Bundle, meta map[string]any) {
	bundle.Name = scalarString(meta["name"], bundle.Name)
	bundle.Description = scalarString(meta["description"], bundle.Description)
	bundle.Version = scalarString(meta["version"], bundle.Version)
	bundle.License = scalarString(meta["license"], bundle.License)
	bundle.AllowedTools = stringList(meta["allowed-tools"])
	if len(bundle.AllowedTools) == 0 {
		bundle.AllowedTools = stringList(meta["allowed_tools"])
	}
}

func scalarString(value any, fallback string) string {
	switch v := value.(type) {
	case nil:
		return fallback
	case string:
		return strings.TrimSpace(v)
	default:
		return strings.TrimSpace(fmt.Sprint(v))
	}
}

func stringList(value any) []string {
	switch v := value.(type) {
	case nil:
		return nil
	case []any:
		out := make([]string, 0, len(v))
		for _, item := range v {
			text := strings.TrimSpace(fmt.Sprint(item))
			if text != "" {
				out = append(out, text)
			}
		}
		return out
	case []string:
		out := make([]string, 0, len(v))
		for _, item := range v {
			item = strings.TrimSpace(item)
			if item != "" {
				out = append(out, item)
			}
		}
		return out
	default:
		text := strings.TrimSpace(fmt.Sprint(v))
		if text == "" {
			return nil
		}
		return []string{text}
	}
}
