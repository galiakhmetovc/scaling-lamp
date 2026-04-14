package skills

import (
	"os"
	"path/filepath"
	"sort"
	"strings"
)

type FilesystemCatalog struct {
	root string
}

func NewFilesystemCatalog(root string) FilesystemCatalog {
	return FilesystemCatalog{root: root}
}

func (c FilesystemCatalog) Load(string) ([]Bundle, error) {
	return c.List()
}

func (c FilesystemCatalog) List() ([]Bundle, error) {
	searchRoots := []string{
		filepath.Join(c.root, "skills"),
		filepath.Join(c.root, ".agents", "skills"),
	}
	seen := make(map[string]struct{})
	bundles := make([]Bundle, 0)
	for _, skillsRoot := range searchRoots {
		rootBundles, err := c.listFromRoot(skillsRoot)
		if err != nil {
			return nil, err
		}
		for _, bundle := range rootBundles {
			if _, ok := seen[bundle.Name]; ok {
				continue
			}
			seen[bundle.Name] = struct{}{}
			bundles = append(bundles, bundle)
		}
	}
	sort.Slice(bundles, func(i, j int) bool {
		return strings.Compare(bundles[i].Name, bundles[j].Name) < 0
	})
	return bundles, nil
}

func (c FilesystemCatalog) Get(name string) (Bundle, bool, error) {
	bundles, err := c.List()
	if err != nil {
		return Bundle{}, false, err
	}
	for _, bundle := range bundles {
		if bundle.Name == name {
			return bundle, true, nil
		}
	}
	return Bundle{}, false, nil
}

func hydrateResources(bundle *Bundle) error {
	skillRoot := filepath.Dir(bundle.Path)
	var err error
	if bundle.Scripts, err = listResourceFiles(skillRoot, "scripts"); err != nil {
		return err
	}
	if bundle.References, err = listResourceFiles(skillRoot, "references"); err != nil {
		return err
	}
	if bundle.Assets, err = listResourceFiles(skillRoot, "assets"); err != nil {
		return err
	}
	return nil
}

func listResourceFiles(skillRoot string, dir string) ([]string, error) {
	root := filepath.Join(skillRoot, dir)
	entries := make([]string, 0)
	if _, err := os.Stat(root); err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}
	err := filepath.WalkDir(root, func(path string, d os.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		if d.IsDir() {
			return nil
		}
		rel, err := filepath.Rel(skillRoot, path)
		if err != nil {
			return err
		}
		entries = append(entries, filepath.ToSlash(rel))
		return nil
	})
	if err != nil {
		return nil, err
	}
	sort.Slice(entries, func(i, j int) bool {
		return strings.Compare(entries[i], entries[j]) < 0
	})
	return entries, nil
}

func (c FilesystemCatalog) listFromRoot(skillsRoot string) ([]Bundle, error) {
	entries, err := os.ReadDir(skillsRoot)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}
	names := make([]string, 0, len(entries))
	for _, entry := range entries {
		if entry.IsDir() {
			names = append(names, entry.Name())
		}
	}
	sort.Strings(names)
	bundles := make([]Bundle, 0, len(names))
	for _, name := range names {
		skillPath := filepath.Join(skillsRoot, name, "SKILL.md")
		raw, err := os.ReadFile(skillPath)
		if err != nil {
			if os.IsNotExist(err) {
				continue
			}
			return nil, err
		}
		bundle, err := ParseMarkdown(skillPath, string(raw))
		if err != nil {
			continue
		}
		if err := hydrateResources(&bundle); err != nil {
			return nil, err
		}
		bundles = append(bundles, bundle)
	}
	return bundles, nil
}
